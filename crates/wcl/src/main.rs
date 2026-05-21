use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use wcl_lang::{Block, Document, Field, Value};

#[derive(Parser)]
#[command(name = "wcl", version, about = "WCL command-line interface")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Parse a WCL file and print the resulting document tree (forces evaluation).
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
        Command::Parse { file } => match Document::from_file(&file) {
            Ok(doc) => {
                let mut out = String::new();
                dump_document(&doc, &mut out);
                print!("{out}");
                ExitCode::SUCCESS
            }
            Err(err) => {
                eprintln!("{:?}", miette::Report::new(err));
                ExitCode::FAILURE
            }
        },
        Command::Check { file } => match Document::from_file(&file) {
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

fn dump_document(doc: &Document, out: &mut String) {
    for f in doc.fields() {
        dump_field(&f, 0, out);
    }
    for b in doc.blocks() {
        dump_block(&b, 0, out);
    }
}

fn dump_field(f: &Field<'_>, depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth);
    let _ = write!(out, "{pad}{} = ", f.name());
    match f.value() {
        Ok(v) => writeln!(out, "{}", value_repr(v)).unwrap(),
        Err(e) => writeln!(out, "<error: {e}>").unwrap(),
    }
}

fn dump_block(b: &Block<'_>, depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth);
    let _ = write!(out, "{pad}{}", b.kind());
    for label in b.labels() {
        let _ = write!(out, " \"{label}\"");
    }
    writeln!(out, " {{").unwrap();
    for f in b.fields() {
        dump_field(&f, depth + 1, out);
    }
    for inner in b.blocks() {
        dump_block(&inner, depth + 1, out);
    }
    writeln!(out, "{pad}}}").unwrap();
}

fn value_repr(v: &Value) -> String {
    match v {
        Value::String(s) => format!("\"{s}\""),
        Value::Int(n) => n.to_string(),
        Value::Float(f) => f.to_string(),
        Value::Bool(b) => b.to_string(),
    }
}
