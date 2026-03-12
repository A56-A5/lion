use crate::errors::{LionError, Result};
use std::env;
use std::path::PathBuf;
use std::process::Command;
use tracing::{debug, error, info, warn};

use crate::sandbox_engine::builder::build_bwrap;
use crate::sandbox_engine::environment::apply_environment;
use crate::sandbox_engine::mounts::apply_system_mounts;
use crate::sandbox_engine::userns::check_userns_available;

/// Central entry point — builds and runs the sandboxed process.
pub fn run_sandboxed(
    cmd: Vec<String>,
    network_mode: crate::sandbox_engine::network::NetworkMode,
    dry_run: bool,
    gui: bool,
    _optional: Vec<String>,
) -> Result<()> {
    // 1. Core Dependency Check
    if Command::new("bwrap")
        .arg("--version")
        .output()
        .map(|o| !o.status.success())
        .unwrap_or(true)
    {
        error!("bubblewrap (bwrap) is not installed.");
        return Err(LionError::DependencyMissing("bubblewrap (bwrap)".to_string()));
    }

    // 2. User namespace pre-flight
    if !dry_run {
        check_userns_available().map_err(|e| LionError::NamespaceError(e.to_string()))?;
    }

    info!("Running inside sandbox...");

    let project_dir: PathBuf = env::current_dir().map_err(|e| LionError::EnvironmentError(e.to_string()))?;
    let project_path = project_dir.to_str().ok_or_else(|| LionError::EnvironmentError("path is not valid UTF-8".to_string()))?;
    let src_dir = project_dir.join("src");
    let has_src = src_dir.exists() && src_dir.is_dir();

    if has_src && !dry_run {
        info!("Protecting src/ as read-only");
    }
    if !dry_run {
        info!("Project dir: {}", project_dir.display());
    }

    // 3. Build bwrap command
    let mut bwrap = build_bwrap(project_path, network_mode, dry_run);

    // 4. Mounts
    apply_system_mounts(&mut bwrap, gui);

    if has_src {
        let src_path = src_dir.to_str().unwrap();
        bwrap.arg("--ro-bind").arg(src_path).arg(src_path);
    }

    // 5. Env
    apply_environment(&mut bwrap, gui);

    bwrap.arg("--chdir").arg(&project_dir).arg("--").args(&cmd);

    if dry_run {
        let program = bwrap.get_program().to_string_lossy();
        let args = bwrap
            .get_args()
            .map(|a: &std::ffi::OsStr| a.to_string_lossy().to_string())
            .collect::<Vec<_>>()
            .join(" ");
        println!("Dry run mode: command not executed\n{} {}", program, args);
        return Ok(());
    }

    // 6. Execute
    let status = bwrap.status().map_err(|e| LionError::Internal(e.to_string()))?;

    if status.success() {
        debug!("Command completed successfully");
        Ok(())
    } else {
        let code = status.code().unwrap_or(1);
        let program_name = cmd.first().cloned().unwrap_or_else(|| "unknown".to_string());
        
        // On some systems bwrap returns 1 for execvp failures.
        // We do a manual check to provide better diagnostics if code is 1, 126, or 127.
        if code == 1 || code == 126 || code == 127 {
            if let Some(path) = find_binary(&program_name) {
                // If it exists but is not executable, it's PermissionDenied
                if is_executable(&path) {
                    if code == 1 {
                        return Err(LionError::ExecutionError(code));
                    }
                } else {
                    return Err(LionError::PermissionDenied(program_name));
                }
            } else {
                return Err(LionError::CommandNotFound(program_name));
            }
        }

        match code {
            127 => Err(LionError::CommandNotFound(program_name)),
            126 => Err(LionError::PermissionDenied(program_name)),
            _ => {
                warn!("Command exited with status: {}", code);
                Err(LionError::ExecutionError(code))
            }
        }
    }
}

fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn find_binary(name: &str) -> Option<PathBuf> {
    if name.contains('/') {
        let path = PathBuf::from(name);
        if path.exists() {
            return Some(path);
        }
        return None;
    }

    if let Ok(path_var) = env::var("PATH") {
        for dir in path_var.split(':') {
            let full_path = PathBuf::from(dir).join(name);
            if full_path.exists() {
                return Some(full_path);
            }
        }
    }
    None
}
