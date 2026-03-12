use std::io::{BufRead, BufReader};
use std::process::ChildStderr;

use chrono::Local;

/// Read stderr from the sandboxed process line-by-line and print live events.
pub fn watch(stderr: ChildStderr, ro_paths: Vec<String>) {
    let reader = BufReader::new(stderr);
    watch_buffered(reader, ro_paths);
}

/// Read events from any BufRead (e.g. FIFO) and print live events.
pub fn watch_buffered<R: BufRead>(reader: R, ro_paths: Vec<String>) {
    print_banner(&ro_paths);

    for line in reader.lines() {
        match line {
            Ok(line) if !line.trim().is_empty() => print_event(&line),
            _ => {}
        }
    }
}

fn print_banner(ro_paths: &[String]) {
    eprintln!("\x1b[1m\x1b[34m╔══════════════════════════════════════════════════╗\x1b[0m");
    eprintln!("\x1b[1m\x1b[34m║  LION MONITOR  ·  live sandbox events            ║\x1b[0m");
    if ro_paths.is_empty() {
        eprintln!("\x1b[1m\x1b[34m║  no read-only paths configured                   ║\x1b[0m");
    } else {
        for path in ro_paths {
            eprintln!("\x1b[1m\x1b[34m║  \x1b[33mRO\x1b[34m  \x1b[0m{:<44}\x1b[1m\x1b[34m║\x1b[0m", path);
        }
    }
    eprintln!("\x1b[1m\x1b[34m╚══════════════════════════════════════════════════╝\x1b[0m");
}

fn print_event(line: &str) {
    let now = Local::now().format("%H:%M:%S");

    let (tag, color) = classify(line);
    let path = extract_path(line);
    let reason = extract_reason(line);

    match path {
        Some(p) => eprintln!(
            "[LION] {}  {}{}{}  \x1b[1m{}\x1b[0m  \x1b[90m({})\x1b[0m",
            now, color, tag, "\x1b[0m", p, reason
        ),
        None => eprintln!(
            "[LION] {}  {}{}{}  \x1b[90m{}\x1b[0m",
            now, color, tag, "\x1b[0m", line
        ),
    }
}

fn classify(line: &str) -> (&'static str, &'static str) {
    if line.contains("Read-only file system")
        || line.contains("Permission denied")
        || line.contains("Operation not permitted")
    {
        ("BLOCKED", "\x1b[1m\x1b[31m") // bold red
    } else if line.contains("No such file or directory") {
        ("MISSING", "\x1b[1m\x1b[33m") // bold yellow
    } else {
        ("info   ", "\x1b[90m")         // grey
    }
}

/// Pull the first path-like token (starts with '/') out of an error line.
fn extract_path(line: &str) -> Option<String> {
    // typical format: "program: /some/path: Reason"
    // or:             "program: error message '/some/path': Reason"
    let cleaned = line.trim_start_matches(|c: char| !c.is_whitespace()); // skip program name
    for token in cleaned.split_whitespace() {
        let t = token.trim_matches(|c| c == '\'' || c == '"' || c == ':');
        if t.starts_with('/') {
            return Some(t.to_string());
        }
    }
    None
}

/// Pull the human-readable reason (last colon-delimited segment).
fn extract_reason(line: &str) -> String {
    line.rsplit(':').next().map(|s| s.trim().to_string()).unwrap_or_default()
}
