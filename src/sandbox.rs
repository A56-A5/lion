//! `sandbox.rs`
//!
//! Responsible for constructing and executing the `bwrap` sandbox command.
//!
//! Execution flow:
//!   1. Verify bwrap is installed
//!   2. Pre-flight check that user namespaces are available (AppArmor probe)
//!   3. Build the bwrap command with namespace isolation flags
//!   4. Mount required system directories and user-defined paths
//!   5. Forward safe environment variables
//!   6. Execute and forward the exit code

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

    // Network isolation:
    // - Default (no --network flag): sandbox gets its own network namespace with
    //   no interfaces, so it literally cannot reach the internet or LAN.
    // - With --network flag: the sandbox shares the host network namespace, and
    //   we bind in the DNS/TLS config files it needs to resolve names.
    if !network {
        // --unshare-net gives the sandbox a fresh empty network namespace.
        // No interfaces = no outbound connections possible.
        bwrap.arg("--unshare-net");
        if !dry_run {
            println!("🌐 Network: disabled");
        }
    } else {
        // Bind in the minimum files needed for DNS resolution and HTTPS.
        // We use --ro-bind so the sandboxed process cannot modify them.
        if std::path::Path::new("/etc/resolv.conf").exists() {
            bwrap.args(["--ro-bind", "/etc/resolv.conf", "/etc/resolv.conf"]);
        }
        if std::path::Path::new("/etc/ssl").exists() {
            bwrap.args(["--ro-bind", "/etc/ssl", "/etc/ssl"]);
        }
        if std::path::Path::new("/etc/pki").exists() {
            bwrap.args(["--ro-bind", "/etc/pki", "/etc/pki"]);
        }
        if !dry_run {
            println!("🌐 Network: enabled");
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
        // /usr/share/fonts: read-only is correct, fonts are static data.
        if std::path::Path::new("/usr/share/fonts").exists() {
            bwrap.args(["--ro-bind", "/usr/share/fonts", "/usr/share/fonts"]);
        }

        // /tmp/.X11-unix: X11 display sockets live here.
        // Must be --bind (read-write) — X11 clients connect by *writing* to
        // these Unix sockets.  Using --ro-bind here silently breaks all X11 GUI apps.
        if std::path::Path::new("/tmp/.X11-unix").exists() {
            bwrap.args(["--bind", "/tmp/.X11-unix", "/tmp/.X11-unix"]);
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

/// Probes whether bwrap can actually create a user namespace on this machine.
///
/// Returns `Ok(())` if it works, or an `Err` with an actionable message if
/// AppArmor (or another policy) is blocking `--unshare-user`.
pub fn check_userns_available() -> Result<()> {
    // bwrap requires at least a root bind + /dev + /proc to execute successfully.
    // Without --ro-bind / / the command always fails regardless of userns policy.
    let output = Command::new("bwrap")
        .args([
            "--unshare-user",
            "--ro-bind", "/", "/",
            "--dev", "/dev",
            "--proc", "/proc",
            "--", "true",
        ])
        .output();

    match output {
        Ok(o) if o.status.success() => Ok(()),
        _ => {
            // Check whether the AppArmor restriction knob is the cause.
            let apparmor_blocking = std::fs::read_to_string(
                "/proc/sys/kernel/apparmor_restrict_unprivileged_userns",
            )
            .unwrap_or_default()
            .trim()
                == "1";

            if apparmor_blocking {
                bail!(
                    "[lion] AppArmor is blocking bwrap from creating user namespaces.\n\
                     This is a one-time setup issue — run:\n\
                     \n\
                     \x1b[1m    sudo lion install\x1b[0m\n\
                     \n\
                     This creates a targeted AppArmor rule for bwrap only.\n\
                     It does NOT disable AppArmor globally."
                );
            } else {
                bail!(
                    "[lion] bwrap cannot create user namespaces on this system.\n\
                     Check: dmesg | grep -i 'userns\\|bwrap'"
                );
            }
        }
    }
}

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
