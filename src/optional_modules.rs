//! `optional_modules.rs`
//!
//! Manages project-local optional modules stored in `optionalmodules.toml`.
//! Modules can be added, removed, toggled, and listed.

use serde::{Deserialize, Serialize};
use std::path::Path;
use std::fs;
use anyhow::{Context, Result};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OptionalModule {
    pub name: String,
    pub path: String,
    pub state: i32, // 1 = enabled, 0 = blocked
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct OptionalModulesConfig {
    #[serde(default)]
    pub modules: Vec<OptionalModule>,
}

impl OptionalModulesConfig {
    pub fn load(project_dir: &Path) -> Result<Self> {
        let path = project_dir.join("optionalmodules.toml");
        if !path.exists() {
            return Ok(Self::default());
        }
        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let config: Self = toml::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        Ok(config)
    }

    pub fn save(&self, project_dir: &Path) -> Result<()> {
        let path = project_dir.join("optionalmodules.toml");
        let content = toml::to_string_pretty(self)
            .context("failed to serialize optional modules")?;
        fs::write(&path, content)
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }
}

pub fn list(project_dir: &Path) -> Result<()> {
    let config = OptionalModulesConfig::load(project_dir)?;
    if config.modules.is_empty() {
        println!("No optional modules configured.");
        return Ok(());
    }

    println!("🦁 Optional Modules in {}:", project_dir.display());
    for m in &config.modules {
        let state_str = if m.state == 1 {
            "\x1b[1;32menabled\x1b[0m"
        } else {
            "\x1b[1;31mblocked\x1b[0m"
        };
        println!(" - {}: {} ({})", m.name, m.path, state_str);
    }
    Ok(())
}

pub fn add(project_dir: &Path, name: String, path: String, state: i32) -> Result<()> {
    let mut config = OptionalModulesConfig::load(project_dir)?;
    if config.modules.iter().any(|m| m.name == name) {
        anyhow::bail!("Module with name '{}' already exists", name);
    }
    
    config.modules.push(OptionalModule { name: name.clone(), path, state });
    config.save(project_dir)?;
    println!("✅ Added optional module: {}", name);
    Ok(())
}

pub fn remove(project_dir: &Path, name: &str) -> Result<()> {
    let mut config = OptionalModulesConfig::load(project_dir)?;
    let len_before = config.modules.len();
    config.modules.retain(|m| m.name != name);
    
    if config.modules.len() == len_before {
        anyhow::bail!("Module '{}' not found", name);
    }
    
    config.save(project_dir)?;
    println!("✅ Removed optional module: {}", name);
    Ok(())
}

pub fn toggle(project_dir: &Path, name: &str) -> Result<()> {
    let mut config = OptionalModulesConfig::load(project_dir)?;
    let mut found = false;
    for m in &mut config.modules {
        if m.name == name {
            m.state = if m.state == 1 { 0 } else { 1 };
            let state_str = if m.state == 1 { "enabled" } else { "blocked" };
            println!("✅ Toggled module '{}' to: {}", name, state_str);
            found = true;
            break;
        }
    }
    
    if !found {
        anyhow::bail!("Module '{}' not found", name);
    }
    
    config.save(project_dir)?;
    Ok(())
}
