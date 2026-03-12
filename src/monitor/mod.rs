mod log;
pub mod inotify;

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
}

impl MonitorHandle {
    /// Spawn monitor threads:
    ///  - stderr reader: captures bwrap blocked/error events
    ///  - inotify watcher: captures allowed reads on bind-mounted paths
    pub fn start(stderr: ChildStderr, watch_paths: Vec<String>) -> Self {
        let stop = Arc::new(AtomicBool::new(false));

        let stderr_thread = {
            let paths = watch_paths.clone();
            let stop_flag = Arc::clone(&stop);
            thread::spawn(move || {
                log::watch(stderr, paths);
                // stderr EOF means sandbox exited — signal inotify to stop
                stop_flag.store(true, Ordering::Relaxed);
            })
        };

        let inotify_thread = {
            let stop_flag = Arc::clone(&stop);
            thread::spawn(move || {
                self::inotify::watch(watch_paths, stop_flag);
            })
        };

        MonitorHandle {
            stop,
            stderr_thread: Some(stderr_thread),
            inotify_thread: Some(inotify_thread),
        }
    }
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
        eprintln!("\x1b[90m[LION] monitor stopped\x1b[0m");
    }
}
