//! `sandbox_engine/network.rs`
//!
//! Defines the available networking profiles for the sandbox.

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkMode {
    /// No network access (isolated network namespace with only loopback).
    None,
    /// DNS queries only (port 53).
    Dns,
    /// HTTP/HTTPS only (ports 80, 443).
    Http,
    /// Full internet access (shares host network namespace).
    Full,
}

impl Default for NetworkMode {
    fn default() -> Self {
        Self::None
    }
}
