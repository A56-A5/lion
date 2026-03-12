//! `sandbox_engine/mounts.rs`
//!
//! Handles all filesystem mounting logic (bind-mounts) for the sandbox.
//! This is the most critical part of the sandbox's security and compatibility.

// use std::env;
use std::process::Command;

use crate::profile::resolver::ResolvedProfile;

/// Mounts the profile-defined directories and dynamic module mounts.
pub fn apply_profile_mounts(bwrap: &mut Command, resolved: &ResolvedProfile) {
    // 1. Process Resolved RO Mounts
    for (src, dst) in &resolved.ro_mounts {
        bwrap.args(["--ro-bind", src, dst]);
    }

    // 2. Process Resolved RW Mounts
    for (src, dst) in &resolved.rw_mounts {
        bwrap.args(["--bind", src, dst]);
    }

    // 3. Process Resolved DEV Mounts
    for (src, dst) in &resolved.dev_mounts {
        bwrap.args(["--dev-bind", src, dst]);
    }
}
