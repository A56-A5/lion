//! `errors.rs`
//!
//! Defines the structured error types for L.I.O.N using `thiserror`.

use std::path::PathBuf;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LionError {
    #[error("dependency missing: {0}")]
    DependencyMissing(String),

    #[error("Command not found: {0}\nThe executable could not be located in PATH.")]
    CommandNotFound(String),

    #[error("Permission denied: {0}\nFile exists but is not executable.")]
    PermissionDenied(String),

    #[error("Command failed inside sandbox\nExit code: {0}")]
    ExecutionError(i32),

    #[error("failed to mount {path}: {source}")]
    MountError { path: PathBuf, source: std::io::Error },

    #[error("namespace setup failed: {0}")]
    NamespaceError(String),

    #[error("unauthorized operation: {0}")]
    Unauthorized(String),

    #[error("environment error: {0}")]
    EnvironmentError(String),

    #[error("internal error: {0}")]
    Internal(String),
}

pub type Result<T> = std::result::Result<T, LionError>;
