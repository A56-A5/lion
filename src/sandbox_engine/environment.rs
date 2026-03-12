//! `sandbox_engine/environment.rs`
//!
//! Responsible for sanitizing and forwarding environment variables into the sandbox.
//! By default, almost all host environment variables are stripped for security.

use std::process::Command;

use crate::profile::resolver::ResolvedProfile;

/// Passes strictly safe environment variables into the sandbox.
pub fn apply_environment(bwrap: &mut Command, resolved: &ResolvedProfile) {
    // 1. Clear the environment inside the sandbox
    bwrap.arg("--clearenv");

    // 2. Set strictly safe environment variables
    bwrap.args(["--setenv", "PATH", "/usr/bin:/bin"]);
    bwrap.args(["--setenv", "HOME", "/home/lion"]); // Fixed home for sandbox
    bwrap.args(["--setenv", "USER", "lion"]);
    bwrap.args(["--setenv", "LOGNAME", "lion"]);
    bwrap.args(["--setenv", "LANG", "C.UTF-8"]);

    // 3. Apply resolved environment variables from modules
    for (key, val) in &resolved.env_vars {
        bwrap.arg("--setenv").arg(key).arg(val);
    }
}
