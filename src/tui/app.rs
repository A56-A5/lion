//! `tui/app.rs`
//!
//! Holds the entire mutable state of the TUI and drives state transitions
//! in response to `TuiMsg` and keyboard input.

use super::events::{EventKind, PerfSnapshot, SandboxEvent, SandboxInfo, TuiMsg};
use std::collections::VecDeque;

// ── Constants ────────────────────────────────────────────────────────────────

/// Maximum number of log entries retained in memory.
pub const MAX_LOG_ENTRIES: usize = 2_000;
/// How many perf samples to keep for sparklines.
pub const SPARKLINE_LEN: usize = 60;

// ── Tab selection ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Events,
    Perf,
    Status,
}

impl Tab {
    pub const ALL: &'static [Tab] = &[Tab::Events, Tab::Perf, Tab::Status];

    pub fn title(self) -> &'static str {
        match self {
            Tab::Events => " Events ",
            Tab::Perf => " Perf   ",
            Tab::Status => " Status ",
        }
    }

    pub fn next(self) -> Self {
        match self {
            Tab::Events => Tab::Perf,
            Tab::Perf => Tab::Status,
            Tab::Status => Tab::Events,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            Tab::Events => Tab::Status,
            Tab::Perf => Tab::Events,
            Tab::Status => Tab::Perf,
        }
    }
}

// ── App state ────────────────────────────────────────────────────────────────

pub struct App {
    // ── Navigation ───────────────────────────────────────────────────────────
    pub current_tab: Tab,
    /// Whether the TUI has been asked to quit.
    pub should_quit: bool,

    // ── Event log ────────────────────────────────────────────────────────────
    pub log: VecDeque<SandboxEvent>,
    /// If true, the log view is "locked" at the bottom (auto-scroll).
    pub log_follow: bool,
    /// Vertical scroll offset for the log panel.
    pub log_scroll: usize,

    // ── Counters (for the status bar) ────────────────────────────────────────
    pub count_read: usize,
    pub count_write: usize,
    pub count_create: usize,
    pub count_delete: usize,
    pub count_blocked: usize,
    pub count_net_allow: usize,
    pub count_net_block: usize,

    // ── Perf ─────────────────────────────────────────────────────────────────
    pub latest_perf: Option<PerfSnapshot>,
    pub cpu_history: VecDeque<f64>,
    pub ram_history: VecDeque<u64>,

    // ── Sandbox metadata ─────────────────────────────────────────────────────
    pub sandbox_info: SandboxInfo,
    /// Wall-clock seconds since the sandbox started.
    pub elapsed_secs: u64,

    /// Whether the user has requested a force kill.
    pub force_kill_requested: bool,
}

impl App {
    pub fn new() -> Self {
        App {
            current_tab: Tab::Events,
            should_quit: false,
            log: VecDeque::with_capacity(MAX_LOG_ENTRIES),
            log_follow: true,
            log_scroll: 0,
            count_read: 0,
            count_write: 0,
            count_create: 0,
            count_delete: 0,
            count_blocked: 0,
            count_net_allow: 0,
            count_net_block: 0,
            latest_perf: None,
            cpu_history: VecDeque::with_capacity(SPARKLINE_LEN),
            ram_history: VecDeque::with_capacity(SPARKLINE_LEN),
            sandbox_info: SandboxInfo::default(),
            elapsed_secs: 0,
            force_kill_requested: false,
        }
    }

    // ── Message handling ─────────────────────────────────────────────────────

    pub fn handle_msg(&mut self, msg: TuiMsg) -> bool {
        match msg {
            TuiMsg::Log(ev) => {
                self.update_counters(&ev.kind);
                if self.log.len() >= MAX_LOG_ENTRIES {
                    self.log.pop_front();
                    if self.log_scroll > 0 {
                        self.log_scroll = self.log_scroll.saturating_sub(1);
                    }
                }
                self.log.push_back(ev);
                if self.log_follow {
                    self.log_scroll = self.log.len().saturating_sub(1);
                }
            }
            TuiMsg::Perf(snap) => {
                if self.cpu_history.len() >= SPARKLINE_LEN {
                    self.cpu_history.pop_front();
                }
                if self.ram_history.len() >= SPARKLINE_LEN {
                    self.ram_history.pop_front();
                }
                self.cpu_history.push_back(snap.cpu_pct);
                self.ram_history.push_back(snap.rss_kb);
                self.latest_perf = Some(snap);
            }
            TuiMsg::SandboxInfo(info) => {
                self.sandbox_info = info;
            }
            TuiMsg::Shutdown => {
                self.should_quit = true;
            }
            TuiMsg::KillRequested => {
                self.force_kill_requested = true;
            }
        }
        self.should_quit
    }

