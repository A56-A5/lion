mod log;
pub mod inotify;

use std::process::ChildStderr;
use std::thread;

/// A handle to the background monitor threads (stderr + inotify).
/// Both threads are joined on drop.
pub struct MonitorHandle {
    stderr_thread: Option<thread::JoinHandle<()>>,
    inotify_thread: Option<thread::JoinHandle<()>>,
}

impl MonitorHandle {
    /// Spawn monitor threads:
    ///  - stderr reader: captures bwrap blocked/error events
    ///  - inotify watcher: captures allowed reads on bind-mounted paths
    pub fn start(stderr: ChildStderr, watch_paths: Vec<String>) -> Self {
        let stderr_thread = {
            let paths = watch_paths.clone();
            thread::spawn(move || {
                log::watch(stderr, paths);
            })
        };

        let inotify_thread = {
            thread::spawn(move || {
                self::inotify::watch(watch_paths);
            })
        };

        MonitorHandle {
            stderr_thread: Some(stderr_thread),
            inotify_thread: Some(inotify_thread),
        }
    }
}

impl Drop for MonitorHandle {
    fn drop(&mut self) {
        if let Some(t) = self.stderr_thread.take() {
            let _ = t.join();
        }
        if let Some(t) = self.inotify_thread.take() {
            let _ = t.join();
        }
    }
}
