//! `main.rs`
//!
//! This is the entry point for the LION CLI.
//! It defines the command-line interface using `clap` and routes execution
//! to the sandbox runner (`sandbox.rs`).

use clap::{Parser, Subcommand};

pub mod sandbox;

/// Predefined exit codes used by LION.
///
/// We map internal errors to `1`, CLI misuse to `2`, sandbox failures to `125`.
/// The actual command inside the sandbox will return its own nested exit code.
pub mod exit_codes {
    pub const SUCCESS: i32 = 0;
    pub const INTERNAL_ERROR: i32 = 1;
    pub const USAGE_ERROR: i32 = 2;
    pub const SANDBOX_SETUP_FAILED: i32 = 125;
    pub const COMMAND_NOT_EXECUTABLE: i32 = 126;
    pub const COMMAND_NOT_FOUND: i32 = 127;
}

#[derive(Parser)]
#[command(name = "lion")]
#[command(version = "0.1.0")]
#[command(
    about = "Lightweight filesystem sandbox for Linux",
    long_about = "LION is a lightweight, per-execution filesystem sandbox for Linux using bubblewrap."
)]
#[command(arg_required_else_help = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run a command inside a bubblewrap sandbox.
    Run {
        /// The executable and arguments to run inside the sandbox.
        #[arg(last = true, required = true)]
        cmd: Vec<String>,

        /// Allow network access inside the sandbox.
        #[arg(long, default_value_t = false)]
        network: bool,

        /// Print the bwrap command without executing it (for debugging).
        #[arg(long, default_value_t = false)]
        dry_run: bool,

        /// Enable GUI app support (exposes X11/Wayland/fonts).
        #[arg(long, default_value_t = false)]
        gui: bool,

        /// Activate optional modules by name (e.g. `--optional audio`).
        #[arg(long, value_name = "MODULE")]
        optional: Vec<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    // Route the command to the appropriate handler
    let result = match cli.command {
        Commands::Run {
            cmd,
            network,
            dry_run,
            gui,
            optional,
        } => sandbox::run_sandboxed(cmd, network, dry_run, gui, optional),
    };

    // Handle any errors that bubbled up during execution
    if let Err(e) = result {
        // If the error contains a forwarded exit code from the sandboxed process,
        // we exit with the exact same code so the user's shell can read it.
        if let Some(code) = extract_exit_code(&e) {
            std::process::exit(code);
        }

        // Otherwise it's an internal LION setup error.
        eprintln!("error: {e:#}");
        std::process::exit(exit_codes::INTERNAL_ERROR);
    }
}

/// Helper function to parse an exit code back out of a nested string error message.
fn extract_exit_code(e: &anyhow::Error) -> Option<i32> {
    let msg = format!("{e}");
    match msg.strip_prefix("exit code: ") {
        Some(rest) => rest.trim().parse::<i32>().ok(),
        None => None,
    }
}
