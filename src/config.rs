//! `config.rs`
//!
//! Config loading for L.I.O.N.
//!
//! Two config files are supported and merged at runtime:
//!
//!   1. **Global**  — `~/.config/lion/lion.toml`
//!      Always loaded.  Put persistent cross-project mounts here
//!      (e.g. ~/flutter, ~/Test).
//!
//!   2. **Project** — `<cwd>/lion.toml`
//!      Project-specific overrides.  Its `[sandbox]` settings take
//!      precedence over the global ones; its `[[mount]]` entries are
//!      appended to (not replacing) the global list.

use serde::Deserialize;
use std::path::Path;

/// Top-level config loaded from a `lion.toml`.
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
    /// Defaults to "ro".
    #[serde(default = "default_ro")]
    pub project_access: String,

    /// How the project's `src/` subdirectory is mounted when the project is rw.
    /// Overlays a read-only bind on top of the rw project mount.
    /// Defaults to "ro" — source code is protected from accidental writes.
    #[serde(default = "default_ro")]
    pub src_access: String,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            project_access: default_ro(),
            src_access: default_ro(),
        }
    }
}

fn default_ro() -> String {
    "ro".to_string()
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

    /// Whether the project's `src/` subdirectory should be overlaid as read-only.
    /// Only has effect when the project itself is mounted read-write.
    pub fn src_is_readonly(&self) -> bool {
        self.sandbox.src_access.trim().to_lowercase() == "ro"
    }
}

// ── Loaders ──────────────────────────────────────────────────────────────────

/// Parse a single `lion.toml` file.  Returns `None` if the file doesn't exist.
fn parse_file(path: &Path) -> Option<LionConfig> {
    if !path.exists() {
        return None;
    }
    match std::fs::read_to_string(path) {
        Ok(contents) => match toml::from_str::<LionConfig>(&contents) {
            Ok(cfg) => {
                tracing::info!("Loaded {}", path.display());
                Some(cfg)
            }
            Err(e) => {
                tracing::warn!("{}: parse error: {e} — ignoring", path.display());
                None
            }
        },
        Err(e) => {
            tracing::warn!("Could not read {}: {e}", path.display());
            None
        }
    }
}

/// Load the **global** config from `~/.config/lion/lion.toml`.
pub fn load_global() -> LionConfig {
    let path = std::env::var("HOME")
        .map(|h| std::path::PathBuf::from(h).join(".config/lion/lion.toml"))
        .ok();

    path.and_then(|p| parse_file(&p)).unwrap_or_default()
}

/// Load `lion.toml` from `project_dir`. Returns default config if file is absent.
pub fn load(project_dir: &Path) -> LionConfig {
    let path = project_dir.join("lion.toml");
    parse_file(&path).unwrap_or_default()
}

/// **Primary entry point.**  Loads global + project configs and merges them.
///
/// Merge rules:
/// - `[sandbox]` settings come from the **project** config if present,
///   otherwise from global; otherwise defaults.
/// - `[[mount]]` lists are **concatenated** (global first, project second).
///   Duplicate paths are deduplicated by keeping the last entry (project wins).
pub fn load_merged(project_dir: &Path) -> LionConfig {
    let global = load_global();
    let project = load(project_dir);

    // Sandbox settings: project overrides global overrides default.
    // Detect whether a project lion.toml actually set non-default values by
    // comparing against the default string. If the project file was absent its
    // fields are already the default, so the global values should be used.
    let project_file_exists = project_dir.join("lion.toml").exists();
    let sandbox = if project_file_exists {
        project.sandbox
    } else {
        global.sandbox
    };

    // Mounts: global first, then project, dedup by path (last wins).
    let mut seen = std::collections::HashSet::new();
    let all_mounts: Vec<MountEntry> = global
        .mount
        .into_iter()
        .chain(project.mount)
        .rev()                          // reverse so last (project) wins on dedup
        .filter(|m| seen.insert(m.path.clone()))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()                          // restore original order
        .collect();

    LionConfig {
        sandbox,
        mount: all_mounts,
    }
}

