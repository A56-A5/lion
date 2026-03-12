pub mod install;
pub mod sandbox_engine;
pub mod errors;
pub mod logger;
pub mod config;
pub mod profile;

use clap::{Parser, Subcommand};
use crate::errors::LionError;

// ... (exit_codes)
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

    /// Add a writable path, enable a module, or allow a domain.
    Expose {
        /// Host path to expose read-write.
        path: Option<String>,
        /// Module to enable (e.g. gpu, network).
        #[arg(long)]
        module: Option<String>,
        /// Domain to allow (e.g. google.com).
        #[arg(long)]
        domain: Option<String>,
    },

    /// Remove a writable path, disable a module, or restrict a domain.
    Unexpose {
        /// Host path to remove.
        path: Option<String>,
        /// Module to disable.
        #[arg(long)]
        module: Option<String>,
        /// Domain to restrict.
        #[arg(long)]
        domain: Option<String>,
    },

    /// Show current exposure profile.
    Status,

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
    },
}

fn main() {
    let cli = Cli::parse();

    // Route the command to the appropriate handler
    let result: anyhow::Result<()> = match &cli.command {
        Commands::Install => install::run_install().map_err(Into::into),
        Commands::Expose { path, module, domain } => {
            if let Err(e) = logger::init_logging(false) {
                eprintln!("critical error: failed to initialize logger: {e}");
                std::process::exit(exit_codes::INTERNAL_ERROR);
            }
            profile::expose::handle_expose(path.clone(), module.clone(), domain.clone()).map_err(Into::into)
        }
        Commands::Unexpose { path, module, domain } => {
            if let Err(e) = logger::init_logging(false) {
                eprintln!("critical error: failed to initialize logger: {e}");
                std::process::exit(exit_codes::INTERNAL_ERROR);
            }
            profile::unexpose::handle_unexpose(path.clone(), module.clone(), domain.clone()).map_err(Into::into)
        }
        Commands::Status => {
            profile::status::handle_status().map_err(Into::into)
        }
        Commands::Run {
            cmd,
            net,
            dry_run,
            gui,
            optional,
            debug,
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
            )
            .map_err(Into::into)
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
        // Success case
        println!("✔ Command executed successfully");
        print_success_reason(&cli);
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

fn print_success_reason(cli: &Cli) {
    if let Commands::Run { cmd, .. } = &cli.command {
        let program = cmd.first().map(|s| s.as_str()).unwrap_or("");
        let reason = match program {
            "pwd" => "working directory set via --chdir and project directory is bind-mounted inside sandbox",
            _ => "command executed normally inside sandbox with project directory mounted",
        };
        println!("(reason: {})", reason);
    }
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
