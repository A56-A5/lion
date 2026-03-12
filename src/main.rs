pub mod install;
pub mod sandbox_engine;
pub mod errors;
pub mod logger;
pub mod config;
pub mod profile;
pub mod monitor;

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

        /// Shorthand to enable network module
        #[arg(long)]
        network: bool,
        /// Shorthand to enable gpu module
        #[arg(long)]
        gpu: bool,
        /// Shorthand to enable wayland module
        #[arg(long)]
        wayland: bool,
        /// Shorthand to enable x11 module
        #[arg(long)]
        x11: bool,
        /// Shorthand to enable audio module
        #[arg(long)]
        audio: bool,
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

        /// Shorthand to disable network module
        #[arg(long)]
        network: bool,
        /// Shorthand to disable gpu module
        #[arg(long)]
        gpu: bool,
        /// Shorthand to disable wayland module
        #[arg(long)]
        wayland: bool,
        /// Shorthand to disable x11 module
        #[arg(long)]
        x11: bool,
        /// Shorthand to disable audio module
        #[arg(long)]
        audio: bool,
    },

    /// Show current exposure profile.
    Status,

    /// Run a command inside a bubblewrap sandbox.
    Run {
        /// The executable and arguments to run inside the sandbox.
        #[arg(last = true, required = true)]
        cmd: Vec<String>,

        /// Print the bwrap command without executing it (for debugging).
        #[arg(long, default_value_t = false)]
        dry_run: bool,

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
        Commands::Expose { path, module, domain, network, gpu, wayland, x11, audio } => (|| {
            if let Err(e) = logger::init_logging(false) {
                eprintln!("critical error: failed to initialize logger: {e}");
                std::process::exit(exit_codes::INTERNAL_ERROR);
            }
            
            // Collect all modules to expose
            let mut modules = Vec::new();
            if let Some(m) = module { modules.push(m.clone()); }
            if *network { modules.push("network".to_string()); }
            if *gpu { modules.push("gpu".to_string()); }
            if *wayland { modules.push("wayland".to_string()); }
            if *x11 { modules.push("x11".to_string()); }
            if *audio { modules.push("audio".to_string()); }

            if modules.is_empty() {
                profile::expose::handle_expose(path.clone(), None, domain.clone()).map_err(Into::into)
            } else {
                for m in modules {
                   profile::expose::handle_expose(None, Some(m), None)?;
                }
                if path.is_some() || domain.is_some() {
                    profile::expose::handle_expose(path.clone(), None, domain.clone())?;
                }
                Ok(())
            }
        })(),
        Commands::Unexpose { path, module, domain, network, gpu, wayland, x11, audio } => (|| {
            if let Err(e) = logger::init_logging(false) {
                eprintln!("critical error: failed to initialize logger: {e}");
                std::process::exit(exit_codes::INTERNAL_ERROR);
            }

            // Collect all modules to unexpose
            let mut modules = Vec::new();
            if let Some(m) = module { modules.push(m.clone()); }
            if *network { modules.push("network".to_string()); }
            if *gpu { modules.push("gpu".to_string()); }
            if *wayland { modules.push("wayland".to_string()); }
            if *x11 { modules.push("x11".to_string()); }
            if *audio { modules.push("audio".to_string()); }

            if modules.is_empty() {
                profile::unexpose::handle_unexpose(path.clone(), None, domain.clone()).map_err(Into::into)
            } else {
                for m in modules {
                   profile::unexpose::handle_unexpose(None, Some(m), None)?;
                }
                if path.is_some() || domain.is_some() {
                    profile::unexpose::handle_unexpose(path.clone(), None, domain.clone())?;
                }
                Ok(())
            }
        })(),
        Commands::Status => {
            profile::status::handle_status().map_err(Into::into)
        }
        Commands::Run {
            cmd,
            dry_run,
            debug,
        } => {
            // Initialize logging before starting the engine.
            if let Err(e) = logger::init_logging(*debug) {
                eprintln!("critical error: failed to initialize logger: {e}");
                std::process::exit(exit_codes::INTERNAL_ERROR);
            }
            sandbox_engine::run_sandboxed(
                cmd.clone(),
                *dry_run,
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
