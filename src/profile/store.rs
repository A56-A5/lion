//! `profile/store.rs`
//!
//! Handles loading and saving the profile to `~/.config/lion/profile.json`.

use super::Profile;
use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

pub fn get_profile_path() -> Result<PathBuf> {
    let home = std::env::var("HOME").context("HOME env var not set")?;
    Ok(Path::new(&home).join(".config/lion/profile.json"))
}

pub fn load_profile() -> Result<Profile> {
    let path = get_profile_path()?;
    if !path.exists() {
        return Ok(Profile::default());
    }

    let content = fs::read_to_string(&path)
        .with_context(|| format!("Failed to read profile at {}", path.display()))?;
    
    let profile: Profile = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse profile at {}", path.display()))?;
    
    Ok(profile)
}

pub fn save_profile(profile: &Profile) -> Result<()> {
    let path = get_profile_path()?;
    
    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create directory {}", parent.display()))?;
    }

    let content = serde_json::to_string_pretty(profile)
        .context("Failed to serialize profile")?;
    
    fs::write(&path, content)
        .with_context(|| format!("Failed to write profile to {}", path.display()))?;
    
    Ok(())
}
