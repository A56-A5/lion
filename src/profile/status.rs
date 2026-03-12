//! `commands/status.rs`
use anyhow::Result;
use super::store;
use crate::config::MODULES_JSON;
use serde_json::Value;
use colored::*;

pub fn handle_status() -> Result<()> {
    let profile = store::load_profile()?;
    let modules_cfg: Value = serde_json::from_str(MODULES_JSON)?;

    println!("{}", "L.I.O.N Exposure Status".bold().underline());
    println!("Profile: ~/.config/lion/profile.json\n");

    println!("{}", "Modules:".bold());
    if let Some(modules) = modules_cfg.as_object() {
        for (name, cfg) in modules {
            let is_mandatory = cfg.get("mandatory").and_then(|v| v.as_bool()).unwrap_or(false);
            let is_enabled = profile.modules.contains(name) || is_mandatory;

            if is_enabled {
                println!("  {} {:<10} (enabled)", "✔".green(), name);
            } else {
                println!("  {} {:<10} (blocked)", "❌".red(), name);
            }
        }
    }

    println!("\n{}", "Exposed Paths:".bold());
    if profile.custom_paths.is_empty() {
        println!("  (none)");
    } else {
        for path in &profile.custom_paths {
            println!("  {} {}", "✔".green(), path);
        }
    }

    println!("\n{}", "Allowed Domains:".bold());
    if profile.allowed_domains.is_empty() {
        println!("  (none - network blocked if module enabled)");
    } else {
        for domain in &profile.allowed_domains {
            println!("  {} {}", "✔".green(), domain);
        }
    }

    Ok(())
}
