//! `sandbox_engine/network.rs`
//!
//! Defines the available networking profiles for the sandbox.

use clap::ValueEnum;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NetworkMode {
    /// No network access at all (default). Isolated network namespace.
    None,
    /// Only domains listed in proxy.toml are reachable (HTTP/HTTPS proxy filter).
    Allow,
    /// Full unrestricted internet access (shares host network namespace).
    Full,
}

impl Default for NetworkMode {
    fn default() -> Self {
        Self::None
    }
}
