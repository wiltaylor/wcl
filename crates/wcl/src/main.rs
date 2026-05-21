use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "wcl", version, about = "WCL command-line interface")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse a WCL file and print the resulting document tree.
    Parse {
        /// Path to a WCL source file.
        file: PathBuf,
    },
    /// Parse a WCL file and report whether it is syntactically valid.
    Check {
        /// Path to a WCL source file.
        file: PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match cli.command {
        Command::Parse { file } => match wcl_lang::parse_file(&file) {
            Ok(doc) => {
                println!("{doc:#?}");
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("{:?}", miette::Report::new(err));
                ExitCode::FAILURE
            }
        },
        Command::Check { file } => match wcl_lang::parse_file(&file) {
            Ok(_) => {
                println!("OK");
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("{:?}", miette::Report::new(err));
                ExitCode::FAILURE
            }
        },
    }
}
