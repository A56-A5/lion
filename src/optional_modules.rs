//! `optional_modules.rs`
//!
//! Manages saved optional modules stored in `saved.toml`.
//! Modules can be added, removed, enabled, disabled, and listed via `lion saved`.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::fs;
use anyhow::{Context, Result};

/// Comment block written at the top of every saved.toml on each save.
/// TOML serialization strips comments, so we prepend this constant manually
/// to keep the file human-readable and self-documenting at all times.
const SAVED_TOML_HEADER: &str = concat!(
    "# ─────────────────────────────────────────────────────────────────────────────\n",
    "# L.I.O.N — saved.toml   (saved optional modules configuration)\n",
    "# ─────────────────────────────────────────────────────────────────────────────\n",
    "#\n",
    "# This file stores optional modules that can be dynamically enabled/disabled\n",
    "# for sandboxed processes. Each module defines additional mounts, environment\n",
    "# variables, and other resources to expose inside the sandbox.\n",
    "#\n",
    "# ── HOW IT WORKS ─────────────────────────────────────────────────────────────\n",
    "#\n",
    "# Modules can be activated in two complementary ways:\n",
    "#\n",
    "#   1. Permanent  — set  state = 1  in this file.\n",
    "#                  The module loads automatically on every  lion run.\n",
    "#\n",
    "#   2. One-shot   — pass  --optional <name>  on the command line.\n",
    "#                  The module loads only for that run, regardless of state here.\n",
    "#\n",
    "# Both methods are additive; they work together.\n",
    "#\n",
    "# ── MANAGING MODULES ──────────────────────────────────────────────────────────\n",
    "#\n",
    "#   lion saved status                    — list all modules + their state\n",
    "#   lion saved enable  <name>            — set state = 1 (auto-load every run)\n",
    "#   lion saved disable <name>            — set state = 0 (manual --optional only)\n",
    "#   lion saved add --name <n> --path <p> — add a simple directory module\n",
    "#   lion saved remove  <name>            — delete a module entry\n",
    "#\n",
    "# ── FIELDS ───────────────────────────────────────────────────────────────────\n",
    "#\n",
    "#   name    (required) — unique identifier used in CLI flags and log output\n",
    "#   state   (required) — 1 = always load, 0 = only when --optional is passed\n",
    "#   path    (optional) — simple single-path rw bind-mount shorthand\n",
    "#   mounts  (optional) — list of { src, dst, mode } mount specs:\n",
    "#                          src  — host path  (supports ${VAR} expansion)\n",
    "#                          dst  — path inside the sandbox\n",
    "#                          mode — \"ro\"  read-only bind\n",
    "#                                 \"rw\"  read-write bind\n",
    "#                                 \"dev\" character/block device bind\n",
    "#   env     (optional) — host env var names to forward into the sandbox\n",
    "#\n",
    "# ── ADDING YOUR OWN MODULE ────────────────────────────────────────────────────\n",
    "#\n",
    "# Simple directory mount (single path, read-write):\n",
    "#\n",
    "#   [[modules]]\n",
    "#   name  = \"my-data\"\n",
    "#   state = 0          # 0 = off by default; enable with: lion saved enable my-data\n",
    "#   path  = \"/path/to/host/directory\"\n",
    "#\n",
    "# Advanced module — multiple mounts, env forwarding, device nodes:\n",
    "#\n",
    "#   [[modules]]\n",
    "#   name  = \"my-module\"\n",
    "#   state = 0\n",
    "#   env   = [\"MY_VAR\", \"ANOTHER_VAR\"]\n",
    "#\n",
    "#   [[modules.mounts]]\n",
    "#   src  = \"${HOME}/.config/my-app\"    # ${VAR} expands from your environment\n",
    "#   dst  = \"${HOME}/.config/my-app\"\n",
    "#   mode = \"ro\"\n",
    "#\n",
    "#   [[modules.mounts]]\n",
    "#   src  = \"/dev/my-device\"\n",
    "#   dst  = \"/dev/my-device\"\n",
    "#   mode = \"dev\"\n",
    "#\n",
    "# ─────────────────────────────────────────────────────────────────────────────\n",
    "\n",
);

