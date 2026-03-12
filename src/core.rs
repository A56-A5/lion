//! `sandbox_engine/core.rs`
//!
//! Handles building and executing the `bwrap` command core namespace parameters.

use std::process::Command;

/// Initializes the bubblewrap command with the fundamental namespace unshares.
pub fn build_bwrap(project_path: &str, network: bool, dry_run: bool) -> Command {
    let mut bwrap = Command::new("bwrap");

    // Core isolation flags
    bwrap.args([
        "--unshare-user",   // Isolate user/group IDs
        "--unshare-ipc",    // Isolate Inter-Process Communication
        "--unshare-pid",    // Isolate process tree
        "--unshare-uts",    // Isolate hostname
        "--unshare-cgroup", // Isolate cgroups
        "--tmpfs",
        "/tmp", // Fresh tmp system
        "--proc",
        "/proc", // Fresh procfs
        "--dev",
        "/dev", // Fresh dev system
        "--bind",
        project_path,
        project_path, // The project directory itself is always mapped RW
    ]);

    // Apply network restrictions
    if !network {
        bwrap.arg("--unshare-net");
        if !dry_run {
            println!("🌐 Network: disabled");
        }
    } else {
        println!("🌐 Network: enabled");
    }

    bwrap
}
