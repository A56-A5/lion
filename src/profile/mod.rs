//! `profile/mod.rs`
//!
//! Defines the Profile structure which holds user-defined exposure settings.

pub mod store;
pub mod validator;
pub mod resolver;

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Profile {
    /// List of enabled system modules (e.g. "gpu", "network").
    pub modules: HashSet<String>,
    /// List of custom host paths exposed as read-write binds.
    pub custom_paths: HashSet<String>,
    /// List of allowed network domains.
    pub allowed_domains: HashSet<String>,
}

impl Default for Profile {
    fn default() -> Self {
        Self {
            modules: HashSet::new(),
            custom_paths: HashSet::new(),
            allowed_domains: HashSet::new(),
        }
    }
}
