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

use anyhow::{bail, Result};
use std::path::Path;
use std::process::Command;

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
        eprintln!(
            "[lion] This command requires root to write the AppArmor profile.\n\
             Run it as:\n\
             \n\
             \x1b[1m    sudo lion install\x1b[0m\n"
        );
        bail!("exit code: 1");
    }

    // 2. Check AppArmor is present on this machine
    if !is_apparmor_active() {
        println!(
            "[lion] AppArmor does not appear to be active on this system.\n\
             Nothing to do — `bwrap --unshare-user` should already work.\n\
             If it still fails, check `dmesg | grep -i userns`."
        );
        return Ok(());
    }

    // 3. Check whether the restriction is actually on
    let restrict_path = "/proc/sys/kernel/apparmor_restrict_unprivileged_userns";
    if Path::new(restrict_path).exists() {
        let val = std::fs::read_to_string(restrict_path).unwrap_or_default();
        if val.trim() == "0" {
            println!(
                "[lion] `apparmor_restrict_unprivileged_userns` is already 0 — \
                 user namespaces are unrestricted.\n\
                 No profile installation needed."
            );
            return Ok(());
        }
    }

    // 4. Write the profile
    std::fs::write(PROFILE_PATH, APPARMOR_PROFILE).map_err(|e| {
        anyhow::anyhow!(
            "Failed to write AppArmor profile to {}: {}\n\
             Make sure you are running as root.",
            PROFILE_PATH,
            e
        )
    })?;

    println!("[lion] Wrote AppArmor profile → {}", PROFILE_PATH);

    // 5. Load the profile into the running kernel
    let status = Command::new("apparmor_parser")
        .args(["-r", PROFILE_PATH])
        .status();

    match status {
        Ok(s) if s.success() => {
            println!(
                "[lion] Profile loaded successfully.\n\
                 \n\
                 \x1b[1m[lion] Setup complete.\x1b[0m \
                 You can now run `lion run` from any terminal without root."
            );
            Ok(())
        }
        Ok(s) => {
            let code = s.code().unwrap_or(1);
            eprintln!(
                "[lion] apparmor_parser exited with code {}.\n\
                 The profile was written to {} but may not be active.\n\
                 Try: sudo apparmor_parser -r {}",
                code, PROFILE_PATH, PROFILE_PATH
            );
            bail!("exit code: {}", code);
        }
        Err(e) => {
            eprintln!(
                "[lion] Could not run apparmor_parser: {}\n\
                 Ensure `apparmor-utils` is installed: sudo apt install apparmor-utils",
                e
            );
            bail!("exit code: 1");
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
