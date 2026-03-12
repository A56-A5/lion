//! `logger.rs`
//!
//! Handles initialization of the `tracing` diagnostic framework.
//! Provides dual-target logging: stderr (formatted) and file (full trace).

use std::path::PathBuf;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub fn init_logging(debug: bool) -> anyhow::Result<()> {
    // 1. File Logging (Full Trace)
    // Logs are stored in ~/.lion/logs/last-run.log
    let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
    let log_dir = PathBuf::from(home).join(".lion/logs");
    
    // Ensure log directory exists
    std::fs::create_dir_all(&log_dir)?;

    let file_appender = tracing_appender::rolling::never(log_dir, "last-run.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);

    // Forget the guard - we want it to live for the duration of the program.
    // In a production app, you might want to manage this properly.
    std::mem::forget(_guard);

    // 2. Console Logging (Actionable)
    let stderr_level = if debug { "debug" } else { "info" };
    
    let stderr_layer = fmt::layer()
        .with_writer(std::io::stderr)
        .with_target(false)
        .with_thread_ids(false)
        .with_file(false)
        .with_line_number(false)
        .with_filter(EnvFilter::new(stderr_level));

    let file_layer = fmt::layer()
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_filter(EnvFilter::new("trace"));

    tracing_subscriber::registry()
        .with(stderr_layer)
        .with(file_layer)
        .init();

    Ok(())
}
