//! `sandbox_engine/builder.rs`
//!
//! Handles the initial creation of the `Command` object and sets up
//! the core namespace isolation flags.

use std::process::Command;

/// Initializes the bubblewrap command with the fundamental namespace unshares.
///
/// This function sets up the "jail" by unsharing all standard Linux namespaces
/// and providing basic temporary filesystems like `/tmp`, `/proc`, and `/dev`.
pub fn build_bwrap(
    project_path: &str,
    network_mode: crate::sandbox_engine::network::NetworkMode,
    dry_run: bool,
    project_ro: bool,
) -> Command {
    let mut bwrap = Command::new("bwrap");

    // Core isolation flags
    bwrap.args([
        "--unshare-user",   // Isolate user/group IDs
        "--unshare-ipc",    // Isolate Inter-Process Communication
        "--unshare-pid",    // Isolate process tree
        "--unshare-uts",    // Isolate hostname
        "--unshare-cgroup", // Isolate cgroups
        "--die-with-parent", // Kill sandbox if lion process dies — no orphans
        "--new-session",     // Detach from host terminal session
        "--hostname", "lion", // Fake hostname inside sandbox
        "--tmpfs", "/",      // Synthetic root — nothing leaks in
        "--tmpfs", "/tmp",   // Fresh tmp
        "--proc", "/proc",   // Fresh procfs
        "--dev", "/dev",     // Fresh dev
        "--dir", "/usr",
        "--dir", "/bin",
        "--dir", "/lib",
        "--dir", "/lib64",
        "--dir", "/etc",
        "--dir", "/run",
    ]);

    let mount_flag = if project_ro { "--ro-bind" } else { "--bind" };
    bwrap.arg(mount_flag).arg(project_path).arg(project_path);

    use crate::sandbox_engine::network::NetworkMode;
    use tracing::info;

    match network_mode {
        NetworkMode::None => {
            // --unshare-net gives the sandbox a fresh empty network namespace.
            // No interfaces = no outbound connections possible.
            bwrap.arg("--unshare-net");
            if !dry_run {
                info!("Network: disabled");
            }
        }
        NetworkMode::Full => {
            // Share the host network namespace.
            apply_full_network_mounts(&mut bwrap);
            if !dry_run {
                info!("Network: full access");
            }
        }
        NetworkMode::Dns => {
            // For now, DNS mode shares the network but we should ideally restrict it.
            // Minimal implementation: Share net but ONLY mount resolv.conf.
            if std::path::Path::new("/etc/resolv.conf").exists() {
                bwrap.args(["--ro-bind", "/etc/resolv.conf", "/etc/resolv.conf"]);
            }
            if !dry_run {
                info!("Network: DNS only (restricted)");
            }
        }
        NetworkMode::Http => {
            // Placeholder: In a full implementation, this would trigger the proxy.
            // For now, we allow full network but print a warning about the proxy.
            apply_full_network_mounts(&mut bwrap);
            if !dry_run {
                info!("Network: HTTP/HTTPS (proxy not yet implemented, allowing full)");
            }
        }
    }

    bwrap
}

/// Helper to mount basic networking files (resolv.conf, SSL certs).
fn apply_full_network_mounts(bwrap: &mut Command) {
    if std::path::Path::new("/etc/resolv.conf").exists() {
        bwrap.args(["--ro-bind", "/etc/resolv.conf", "/etc/resolv.conf"]);
    }
    if std::path::Path::new("/etc/ssl").exists() {
        bwrap.args(["--ro-bind", "/etc/ssl", "/etc/ssl"]);
    }
    if std::path::Path::new("/etc/pki").exists() {
        bwrap.args(["--ro-bind", "/etc/pki", "/etc/pki"]);
    }
}
