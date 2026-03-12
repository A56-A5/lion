//! `profile/validator.rs`
//!
//! Security checks for host paths before they are added to the profile.

use anyhow::{bail, Result};
use std::path::Path;

/// Deny list of top-level directories that should never be writable inside a sandbox.
const DENY_LIST: &[&str] = &[
    "/",
    "/home",
    "/etc",
    "/root",
    "/var",
    "/proc",
    "/dev",
    "/sys",
    "/bin",
    "/sbin",
    "/lib",
    "/lib64",
    "/usr",
    "/boot",
];

pub fn validate_custom_path(path_str: &str) -> Result<()> {
    let path = Path::new(path_str);

    // 1. Must be an absolute path
    if !path.is_absolute() {
        bail!("Path must be absolute: {}", path_str);
    }

    // 2. Must exist on disk
    if !path.exists() {
        bail!("Path does not exist: {}", path_str);
    }

    // 3. Must not be in the deny list
    let canon = path.canonicalize()?;
    let canon_str = canon.to_string_lossy();

    for denied in DENY_LIST {
        if canon_str == *denied {
            bail!("Access to {} is restricted for security reasons", denied);
        }
    }

    // Check if it's a direct child of /home (e.g. /home/user is OK, but /home is NOT)
    if canon_str == "/home" {
         bail!("Exposing /home is too broad. Expose a specific subdirectory instead.");
    }

    Ok(())
}
