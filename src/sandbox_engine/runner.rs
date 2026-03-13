use crate::errors::{LionError, Result};
use std::env;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tracing::{error, info, warn};

use crate::proxy::ProxyHandle;
use crate::sandbox_engine::builder::build_bwrap;
use crate::sandbox_engine::environment::apply_environment;
use crate::sandbox_engine::mounts::apply_system_mounts;
use crate::sandbox_engine::procfs::{get_direct_child, get_process_tree};
use crate::sandbox_engine::userns::check_userns_available;

fn is_executable(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path)
        .map(|m| m.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
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
        return Err(LionError::DependencyMissing(
            "bubblewrap (bwrap)".to_string(),
        ));
    }

    // 2. User namespace pre-flight
    if !dry_run {
        check_userns_available().map_err(|e| LionError::NamespaceError(e.to_string()))?;
    }

    info!("Running inside sandbox...");

    let project_dir: PathBuf =
        env::current_dir().map_err(|e| LionError::EnvironmentError(e.to_string()))?;
    let project_path = project_dir
        .to_str()
        .ok_or_else(|| LionError::EnvironmentError("path is not valid UTF-8".to_string()))?;

    // 3. Load configuration
    let lion_cfg = crate::config::load_merged(&project_dir);
    let project_ro = lion_cfg.project_is_readonly();
    let src_ro = lion_cfg.src_is_readonly();
    if !dry_run {
        let access = if project_ro {
            "read-only"
        } else {
            "read-write"
        };
        info!("Project dir ({}): {}", access, project_dir.display());
    }

    // Build bwrap command
    let mut bwrap = build_bwrap(project_path, network_mode, dry_run, project_ro);

    // 4. System mounts (includes --ro-bind $HOME $HOME)
    apply_system_mounts(&mut bwrap);

    // 4a. src/ overlay: when project is rw but src/ should stay read-only,
    // add a second --ro-bind on top.  bwrap processes mounts in order so this
    // correctly shadows the rw project bind with a ro one just for src/.
    let src_path = format!("{}/src", project_path);
    if !project_ro && src_ro && std::path::Path::new(&src_path).exists() {
        info!("src/ overlay: read-only");
        bwrap.arg("--ro-bind").arg(&src_path).arg(&src_path);
    }

    // 4b. Mounts from lion.toml
    // Note: if a [[mount]] entry targets $HOME with access = "rw", it will
    // override the ro-bind set in apply_system_mounts because bwrap bind
    // mounts in order and the later one wins for overlapping paths.
    for entry in &lion_cfg.mount {
        let resolved = entry.resolved_path();
        if std::path::Path::new(&resolved).exists() {
            let flag = if entry.is_readonly() {
                "--ro-bind"
            } else {
                "--bind"
            };
            bwrap.arg(flag).arg(&resolved).arg(&resolved);
        }
    }

    // 4c. CLI --ro flags
    for path in &ro_paths {
        if std::path::Path::new(path).exists() {
            bwrap.arg("--ro-bind").arg(path).arg(path);
        }
    }

    // 4d. Environment
    apply_environment(&mut bwrap);

    // 4e. Optional Modules
    let opt_cfg = crate::optional_modules::OptionalModulesConfig::load(&project_dir)
        .map_err(|e| LionError::Internal(e.to_string()))?;
    let mut active_modules: Vec<String> = Vec::new();

    for m in &opt_cfg.modules {
        let is_requested = optional_names.contains(&m.name);
        let is_enabled = m.state == 1;

        if is_enabled || is_requested {
            active_modules.push(m.name.clone());
            for mount in &m.mounts {
                let src = crate::optional_modules::resolve_vars(&mount.src);
                let dst = crate::optional_modules::resolve_vars(&mount.dst);
                if std::path::Path::new(&src).exists() {
                    let flag = match mount.mode.as_str() {
                        "rw" | "bind" => "--bind",
                        "dev" | "dev-bind" => "--dev-bind",
                        _ => "--ro-bind",
                    };
                    bwrap.arg(flag).arg(&src).arg(&dst);
                }
            }
            if let Some(path) = &m.path {
                let resolved = crate::optional_modules::resolve_vars(path);
                if std::path::Path::new(&resolved).exists() {
                    bwrap.arg("--bind").arg(&resolved).arg(&resolved);
                }
            }
            for var_name in &m.env {
                if let Ok(val) = std::env::var(var_name) {
                    bwrap.arg("--setenv").arg(var_name).arg(val);
                }
            }
        }
    }

    // 5. Network
    let _proxy: Option<ProxyHandle> = match network_mode {
        crate::sandbox_engine::network::NetworkMode::Allow => {
            let proxy_cfg = crate::proxy::load_config(&project_dir);
            let mut final_domains = allowed_domains.clone();
            final_domains.extend(proxy_cfg.domains);
            final_domains.sort();
            final_domains.dedup();

            match ProxyHandle::spawn(&final_domains) {
                Ok(p) => {
                    let proxy_url = format!("http://127.0.0.1:{}", p.port);
                    bwrap.arg("--setenv").arg("HTTP_PROXY").arg(&proxy_url);
                    bwrap.arg("--setenv").arg("HTTPS_PROXY").arg(&proxy_url);
                    bwrap.arg("--setenv").arg("http_proxy").arg(&proxy_url);
                    bwrap.arg("--setenv").arg("https_proxy").arg(&proxy_url);
                    bwrap.arg("--setenv").arg("ALL_PROXY").arg(&proxy_url);
                    bwrap.arg("--setenv").arg("all_proxy").arg(&proxy_url);
                    bwrap
                        .arg("--setenv")
                        .arg("npm_config_proxy")
                        .arg(&proxy_url);
                    bwrap
                        .arg("--setenv")
                        .arg("npm_config_https_proxy")
                        .arg(&proxy_url);
                    bwrap.arg("--setenv").arg("PIP_PROXY").arg(&proxy_url);
                    info!(
                        "Proxy ready on :{} — {} domain(s) allowed",
                        p.port,
                        final_domains.len()
                    );
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
        println!("Dry run mode: command not executed");
        return Ok(());
    }

    // 6. Execute
    bwrap.stderr(Stdio::piped());
    if use_tui {
        // In TUI mode, capture stdout so we can display it in the output panel.
        // In non-TUI mode, leave stdout connected to the real terminal.
        bwrap.stdout(Stdio::piped());
    }

    let mut child = bwrap
        .spawn()
        .map_err(|e| LionError::Internal(e.to_string()))?;
    let bwrap_pid = child.id();

    // 7. SIGINT handler
    let _ = ctrlc::set_handler(move || {
        eprintln!("\n[LION] Interrupted, cleaning up sandbox...");
        let _ = Command::new("kill")
            .arg("-TERM")
            .arg(bwrap_pid.to_string())
            .status();
    });

    let mut watch_paths = vec![project_path.to_string()];
    watch_paths.extend(ro_paths.clone());
    watch_paths.dedup();

    if use_tui {
        // ── TUI Mode ────────────────────────────────────────────────────────────
        let (tui_handle, tui_join) = crate::tui::TuiHandle::spawn();

        let mut exposed_paths: Vec<String> = Vec::new();

        // Home dir is --dir (empty tmpfs dir) — NOT a bind mount, so intentionally
        // not listed here. Only explicit [[mount]] entries that resolve to real
        // paths are shown as "exposed".

        // Project directory
        exposed_paths.push(format!(
            "{} ({})",
            project_path,
            if project_ro { "ro" } else { "rw" }
        ));

        // src/ overlay (only visible when project is rw and src is ro)
        if !project_ro && src_ro && std::path::Path::new(&src_path).exists() {
            exposed_paths.push(format!("{} (ro)", src_path));
        }

        // Host mounts from lion.toml — these may override the home ro-bind.
        for entry in &lion_cfg.mount {
            let res = entry.resolved_path();
            if std::path::Path::new(&res).exists() {
                // If this entry overrides the home dir with rw, update TUI label.
                let label = format!(
                    "{} ({})",
                    res,
                    if entry.is_readonly() { "ro" } else { "rw" }
                );
                // Remove any earlier "<path> (ro)" entry for the same path so
                // the rw override is the definitive entry shown.
                exposed_paths.retain(|p| {
                    let p_clean = p.split(" (").next().unwrap_or(p);
                    p_clean != res
                });
                exposed_paths.push(label);
            }
        }

        // CLI --ro paths
        for p in &ro_paths {
            if std::path::Path::new(p).exists() {
                exposed_paths.push(format!("{} (ro)", p));
            }
        }

        // Optional module mounts
        for m in &opt_cfg.modules {
            if m.state == 1 || optional_names.contains(&m.name) {
                for mount in &m.mounts {
                    let src = crate::optional_modules::resolve_vars(&mount.src);
                    if std::path::Path::new(&src).exists() {
                        exposed_paths.push(format!("{} ({})", src, mount.mode));
                    }
                }
                if let Some(path) = &m.path {
                    let res = crate::optional_modules::resolve_vars(path);
                    if std::path::Path::new(&res).exists() {
                        exposed_paths.push(format!("{} (rw)", res));
                    }
                }
            }
        }

        exposed_paths.sort();
        exposed_paths.dedup();
        active_modules.sort();
        active_modules.dedup();

        // is_home_exposed: home is --dir (empty), never auto-exposed.
        // Only flag it if the user added an explicit [[mount]] entry for $HOME with rw.
        let home_dir = std::env::var("HOME").ok();
        let is_home_exposed = home_dir.as_ref().map(|h| {
            exposed_paths.iter().any(|p| {
                let p_clean = p.split(" (").next().unwrap_or(p);
                p_clean == h && p.contains("(rw)")
            })
        }).unwrap_or(false);

        tui_handle.send_info(crate::tui::SandboxInfo {
            command: cmd.clone(),
            network_mode: format!("{network_mode:?}").to_lowercase(),
            pid: bwrap_pid,
            started_at: Some(chrono::Local::now()),
            project_access: if project_ro {
                "ro".to_string()
            } else {
                "rw".to_string()
            },
            exposed_paths,
            active_modules,
            is_home_exposed,
        });

        let _monitor = child.stderr.take().map(|s| {
            crate::monitor::MonitorHandle::start_with_tui(s, watch_paths, tui_handle.clone())
        });

        // ── stdout → TUI Output Panel ─────────────────────────────────────────
        let _stdout_reader = child.stdout.take().map(|stdout| {
            use std::io::BufRead;
            let tui_out = tui_handle.clone();
            std::thread::spawn(move || {
                let mut reader = std::io::BufReader::new(stdout);
                let mut line = String::new();
                loop {
                    line.clear();
                    match reader.read_line(&mut line) {
                        Ok(0) => break, // EOF
                        Ok(_) => {
                            // Strip trailing newline but preserve the content
                            let trimmed = line.trim_end_matches('\n').trim_end_matches('\r');
                            tui_out.output(trimmed.to_string());
                        }
                        Err(_) => break,
                    }
                }
            })
        });


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
            if let Ok(Some(s)) = child.try_wait() {
                break s;
            }
            if !std::path::Path::new(&format!("/proc/{}", leader_pid)).exists() {
                if get_process_tree(bwrap_pid).len() <= 1 {
                    let _ = child.kill();
                    break child
                        .wait()
                        .unwrap_or_else(|_| Command::new("true").status().unwrap());
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        };

        tui_handle.log(crate::tui::SandboxEvent::info(format!(
            "[LION] sandbox exited — status {}",
            status.code().unwrap_or(-1)
        )));
        tui_handle.shutdown(tui_join);
        finalize_execution(status, cmd)
    } else {
        // ── Standard CLI Mode ──────────────────────────────────────────────────
        let _monitor = child
            .stderr
            .take()
            .map(|s| crate::monitor::MonitorHandle::start(s, watch_paths));

        let mut leader_pid = None;
        for _ in 0..20 {
            if let Some(p) = get_direct_child(bwrap_pid) {
                leader_pid = Some(p);
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let leader_pid = leader_pid.unwrap_or(bwrap_pid);
        let _perf = crate::monitor::perf::PerfHandle::spawn(
            leader_pid,
            &cmd.first().cloned().unwrap_or_default(),
        );

        info!("Waiting for sandbox leader (PID {})...", leader_pid);

        let status = loop {
            if let Ok(Some(s)) = child.try_wait() {
                break s;
            }
            if !std::path::Path::new(&format!("/proc/{}", leader_pid)).exists() {
                if get_process_tree(bwrap_pid).len() <= 1 {
                    let _ = child.kill();
                    break child
                        .wait()
                        .unwrap_or_else(|_| Command::new("true").status().unwrap());
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        };

        finalize_execution(status, cmd)
    }
}

fn finalize_execution(status: std::process::ExitStatus, cmd: Vec<String>) -> Result<()> {
    if status.success() {
        Ok(())
    } else {
        let code = status.code().unwrap_or(1);
        let program_name = cmd
            .first()
            .cloned()
            .unwrap_or_else(|| "unknown".to_string());
        if code == 1 || code == 126 || code == 127 {
            if let Some(path) = find_binary(&program_name) {
                if !is_executable(&path) {
                    return Err(LionError::PermissionDenied(program_name));
                }
            } else {
                return Err(LionError::CommandNotFound(program_name));
            }
        }
        Err(LionError::ExecutionError(code))
    }
}

fn find_binary(name: &str) -> Option<PathBuf> {
    if name.contains('/') {
        let path = PathBuf::from(name);
        return if path.exists() { Some(path) } else { None };
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