/// Resolves `${VAR}` placeholders in a path string using environment variables.
/// Handles any env var generically. Unknown vars expand to empty string.
pub fn resolve_vars(path: &str) -> String {
    let mut resolved = path.to_string();
    // Repeatedly scan for ${...} and replace each occurrence
    loop {
        if let Some(start) = resolved.find("${") {
            if let Some(end_offset) = resolved[start + 2..].find('}') {
                let end = start + 2 + end_offset;
                let var_name = resolved[start + 2..end].to_string();
                let val = std::env::var(&var_name).unwrap_or_default();
                resolved.replace_range(start..=end, &val);
            } else {
                break; // Unclosed brace — leave rest as-is
            }
        } else {
            break;
        }
    }
    resolved
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ModuleMount {
    pub src: String,
    pub dst: String,
    #[serde(default = "default_mount_mode")]
    pub mode: String, // "ro", "rw", "dev"
}

fn default_mount_mode() -> String {
    "ro".to_string()
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct OptionalModule {
    pub name: String,
    /// Simple single-path shorthand (rw bind-mount). Used by `lion saved add`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub mounts: Vec<ModuleMount>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env: Vec<String>,
    /// 1 = always load, 0 = only when --optional <name> is passed
    pub state: i32,
}

#[derive(Debug, Serialize, Deserialize, Default)]
pub struct OptionalModulesConfig {
    #[serde(default)]
    pub modules: Vec<OptionalModule>,
}

impl OptionalModulesConfig {
    pub fn load(project_dir: &Path) -> Result<Self> {
        let local_path = project_dir.join("saved.toml");
        
        // Target path: prefer local lion.toml, fallback to ~/.lion/saved.toml
        let path = if local_path.exists() {
            local_path
        } else {
            let home = std::env::var("HOME").unwrap_or_else(|_| "/tmp".to_string());
            PathBuf::from(home).join(".lion/saved.toml")
        };

        if !path.exists() {
            return Ok(Self::default());
        }

        let content = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        let config: Self = toml::from_str(&content)
            .with_context(|| format!("failed to parse {}", path.display()))?;
        
        if path.to_string_lossy().contains(".lion/saved.toml") {
            tracing::info!("Loaded global modules from ~/.lion/saved.toml");
        } else {
            tracing::info!("Loaded local modules from {}", path.display());
        }

        Ok(config)
    }

    pub fn save(&self, project_dir: &Path) -> Result<()> {
        let path = project_dir.join("saved.toml");
        let data = toml::to_string_pretty(self)
            .context("failed to serialize optional modules")?;
        // Prepend the static header — the TOML serializer discards comments,
        // so we write the header block ourselves to keep the file self-documenting.
        let content = format!("{}{}", SAVED_TOML_HEADER, data);
        fs::write(&path, content)
            .with_context(|| format!("failed to write {}", path.display()))?;
        Ok(())
    }
}

pub fn status(project_dir: &Path) -> Result<()> {
    let config = OptionalModulesConfig::load(project_dir)?;
    if config.modules.is_empty() {
        println!("No saved modules configured.");
        println!("\nEdit saved.toml directly, or add a module with:");
        println!("  lion saved add --name <name> --path <path>");
        return Ok(());
    }

    println!("\u{1f981} Saved Optional Modules ({})\n", project_dir.display());
    println!("{:<4} {:<20} {:<12} {}", "No.", "Name", "Active", "Path / Mounts");
    println!("{}", "\u{2500}".repeat(72));

    for (idx, m) in config.modules.iter().enumerate() {
        let (active_label, active_pad) = if m.state == 1 {
            ("\x1b[1;32m\u{2713} enabled\x1b[0m", "  ")
        } else {
            ("\x1b[1;31m\u{2717} disabled\x1b[0m", " ")
        };

        let path_info = if let Some(p) = &m.path {
            p.clone()
        } else if !m.mounts.is_empty() {
            format!("{} mount(s)", m.mounts.len())
        } else {
            "no mounts".to_string()
        };

        println!(
            "{:<4} {:<20} {}{} {}",
            idx + 1,
            m.name,
            active_label,
            active_pad,
            path_info
        );
    }

    let enabled = config.modules.iter().filter(|m| m.state == 1).count();
    println!(
        "\n  {}/{} module(s) enabled",
        enabled,
        config.modules.len()
    );
    println!("\n\u{1f4a1} Tip: 'lion saved enable <name>' / 'lion saved disable <name>' to toggle");
    println!("     'lion run --optional <name> -- <cmd>' to activate for one run only");
    Ok(())
}

/// Add a simple single-path module via the CLI.
/// Uses the `path` shorthand field; the runner binds it rw.
pub fn add(project_dir: &Path, name: String, path: String, state: i32) -> Result<()> {
    let mut config = OptionalModulesConfig::load(project_dir)?;
    if config.modules.iter().any(|m| m.name == name) {
        anyhow::bail!(
            "Module '{}' already exists. Use 'lion saved status' to list modules.",
            name
        );
    }

    config.modules.push(OptionalModule {
        name: name.clone(),
        path: Some(path),
        mounts: Vec::new(),
        env: Vec::new(),
        state,
    });
    config.save(project_dir)?;
    println!(
        "\u{2705} Added module '{}' ({})",
        name,
        if state == 1 { "enabled" } else { "disabled" }
    );
    Ok(())
}

pub fn remove(project_dir: &Path, name: &str) -> Result<()> {
    let mut config = OptionalModulesConfig::load(project_dir)?;
    let before = config.modules.len();
    config.modules.retain(|m| m.name != name);

    if config.modules.len() == before {
        anyhow::bail!(
            "Module '{}' not found. Use 'lion saved status' to list modules.",
            name
        );
    }

    config.save(project_dir)?;
    println!("\u{2705} Removed module '{}'", name);
    Ok(())
}

pub fn enable(project_dir: &Path, name: &str) -> Result<()> {
    set_state(project_dir, name, 1)
}

pub fn disable(project_dir: &Path, name: &str) -> Result<()> {
    set_state(project_dir, name, 0)
}

/// Internal helper — shared by `enable` and `disable`.
fn set_state(project_dir: &Path, name: &str, new_state: i32) -> Result<()> {
    let mut config = OptionalModulesConfig::load(project_dir)?;

    let module = config
        .modules
        .iter_mut()
        .find(|m| m.name == name)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Module '{}' not found. Use 'lion saved status' to list modules.",
                name
            )
        })?;

    let label = if new_state == 1 { "enabled" } else { "disabled" };

    if module.state == new_state {
        println!("\u{2139}\u{fe0f}  Module '{}' is already {}", name, label);
    } else {
        module.state = new_state;
        config.save(project_dir)?;
        println!("\u{2705} Module '{}' {}", name, label);
    }

    Ok(())
}
