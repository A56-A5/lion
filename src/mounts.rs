//! `sandbox_engine/mounts.rs`
//!
//! Applies volume logic to the isolated `bwrap` container sandbox environment.

use crate::config::SystemConfig;
use std::process::Command;

/// Mounts all verified system directories from `system.toml` mapping file.
pub fn apply_system_mounts(
    bwrap: &mut Command,
    system_config: &SystemConfig,
    network: bool,
    gui: bool,
    optional: &[String],
) {
    for mount in &system_config.mounts {
        // Skip mounts the user hasn't toggled via CLI flags
        if mount.when == "network" && !network {
            continue;
        }
        if mount.when == "gui" && !gui {
            continue;
        }
        if mount.when == "optional" && !optional.contains(&mount.name) {
            continue;
        }

        if !mount.verified {
            if mount.when == "optional" && optional.contains(&mount.name) {
                eprintln!(
                    "warning: optional module '{}' requested but unverified — skipping",
                    mount.name
                );
            }
            continue;
        }

        // Apply string bind types directly from the struct (e.g., "--ro-bind")
        let arg_flag = format!("--{}", mount.bind_type);
        bwrap.arg(&arg_flag).arg(&mount.src).arg(&mount.dest);
    }
}

/// Mounts custom local directories listed in the project's `lion.toml`.
pub fn apply_user_mounts(bwrap: &mut Command, dry_run: bool) {
    let user_config = match crate::config::find_user_config() {
        Ok(Some(cfg)) => cfg,
        Ok(None) => return,
        Err(e) => {
            eprintln!("warning: could not read lion.toml: {}", e);
            return;
        }
    };

    if user_config.mounts.is_empty() {
        return;
    }

    // Interactively ask user to approve potentially dangerous custom local mapping
    let apply = if dry_run {
        true
    } else {
        println!("\n⚠️  lion.toml found with custom path exposures.");
        loop {
            print!("   Apply these mounts? [Enter=yes / N=no / D=show paths]: ");
            std::io::Write::flush(&mut std::io::stdout()).unwrap_or(());
            let mut input = String::new();
            std::io::stdin().read_line(&mut input).unwrap_or(0);
            match input.trim().to_uppercase().as_str() {
                "" | "Y" => break true,
                "N" => {
                    println!("   Skipping lion.toml mounts.");
                    break false;
                }
                "D" => {
                    println!("   Paths in lion.toml:");
                    for m in &user_config.mounts {
                        println!("     [{}]  {}", m.mode, m.src);
                    }
                }
                _ => println!("   Unknown input. Enter=yes, N=no, D=show paths."),
            }
        }
    };

    if apply {
        for mount in &user_config.mounts {
            let arg_flag = if mount.mode == "rw" {
                "--bind"
            } else {
                "--ro-bind"
            };
            bwrap.arg(arg_flag).arg(&mount.src).arg(&mount.dest);
        }
    }
}
