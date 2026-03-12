use crate::errors::{LionError, Result};
use std::env;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tracing::{debug, error, info, warn};

use crate::sandbox_engine::builder::build_bwrap;
use crate::sandbox_engine::environment::apply_environment;
use crate::sandbox_engine::mounts::apply_system_mounts;
use crate::sandbox_engine::userns::check_userns_available;
use crate::proxy::ProxyHandle;

fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

fn get_direct_child(ppid: u32) -> Option<u32> {
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.flatten() {
            if let Ok(name) = entry.file_name().into_string() {
                if let Ok(pid) = name.parse::<u32>() {
                    let stat_path = format!("/proc/{pid}/stat");
                    if let Ok(stat) = std::fs::read_to_string(stat_path) {
                        if let Some(last_rparen) = stat.rfind(')') {
                            let after_comm = &stat[last_rparen + 1..];
                            let fields: Vec<&str> = after_comm.split_whitespace().collect();
                            if fields.len() > 1 {
                                if let Ok(p) = fields[1].parse::<u32>() {
                                    if p == ppid {
                                        return Some(pid);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    None
}

/// Central entry point — builds and runs the sandboxed process.
pub fn run_sandboxed(
    cmd: Vec<String>,
    network_mode: crate::sandbox_engine::network::NetworkMode,
    dry_run: bool,
    ro_paths: Vec<String>,
    allowed_domains: Vec<String>,
    optional_names: Vec<String>,
    use_tui: bool,
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

    // 3. Load lion.toml config (silently ignored if absent)
    let lion_cfg = crate::config::load(&project_dir);
    let project_ro = lion_cfg.project_is_readonly();
    if !dry_run {
        let access = if project_ro { "read-only" } else { "read-write" };
        info!("Project dir ({}): {}", access, project_dir.display());
    }

    // Build bwrap command
    let mut bwrap = build_bwrap(project_path, network_mode, dry_run, project_ro);

    // 4. Mounts
    apply_system_mounts(&mut bwrap);

    if has_src && !project_ro {
        // Only separately pin src/ as ro when project itself is rw
        let src_path = src_dir.to_str().unwrap();
        bwrap.arg("--ro-bind").arg(src_path).arg(src_path);
    }

    // 4b. Mounts from lion.toml [[mount]] entries
    for entry in &lion_cfg.mount {
        let resolved = entry.resolved_path();
        let p = std::path::Path::new(&resolved);
        if p.exists() {
            let flag = if entry.is_readonly() { "--ro-bind" } else { "--bind" };
            let tag  = if entry.is_readonly() { "ro" } else { "rw" };
            info!("Mounting ({}) from lion.toml: {}", tag, resolved);
            bwrap.arg(flag).arg(&resolved).arg(&resolved);
        } else {
            warn!("lion.toml mount path does not exist, skipping: {}", resolved);
        }
    }

    // 4c. CLI --ro flags (appended on top of config)
    for path in &ro_paths {
        let p = std::path::Path::new(path);
        if p.exists() {
            info!("Mounting read-only (--ro): {}", path);
            bwrap.arg("--ro-bind").arg(path).arg(path);
        } else {
            warn!("--ro path does not exist, skipping: {}", path);
        }
    }

    // 4d. Environment
    apply_environment(&mut bwrap);

    // 4e. Optional Modules from saved.toml + CLI --optional
    // Modules are loaded if EITHER:
    //   1. They have state == 1 (enabled) in saved.toml, OR
    //   2. They are explicitly requested via --optional <name> on the CLI
    // This allows mixing saved configuration with per-run overrides.
    let opt_cfg = crate::optional_modules::OptionalModulesConfig::load(&project_dir)
        .map_err(|e| LionError::Internal(e.to_string()))?;
    let mut active_modules: Vec<String> = Vec::new();
    
    for m in opt_cfg.modules {
        let is_requested = optional_names.contains(&m.name);
        let is_enabled = m.state == 1;
        
        if is_enabled || is_requested {
            active_modules.push(m.name.clone());
            let activation_reason = if is_enabled && is_requested {
                "saved + CLI"
            } else if is_enabled {
                "saved"
            } else {
                "CLI"
            };
            info!("Activating optional module '{}' ({})", m.name, activation_reason);

            // 1. Process mounts
            for mount in &m.mounts {
                let src = crate::optional_modules::resolve_vars(&mount.src);
                let dst = crate::optional_modules::resolve_vars(&mount.dst);
                let src_path = std::path::Path::new(&src);

                if src_path.exists() {
                    let flag = match mount.mode.as_str() {
                        "rw" | "bind" => "--bind",
                        "dev" | "dev-bind" => "--dev-bind",
                        _ => "--ro-bind",
                    };
                    bwrap.arg(flag).arg(&src).arg(&dst);
                } else if is_requested {
                    warn!("Requested module '{}' mount path does not exist: {}", m.name, src);
                }
            }

            // 2. Process legacy single path
            if let Some(path) = &m.path {
                let resolved = crate::optional_modules::resolve_vars(path);
                if std::path::Path::new(&resolved).exists() {
                    bwrap.arg("--bind").arg(&resolved).arg(&resolved);
                }
            }

            // 3. Process environment variables
            for var_name in &m.env {
                if let Ok(val) = std::env::var(var_name) {
                    bwrap.arg("--setenv").arg(var_name).arg(val);
                }
            }
        }
    }

    // 6. Net
    let _proxy: Option<ProxyHandle> = match network_mode {
        crate::sandbox_engine::network::NetworkMode::Allow => {
            // Load persistent domains from proxy.toml (project dir first, then ~/.config/lion/)
            let proxy_cfg = crate::proxy::load_config(&project_dir);
            let mut final_domains = allowed_domains.clone();
            final_domains.extend(proxy_cfg.domains);
            final_domains.sort();
            final_domains.dedup();

            match ProxyHandle::spawn(&final_domains) {
                Ok(p) => {
                    let proxy_url = format!("http://127.0.0.1:{}", p.port);
                    // Standard env vars (curl, wget, pip, cargo, …)
                    bwrap.arg("--setenv").arg("HTTP_PROXY").arg(&proxy_url);
                    bwrap.arg("--setenv").arg("HTTPS_PROXY").arg(&proxy_url);
                    bwrap.arg("--setenv").arg("http_proxy").arg(&proxy_url);
                    bwrap.arg("--setenv").arg("https_proxy").arg(&proxy_url);
                    bwrap.arg("--setenv").arg("ALL_PROXY").arg(&proxy_url);
                    bwrap.arg("--setenv").arg("all_proxy").arg(&proxy_url);
                    // npm-specific proxy config (npm ignores HTTP_PROXY)
                    bwrap.arg("--setenv").arg("npm_config_proxy").arg(&proxy_url);
                    bwrap.arg("--setenv").arg("npm_config_https_proxy").arg(&proxy_url);
                    // pip honours HTTP_PROXY but also reads these
                    bwrap.arg("--setenv").arg("PIP_PROXY").arg(&proxy_url);
                    info!("Proxy ready on :{} — {} domain(s) allowed", p.port, final_domains.len());
                    Some(p)
                }
                Err(e) => {
                    warn!("Proxy failed to start: {} — continuing without proxy", e);
                    None
                }
            }
        }
        _ => None,
    };

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
    bwrap.stderr(Stdio::piped());
    let mut child = bwrap.spawn().map_err(|e| LionError::Internal(e.to_string()))?;
    let bwrap_pid = child.id();

    // Setup SIGINT handler to kill the sandbox immediately on Ctrl-C
    let _ = ctrlc::set_handler(move || {
        eprintln!("\n\x1b[1;33m[LION] Interrupted, cleaning up sandbox (PID {})...\x1b[0m", bwrap_pid);
        let mut kill = Command::new("kill");
        kill.arg("-TERM").arg(bwrap_pid.to_string());
        let _ = kill.status();
        // The drop handles for monitor/perf will trigger as well when main() exits,
        // but explicit kill here ensures bwrap dies even if we're stuck in child.wait().
    });

    // Build watch list: project dir + any explicit --ro paths
    let mut watch_paths = vec![project_path.to_string()];
    watch_paths.extend(ro_paths.clone());
    watch_paths.dedup();

    if use_tui {
        // ── TUI Mode ────────────────────────────────────────────────────────────
        let (tui_handle, tui_join) = crate::tui::TuiHandle::spawn();

        let mut exposed_paths: Vec<String> = Vec::new();
        exposed_paths.push(format!("{} ({})", project_path, if project_ro { "ro" } else { "rw" }));
        for entry in &lion_cfg.mount {
            let resolved = entry.resolved_path();
            let access = if entry.is_readonly() { "ro" } else { "rw" };
            exposed_paths.push(format!("{} ({})", resolved, access));
        }
        for path in &ro_paths {
            exposed_paths.push(format!("{} (ro)", path));
        }
        exposed_paths.sort();
        exposed_paths.dedup();

        active_modules.sort();
        active_modules.dedup();

        tui_handle.send_info(crate::tui::SandboxInfo {
            command:      cmd.clone(),
            network_mode: format!("{network_mode:?}").to_lowercase(),
            pid:          bwrap_pid,
            started_at:   Some(chrono::Local::now()),
            project_access: if project_ro { "ro".to_string() } else { "rw".to_string() },
            exposed_paths,
            active_modules,
        });
        tui_handle.log(crate::tui::SandboxEvent::info(format!("[LION] sandbox started — bwrap PID {bwrap_pid}")));

        // Monitors
        let _monitor = child.stderr.take().map(|s| {
            crate::monitor::MonitorHandle::start_with_tui(s, watch_paths, tui_handle.clone())
        });

        // Find leader PID
        let mut leader_pid = None;
        for _ in 0..20 {
            if let Some(p) = get_direct_child(bwrap_pid) {
                leader_pid = Some(p);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let leader_pid = leader_pid.unwrap_or(bwrap_pid);
        let _perf = crate::tui::PerfCollectorHandle::spawn(leader_pid, tui_handle.clone());

        info!("Waiting for sandbox leader (PID {})...", leader_pid);
        
        let status = loop {
            if let Ok(Some(s)) = child.try_wait() { break s; }
            if !std::path::Path::new(&format!("/proc/{}", leader_pid)).exists() {
                info!("Leader process {} exited, terminating sandbox...", leader_pid);
                let _ = child.kill();
                break child.wait().unwrap_or_else(|_| std::process::Command::new("true").status().unwrap());
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        };

        tui_handle.log(crate::tui::SandboxEvent::info(format!("[LION] sandbox exited — status {}", status.code().unwrap_or(-1))));
        tui_handle.shutdown(tui_join);
        finalize_execution(status, cmd)
    } else {
        // ── Standard CLI Mode (Multi-terminal) ──────────────────────────────────
        let _monitor = child.stderr.take().map(|s| crate::monitor::MonitorHandle::start(s, watch_paths));

        // Find leader PID
        let mut leader_pid = None;
        for _ in 0..20 {
            if let Some(p) = get_direct_child(bwrap_pid) {
                leader_pid = Some(p);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let leader_pid = leader_pid.unwrap_or(bwrap_pid);
        
        let cmd_label = cmd.first().cloned().unwrap_or_else(|| "sandbox".to_string());
        let _perf = crate::monitor::perf::PerfHandle::spawn(leader_pid, &cmd_label);

        info!("Waiting for sandbox leader (PID {})...", leader_pid);
        
        let status = loop {
            if let Ok(Some(s)) = child.try_wait() { break s; }
            if !std::path::Path::new(&format!("/proc/{}", leader_pid)).exists() {
                info!("Leader process {} exited, terminating sandbox...", leader_pid);
                let _ = child.kill();
                break child.wait().unwrap_or_else(|_| std::process::Command::new("true").status().unwrap());
            }
            std::thread::sleep(std::time::Duration::from_millis(500));
        };

        finalize_execution(status, cmd)
    }
}

fn finalize_execution(status: std::process::ExitStatus, cmd: Vec<String>) -> Result<()> {
    if status.success() {
        debug!("Command completed successfully");
        Ok(())
    } else {
        let code = status.code().unwrap_or(1);
        let program_name = cmd.first().cloned().unwrap_or_else(|| "unknown".to_string());
        
        if code == 1 || code == 126 || code == 127 {
            if let Some(path) = find_binary(&program_name) {
                if is_executable(&path) {
                    if code == 1 { return Err(LionError::ExecutionError(code)); }
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
