//! `sandbox_engine/mod.rs`
//!
//! Main interface for executing sandbox processes inside Bubblewrap.

pub mod core;
pub mod env;
pub mod mounts;

use crate::scanner_engine::integrity::integrity_check;
use anyhow::{Result, bail};
use std::path::PathBuf;
use std::process::Command;

/// Central function to run the process. Executes steps in order:
/// 1. Validation
/// 2. Bwrap execution object construction
/// 3. Environment & CLI Application
/// 4. Execution wrapper handling specific return codes.
pub fn run_sandboxed(
    cmd: Vec<String>,
    network: bool,
    dry_run: bool,
    gui: bool,
    optional: Vec<String>,
) -> Result<()> {
    // 1. Core Dependency Check
    if Command::new("bwrap")
        .arg("--version")
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true)
    {
        eprintln!(
            "error: bubblewrap (bwrap) is not installed.\n\
             Install it via your package manager (e.g. `sudo apt install bubblewrap`)."
        );
        bail!("exit code: 125");
    }

    println!("🔒 Running inside sandbox...");

    // Get basic bounds around what source codes to isolate.
    let project_dir: PathBuf = std::env::current_dir()?;
    let project_path = project_dir.to_str().unwrap();
    let src_dir = project_dir.join("src");
    let has_src = src_dir.exists() && src_dir.is_dir();

    if has_src && !dry_run {
        println!("🔒 Protecting src/ as read-only");
    }
    if !dry_run {
        println!("📂 Project dir: {}", project_dir.display());
    }

    // Re-verify the machine dependencies haven't broken.
    let system_config = integrity_check(network, gui)?;

    // 2. Build Execution object
    let mut bwrap = core::build_bwrap(project_path, network, dry_run);

    // 3. Mounts & Environment
    mounts::apply_system_mounts(&mut bwrap, &system_config, network, gui, &optional);
    mounts::apply_user_mounts(&mut bwrap, dry_run);

    // Enforce read-only constraint manually onto src directory
    if has_src {
        let src_path = src_dir.to_str().unwrap();
        bwrap.arg("--ro-bind").arg(src_path).arg(src_path);
    }

    env::apply_environment(&mut bwrap, gui);

    bwrap.arg("--chdir").arg(&project_dir).arg("--").args(&cmd);

    // Debug print
    if dry_run {
        let program = bwrap.get_program().to_string_lossy();
        let args = bwrap
            .get_args()
            .map(|a| a.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(" ");
        println!(
            "🧪 Dry run mode: command not executed\n{} {}",
            program, args
        );
        return Ok(());
    }

    // 4. Execute
    let status = bwrap.status()?;

    // Route exit values to identical mappings.
    if status.success() {
        println!("✅ Command completed successfully");
        Ok(())
    } else {
        let code = status.code().unwrap_or(1);
        eprintln!("❌ Command exited with status: {}", code);
        bail!("exit code: {}", code);
    }
}
