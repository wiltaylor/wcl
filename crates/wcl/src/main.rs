use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use wcl_lang::{Block, Document, Field, TypeDecl, TypeRef, Value};

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
    for t in doc.type_decls() {
        dump_type_decl(&t, out);
    }
    for f in doc.fields() {
        dump_field(&f, 0, out);
    }
    for b in doc.blocks() {
        dump_block(&b, 0, out);
    }
}

fn dump_type_decl(t: &TypeDecl<'_>, out: &mut String) {
    writeln!(out, "type {} {{", t.name()).unwrap();
    for field in t.fields() {
        let ty = type_repr(field.type_ref());
        let q = if field.optional() { "?" } else { "" };
        writeln!(out, "  {}: {ty}{q}", field.name()).unwrap();
    }
    writeln!(out, "}}").unwrap();
}

fn type_repr(t: &TypeRef) -> String {
    match t {
        TypeRef::Builtin(b) => b.name().to_string(),
        TypeRef::Named(s) => s.clone(),
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
        Value::Bool(b) => b.to_string(),

        // Default-typed integers/floats render without a suffix; everything
        // else renders with its Rust-style suffix so the dump is round-trippable.
        Value::I64(n) => n.to_string(),
        Value::F64(n) => format_float(*n, "f64"),

        Value::I8(n) => format!("{n}i8"),
        Value::I16(n) => format!("{n}i16"),
        Value::I32(n) => format!("{n}i32"),
        Value::I128(n) => format!("{n}i128"),
        Value::Isize(n) => format!("{n}isize"),

        Value::U8(n) => format!("{n}u8"),
        Value::U16(n) => format!("{n}u16"),
        Value::U32(n) => format!("{n}u32"),
        Value::U64(n) => format!("{n}u64"),
        Value::U128(n) => format!("{n}u128"),
        Value::Usize(n) => format!("{n}usize"),

        Value::F32(n) => format!("{}f32", format_float(*n as f64, "f32")),

        Value::Utf8(s) => format!("\"{}\"", escape_string(s)),
        Value::Ascii(s) => format!("ascii\"{}\"", escape_string(s)),
        Value::Utf16(units) => {
            let s = String::from_utf16_lossy(units);
            format!("utf16\"{}\"", escape_string(&s))
        }
        Value::Utf32(chars) => {
            let s: String = chars.iter().collect();
            format!("utf32\"{}\"", escape_string(&s))
        }

        Value::Identifier(s) => s.clone(),
        Value::None => "none".to_string(),
    }
}

fn format_float(n: f64, _ty: &str) -> String {
    // Ensure the rendered form is unambiguously a float (contains '.' or 'e')
    // so re-parsing the dump preserves the float type.
    let s = format!("{n}");
    if s.contains('.') || s.contains('e') || s.contains('E') || !n.is_finite() {
        s
    } else {
        format!("{s}.0")
    }
}

fn escape_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            other => out.push(other),
        }
    }
    out
}
