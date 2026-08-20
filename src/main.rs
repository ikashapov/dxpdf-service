//! dxpdf-service — HTTP wrapper around the dxpdf DOCX→PDF converter,
//! deployable as a native Windows service.
//!
//! Subcommands:
//! - `run`       — run the HTTP server in the foreground (any OS, Ctrl+C stops)
//! - `service`   — internal entry point invoked by the Windows SCM
//! - `install`   — register (auto-start) and start the Windows service
//! - `uninstall` — stop and remove the Windows service

mod server;
#[cfg(windows)]
mod winsvc;

use clap::{Args, Parser, Subcommand};
use server::ServerConfig;

pub const SERVICE_NAME: &str = "DxPdfService";
pub const SERVICE_DISPLAY_NAME: &str = "dxpdf DOCX to PDF converter";

#[derive(Parser)]
#[command(
    name = "dxpdf-service",
    version,
    about = "HTTP DOCX->PDF conversion service"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Args, Clone, Debug)]
pub struct ServeArgs {
    /// Address to bind
    #[arg(long, default_value = "0.0.0.0")]
    host: String,

    /// TCP port to listen on
    #[arg(long, default_value_t = 8080)]
    port: u16,

    /// Maximum accepted request body size, in megabytes
    #[arg(long, default_value_t = 100)]
    max_body_mb: usize,

    /// Maximum concurrent conversions (0 = number of CPUs)
    #[arg(long, default_value_t = 0)]
    concurrency: usize,

    /// Log file path (service mode logs here; foreground logs to stderr too)
    #[arg(long)]
    log_file: Option<std::path::PathBuf>,
}

impl ServeArgs {
    fn config(&self) -> ServerConfig {
        let cpus = std::thread::available_parallelism().map_or(2, |n| n.get());
        ServerConfig {
            host: self.host.clone(),
            port: self.port,
            max_body_mb: self.max_body_mb.max(1),
            concurrency: if self.concurrency == 0 {
                cpus
            } else {
                self.concurrency
            },
        }
    }

    /// Rebuilds the argument vector used to launch the SCM entry point, so
    /// `install` freezes the current settings into the service registration.
    fn to_service_args(&self) -> Vec<std::ffi::OsString> {
        let mut args: Vec<std::ffi::OsString> = vec![
            "service".into(),
            "--host".into(),
            self.host.clone().into(),
            "--port".into(),
            self.port.to_string().into(),
            "--max-body-mb".into(),
            self.max_body_mb.to_string().into(),
            "--concurrency".into(),
            self.concurrency.to_string().into(),
        ];
        if let Some(path) = &self.log_file {
            args.push("--log-file".into());
            args.push(path.clone().into());
        }
        args
    }
}

#[derive(Subcommand)]
enum Command {
    /// Run the HTTP server in the foreground
    Run(ServeArgs),
    /// Entry point used by the Windows Service Control Manager (internal)
    Service(ServeArgs),
    /// Install and start the Windows service with the given settings
    Install(ServeArgs),
    /// Stop and remove the Windows service
    Uninstall,
}

/// Initializes logging to stderr and, when given, a log file. Service mode has
/// no console, so the file is the only place its logs go.
fn init_logging(log_file: Option<&std::path::Path>) {
    let mut builder =
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"));
    if let Some(path) = log_file {
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
        {
            Ok(file) => {
                builder.target(env_logger::Target::Pipe(Box::new(file)));
            }
            Err(e) => eprintln!("warning: cannot open log file {}: {e}", path.display()),
        }
    }
    builder.init();
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Command::Run(args) => {
            init_logging(args.log_file.as_deref());
            let config = args.config();
            let runtime = tokio::runtime::Runtime::new().expect("failed to start tokio runtime");
            let result = runtime.block_on(server::serve(config, async {
                let _ = tokio::signal::ctrl_c().await;
                log::info!("Ctrl+C received, shutting down");
            }));
            if let Err(e) = result {
                eprintln!("server error: {e}");
                std::process::exit(1);
            }
        }
        Command::Service(mut args) => {
            #[cfg(windows)]
            {
                // A service has no console: without a log file, startup errors
                // (e.g. the port already being in use) would be invisible.
                // Default to a log next to the exe.
                if args.log_file.is_none() {
                    args.log_file = std::env::current_exe()
                        .ok()
                        .map(|exe| exe.with_file_name("dxpdf-service.log"));
                }
                init_logging(args.log_file.as_deref());
                if let Err(e) = winsvc::run(args) {
                    log::error!("service dispatcher failed: {e}");
                    std::process::exit(1);
                }
            }
            #[cfg(not(windows))]
            {
                let _ = args;
                eprintln!("`service` is only available on Windows; use `run`");
                std::process::exit(1);
            }
        }
        Command::Install(args) => {
            #[cfg(windows)]
            {
                if let Err(e) = winsvc::install(&args) {
                    eprintln!("install failed: {e}");
                    std::process::exit(1);
                }
                println!(
                    "service `{SERVICE_NAME}` installed and started (port {})",
                    args.port
                );
            }
            #[cfg(not(windows))]
            {
                let _ = args;
                eprintln!("`install` is only available on Windows");
                std::process::exit(1);
            }
        }
        Command::Uninstall => {
            #[cfg(windows)]
            {
                if let Err(e) = winsvc::uninstall() {
                    eprintln!("uninstall failed: {e}");
                    std::process::exit(1);
                }
                println!("service `{SERVICE_NAME}` removed");
            }
            #[cfg(not(windows))]
            {
                eprintln!("`uninstall` is only available on Windows");
                std::process::exit(1);
            }
        }
    }
}
