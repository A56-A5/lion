use inotify::{EventMask, Inotify, WatchMask};
use std::collections::HashMap;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Spawn an inotify watcher on `paths` in the current thread (call from a background thread).
///
/// Watches until `stop` is set to true (sandbox process exited).
/// Because bwrap bind-mounts host directories, inotify watchers on the host fire
/// for every access that happens inside the sandbox as well.
pub fn watch(paths: Vec<String>, stop: Arc<AtomicBool>) {
    let (mut inotify, wd_map) = match init_watcher(&paths) {
        Ok(v) => v,
        Err(msg) => {
            eprintln!("\x1b[90m[LION] {}\x1b[0m", msg);
            return;
        }
    };

    if wd_map.is_empty() {
        eprintln!("\x1b[90m[LION] inotify: no valid paths to watch\x1b[0m");
        return;
    }

    let watched: Vec<_> = wd_map.values().collect();
    eprintln!(
        "\x1b[90m[LION] inotify watching {} path(s)\x1b[0m",
        watched.len()
    );

    let mut buffer = [0u8; 4096];
    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }

        let events = match inotify.read_events(&mut buffer) {
            Ok(e) => e,
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // No events right now — check stop flag and retry
                std::thread::sleep(std::time::Duration::from_millis(50));
                continue;
            }
            Err(_) => break,
        };

        for event in events {
            let base = match wd_map.get(&event.wd) {
                Some(p) => p.clone(),
                None => continue,
            };

            let full_path = if let Some(name) = &event.name {
                base.join(name)
            } else {
                base
            };

            let path_str = full_path.to_string_lossy();
            let (tag, color) = classify_event(event.mask);

            // Skip noisy internal events (e.g. OPEN on directory itself)
            if event.mask.contains(EventMask::ISDIR) && event.mask.contains(EventMask::OPEN) {
                continue;
            }

            let now = chrono::Local::now().format("%H:%M:%S");
            eprintln!(
                "[LION] {}  {}{}{}\x1b[0m  \x1b[36m{}\x1b[0m",
                now, color, tag, "\x1b[0m", path_str
            );
        }
    }
}

fn add_watch(
    inotify: &mut Inotify,
    map: &mut HashMap<inotify::WatchDescriptor, PathBuf>,
    path: &Path,
    mask: WatchMask,
) {
    match inotify.watches().add(path, mask) {
        Ok(wd) => {
            map.insert(wd, path.to_path_buf());
        }
        Err(e) => {
            eprintln!(
                "\x1b[90m[LION] inotify: cannot watch {}: {}\x1b[0m",
                path.display(),
                e
            );
        }
    }
}

fn classify_event(mask: EventMask) -> (&'static str, &'static str) {
    if mask.contains(EventMask::ACCESS) || mask.contains(EventMask::OPEN) || mask.contains(EventMask::CLOSE_NOWRITE) {
        ("READ   ", "\x1b[1m\x1b[32m") // green — allowed read
    } else if mask.contains(EventMask::MODIFY) {
        ("WRITE  ", "\x1b[1m\x1b[33m") // yellow — write
    } else if mask.contains(EventMask::CREATE) {
        ("CREATE ", "\x1b[1m\x1b[34m") // blue — create
    } else if mask.contains(EventMask::DELETE) {
        ("DELETE ", "\x1b[1m\x1b[31m") // red — delete
    } else {
        ("EVENT  ", "\x1b[90m")
    }
}

pub fn watch_with_tui(paths: Vec<String>, stop: Arc<AtomicBool>, tui: crate::tui::TuiHandle) {
    let (mut inotify, wd_map) = match init_watcher(&paths) {
        Ok(v) => v,
        Err(msg) => {
            tui.log(crate::tui::SandboxEvent::info(format!("inotify unavailable: {msg}")));
            return;
        }
    };

    tui.log(crate::tui::SandboxEvent::info(format!("inotify watching {} path(s)", wd_map.len())));

    let mut buffer = [0; 4096];
    while !stop.load(Ordering::Relaxed) {
        match inotify.read_events(&mut buffer) {
            Ok(events) => {
                for event in events {
                    if event.mask.contains(EventMask::ISDIR) && event.mask.contains(EventMask::OPEN) {
                        continue;
                    }
                    if let Some(parent) = wd_map.get(&event.wd) {
                        let full_path = if let Some(name) = event.name {
                            parent.join(name)
                        } else {
                            parent.clone()
                        };
                        let path_str = full_path.to_string_lossy().to_string();
                        tui.log(crate::tui::inotify_event(event.mask, path_str));
                    }
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(100));
            }
            Err(_) => break,
        }
    }
}

fn init_watcher(paths: &[String]) -> std::result::Result<(Inotify, HashMap<inotify::WatchDescriptor, PathBuf>), String> {
    let mut inotify = Inotify::init().map_err(|e| format!("inotify init failed: {e}"))?;

    unsafe {
        let fd = inotify.as_raw_fd();
        let flags = libc::fcntl(fd, libc::F_GETFL, 0);
        libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
    }

    let mut wd_map: HashMap<inotify::WatchDescriptor, PathBuf> = HashMap::new();
    let mask = WatchMask::ACCESS
        | WatchMask::OPEN
        | WatchMask::CLOSE_NOWRITE
        | WatchMask::MODIFY
        | WatchMask::CREATE
        | WatchMask::DELETE;

    for raw in paths {
        let p = Path::new(raw);
        if !p.exists() {
            continue;
        }

        add_watch(&mut inotify, &mut wd_map, p, mask);
        if p.is_dir() {
            if let Ok(entries) = std::fs::read_dir(p) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() {
                        add_watch(&mut inotify, &mut wd_map, &entry.path(), mask);
                    }
                }
            }
        }
    }

    Ok((inotify, wd_map))
}
