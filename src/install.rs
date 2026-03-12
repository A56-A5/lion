//! `install.rs`
//!
//! One-time system setup for L.I.O.N.
//!
//! Creates a targeted AppArmor profile that allows `bwrap` to create user
//! namespaces, without disabling AppArmor globally.  This is the same
//! approach used by Flatpak and Podman packaging.
//!
//! Must be run with root privileges once per machine:
//!   sudo lion install

use crate::errors::{LionError, Result};
use std::path::Path;
use std::process::Command;
use tracing::{error, info};

/// Profile written to `/etc/apparmor.d/`.
///
/// `flags=(unconfined)` keeps bwrap's own behaviour unrestricted while
/// granting only the `userns` capability — the single permission that
/// `apparmor_restrict_unprivileged_userns=1` blocks.
const APPARMOR_PROFILE: &str = r#"abi <abi/4.0>,
include <tunables/global>

profile bwrap /usr/bin/bwrap flags=(unconfined) {
  userns,
}
"#;

const PROFILE_PATH: &str = "/etc/apparmor.d/bwrap-userns-restrict";

/// Entry point for `lion install`.
pub fn run_install() -> Result<()> {
    // 1. Must run as root — we write to /etc/apparmor.d/
    if !is_root() {
        return Err(LionError::Unauthorized(
            "This command requires root to write the AppArmor profile.".to_string(),
        ));
    }

    // 2. Check AppArmor is present on this machine
    if !is_apparmor_active() {
        info!("AppArmor does not appear to be active on this system. Nothing to do.");
        return Ok(());
    }

    // 3. Check whether the restriction is actually on
    let restrict_path = "/proc/sys/kernel/apparmor_restrict_unprivileged_userns";
    if Path::new(restrict_path).exists() {
        let val = std::fs::read_to_string(restrict_path).unwrap_or_default();
        if val.trim() == "0" {
            info!("User namespaces are already unrestricted. No profile needed.");
            return Ok(());
        }
    }

    // 4. Write the profile
    std::fs::write(PROFILE_PATH, APPARMOR_PROFILE).map_err(|e| {
        LionError::Internal(format!("Failed to write AppArmor profile to {}: {}", PROFILE_PATH, e))
    })?;

    info!("Wrote AppArmor profile to {}", PROFILE_PATH);

    // 5. Load the profile into the running kernel
    let status = Command::new("apparmor_parser")
        .args(["-r", PROFILE_PATH])
        .status();

    match status {
        Ok(s) if s.success() => {
            info!("AppArmor profile loaded successfully. Setup complete.");
            Ok(())
        }
        Ok(s) => {
            let code = s.code().unwrap_or(1);
            error!("apparmor_parser exited with code {}.", code);
            Err(LionError::ExecutionError(code))
        }
        Err(e) => {
            error!("Could not run apparmor_parser: {}. Ensure apparmor-utils is installed.", e);
            Err(LionError::DependencyMissing("apparmor-utils".to_string()))
        }
    }
}

/// Returns true if the effective user is root.
fn is_root() -> bool {
    // SAFETY: getuid() / geteuid() are always safe to call.
    unsafe { libc::geteuid() == 0 }
}

/// Returns true if AppArmor appears active (the securityfs entry exists).
fn is_apparmor_active() -> bool {
    Path::new("/sys/kernel/security/apparmor").exists()
        || Path::new("/proc/sys/kernel/apparmor_restrict_unprivileged_userns").exists()
}
