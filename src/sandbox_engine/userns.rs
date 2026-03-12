use anyhow::{bail, Result};
use std::process::Command;

/// Probes whether bwrap can actually create a user namespace on this machine.
///
/// Returns `Ok(())` if it works, or an `Err` with an actionable message if
/// AppArmor (or another policy) is blocking `--unshare-user`.
pub fn check_userns_available() -> Result<()> {
    // bwrap requires at least a root bind + /dev + /proc to execute successfully.
    // Without --ro-bind / / the command always fails regardless of userns policy.
    let output = Command::new("bwrap")
        .args([
            "--unshare-user",
            "--ro-bind",
            "/",
            "/",
            "--dev",
            "/dev",
            "--proc",
            "/proc",
            "--",
            "true",
        ])
        .output();

    match output {
        Ok(o) if o.status.success() => Ok(()),
        _ => {
            // Check whether the AppArmor restriction knob is the cause.
            let apparmor_blocking = std::fs::read_to_string(
                "/proc/sys/kernel/apparmor_restrict_unprivileged_userns",
            )
            .unwrap_or_default()
            .trim()
                == "1";

            if apparmor_blocking {
                bail!(
                    "[lion] AppArmor is blocking bwrap from creating user namespaces.\n\
                     This is a one-time setup issue — run:\n\
                     \n\
                     \x1b[1m    sudo lion install\x1b[0m\n\
                     \n\
                     This creates a targeted AppArmor rule for bwrap only.\n\
                     It does NOT disable AppArmor globally."
                );
            } else {
                bail!(
                    "[lion] bwrap cannot create user namespaces on this system.\n\
                     Check: dmesg | grep -i 'userns\\|bwrap'"
                );
            }
        }
    }
}
