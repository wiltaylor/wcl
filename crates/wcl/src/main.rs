use std::fmt::Write as _;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use wcl_lang::{
    Block, Decorator, Document, Field, SymbolSetDecl, TensorDim, TypeDecl, TypeRef, UnionDecl,
    UnionVariant, UseDeclView, UseFormView, Value, VariantBodyView,
};

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
    if !doc.namespace().is_empty() {
        writeln!(out, "namespace {}", doc.namespace().join(".")).unwrap();
    }
    for u in doc.uses() {
        dump_use_decl(&u, out);
    }
    for t in doc.type_decls() {
        dump_type_decl(&t, out);
    }
    for u in doc.union_decls() {
        dump_union_decl(&u, out);
    }
    for s in doc.symbol_sets() {
        dump_symbol_set_decl(&s, out);
    }
    for f in doc.fields() {
        dump_field(&f, 0, out);
    }
    for b in doc.blocks() {
        dump_block(&b, 0, out);
    }
}

fn dump_use_decl(u: &UseDeclView<'_>, out: &mut String) {
    let prefix = u.path().join(".");
    match u.form() {
        UseFormView::Bare(None) => writeln!(out, "use {prefix}").unwrap(),
        UseFormView::Bare(Some(alias)) => writeln!(out, "use {prefix} as {alias}").unwrap(),
        UseFormView::List => {
            let parts: Vec<String> = u
                .items()
                .map(|it| match it.alias() {
                    Some(a) => format!("{} as {a}", it.name()),
                    None => it.name().to_string(),
                })
                .collect();
            writeln!(out, "use {prefix}.{{{}}}", parts.join(", ")).unwrap();
        }
    }
}

fn dump_symbol_set_decl(s: &SymbolSetDecl<'_>, out: &mut String) {
    dump_decorators(s.decorators(), 0, out);
    writeln!(out, "symbol_set {} {{", s.name_segments().join(".")).unwrap();
    for entry in s.symbols() {
        dump_decorators(entry.decorators(), 1, out);
        writeln!(out, "  {}", entry.name()).unwrap();
    }
    writeln!(out, "}}").unwrap();
}

fn dump_decorators<'a>(decs: impl Iterator<Item = Decorator<'a>>, depth: usize, out: &mut String) {
    let pad = "  ".repeat(depth);
    for d in decs {
        let name = d.full_name();
        let positional: Vec<String> = d.positional().iter().map(value_repr).collect();
        let named: Vec<String> = d
            .named()
            .map(|n| format!("{} = {}", n.name(), value_repr(&n.value())))
            .collect();
        let args = if positional.is_empty() && named.is_empty() {
            String::new()
        } else {
            let combined: Vec<String> = positional.into_iter().chain(named).collect();
            format!("({})", combined.join(", "))
        };
        writeln!(out, "{pad}@{name}{args}").unwrap();
    }
}

fn dump_union_decl(u: &UnionDecl<'_>, out: &mut String) {
    dump_decorators(u.decorators(), 0, out);
    writeln!(out, "union {} {{", u.name_segments().join(".")).unwrap();
    for v in u.variants() {
        dump_variant(&v, out);
    }
    writeln!(out, "}}").unwrap();
}

fn dump_variant(v: &UnionVariant<'_>, out: &mut String) {
    dump_decorators(v.decorators(), 1, out);
    match v.body() {
        VariantBodyView::Record => {
            writeln!(out, "  {} {{", v.name()).unwrap();
            for f in v.fields() {
                dump_decorators(f.decorators(), 2, out);
                let ty = type_repr(f.type_ref());
                let q = if f.optional() { "?" } else { "" };
                writeln!(out, "    {}: {ty}{q}", f.name()).unwrap();
            }
            writeln!(out, "  }}").unwrap();
        }
        VariantBodyView::TypeRef(t) => {
            writeln!(out, "  {} {}", v.name(), type_repr(t)).unwrap();
        }
        VariantBodyView::Unit => {
            writeln!(out, "  {} none", v.name()).unwrap();
        }
    }
}

fn dump_type_decl(t: &TypeDecl<'_>, out: &mut String) {
    dump_decorators(t.decorators(), 0, out);
    writeln!(out, "type {} {{", t.name_segments().join(".")).unwrap();
    for field in t.fields() {
        dump_decorators(field.decorators(), 1, out);
        let ty = type_repr(field.type_ref());
        let q = if field.optional() { "?" } else { "" };
        writeln!(out, "  {}: {ty}{q}", field.name()).unwrap();
    }
    writeln!(out, "}}").unwrap();
}

fn type_repr(t: &TypeRef) -> String {
    match t {
        TypeRef::Builtin(b) => b.name().to_string(),
        TypeRef::Named(path) => path.join("."),
        TypeRef::Reference(inner) => format!("&{}", type_repr(inner)),
        TypeRef::List(inner) => format!("list<{}>", type_repr(inner)),
        TypeRef::Tensor { element, dims } => {
            let dims_str = dims.iter().map(dim_repr).collect::<Vec<_>>().join(", ");
            format!("tensor<{}, [{}]>", type_repr(element), dims_str)
        }
    }
}

fn dim_repr(d: &TensorDim) -> String {
    match d {
        TensorDim::Fixed(n) => n.to_string(),
        TensorDim::Symbolic(s) => s.clone(),
    }
}

fn dump_field(f: &Field<'_>, depth: usize, out: &mut String) {
    dump_decorators(f.decorators(), depth, out);
    let pad = "  ".repeat(depth);
    let _ = write!(out, "{pad}{} = ", f.name());
    match f.value() {
        Ok(v) => writeln!(out, "{}", value_repr(v)).unwrap(),
        Err(e) => writeln!(out, "<error: {e}>").unwrap(),
    }
}

fn dump_block(b: &Block<'_>, depth: usize, out: &mut String) {
    dump_decorators(b.decorators(), depth, out);
    let pad = "  ".repeat(depth);
    let _ = write!(out, "{pad}{}", b.kind());
    for label in b.labels() {
        let _ = write!(out, " {}", value_repr(&label));
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
        Value::Symbol(s) => format!(":{s}"),
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
