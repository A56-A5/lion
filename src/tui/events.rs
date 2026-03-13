//! `tui/events.rs`
//!
//! Defines the data structures used for communicating with the TUI.

use chrono::{DateTime, Local};

/// High-level message types sent to the TUI event loop.
#[derive(Debug, Clone)]
pub enum TuiMsg {
    /// A new sandbox event (log entry).
    Log(SandboxEvent),
    /// A performance snapshot.
    Perf(PerfSnapshot),
    /// Update the sandbox metadata.
    SandboxInfo(SandboxInfo),
    /// Signal the TUI to shutdown.
    Shutdown,
}

impl TuiMsg {
    pub fn is_kill(&self) -> bool {
        false
    }
}

/// The kind of activity being reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Read,
    Write,
    Create,
    Delete,
    Blocked,
    Missing,
    ProxyAllow,
    ProxyBlock,
    Info,
}

impl EventKind {
    pub fn label(self) -> &'static str {
        match self {
            EventKind::Read       => "READ   ",
            EventKind::Write      => "WRITE  ",
            EventKind::Create     => "CREATE ",
            EventKind::Delete     => "DELETE ",
            EventKind::Blocked    => "BLOCKED",
            EventKind::Missing    => "MISSING",
            EventKind::ProxyAllow => "NET-OK ",
            EventKind::ProxyBlock => "NET-BL ",
            EventKind::Info       => "INFO   ",
        }
    }
}

/// A single log entry in the TUI dashboard.
#[derive(Debug, Clone)]
pub struct SandboxEvent {
    pub timestamp: DateTime<Local>,
    pub kind:      EventKind,
    pub path:      Option<String>,
    pub raw:       String,
}

impl SandboxEvent {
    pub fn new(kind: EventKind, path: Option<String>, raw: impl Into<String>) -> Self {
        Self {
            timestamp: Local::now(),
            kind,
            path,
            raw: raw.into(),
        }
    }

    pub fn info(msg: impl Into<String>) -> Self {
        Self::new(EventKind::Info, None, msg)
    }
}

/// Performance data aggregated across the sandbox process tree.
#[derive(Debug, Clone, Default)]
pub struct PerfSnapshot {
    pub cpu_pct:     f64,
    pub rss_kb:      u64,
    pub vmsz_kb:     u64,
    pub threads:     u32,
    pub io_read_kb:  u64,
    pub io_write_kb: u64,
    pub state:       char, // R, S, D, etc.
    pub processes:   Vec<ProcessInfo>,
}

#[derive(Debug, Clone, Default)]
pub struct ProcessInfo {
    pub pid:  u32,
    pub comm: String,
    pub cpu:  f64,
    pub mem:  u64,
}

/// Static metadata about the running sandbox.
#[derive(Debug, Clone, Default)]
pub struct SandboxInfo {
    pub command:      Vec<String>,
    pub network_mode: String,
    pub pid:          u32,
    pub started_at:   Option<DateTime<Local>>,
    pub project_access: String,
    pub exposed_paths: Vec<String>,
    pub active_modules: Vec<String>,
    pub is_home_exposed: bool,
}

impl SandboxInfo {
    pub fn command_str(&self) -> String {
        self.command.join(" ")
    }
}
