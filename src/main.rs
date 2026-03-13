pub mod install;
pub mod sandbox_engine;
pub mod errors;
pub mod logger;
pub mod monitor;
pub mod config;
pub mod proxy;
pub mod optional_modules;
pub mod tui;

use clap::{Parser, Subcommand};
use crate::errors::LionError;
use anyhow::Context;

mod exit_codes {
    pub const INTERNAL_ERROR: i32 = 1;
}

#[derive(Parser)]
#[command(name = "lion")]
#[command(version = "0.1.0")]
#[command(
    about = "L.I.O.N \u{2014} Lightweight Isolated Orchestration Node",
    long_about = "Run any command inside a disposable Linux sandbox.\n\
                  Bubblewrap-powered: isolated namespaces, wiped environment,\n\
                  synthetic root \u{2014} cage is destroyed the moment the process exits."
)]
#[command(arg_required_else_help = true)]
#[command(propagate_version = true)]
#[command(help_template = "\
{before-help}{name} {version}
{about-with-newline}
{usage-heading} {usage}

{all-args}{after-help}
")]
#[command(after_help = "\
\x1b[1;96mEXAMPLES:\x1b[0m
  \x1b[90m# Basic isolation\x1b[0m
  lion run -- ls -la
  lion run -- python3 script.py

  \x1b[90m# Network modes\x1b[0m
  lion run --net=full  -- curl https://example.com
  lion run --net=allow -- npm install

  \x1b[90m# GUI apps\x1b[0m
  lion run --tui --gui -- gnome-text-editor

  \x1b[90m# Mount extra paths\x1b[0m
  lion run --ro /tmp -- python3 script.py

  \x1b[90m# Optional modules\x1b[0m
  lion saved status
  lion saved enable GPU
  lion run --optional GPU -- glxgears

\x1b[1;96mCONFIG:\x1b[0m
  Drop a \x1b[97mlion.toml\x1b[0m in your project root to set default mounts,
  network mode, and access level. Run \x1b[97mlion run\x1b[0m \u{2014} it auto-detects it.

\x1b[1;96mFIRST TIME?\x1b[0m
  sudo $(which lion) install     (one-time AppArmor setup)
")]
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
        /// Use '--' to separate lion flags from the command, e.g. 'lion run -- ls -la'.
        #[arg(last = true, required = true, value_name = "COMMAND")]
        cmd: Vec<String>,

        /// Network permission profile:
        ///   none   — no network access at all (default)
        ///   allow  — only domains in proxy.toml are reachable
        ///   full   — unrestricted internet access
        #[arg(long, value_name = "PROFILE", default_value = "none")]
        net: crate::sandbox_engine::network::NetworkMode,

        /// Print the generated bubblewrap command without executing it.
        /// Useful for inspecting the sandbox construction.
        #[arg(long, default_value_t = false)]
        dry_run: bool,

        /// Activate optional system modules by name (e.g. `--optional X11`).
        /// These override saved module states in saved.toml.
        /// Use 'lion saved status' to see all available modules.
        #[arg(long, value_name = "MODULE")]
        optional: Vec<String>,

        /// Enable detailed technical tracing logs in the main terminal.
        #[arg(long, default_value_t = false)]
        debug: bool,

        /// Mount a host directory as read-only inside the sandbox.
        /// Can be used multiple times: '--ro /bin --ro /lib/modules'.
        #[arg(long, value_name = "PATH")]
        ro: Vec<String>,

        /// Allow these domains through the network proxy (if active).
        /// Multiple domains can be specified: '--domain google.com --domain github.com' or comma-separated.
        /// Use '--domain *' to allow all (not recommended for strict sandboxing).
        #[arg(long = "domain", value_name = "DOMAIN", value_delimiter = ',')]
        domains: Vec<String>,

        /// Enable GUI support (shorthand for adding X11/desktop modules).
        #[arg(long, default_value_t = false)]
        gui: bool,

        /// Enable the Ratatui-based TUI for monitoring.
        #[arg(long, default_value_t = false)]
        tui: bool,
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

    /// Manage saved optional modules (saved.toml).
    Saved {
        #[command(subcommand)]
        sub: SavedCommands,
    },
}

#[derive(Subcommand)]
pub enum SavedCommands {
    /// Show all available modules with their current state.
    Status,
    /// Enable a saved module by name.
    Enable {
        /// Name of the module to enable.
        name: String,
    },
    /// Disable a saved module by name.
    Disable {
        /// Name of the module to disable.
        name: String,
    },
    /// Add a new module to the saved modules configuration.
    Add {
        /// Unique name for the module.
        #[arg(long)]
        name: String,
        /// Host path to mount.
        #[arg(long)]
        path: String,
        /// Initialize as enabled.
        #[arg(long, default_value_t = false)]
        enabled: bool,
    },
    /// Remove a saved module by name.
    Remove {
        /// Name of the module to remove.
        name: String,
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
            optional,
            debug,
            ro,
            domains,
            gui,
            tui,
        } => {
            // Initialize logging before starting the engine.
            if let Err(e) = logger::init_logging(*debug) {
                eprintln!("critical error: failed to initialize logger: {e}");
                std::process::exit(exit_codes::INTERNAL_ERROR);
            }

            let mut final_optional = optional.clone();
            if *gui {
                for m in ["X11", "Wayland", "GPU", "Fonts", "D-Bus"] {
                    if !final_optional.contains(&m.to_string()) {
                        final_optional.push(m.to_string());
                    }
                }
            }

            sandbox_engine::run_sandboxed(
                cmd.clone(),
                net.clone(),
                *dry_run,
                ro.clone(),
                domains.clone(),
                final_optional,
                *tui,
            )
            .map_err(Into::into)
        }
        Commands::Monitor { fifo, watch_paths } => {
            // Monitor mode doesn't need full logging init, it's the UI itself.
            monitor::run_monitor_subcommand(fifo.clone(), watch_paths.clone()).map_err(Into::into)
        }
        Commands::Saved { sub } => (|| {
            let project_dir = std::env::current_dir().context("failed to get current directory")?;
            match sub {
                SavedCommands::Status => optional_modules::status(&project_dir),
                SavedCommands::Enable { name } => optional_modules::enable(&project_dir, name),
                SavedCommands::Disable { name } => optional_modules::disable(&project_dir, name),
                SavedCommands::Add { name, path, enabled } => {
                    let state = if *enabled { 1 } else { 0 };
                    optional_modules::add(&project_dir, name.clone(), path.clone(), state)
                }
                SavedCommands::Remove { name } => optional_modules::remove(&project_dir, name),
            }
        })().map_err(Into::into),
    };

    // Handle any errors that bubbled up during execution
    if let Err(e) = result {
        // If it's a structured LionError, we give it a premium UI treatment.
        if let Some(lion_err) = e.downcast_ref::<LionError>() {
            let exit_code = extract_exit_code(&e);
            print_diagnostic_box(lion_err);
            
            match lion_err {
                LionError::ExecutionError(_) => eprintln!("❌ Command exited with failure"),
                _ => eprintln!("❌ Sandbox execution failed"),
            }
            print_failure_reason(lion_err, exit_code);
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

fn print_failure_reason(err: &LionError, exit_code: Option<i32>) {
    let reason: std::borrow::Cow<str> = match err {
        LionError::CommandNotFound(_) => "binary not found inside sandbox — check the command name".into(),
        LionError::PermissionDenied(_) => "executable permission missing".into(),
        LionError::ExecutionError(_) => match exit_code {
            // curl / wget: couldn't resolve host — almost always means no network
            Some(6) => "couldn't resolve host — sandbox network is disabled. Try: --net=allow or --net=full".into(),
            // curl: failed to connect
            Some(7) => "connection refused or unreachable — try: --net=full".into(),
            // curl: SSL/TLS error
            Some(35) => "SSL handshake failed inside sandbox — try: --net=full".into(),
            // generic non-zero
            Some(code) => format!("command exited with code {code} (check program output above)").into(),
            None => "command failed inside sandbox".into(),
        },
        _ => "internal sandbox setup failure".into(),
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