    pub fn tick(&mut self) {
        self.elapsed_secs += 1;
    }

    // ── Keyboard ─────────────────────────────────────────────────────────────

    pub fn on_key(&mut self, key: crossterm::event::KeyCode) -> bool {
        use crossterm::event::KeyCode as K;
        match key {
            K::Char('q') | K::Char('Q') => {
                self.should_quit = true;
                false
            }
            K::Char('K') => true, // SHIFT+K to trigger force kill
            K::Tab => {
                self.current_tab = self.current_tab.next();
                false
            }
            K::BackTab => {
                self.current_tab = self.current_tab.prev();
                false
            }
            K::Char('1') => {
                self.current_tab = Tab::Events;
                false
            }
            K::Char('2') => {
                self.current_tab = Tab::Perf;
                false
            }
            K::Char('3') => {
                self.current_tab = Tab::Status;
                false
            }
            K::Down | K::Char('j') => {
                self.log_follow = false;
                let max = self.log.len().saturating_sub(1);
                if self.log_scroll < max {
                    self.log_scroll += 1;
                }
                false
            }
            K::Up | K::Char('k') => {
                self.log_follow = false;
                self.log_scroll = self.log_scroll.saturating_sub(1);
                false
            }
            K::Char('G') | K::End => {
                self.log_follow = true;
                self.log_scroll = self.log.len().saturating_sub(1);
                false
            }
            K::Char('g') | K::Home => {
                self.log_follow = false;
                self.log_scroll = 0;
                false
            }
            K::Char('f') => {
                self.log_follow = !self.log_follow;
                if self.log_follow {
                    self.log_scroll = self.log.len().saturating_sub(1);
                }
                false
            }
            _ => false,
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn update_counters(&mut self, kind: &EventKind) {
        match kind {
            EventKind::Read => self.count_read += 1,
            EventKind::Write => self.count_write += 1,
            EventKind::Create => self.count_create += 1,
            EventKind::Delete => self.count_delete += 1,
            EventKind::Blocked => self.count_blocked += 1,
            EventKind::ProxyAllow => self.count_net_allow += 1,
            EventKind::ProxyBlock => self.count_net_block += 1,
            _ => {}
        }
    }

    pub fn elapsed_str(&self) -> String {
        let h = self.elapsed_secs / 3600;
        let m = (self.elapsed_secs % 3600) / 60;
        let s = self.elapsed_secs % 60;
        format!("{h:02}:{m:02}:{s:02}")
    }

    pub fn cpu_pct_u64(&self) -> u64 {
        self.latest_perf
            .as_ref()
            .map(|p| p.cpu_pct.round() as u64)
            .unwrap_or(0)
            .min(100)
    }

    pub fn ram_pct_u64(&self) -> u64 {
        let rss = self.latest_perf.as_ref().map(|p| p.rss_kb).unwrap_or(0);
        let max = self.ram_history.iter().copied().max().unwrap_or(1).max(1);
        ((rss * 100) / max).min(100)
    }

    pub fn cpu_spark_data(&self) -> Vec<u64> {
        self.cpu_history.iter().map(|&v| v.round() as u64).collect()
    }

    pub fn ram_spark_data(&self) -> Vec<u64> {
        let max = self.ram_history.iter().copied().max().unwrap_or(1).max(1);
        self.ram_history
            .iter()
            .map(|&v| (v * 100 / max).min(100))
            .collect()
    }

    pub fn ram_spark_data_absolute(&self) -> Vec<u64> {
        self.ram_history.iter().copied().collect()
    }
}
