mod log;

use std::process::ChildStderr;
use std::thread;

/// A handle to the background monitor thread.
/// Joining happens automatically on drop.
pub struct MonitorHandle {
    thread: Option<thread::JoinHandle<()>>,
}

impl MonitorHandle {
    /// Spawn the monitor thread, consuming the child's stderr pipe.
    pub fn start(stderr: ChildStderr, ro_paths: Vec<String>) -> Self {
        let thread = thread::spawn(move || {
            log::watch(stderr, ro_paths);
        });
        MonitorHandle {
            thread: Some(thread),
        }
    }
}

impl Drop for MonitorHandle {
    fn drop(&mut self) {
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
