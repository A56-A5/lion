//! `sandbox_engine/builder.rs`
//!
//! Handles the initial creation of the `Command` object and sets up
//! the core namespace isolation flags.

use std::process::Command;

use crate::profile::resolver::ResolvedProfile;

/// Initializes the bubblewrap command with the fundamental namespace unshares.
pub fn build_bwrap(
    project_path: &str,
    resolved: &ResolvedProfile,
    dry_run: bool,
) -> Command {
    let mut bwrap = Command::new("bwrap");

    // Core isolation flags
    bwrap.args([
        "--unshare-user",   // Isolate user/group IDs
        "--unshare-ipc",    // Isolate Inter-Process Communication
        "--unshare-pid",    // Isolate process tree
        "--unshare-uts",    // Isolate hostname
        "--unshare-cgroup", // Isolate cgroups
        "--die-with-parent", // Kill sandbox if parent dies
        "--hostname",
        "lion", // Set fake hostname
        "--new-session", // Detach from terminal session
        "--tmpfs",
        "/", // Start with a fresh, empty root
        "--dir",
        "/usr", // Stub for system binaries/libraries
        "--dir",
        "/bin", // Stub for common binaries
        "--dir",
        "/lib", // Stub for core libraries
        "--dir",
        "/tmp", // Fresh tmp system
        "--dir",
        "/run", // Stub for runtime files
        "--proc",
        "/proc", // Fresh procfs
        "--dev",
        "/dev", // Fresh dev system
        "--bind",
        project_path,
        project_path, // The project directory itself is always mapped RW
    ]);

    use tracing::info;

    if !resolved.network_enabled {
        // --unshare-net gives the sandbox a fresh empty network namespace.
        // No interfaces = no outbound connections possible.
        bwrap.arg("--unshare-net");
        if !dry_run {
            info!("Network: disabled");
        }
    } else {
        if !dry_run {
            info!("Network: enabled");
        }
    }

    bwrap
}
