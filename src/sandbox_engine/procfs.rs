//! `sandbox_engine/procfs.rs`
//!
//! Utilities for interacting with /proc to discover process trees and status.

use std::collections::{HashMap, HashSet};
use std::fs;

/// Resolves a list of all PIDs belonging to the same tree, starting from `root_pid`.
pub fn get_process_tree(root_pid: u32) -> Vec<u32> {
    let mut pids = vec![root_pid];
    let mut children_map: HashMap<u32, Vec<u32>> = HashMap::new();

    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            if let Ok(name) = entry.file_name().into_string() {
                if let Ok(pid) = name.parse::<u32>() {
                    if let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) {
                        if let Some(last_rparen) = stat.rfind(')') {
                            let fields: Vec<&str> = stat[last_rparen + 1..].split_whitespace().collect();
                            if fields.len() > 1 {
                                if let Ok(ppid) = fields[1].parse::<u32>() {
                                    children_map.entry(ppid).or_default().push(pid);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    let mut stack = vec![root_pid];
    let mut visited = HashSet::new();
    visited.insert(root_pid);

    while let Some(curr) = stack.pop() {
        if let Some(children) = children_map.get(&curr) {
            for &child in children {
                if visited.insert(child) {
                    pids.push(child);
                    stack.push(child);
                }
            }
        }
    }
    pids
}

/// Finds a direct child of the given PPID.
pub fn get_direct_child(ppid: u32) -> Option<u32> {
    if let Ok(entries) = fs::read_dir("/proc") {
        for entry in entries.flatten() {
            if let Ok(name) = entry.file_name().into_string() {
                if let Ok(pid) = name.parse::<u32>() {
                    let stat_path = format!("/proc/{pid}/stat");
                    if let Ok(stat) = fs::read_to_string(stat_path) {
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

/// Kills an entire process tree starting from `root_pid` with SIGKILL (-9).
pub fn kill_process_tree(root_pid: u32) {
    let pids = get_process_tree(root_pid);
    // Kill in reverse (bottom-up) so children don't become orphans before they can be killed.
    for pid in pids.iter().rev() {
        // We use the 'kill' command for simplicity and to avoid direct libc dependencies where possible.
        let _ = std::process::Command::new("kill")
            .arg("-9")
            .arg(pid.to_string())
            .stderr(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .status();
    }
}
