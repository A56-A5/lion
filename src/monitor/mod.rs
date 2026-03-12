mod log;
pub mod inotify;
pub mod perf;

use std::io::BufRead;
use std::process::ChildStderr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

/// A handle to the background monitor threads (stderr + inotify).
/// Both threads are signalled and joined on drop.
pub struct MonitorHandle {
    stop: Arc<AtomicBool>,
    stderr_thread: Option<thread::JoinHandle<()>>,
    inotify_thread: Option<thread::JoinHandle<()>>,
    terminal_child: Option<std::process::Child>,
}

impl MonitorHandle {
    /// Spawn monitor threads:
    ///  - stderr reader: captures bwrap blocked/error events
    ///  - inotify watcher: captures allowed reads on bind-mounted paths
    pub fn start(stderr: ChildStderr, watch_paths: Vec<String>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));

        // 1. Attempt to launch in a separate terminal
        let (fifo_path, terminal_child) = if let Some((path, child)) = launch_terminal_monitor(&watch_paths) {
            (Some(path), Some(child))
        } else {
            (None, None)
        };

        let stop_flag = Arc::clone(&stop);
        let paths = watch_paths.clone();
        let is_separate_terminal = fifo_path.is_some();

        let stderr_thread = thread::spawn(move || {
            if let Some(path) = fifo_path {
                use std::io::Write;
                // Pipe stderr to the FIFO
                if let Ok(mut fifo) = std::fs::OpenOptions::new().write(true).open(&path) {
                    let mut reader = std::io::BufReader::new(stderr);
                    let mut line = String::new();
                    while !stop_flag.load(Ordering::Relaxed) {
                        line.clear();
                        if let Ok(n) = reader.read_line(&mut line) {
                            if n == 0 { break; }
                            let _ = fifo.write_all(line.as_bytes());
                            let _ = fifo.flush();
                        } else {
                            break;
                        }
                    }
                }
                // Cleanup FIFO
                let _ = std::fs::remove_file(path);
            } else {
                // Fallback: internal watcher
                log::watch(stderr, paths);
            }
            // Signal inotify to stop
            stop_flag.store(true, Ordering::Relaxed);
        });

        let inotify_thread = if !is_separate_terminal {
            // Only spawn inotify locally if NOT using separate terminal
            let sf = Arc::clone(&stop);
            let wp = watch_paths.clone();
            Some(thread::spawn(move || {
                self::inotify::watch(wp, sf);
            }))
        } else {
            None
        };

        MonitorHandle {
            stop,
            stderr_thread: Some(stderr_thread),
            inotify_thread,
            terminal_child,
        }
    }
}

pub fn run_monitor_subcommand(fifo_path: String, watch_paths: Vec<String>) -> anyhow::Result<()> {
    use std::fs::File;
    use std::io::BufReader;

    let stop = Arc::new(AtomicBool::new(false));
    let stop_flag = Arc::clone(&stop);
    let wp = watch_paths.clone();

    // Spawn inotify watcher in the monitor process
    let inotify_thread = thread::spawn(move || {
        self::inotify::watch(wp, stop_flag);
    });

    let file = File::open(&fifo_path)?;
    let reader = BufReader::new(file);

    log::watch_buffered(reader, watch_paths);

    // Stop inotify when FIFO is closed
    stop.store(true, Ordering::Relaxed);
    let _ = inotify_thread.join();

    Ok(())
}

fn launch_terminal_monitor(watch_paths: &[String]) -> Option<(String, std::process::Child)> {
    use std::process::Command;
    
    // 1. Create a unique FIFO path
    let pid = std::process::id();
    let fifo_path = format!("/tmp/lion-monitor-{}", pid);
    
    // Delete if exists (stale)
    let _ = std::fs::remove_file(&fifo_path);
    
    // Create FIFO using mkfifo
    if !Command::new("mkfifo")
        .arg(&fifo_path)
        .status()
        .map(|s| s.success())
        .unwrap_or(false) 
    {
        return None;
    }

    // 2. Build the command to run in the new terminal
    let exe = std::env::current_exe().unwrap_or_else(|_| "lion".into());
    let mut lion_cmd = vec![
        exe.to_string_lossy().to_string(),
        "monitor".to_string(),
        fifo_path.clone(),
    ];
    
    for path in watch_paths {
        lion_cmd.push("--watch-paths".to_string());
        lion_cmd.push(path.clone());
    }

    // 3. Try available terminals
    let terminals = [
        ("gnome-terminal", vec!["--", "bash", "-c"]),
        ("kitty", vec!["bash", "-c"]),
    ];

    for (term, args) in terminals {
        let mut cmd = Command::new(term);
        for arg in args {
            cmd.arg(arg);
        }
        
        let lion_cmd_str = lion_cmd.join(" ");
        cmd.arg(&lion_cmd_str);

        if let Ok(child) = cmd.spawn() {
            return Some((fifo_path, child));
        }
    }

    // Fallback: cleanup FIFO and return None
    let _ = std::fs::remove_file(&fifo_path);
    None
}

impl Drop for MonitorHandle {
    fn drop(&mut self) {
        // Signal both threads to stop in case the process was killed externally
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.stderr_thread.take() {
            let _ = t.join();
        }
        if let Some(t) = self.inotify_thread.take() {
            let _ = t.join();
        }
        if let Some(mut child) = self.terminal_child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        eprintln!("\x1b[90m[LION] monitor stopped\x1b[0m");
    }
}
