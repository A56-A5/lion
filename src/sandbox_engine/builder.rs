//! `sandbox_engine/builder.rs`
//!
//! Handles the initial creation of the `Command` object and sets up
//! the core namespace isolation flags.

/// Initializes the bubblewrap command with the fundamental namespace unshares.
///
/// This function sets up the "jail" by unsharing all standard Linux namespaces
/// and providing basic temporary filesystems like `/tmp`, `/proc`, and `/dev`.
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

    // Network isolation:
    // - Default (no --network flag): sandbox gets its own network namespace with
    //   no interfaces, so it literally cannot reach the internet or LAN.
    // - With --network flag: the sandbox shares the host network namespace, and
    //   we bind in the DNS/TLS config files it needs to resolve names.
    if !network {
        // --unshare-net gives the sandbox a fresh empty network namespace.
        // No interfaces = no outbound connections possible.
        bwrap.arg("--unshare-net");
        if !dry_run {
            println!("🌐 Network: disabled");
        }
    } else {
        // Bind in the minimum files needed for DNS resolution and HTTPS.
        // We use --ro-bind so the sandboxed process cannot modify them.
        if std::path::Path::new("/etc/resolv.conf").exists() {
            bwrap.args(["--ro-bind", "/etc/resolv.conf", "/etc/resolv.conf"]);
        }
        if std::path::Path::new("/etc/ssl").exists() {
            bwrap.args(["--ro-bind", "/etc/ssl", "/etc/ssl"]);
        }
        if std::path::Path::new("/etc/pki").exists() {
            bwrap.args(["--ro-bind", "/etc/pki", "/etc/pki"]);
        }
        if !dry_run {
            println!("🌐 Network: enabled");
        }
    }

    bwrap
}
