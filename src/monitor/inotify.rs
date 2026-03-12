use inotify::{EventMask, Inotify, WatchMask};
use std::collections::HashMap;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Spawn an inotify watcher on `paths` in the current thread (call from a background thread).
///
/// Watches until `stop` is set to true (sandbox process exited).
/// Because bwrap bind-mounts host directories, inotify watchers on the host fire
/// for every access that happens inside the sandbox as well.
pub fn watch(paths: Vec<String>, stop: Arc<AtomicBool>) {
    let mut inotify = match Inotify::init() {
        Ok(i) => i,
        Err(e) => {
            eprintln!("\x1b[90m[LION] inotify init failed: {}\x1b[0m", e);
            return;
        }
    };

    // Set O_NONBLOCK so read_events returns immediately when there are no events
    unsafe {
        let fd = inotify.as_raw_fd();
        let flags = libc::fcntl(fd, libc::F_GETFL, 0);
        libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
    }

    // wd -> canonical path mapping so we can print useful names
    let mut wd_map: HashMap<inotify::WatchDescriptor, PathBuf> = HashMap::new();

    let mask = WatchMask::ACCESS
        | WatchMask::OPEN
        | WatchMask::CLOSE_NOWRITE
        | WatchMask::MODIFY
        | WatchMask::CREATE
        | WatchMask::DELETE;

    for raw in &paths {
        let p = Path::new(raw);
        if !p.exists() {
            continue;
        }
        // Watch the path itself
        add_watch(&mut inotify, &mut wd_map, p, mask);

        // One level of subdirectory recursion — enough for project dirs
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
            let (tag, color, icon) = classify_event(event.mask);

            // Skip noisy internal events (e.g. OPEN on directory itself)
            if event.mask.contains(EventMask::ISDIR) && event.mask.contains(EventMask::OPEN) {
                continue;
            }

            let now = chrono::Local::now().format("%H:%M:%S");
            eprintln!(
                "[LION] {}  {}{}  {}{}\x1b[0m  \x1b[36m{}\x1b[0m",
                now, color, tag, icon, "\x1b[0m", path_str
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

fn classify_event(mask: EventMask) -> (&'static str, &'static str, &'static str) {
    if mask.contains(EventMask::ACCESS) || mask.contains(EventMask::OPEN) || mask.contains(EventMask::CLOSE_NOWRITE) {
        ("READ   ", "\x1b[1m\x1b[32m", "✅ ") // green — allowed read
    } else if mask.contains(EventMask::MODIFY) {
        ("WRITE  ", "\x1b[1m\x1b[33m", "✏️  ") // yellow — write
    } else if mask.contains(EventMask::CREATE) {
        ("CREATE ", "\x1b[1m\x1b[34m", "📄 ") // blue — create
    } else if mask.contains(EventMask::DELETE) {
        ("DELETE ", "\x1b[1m\x1b[31m", "🗑️  ") // red — delete
    } else {
        ("EVENT  ", "\x1b[90m", "")
    }
}
