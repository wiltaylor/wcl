use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod build;
mod render;

const EXIT_OK: u8 = 0;
const EXIT_PARSE: u8 = 1;
const EXIT_SCHEMA: u8 = 2;
const EXIT_IO: u8 = 4;
const EXIT_EVAL: u8 = 3;

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
    };
    ExitCode::from(code)
}
