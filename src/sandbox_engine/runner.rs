use anyhow::{bail, Result};
use std::env;
use std::path::PathBuf;
use std::process::Command;

use crate::sandbox_engine::builder::build_bwrap;
use crate::sandbox_engine::environment::apply_environment;
use crate::sandbox_engine::mounts::apply_system_mounts;
use crate::sandbox_engine::userns::check_userns_available;

/// Central entry point — builds and runs the sandboxed process.
///
/// Steps in order:
///   1. Verify bwrap is installed
///   2. Pre-flight: confirm user namespaces are available
///   3. Build the bwrap command object with namespace flags
///   4. Apply system mounts + user mounts + src/ protection
///   5. Forward environment variables
///   6. Execute and forward the child's exit code
pub fn run_sandboxed(
    cmd: Vec<String>,
    network: bool,
    dry_run: bool,
    gui: bool,
    _optional: Vec<String>, // optional modules not yet wired in this build
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

    // 2. User namespace pre-flight — fail early with actionable message
    //    (skip in dry_run so developers can still inspect the full command)
    if !dry_run {
        check_userns_available()?;
    }

    println!("🔒 Running inside sandbox...");

    // Get basic bounds around what source codes to isolate.
    let project_dir: PathBuf = env::current_dir()?;
    let project_path = project_dir.to_str().unwrap();
    let src_dir = project_dir.join("src");
    let has_src = src_dir.exists() && src_dir.is_dir();

    if has_src && !dry_run {
        println!("🔒 Protecting src/ as read-only");
    }
    if !dry_run {
        println!("📂 Project dir: {}", project_dir.display());
    }

    // 3. Build the bwrap command with all namespace isolation flags.
    let mut bwrap = build_bwrap(project_path, network, dry_run);

    // 4. Mounts: system directories, user-defined paths, src/ protection.
    apply_system_mounts(&mut bwrap, gui);

    // Always protect src/ as read-only so the sandboxed process cannot
    // modify your source code — even if it has write access to the project dir.
    if has_src {
        let src_path = src_dir.to_str().unwrap();
        bwrap.arg("--ro-bind").arg(src_path).arg(src_path);
    }

    // 5. Forward safe environment variables into the sandbox.
    apply_environment(&mut bwrap, gui);

    // Set the working directory inside the sandbox to match the host cwd,
    // then pass the user's command after the -- separator.
    bwrap.arg("--chdir").arg(&project_dir).arg("--").args(&cmd);

    // Print the full bwrap command for debugging and return early.
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

    // 6. Execute — hand control to bwrap and wait for the child to finish.
    let status = bwrap.status()?;

    // Forward the exact exit code so the caller's shell can read it.
    // If the process was killed by a signal, status.code() is None — we use 1.
    if status.success() {
        println!("✅ Command completed successfully");
        Ok(())
    } else {
        let code = status.code().unwrap_or(1);
        eprintln!("❌ Command exited with status: {}", code);
        bail!("exit code: {}", code);
    }
}
