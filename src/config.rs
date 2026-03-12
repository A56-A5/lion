//! `config.rs`
//!
//! Reads an optional `lion.toml` from the current project directory.
//! If the file is absent, all defaults apply — nothing breaks.

use serde::Deserialize;
use std::path::Path;

/// Top-level config loaded from `lion.toml`.
#[derive(Debug, Deserialize, Default)]
pub struct LionConfig {
    #[serde(default)]
    pub sandbox: SandboxConfig,

    /// Extra mount entries declared as `[[mount]]` blocks.
    #[serde(default)]
    pub mount: Vec<MountEntry>,
}

/// `[sandbox]` section.
#[derive(Debug, Deserialize)]
pub struct SandboxConfig {
    /// How the project directory itself is mounted: "ro" or "rw".
    /// Defaults to "rw".
    #[serde(default = "default_rw")]
    pub project_access: String,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self { project_access: default_rw() }
    }
}

fn default_rw() -> String {
    "rw".to_string()
}

/// One `[[mount]]` entry.
#[derive(Debug, Deserialize)]
pub struct MountEntry {
    /// Absolute or `~`-prefixed path on the host.
    pub path: String,
    /// "ro" for read-only, "rw" for read-write.
    pub access: String,
}

impl MountEntry {
    /// Resolve `~` to the real home directory.
    pub fn resolved_path(&self) -> String {
        if self.path.starts_with('~') {
            if let Ok(home) = std::env::var("HOME") {
                return self.path.replacen('~', &home, 1);
            }
        }
        self.path.clone()
    }

    pub fn is_readonly(&self) -> bool {
        self.access.trim().to_lowercase() == "ro"
    }
}

impl LionConfig {
    /// Whether the project directory should be read-only.
    pub fn project_is_readonly(&self) -> bool {
        self.sandbox.project_access.trim().to_lowercase() == "ro"
    }
}

/// Load `lion.toml` from `project_dir`. Returns default config if file is absent.
pub fn load(project_dir: &Path) -> LionConfig {
    let path = project_dir.join("lion.toml");
    if !path.exists() {
        return LionConfig::default();
    }

    match std::fs::read_to_string(&path) {
        Ok(contents) => match toml::from_str::<LionConfig>(&contents) {
            Ok(cfg) => {
                tracing::info!("Loaded lion.toml from {}", project_dir.display());
                cfg
            }
            Err(e) => {
                tracing::warn!("lion.toml parse error: {e} — using defaults");
                LionConfig::default()
            }
        },
        Err(e) => {
            tracing::warn!("Could not read lion.toml: {e} — using defaults");
            LionConfig::default()
        }
    }
}
