//! `sandbox.rs`
//!
//! Handles building and executing the `bwrap` command manually.
//! It applies standard hardcoded system bounds and resolves the
//! isolated shell/process context.

use anyhow::{Result, bail};
use std::env;
use std::path::PathBuf;
use std::process::Command;

/// Initializes the bubblewrap command with the fundamental namespace unshares.
pub fn build_bwrap(project_path: &str, network: bool, dry_run: bool) -> Command {
    let mut bwrap = Command::new("bwrap");

    // Core isolation flags
    bwrap.args([
        "--unshare-user",   // Isolate user/group IDs
        "--unshare-ipc",    // Isolate Inter-Process Communication
        "--unshare-pid",    // Isolate process tree
        "--unshare-uts",    // Isolate hostname
        "--unshare-cgroup", // Isolate cgroups
        "--tmpfs",
        "/tmp", // Fresh tmp system
        "--proc",
        "/proc", // Fresh procfs
        "--dev",
        "/dev", // Fresh dev system
        "--bind",
        project_path,
        project_path, // The project directory itself is always mapped RW
    ]);

    // Apply network restrictions
    if !network {
        bwrap.arg("--unshare-net");
        if !dry_run {
            println!("🌐 Network: disabled");
        }
    } else {
        println!("🌐 Network: enabled");
        if std::path::Path::new("/etc/resolv.conf").exists() {
            bwrap.args(["--ro-bind", "/etc/resolv.conf", "/etc/resolv.conf"]);
        }
        if std::path::Path::new("/etc/ssl").exists() {
            bwrap.args(["--ro-bind", "/etc/ssl", "/etc/ssl"]);
        }
        if std::path::Path::new("/etc/pki").exists() {
            bwrap.args(["--ro-bind", "/etc/pki", "/etc/pki"]);
        }
    }

    bwrap
}

/// Passes strictly safe environment variables into the sandbox.
pub fn apply_environment(bwrap: &mut Command, gui: bool) {
    // If GUI is allowed, pass display servers to allow windowing.
    if gui {
        if let Ok(display) = std::env::var("DISPLAY") {
            bwrap.arg("--setenv").arg("DISPLAY").arg(&display);
        }
        if let Ok(wayland_display) = std::env::var("WAYLAND_DISPLAY") {
            bwrap
                .arg("--setenv")
                .arg("WAYLAND_DISPLAY")
                .arg(&wayland_display);
        }
    }

    // Pass essential identity / localized env vars
    for var in [
        "HOME",
        "USER",
        "LOGNAME",
        "LANG",
        "LC_ALL",
        "PATH",
        "XDG_RUNTIME_DIR",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
    ] {
        if let Ok(val) = std::env::var(var) {
            bwrap.arg("--setenv").arg(var).arg(val);
        }
    }
}

/// Mounts static hardcoded system directories needed to run typical binaries.
pub fn apply_system_mounts(bwrap: &mut Command, gui: bool) {
    let standard_paths = ["/usr", "/bin", "/lib", "/lib64", "/etc/alternatives"];

    for path in standard_paths {
        if std::path::Path::new(path).exists() {
            bwrap.args(["--ro-bind", path, path]);
        }
    }

    if gui {
        let gui_paths = ["/tmp/.X11-unix", "/usr/share/fonts"];

        for path in gui_paths {
            if std::path::Path::new(path).exists() {
                bwrap.args(["--ro-bind", path, path]);
            }
        }

        if let Ok(runtime_dir) = env::var("XDG_RUNTIME_DIR")
            && let Ok(wayland_display) = env::var("WAYLAND_DISPLAY")
        {
            let socket = format!("{}/{}", runtime_dir, wayland_display);
            if std::path::Path::new(&socket).exists() {
                bwrap.args(["--bind", &socket, &socket]);
            }
        }
    }
}

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
    _optional: Vec<String>,
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

    // 2. Build Execution object
    let mut bwrap = build_bwrap(project_path, network, dry_run);

    // 3. Mounts & Environment
    apply_system_mounts(&mut bwrap, gui);

    // Enforce read-only constraint manually onto src directory
    if has_src {
        let src_path = src_dir.to_str().unwrap();
        bwrap.arg("--ro-bind").arg(src_path).arg(src_path);
    }

    apply_environment(&mut bwrap, gui);

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
