//! `sandbox_engine/environment.rs`
//!
//! Responsible for sanitizing and forwarding environment variables into the sandbox.
//! By default, almost all host environment variables are stripped for security.

use std::process::Command;

/// Passes strictly safe environment variables into the sandbox.
///
/// This prevents "environment leakage" where host secrets or aliases
/// might affect the sandboxed application's behavior.
pub fn apply_environment(bwrap: &mut Command, gui: bool) {
    // Strip ALL host env vars first — API keys, tokens, shell secrets, aliases — all gone.
    // We then allowlist only what the sandbox actually needs.
    bwrap.arg("--clearenv");

    // If GUI is allowed, pass display servers to allow windowing.
    if gui {
        if let Ok(display) = std::env::var("DISPLAY") {
            bwrap.arg("--setenv").arg("DISPLAY").arg(&display);
        }
        if let Ok(wayland_display) = std::env::var("WAYLAND_DISPLAY") {
            bwrap
                .arg("--setenv")
                .arg("WAYLAND_DISPLAY")
                .arg(&wayland_display);
        }
    }

    // Pass essential identity / localized env vars
    for var in [
        "HOME",
        "USER",
        "LOGNAME",
        "LANG",
        "LC_ALL",
        "PATH",
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
}
