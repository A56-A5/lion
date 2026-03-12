pub mod install;
pub mod sandbox_engine;
pub mod errors;
pub mod logger;
pub mod monitor;
pub mod config;
pub mod proxy;

use clap::{Parser, Subcommand};
use crate::errors::LionError;

mod exit_codes {
    pub const INTERNAL_ERROR: i32 = 1;
}

#[derive(Parser)]
// ... Cli struct
#[command(name = "lion")]
#[command(version = "0.1.0")]
#[command(
    about = "L.I.O.N: Lightweight Isolated Orchestration Node",
    long_about = "L.I.O.N is a per-execution filesystem sandbox for Linux using bubblewrap. \
                  It builds a synthetic root, wipes the environment, and creates a fresh \
                  independent namespace cage for every command. Perfect for running \
                  untrusted code, isolating builds, or analyzing file access patterns."
)]
#[command(arg_required_else_help = true)]
#[command(propagate_version = true)]
#[command(help_template = "\
{before-help}{name} {version}
{author-with-newline}{about-with-newline}
{usage-heading} {usage}

{all-args}{after-help}

EXAMPLES:
    lion run -- ls -la                  Run 'ls' in isolated environment
    lion run --net=full -- curl google.com     Full internet access
    lion run --net=allow -- npm install         Only proxy.toml domains allowed
    lion run --gui -- xclock            GUI support enabled
    lion run --ro /tmp -- python script.py    Mount host /tmp as read-only
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

        /// Enable GUI app support.
        /// Exposes X11/Wayland sockets, fonts, and GPU drivers.
        #[arg(long, default_value_t = false)]
        gui: bool,

        /// Activate optional system modules by name (e.g. `--optional audio`).
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
            optional: _,
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
