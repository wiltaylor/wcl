use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod build;
mod render;
mod serve;

const EXIT_OK: u8 = 0;
const EXIT_PARSE: u8 = 1;
const EXIT_SCHEMA: u8 = 2;
const EXIT_IO: u8 = 4;
const EXIT_EVAL: u8 = 3;
const EXIT_SERVE: u8 = 5;

const DEFAULT_ADDR: &str = "127.0.0.1:8080";

#[derive(Parser)]
#[command(name = "wdoc", version, about = "WCL-driven static site generator")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Build a WCL site source file. Each `page <name> { ... }` block
    /// becomes `<out>/<name>.html`.
    Build {
        /// Path to a WCL source file declaring one or more `page` blocks.
        file: PathBuf,
        /// Output directory. Created if missing.
        #[arg(long)]
        out: PathBuf,
    },
    /// Run a local dev server. Watches the source for `.wcl` changes
    /// and re-renders on each modification — refresh the browser to
    /// see updates.
    Serve {
        /// Path to a WCL source file declaring one or more `page` blocks.
        file: PathBuf,
        /// Bind address. Default `127.0.0.1:8080`.
        #[arg(long, default_value = DEFAULT_ADDR)]
        addr: SocketAddr,
        /// Output directory. When omitted, a temp directory is used
        /// and removed on shutdown.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    let code = match cli.command {
        Command::Build { file, out } => match build::build(&file, &out) {
            Ok(n) => {
                println!("wrote {n} page{}", if n == 1 { "" } else { "s" });
                EXIT_OK
            }
            Err(err) => {
                let code = match &err {
                    build::BuildError::Io(..) => EXIT_IO,
                    build::BuildError::Parse(_) => EXIT_PARSE,
                    build::BuildError::Schema(_) => EXIT_SCHEMA,
                    build::BuildError::BadPage(_) => EXIT_EVAL,
                };
                err.report();
                code
            }
        },
        Command::Serve { file, addr, out } => {
            let rt = match tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    eprintln!("failed to start tokio runtime: {e}");
                    return ExitCode::from(EXIT_IO);
                }
            };
            match rt.block_on(serve::serve(file, out, addr)) {
                Ok(()) => EXIT_OK,
                Err(e) => {
                    eprintln!("serve failed: {e}");
                    EXIT_SERVE
                }
            }
        }
    };
    ExitCode::from(code)
}
