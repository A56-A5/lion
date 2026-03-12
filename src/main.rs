pub mod install;
pub mod sandbox_engine;
pub mod errors;
pub mod logger;
pub mod monitor;
pub mod config;
pub mod proxy;

use clap::{Parser, Subcommand};
use crate::errors::LionError;

/// Predefined exit codes used by L.I.O.N.
/// ... (rest of mod exit_codes)
pub mod exit_codes {
    pub const SUCCESS: i32 = 0;
    pub const INTERNAL_ERROR: i32 = 1;
    pub const USAGE_ERROR: i32 = 2;
    pub const SANDBOX_SETUP_FAILED: i32 = 125;
    pub const COMMAND_NOT_EXECUTABLE: i32 = 126;
    pub const COMMAND_NOT_FOUND: i32 = 127;
}

#[derive(Parser)]
// ... Cli struct
#[command(name = "lion")]
#[command(version = "0.1.0")]
#[command(
    about = "Lightweight filesystem sandbox for Linux",
    long_about = "L.I.O.N is a lightweight, per-execution filesystem sandbox for Linux using bubblewrap."
)]
#[command(arg_required_else_help = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Perform one-time system setup (requires sudo).
    Install,

    /// Run a command inside a bubblewrap sandbox.
    Run {
        /// The executable and arguments to run inside the sandbox.
        #[arg(last = true, required = true)]
        cmd: Vec<String>,

        /// Network permission profile.
        #[arg(long, value_name = "PROFILE", default_value = "none")]
        net: crate::sandbox_engine::network::NetworkMode,

        /// Print the bwrap command without executing it (for debugging).
        #[arg(long, default_value_t = false)]
        dry_run: bool,

        /// Enable GUI app support (exposes X11/Wayland/fonts).
        #[arg(long, default_value_t = false)]
        gui: bool,

        /// Activate optional modules by name (e.g. `--optional audio`).
        #[arg(long, value_name = "MODULE")]
        optional: Vec<String>,

        /// Enable detailed technical logging in the terminal.
        #[arg(long, default_value_t = false)]
        debug: bool,

        /// Mount a directory as read-only inside the sandbox (repeatable, e.g. --ro /home/user/docs).
        #[arg(long, value_name = "PATH")]
        ro: Vec<String>,

        /// Domains the proxy will allow through (requires --net=http or --net=full).
        /// Use '*' to allow all. Repeatable: --domain google.com --domain api.github.com
        #[arg(long = "domain", value_name = "DOMAIN", value_delimiter = ',')]
        domains: Vec<String>,
    },

    /// INTERNAL: Listen for events on a FIFO and print them.
    #[command(hide = true)]
    Monitor {
        /// The FIFO path to read from.
        fifo: String,
        /// The watch paths for the banner.
        #[arg(long)]
        watch_paths: Vec<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    // Route the command to the appropriate handler
    let result: anyhow::Result<()> = match &cli.command {
        Commands::Install => install::run_install().map_err(Into::into),
        Commands::Run {
            cmd,
            net,
            dry_run,
            gui,
            optional,
            debug,
            ro,
            domains,
        } => {
            // Initialize logging before starting the engine.
            if let Err(e) = logger::init_logging(*debug) {
                eprintln!("critical error: failed to initialize logger: {e}");
                std::process::exit(exit_codes::INTERNAL_ERROR);
            }
            sandbox_engine::run_sandboxed(
                cmd.clone(),
                net.clone(),
                *dry_run,
                *gui,
                optional.clone(),
                ro.clone(),
                domains.clone(),
            )
            .map_err(Into::into)
        }
        Commands::Monitor { fifo, watch_paths } => {
            // Monitor mode doesn't need full logging init, it's the UI itself.
            monitor::run_monitor_subcommand(fifo.clone(), watch_paths.clone()).map_err(Into::into)
        }
    };

    // Handle any errors that bubbled up during execution
    if let Err(e) = result {
        // If it's a structured LionError, we give it a premium UI treatment.
        if let Some(lion_err) = e.downcast_ref::<LionError>() {
            print_diagnostic_box(lion_err);
            
            match lion_err {
                LionError::ExecutionError(_) => eprintln!("❌ Command exited with failure"),
                _ => eprintln!("❌ Sandbox execution failed"),
            }
            print_failure_reason(lion_err);
        } else {
            // Otherwise it's an internal L.I.O.N setup error.
            eprintln!("error: {e:#}");
        }

        // Forward exit code
        if let Some(code) = extract_exit_code(&e) {
            std::process::exit(code);
        }
        std::process::exit(exit_codes::INTERNAL_ERROR);
    } else {
        eprintln!("\x1b[90m[LION] sandbox exited cleanly — cage destroyed\x1b[0m");
    }
}

/// Renders an ASCII diagnostic box for Lion failures.
fn print_diagnostic_box(err: &LionError) {
    eprintln!("\n+----------------------------------------------------------+");
    eprintln!("| LION ERROR                                               |");
    eprintln!("+----------------------------------------------------------+");
    
    let msg = format!("{}", err);
    for line in msg.lines() {
        eprintln!("| {:<56} |", line);
    }
    
    // Optional Hint for Permission Denied
    if let LionError::PermissionDenied(path) = err {
        eprintln!("+----------------------------------------------------------+");
        eprintln!("| Try: chmod +x {:<42} |", path);
    }
    
    eprintln!("+----------------------------------------------------------+");
    eprintln!("| See log for details: ~/.lion/logs/last-run.log           |");
    eprintln!("+----------------------------------------------------------+");
}



fn print_failure_reason(err: &LionError) {
    let reason = match err {
        LionError::CommandNotFound(_) => "bubblewrap execvp failed because the binary does not exist",
        LionError::PermissionDenied(_) => "executable permission missing",
        LionError::ExecutionError(_) => "command executed but failed internally due to missing file",
        _ => "internal sandbox setup failure",
    };
    eprintln!("(reason: {})", reason);
}

/// Helper function to parse an exit code back out of a nested string error message.
fn extract_exit_code(e: &anyhow::Error) -> Option<i32> {
    let msg = format!("{e}");
    match msg.strip_prefix("exit code: ") {
        Some(rest) => rest.trim().parse::<i32>().ok(),
        None => None,
    }
}
