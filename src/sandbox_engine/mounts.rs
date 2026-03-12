//! `sandbox_engine/mounts.rs`
//!
//! Handles all filesystem mounting logic (bind-mounts) for the sandbox.
//! This is the most critical part of the sandbox's security and compatibility.

use std::process::Command;

/// Mounts static hardcoded system directories needed to run typical binaries.
///
/// This ensures that the sandboxed process has access to shared libraries (`/lib`),
/// standard tools (`/bin`), and core system configuration (`/etc`).
pub fn apply_system_mounts(bwrap: &mut Command) {
    let standard_paths = [
        "/usr",
        "/bin",
        "/lib",
        "/lib64",
        "/etc/alternatives",
        "/snap",
        "/opt",
    ];

    for path in standard_paths {
        if std::path::Path::new(path).exists() {
            bwrap.args(["--ro-bind", path, path]);
        }
    }

    // Ensure $HOME exists inside the sandbox as an empty directory in the tmpfs.
    // Without this, tools like npm, pip, and cargo cannot access ~/.npm, ~/.cache,
    // ~/.cargo etc. and fail immediately — even though HOME is set in the environment.
    // We do NOT bind-mount the real home dir (no host files leak in); the directory
    // is created empty in the synthetic root and any writes are discarded on exit.
    if let Ok(home) = std::env::var("HOME") {
        bwrap.arg("--dir").arg(&home);
    }

    // Snap runtime support: when node/python/etc. are installed as snap packages,
    // their launcher binary (/snap/bin/X → /usr/bin/snap) connects to the host
    // snapd daemon via Unix sockets.  Without these bind-mounts, snap hangs
    // indefinitely waiting for a socket that doesn't exist in the sandbox.
    for path in &["/run/snapd.socket", "/run/snapd-snap.socket"] {
        if std::path::Path::new(path).exists() {
            bwrap.args(["--bind", path, path]);
        }
    }
    // Snap namespace directory (used by snapd for snap confinement bookkeeping)
    if std::path::Path::new("/run/snapd/ns").exists() {
        bwrap.args(["--bind", "/run/snapd/ns", "/run/snapd/ns"]);
    }
    // Per-user snapd session agent socket (path derived from XDG_RUNTIME_DIR
    // which is already forwarded into the sandbox via apply_environment)
    if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
        let agent_sock = format!("{}/snapd-session-agent.socket", runtime_dir);
        if std::path::Path::new(&agent_sock).exists() {
            bwrap.arg("--dir").arg(&runtime_dir);
            bwrap.args(["--bind", &agent_sock, &agent_sock]);
        }
    }
}
