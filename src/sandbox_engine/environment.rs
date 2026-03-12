//! `sandbox_engine/environment.rs`
//!
//! Responsible for sanitizing and forwarding environment variables into the sandbox.
//! By default, almost all host environment variables are stripped for security.

use std::process::Command;

/// Passes strictly safe environment variables into the sandbox.
///
/// This prevents "environment leakage" where host secrets or aliases
/// might affect the sandboxed application's behavior.
pub fn apply_environment(bwrap: &mut Command) {
    // Strip ALL host env vars first — API keys, tokens, shell secrets, aliases — all gone.
    // We then allowlist only what the sandbox actually needs.
    bwrap.arg("--clearenv");

    // Pass essential identity / localized env vars
    for var in [
        "HOME",
        "USER",
        "LOGNAME",
        "LANG",
        "LC_ALL",
        "XDG_RUNTIME_DIR",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
        "XAUTHORITY",
    ] {
        if let Ok(val) = std::env::var(var) {
            bwrap.arg("--setenv").arg(var).arg(val);
        }
    }

    // Build PATH, prepending real snap package bin dirs before the standard PATH.
    //
    // When a tool is installed as a snap (e.g. node, python3), the entry in PATH
    // is a shim: /snap/bin/node → /usr/bin/snap.  That shim requires the snapd
    // daemon and snap-confine (a setuid binary) — neither works inside a user
    // namespace sandbox.  By prepending /snap/PKGNAME/current/bin we ensure that
    // `node`, `npm`, etc. resolve to the real ELF binaries already present on the
    // read-only /snap mount, completely bypassing snap confinement.
    let host_path = std::env::var("PATH").unwrap_or_else(|_| "/usr/local/bin:/usr/bin:/bin".to_string());
    let snap_prepend = snap_real_bin_dirs(&host_path);
    let final_path = if snap_prepend.is_empty() {
        host_path
    } else {
        format!("{}:{}", snap_prepend.join(":"), host_path)
    };
    bwrap.arg("--setenv").arg("PATH").arg(&final_path);
}

/// For every snap shim found in `path_var`, resolve the real binary directory
/// inside the snap package and return the list of dirs to prepend to PATH.
///
/// Example:  /snap/bin/node → /usr/bin/snap  →  /snap/node/current/bin
///           /snap/bin/npm  → node.npm        →  /snap/node/current/bin  (deduped)
fn snap_real_bin_dirs(path_var: &str) -> Vec<String> {
    let mut dirs: Vec<String> = Vec::new();

    for dir in path_var.split(':') {
        // We only care about snap-managed bin dirs
        if dir != "/snap/bin" && dir != "/var/lib/snapd/snap/bin" {
            continue;
        }
        let snap_bin_dir = std::path::Path::new(dir);
        let Ok(entries) = std::fs::read_dir(snap_bin_dir) else { continue };

        for entry in entries.flatten() {
            let link_target = match std::fs::read_link(entry.path()) {
                Ok(t) => t,
                Err(_) => continue,
            };
            let target_str = link_target.to_string_lossy();

            // Determine the snap package name from the symlink target:
            //   "/usr/bin/snap"  → snap name == filename of the shim  (e.g. "node")
            //   "node.npm"       → snap name == part before "."        (e.g. "node")
            let snap_name: String = if target_str.contains('/') {
                // Points to an absolute path (/usr/bin/snap) — snap name = entry name
                entry.file_name().to_string_lossy().to_string()
            } else {
                // Relative target like "node.npm" — snap name = prefix before "."
                target_str
                    .split('.')
                    .next()
                    .unwrap_or(&target_str)
                    .to_string()
            };

            if snap_name.is_empty() {
                continue;
            }

            let real_bin = format!("/snap/{}/current/bin", snap_name);
            if std::path::Path::new(&real_bin).exists() && !dirs.contains(&real_bin) {
                dirs.push(real_bin);
            }
        }
    }

    dirs
}
