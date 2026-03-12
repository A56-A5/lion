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
    // 1. Clear the environment inside the sandbox
    bwrap.arg("--clearenv");

    // 2. Set strictly safe environment variables
    bwrap.args(["--setenv", "PATH", "/usr/bin:/bin"]);
    bwrap.args(["--setenv", "HOME", "/home/lion"]); // Fixed home for sandbox
    bwrap.args(["--setenv", "USER", "lion"]);
    bwrap.args(["--setenv", "LOGNAME", "lion"]);
    bwrap.args(["--setenv", "LANG", "C.UTF-8"]);

    // If GUI is allowed, pass display servers to allow windowing.
    // Note: These are now only added AFTER env-clear.
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
}
