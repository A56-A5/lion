//! `sandbox_engine/env.rs`
//!
//! Applies strictly isolated runtime shell variables into the `bwrap` execution.

use std::process::Command;

/// Passes strictly safe environment variables into the sandbox.
pub fn apply_environment(bwrap: &mut Command, gui: bool) {
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
    ] {
        if let Ok(val) = std::env::var(var) {
            bwrap.arg("--setenv").arg(var).arg(val);
        }
    }
}
