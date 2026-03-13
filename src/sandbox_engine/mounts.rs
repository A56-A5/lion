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

    // Create $HOME as an empty directory inside the sandbox tmpfs.
    // This is intentional and security-critical: we do NOT bind-mount the real
    // home directory — that would expose SSH keys, dotfiles, credentials, etc.
    // Instead, any specific home paths the user actually needs (e.g. ~/.npmrc)
    // must be listed as [[mount]] entries in lion.toml.  Those entries are
    // applied later in runner.rs and will create bind-mounts inside this empty dir.
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
