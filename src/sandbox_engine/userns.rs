use crate::errors::{LionError, Result};
use std::process::Command;

/// Probes whether bwrap can actually create a user namespace on this machine.
pub fn check_userns_available() -> Result<()> {
    let output = Command::new("bwrap")
        .args([
            "--unshare-user",
            "--ro-bind", "/", "/",
            "--dev", "/dev",
            "--proc", "/proc",
            "--",
            "true",
        ])
        .output();

    match output {
        Ok(o) if o.status.success() => Ok(()),
        _ => {
            let apparmor_blocking = std::fs::read_to_string(
                "/proc/sys/kernel/apparmor_restrict_unprivileged_userns",
            )
            .unwrap_or_default()
            .trim() == "1";

            if apparmor_blocking {
                Err(LionError::NamespaceError(
                    "AppArmor is blocking bwrap from creating user namespaces. \
                     Run 'sudo lion install' to fix this.".to_string()
                ))
            } else {
                Err(LionError::NamespaceError(
                    "bwrap cannot create user namespaces on this system. \
                     Check dmesg for details.".to_string()
                ))
            }
        }
    }
}
