//! `tui/mod.rs`
//!
//! Public API and event loop for the L.I.O.N TUI.

pub mod app;
pub mod events;
pub mod ui;

use std::sync::mpsc::{self, Sender};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::{
    event::{Event, KeyEventKind},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

pub use events::{EventKind, PerfSnapshot, SandboxEvent, SandboxInfo, TuiMsg};

/// How often the TUI redraws even without incoming messages.
const TICK_MS: u64 = 500;

// ── TuiHandle ────────────────────────────────────────────────────────────────

/// A lightweight, clone-able sender for pushing events to the running TUI.
#[derive(Clone)]
pub struct TuiHandle {
    tx: Sender<TuiMsg>,
}

impl TuiHandle {
    /// Spawn the TUI event loop in a background thread and return a handle.
    pub fn spawn() -> (Self, thread::JoinHandle<()>) {
        let (tx, rx) = mpsc::channel::<TuiMsg>();
        let tx_clone = tx.clone();
        let join = thread::spawn(move || {
            if let Err(e) = run_tui_loop(rx, tx_clone) {
                eprintln!("\x1b[90m[LION/TUI] TUI exited with error: {e}\x1b[0m");
            }
        });
        (TuiHandle { tx }, join)
    }

    pub fn send(&self, msg: TuiMsg) {
        let _ = self.tx.send(msg);
    }

    pub fn log(&self, ev: SandboxEvent) {
        self.send(TuiMsg::Log(ev));
    }

    pub fn perf(&self, snap: PerfSnapshot) {
        self.send(TuiMsg::Perf(snap));
    }

    pub fn output(&self, line: String) {
        self.send(TuiMsg::Output(line));
    }

    pub fn send_info(&self, info: SandboxInfo) {
        self.send(TuiMsg::SandboxInfo(info));
    }

    pub fn shutdown(self, join: thread::JoinHandle<()>) {
        let _ = self.tx.send(TuiMsg::Shutdown);
        let _ = join.join();
    }

}

// ── TUI event loop ────────────────────────────────────────────────────────────

fn run_tui_loop(
    rx: mpsc::Receiver<TuiMsg>,
    _tx: mpsc::Sender<TuiMsg>,
) -> anyhow::Result<()> {
    enable_raw_mode()?;
    let mut stdout = std::io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app_state = app::App::new();
    let result = run_event_loop(&mut terminal, &mut app_state, rx, _tx);

    disable_raw_mode()?;
    execute!(terminal.backend_mut(), LeaveAlternateScreen)?;
    terminal.show_cursor()?;
    result
}

fn run_event_loop(
    terminal: &mut Terminal<CrosstermBackend<std::io::Stdout>>,
    app_state: &mut app::App,
    rx: mpsc::Receiver<TuiMsg>,
    _tx: mpsc::Sender<TuiMsg>,
) -> anyhow::Result<()> {
    let tick = Duration::from_millis(TICK_MS);
    let mut last_tick = Instant::now();

    loop {
        // Drain incoming messages — cap at 200 per frame so a flood of output
        // lines (e.g. cargo/npm printing hundreds of lines at once) can't block
        // the render loop. Remaining messages are picked up next iteration.
        let mut processed = 0;
        while processed < 200 {
            match rx.try_recv() {
                Ok(msg) => {
                    if app_state.handle_msg(msg) {
                        return Ok(());
                    }
                    processed += 1;
                }
                Err(_) => break,
            }
        }

        terminal.draw(|frame| ui::render(app_state, frame))?;

        let timeout = tick.saturating_sub(last_tick.elapsed());
        if crossterm::event::poll(timeout)? {
            match crossterm::event::read()? {
                Event::Key(key) => {
                    if key.kind == KeyEventKind::Press {
                        app_state.on_key(key.code);
                    }
                }
                // Terminal resized (or pop-out/maximize) — clear stale chars
                // and force a full redraw so no artifacts are left on screen.
                Event::Resize(_, _) => {
                    terminal.clear()?;
                }
                _ => {}
            }
        }

        if last_tick.elapsed() >= tick {
            app_state.tick();
            last_tick = Instant::now();
        }

        if app_state.should_quit {
            return Ok(());
        }
    }

}

// ── Perf collector ────────────────────────────────────────────────────────────

pub struct PerfCollectorHandle {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}

impl PerfCollectorHandle {
    pub fn spawn(pid: u32, tx: TuiHandle) -> Self {
        let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let stop_flag = stop.clone();
        let handle = thread::spawn(move || {
            perf_loop(pid, tx, stop_flag);
        });
        PerfCollectorHandle {
            stop,
            thread: Some(handle),
        }
    }
}

impl Drop for PerfCollectorHandle {
    fn drop(&mut self) {
        self.stop.store(true, std::sync::atomic::Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

fn perf_loop(root_pid: u32, tx: TuiHandle, stop: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    use std::sync::atomic::Ordering;
    use crate::sandbox_engine::procfs::get_process_tree;
    
    let user_hz = unsafe { libc::sysconf(libc::_SC_CLK_TCK) } as f64;
    let mut prev_ticks: Option<u64> = None;
    let mut prev_time = Instant::now();

    loop {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        thread::sleep(Duration::from_millis(500));
        if stop.load(Ordering::Relaxed) {
            break;
        }

        if !std::path::Path::new(&format!("/proc/{root_pid}")).exists() {
            break;
        }
        let pids = get_process_tree(root_pid);

        let mut processes = Vec::new();
        let mut total_utime = 0u64;
        let mut total_stime = 0u64;
        let mut total_rss_kb = 0u64;
        let mut total_vmsz_kb = 0u64;
        let mut total_threads = 0u32;
        let mut total_io_read_kb = 0u64;
        let mut total_io_write_kb = 0u64;
        let mut states = std::collections::HashSet::new();

        for pid in pids {
            let mut p_rss = 0u64;
            let mut p_comm = String::new();

            if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
                if let Some(start) = stat.find('(') {
                    if let Some(end) = stat.rfind(')') {
                        p_comm = stat[start + 1..end].to_string();
                    }
                }
                if let Some(last_rparen) = stat.rfind(')') {
                    let fields: Vec<&str> = stat[last_rparen + 1..].split_whitespace().collect();
                    if fields.len() >= 13 {
                        total_utime += fields[11].parse().unwrap_or(0);
                        total_stime += fields[12].parse().unwrap_or(0);
                        states.insert(fields[0].chars().next().unwrap_or('?'));
                    }
                }
            }
            if let Ok(status) = std::fs::read_to_string(format!("/proc/{pid}/status")) {
                for line in status.lines() {
                    if let Some(v) = line.strip_prefix("VmRSS:") {
                        let val = v.split_whitespace().next().and_then(|n| n.parse::<u64>().ok()).unwrap_or(0);
                        p_rss = val;
                        total_rss_kb += val;
                    } else if let Some(v) = line.strip_prefix("VmSize:") {
                        let val = v.split_whitespace().next().and_then(|n| n.parse::<u64>().ok()).unwrap_or(0);
                        total_vmsz_kb += val;
                    } else if let Some(v) = line.strip_prefix("Threads:") {
                        total_threads += v.split_whitespace().next().and_then(|n| n.parse().ok()).unwrap_or(1);
                    }
                }
            }

            processes.push(crate::tui::events::ProcessInfo {
                pid,
                comm: p_comm,
                cpu: 0.0,
                mem: p_rss,
            });

            if let Ok(io) = std::fs::read_to_string(format!("/proc/{pid}/io")) {
                for line in io.lines() {
                    if let Some(v) = line.strip_prefix("read_bytes:") {
                        total_io_read_kb += v.trim().parse::<u64>().unwrap_or(0) / 1024;
                    } else if let Some(v) = line.strip_prefix("write_bytes:") {
                        total_io_write_kb += v.trim().parse::<u64>().unwrap_or(0) / 1024;
                    }
                }
            }
        }

        let ticks = total_utime + total_stime;
        let now = Instant::now();
        let elapsed_secs = now.duration_since(prev_time).as_secs_f64().max(0.001);
        let cpu_pct = if let Some(prev) = prev_ticks {
            let delta_ticks = ticks.saturating_sub(prev) as f64;
            (delta_ticks / user_hz / elapsed_secs * 100.0).min(100.0 * num_cpus())
        } else {
            0.0
        };

        prev_ticks = Some(ticks);
        prev_time = now;

        let state_char = if states.contains(&'R') { 'R' } else if states.contains(&'D') { 'D' } else if states.contains(&'S') { 'S' } else { states.iter().next().cloned().unwrap_or('?') };

        tx.perf(PerfSnapshot {
            cpu_pct,
            rss_kb: total_rss_kb,
            vmsz_kb: total_vmsz_kb,
            threads: total_threads,
            io_read_kb: total_io_read_kb,
            io_write_kb: total_io_write_kb,
            state: state_char,
            processes,
        });
    }
}

#[inline]
fn num_cpus() -> f64 {
    unsafe { libc::sysconf(libc::_SC_NPROCESSORS_ONLN) as f64 }.max(1.0)
}

// ── Shared logic with monitor ────────────────────────────────────────────────

pub fn parse_monitor_line(line: &str) -> SandboxEvent {
    let line = line.trim();
    if line.contains("[LION-PROXY]") {
        if line.contains("ALLOWED") {
            return SandboxEvent::new(EventKind::ProxyAllow, extract_proxy_target(line), line);
        }
        if line.contains("BLOCKED") {
            return SandboxEvent::new(EventKind::ProxyBlock, extract_proxy_target(line), line);
        }
    }
    if line.contains("Read-only file system") || line.contains("Permission denied") || line.contains("Operation not permitted") {
        SandboxEvent::new(EventKind::Blocked, extract_path(line), line)
    } else if line.contains("No such file or directory") {
        SandboxEvent::new(EventKind::Missing, extract_path(line), line)
    } else {
        SandboxEvent::info(line)
    }
}

pub fn inotify_event(mask: inotify::EventMask, path: String) -> SandboxEvent {
    use inotify::EventMask;
    let kind = if mask.contains(EventMask::DELETE) || mask.contains(EventMask::DELETE_SELF) {
        EventKind::Delete
    } else if mask.contains(EventMask::CREATE) {
        EventKind::Create
    } else if mask.contains(EventMask::MODIFY) {
        EventKind::Write
    } else if mask.contains(EventMask::ACCESS) || mask.contains(EventMask::OPEN) {
        EventKind::Read
    } else {
        EventKind::Info
    };
    SandboxEvent::new(kind, Some(path.clone()), path)
}

fn extract_path(line: &str) -> Option<String> {
    for token in line.split_whitespace() {
        let t = token.trim_matches(|c| c == '\'' || c == '"' || c == ':');
        if t.starts_with('/') {
            return Some(t.to_string());
        }
    }
    None
}

fn extract_proxy_target(line: &str) -> Option<String> {
    line.split_whitespace()
        .rev()
        .map(|s| s.trim_matches(|c| c == '\'' || c == '"' || c == ':' || c == ')' || c == '('))
        .find(|s| {
            !s.is_empty()
                && !s.contains("[LION-PROXY]")
                && *s != "ALLOWED"
                && *s != "BLOCKED"
                && !s.contains("\u{1b}")
        })
        .map(|s| s.to_string())
}
