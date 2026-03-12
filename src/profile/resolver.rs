//! `profile/resolver.rs`
//!
//! Translates a Profile into concrete mount points and environment variables
//! based on the system module definitions.

use anyhow::{Result, Context};
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use crate::config::MODULES_JSON;
use super::Profile;

#[derive(Debug, Default)]
pub struct ResolvedProfile {
    pub ro_mounts: Vec<(String, String)>,
    pub rw_mounts: Vec<(String, String)>,
    pub dev_mounts: Vec<(String, String)>,
    pub env_vars: HashMap<String, String>,
    pub network_enabled: bool,
    pub gui_enabled: bool,
}

pub fn resolve_profile(profile: &Profile) -> Result<ResolvedProfile> {
    let mut resolved = ResolvedProfile::default();
    let modules_cfg: Value = serde_json::from_str(MODULES_JSON)
        .context("Failed to parse modules.json")?;
    
    let modules_obj = modules_cfg.as_object().context("modules.json must be an object")?;

    // 1. Collect all active module names (always include "base")
    let mut active_modules = profile.modules.clone();
    active_modules.insert("base".to_string());

    // 2. Process each active module
    for module_name in &active_modules {
        if let Some(cfg) = modules_obj.get(module_name) {
            apply_module_config(cfg, &mut resolved)?;
            
            // Set capability flags
            if module_name == "network" {
                resolved.network_enabled = true;
            }
            if module_name == "wayland" || module_name == "x11" {
                resolved.gui_enabled = true;
            }
        }
    }

    // 3. Process custom paths as RW mounts
    for path in &profile.custom_paths {
        resolved.rw_mounts.push((path.clone(), path.clone()));
    }

    Ok(resolved)
}

fn apply_module_config(cfg: &Value, resolved: &mut ResolvedProfile) -> Result<()> {
    // Process mounts
    if let Some(mounts) = cfg.get("mounts").and_then(|v| v.as_array()) {
        for m in mounts {
            let m_type = m.get("type").and_then(|v| v.as_str()).unwrap_or("ro-bind");
            let src = m.get("src").and_then(|v| v.as_str()).unwrap_or("");
            let dst = m.get("dst").and_then(|v| v.as_str()).unwrap_or("");
            
            if src.is_empty() || dst.is_empty() { continue; }

            // Resolve source path (handle ~ if present, though validator should have fixed it for custom paths)
            let src_path = resolve_user_path(src);
            if !src_path.exists() { continue; }
            let src_str = src_path.to_string_lossy().to_string();

            match m_type {
                "ro-bind" => resolved.ro_mounts.push((src_str, dst.to_string())),
                "bind" => resolved.rw_mounts.push((src_str, dst.to_string())),
                "dev-bind" => resolved.dev_mounts.push((src_str, dst.to_string())),
                _ => {}
            }
        }
    }

    // Process runtime sockets
    if let Some(sockets) = cfg.get("runtime_sockets").and_then(|v| v.as_array()) {
        if let Ok(runtime_dir) = std::env::var("XDG_RUNTIME_DIR") {
            let runtime_path = Path::new(&runtime_dir);
            for s in sockets {
                if let Some(socket_name) = s.as_str() {
                    let socket_path = runtime_path.join(socket_name);
                    if socket_path.exists() {
                        let path_str = socket_path.to_string_lossy().to_string();
                        resolved.ro_mounts.push((path_str.clone(), path_str));
                    }
                }
            }
        }
    }

    // Process environment variables (forward from host if present)
    if let Some(envs) = cfg.get("env").and_then(|v| v.as_array()) {
        for e in envs {
            if let Some(key) = e.as_str() {
                if let Ok(val) = std::env::var(key) {
                    resolved.env_vars.insert(key.to_string(), val);
                }
            }
        }
    }

    Ok(())
}

fn resolve_user_path(path: &str) -> PathBuf {
    if path.starts_with("~/") {
        if let Ok(home) = std::env::var("HOME") {
            return Path::new(&home).join(&path[2..]);
        }
    }
    PathBuf::from(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_base_only() {
        let profile = Profile::default();
        let resolved = resolve_profile(&profile).unwrap();
        
        // Base should at least have /usr
        assert!(resolved.ro_mounts.iter().any(|(src, _)| src == "/usr"));
    }

    #[test]
    fn test_resolve_with_custom_path() {
        let mut profile = Profile::default();
        profile.custom_paths.insert("/tmp".to_string()); // /tmp always exists
        let resolved = resolve_profile(&profile).unwrap();
        
        assert!(resolved.rw_mounts.iter().any(|(src, dst)| src == "/tmp" && dst == "/tmp"));
    }
}
