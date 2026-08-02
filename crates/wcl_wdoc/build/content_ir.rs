//! Generate the Rust content IR from its WCL declaration.
//!
//! `lib/content.wcl` declares `union Content` — the closed, target-neutral
//! document vocabulary. This module parses the stdlib with the real WCL
//! parser, walks every type reachable from that union, and emits the
//! matching Rust: one `enum` per union, one `struct` per record type, one
//! fieldless `enum` per symbol set, and a `TryFrom<&Value>` for each. The
//! declaration is the source of truth; there is no second copy to drift.
//!
//! Reachability is also the closedness check: a field whose type cannot be
//! carried across backends (a function, a reference, a tensor, a type the
//! stdlib doesn't declare) fails the build here rather than becoming a hole
//! the walkers quietly ignore.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use wcl_lang::ast::{Decorator, Expr, Item, SymbolSetDecl, TypeDecl, TypeField, UnionDecl};
use wcl_lang::{BuiltinType, TypeRef};

/// The union the generated module is rooted at.
const ROOT_UNION: &str = "Content";

/// Every declaration the stdlib makes, indexed by its bare name (the
/// stdlib is one `namespace wdoc`, so the last segment identifies a
/// declaration uniquely).
#[derive(Default)]
struct Decls {
    unions: BTreeMap<String, UnionDecl>,
    records: BTreeMap<String, TypeDecl>,
    symbol_sets: BTreeMap<String, SymbolSetDecl>,
}

/// Parse `lib/*.wcl` and emit the content IR module source.
pub(crate) fn generate(lib_dir: &Path) -> String {
    let decls = parse_lib(lib_dir);
    emit(&decls)
}

fn parse_lib(lib_dir: &Path) -> Decls {
    let mut files: Vec<_> = std::fs::read_dir(lib_dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", lib_dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("wcl"))
        .collect();
    files.sort();

    let mut decls = Decls::default();
    for path in files {
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
        let name = path.display().to_string();
        let source = wcl_lang::parse_for_edit(&src, name.clone())
            .unwrap_or_else(|e| panic!("parse {name}: {e}"));
        for item in source.items {
            match item {
                Item::UnionDecl(u) => {
                    decls.unions.insert(bare(&u.name), u);
                }
                Item::TypeDecl(t) => {
                    decls.records.insert(bare(&t.name), t);
                }
                Item::SymbolSetDecl(s) => {
                    decls.symbol_sets.insert(bare(&s.name), s);
                }
                _ => {}
            }
        }
    }
    decls
}

/// The last segment of a declaration's dotted name.
fn bare(name: &[String]) -> String {
    name.last().cloned().unwrap_or_default()
}

// ── Reachability ──────────────────────────────────────────────────

/// What a named type resolves to.
enum Kind {
    Union,
    Record,
    SymbolSet,
}

fn kind_of(decls: &Decls, name: &str) -> Option<Kind> {
    if decls.unions.contains_key(name) {
        Some(Kind::Union)
    } else if decls.symbol_sets.contains_key(name) {
        Some(Kind::SymbolSet)
    } else if decls.records.contains_key(name) {
        Some(Kind::Record)
    } else {
        None
    }
}

/// Every named type reachable from [`ROOT_UNION`], in deterministic
/// (breadth-first, name-sorted) order.
fn reachable(decls: &Decls) -> Vec<String> {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut order: Vec<String> = Vec::new();
    let mut queue: Vec<String> = vec![ROOT_UNION.to_string()];
    while let Some(name) = queue.first().cloned() {
        queue.remove(0);
        if !seen.insert(name.clone()) {
            continue;
        }
        let mut next: BTreeSet<String> = BTreeSet::new();
        match kind_of(decls, &name) {
            Some(Kind::Union) => {
                for variant in &decls.unions[&name].variants {
                    for field in variant_fields(&decls.unions[&name], variant) {
                        collect_named(&field.ty, &mut next);
                    }
                }
            }
            Some(Kind::Record) => {
                for field in &decls.records[&name].fields {
                    collect_named(&field.ty, &mut next);
                }
            }
            Some(Kind::SymbolSet) => {}
            None => panic!(
                "content IR: `{name}` is referenced but the wdoc stdlib declares no such type"
            ),
        }
        order.push(name);
        queue.extend(next);
    }
    order
}

/// The record fields of a union variant. The content IR is
/// record-shaped throughout: a positional or unit variant has no field
/// names for a backend to read, so it is refused here.
fn variant_fields<'a>(
    union: &'a UnionDecl,
    variant: &'a wcl_lang::ast::UnionVariant,
) -> &'a [TypeField] {
    match &variant.body {
        wcl_lang::ast::VariantBody::Record { fields, .. } => fields,
        _ => panic!(
            "content IR: `{}::{}` must be a record variant — the generated conversion reads payloads by field name",
            bare(&union.name),
            variant.name
        ),
    }
}

fn collect_named(ty: &TypeRef, out: &mut BTreeSet<String>) {
    match ty {
        TypeRef::Named { path, args } => {
            out.insert(named(path, args));
        }
        TypeRef::List(inner) => collect_named(inner, out),
        _ => {}
    }
}

/// The name a `Named` reference resolves to. Type arguments are syntax
/// only in WCL — nothing substitutes them — so the emitter refuses a
/// parameterised type rather than generating `Foo` and silently dropping
/// the `<Bar>` a reader would then never see.
fn named(path: &[String], args: &[TypeRef]) -> String {
    assert!(
        args.is_empty(),
        "content IR: `{}` is written with type arguments, which are syntax only — the IR cannot carry a parameterised type",
        bare(path)
    );
    bare(path)
}

// ── Emission ──────────────────────────────────────────────────────

fn emit(decls: &Decls) -> String {
    let mut out = String::new();
    out.push_str("// @generated by build.rs from lib/content.wcl — do not edit.\n");
    for name in reachable(decls) {
        match kind_of(decls, &name).expect("reachable name resolves") {
            Kind::Union => emit_union(&mut out, &decls.unions[&name]),
            Kind::Record => emit_record(&mut out, &decls.records[&name]),
            Kind::SymbolSet => emit_symbol_set(&mut out, &decls.symbol_sets[&name]),
        }
    }
    out
}

fn emit_union(out: &mut String, union: &UnionDecl) {
    let name = bare(&union.name);
    doc_lines(
        out,
        "",
        &union.decorators,
        &format!("The `{name}` IR node."),
    );
    out.push_str("#[derive(Debug, Clone, PartialEq)]\npub enum ");
    out.push_str(&name);
    out.push_str(" {\n");
    for variant in &union.variants {
        doc_lines(
            out,
            "    ",
            &variant.decorators,
            &format!("`{name}::{}`.", variant.name),
        );
        writeln!(out, "    {} {{", variant.name).unwrap();
        for field in variant_fields(union, variant) {
            doc_lines(out, "        ", &field.decorators, "");
            writeln!(
                out,
                "        {}: {},",
                rust_ident(&field.name),
                rust_type(&field.ty, field.optional)
            )
            .unwrap();
        }
        out.push_str("    },\n");
    }
    out.push_str("}\n\n");

    // `TryFrom<&Value>`: dispatch on the variant name, then read each
    // field out of the payload map by name.
    writeln!(out, "impl TryFrom<&Value> for {name} {{").unwrap();
    out.push_str("    type Error = ContentError;\n");
    out.push_str("    fn try_from(value: &Value) -> Result<Self, ContentError> {\n");
    writeln!(
        out,
        "        let (variant, map) = variant_payload(value, {name:?})?;"
    )
    .unwrap();
    out.push_str("        match variant {\n");
    for variant in &union.variants {
        let owner = format!("{name}::{}", variant.name);
        writeln!(out, "            {:?} => {{", variant.name).unwrap();
        for field in variant_fields(union, variant) {
            emit_field_read(out, "                ", &owner, field);
        }
        writeln!(out, "                Ok({name}::{} {{", variant.name).unwrap();
        for field in variant_fields(union, variant) {
            writeln!(out, "                    {},", rust_ident(&field.name)).unwrap();
        }
        out.push_str("                })\n            }\n");
    }
    writeln!(
        out,
        "            other => Err(ContentError::UnknownVariant {{ owner: {name:?}, variant: other.to_string() }}),"
    )
    .unwrap();
    out.push_str("        }\n    }\n}\n\n");
    emit_owned_try_from(out, &name);
}

fn emit_record(out: &mut String, record: &TypeDecl) {
    let name = bare(&record.name);
    assert!(
        record.alias.is_none(),
        "content IR: `{name}` is a type alias; the IR carries records, unions and symbol sets only"
    );
    doc_lines(
        out,
        "",
        &record.decorators,
        &format!("The `{name}` IR record."),
    );
    out.push_str("#[derive(Debug, Clone, PartialEq)]\npub struct ");
    out.push_str(&name);
    out.push_str(" {\n");
    for field in &record.fields {
        doc_lines(out, "    ", &field.decorators, "");
        writeln!(
            out,
            "    pub {}: {},",
            rust_ident(&field.name),
            rust_type(&field.ty, field.optional)
        )
        .unwrap();
    }
    out.push_str("}\n\n");

    writeln!(out, "impl TryFrom<&Value> for {name} {{").unwrap();
    out.push_str("    type Error = ContentError;\n");
    out.push_str("    fn try_from(value: &Value) -> Result<Self, ContentError> {\n");
    writeln!(out, "        let map = record_fields(value, {name:?})?;").unwrap();
    for field in &record.fields {
        emit_field_read(out, "        ", &name, field);
    }
    writeln!(out, "        Ok({name} {{").unwrap();
    for field in &record.fields {
        writeln!(out, "            {},", rust_ident(&field.name)).unwrap();
    }
    out.push_str("        })\n    }\n}\n\n");
    emit_owned_try_from(out, &name);
}

fn emit_symbol_set(out: &mut String, set: &SymbolSetDecl) {
    let name = bare(&set.name);
    doc_lines(
        out,
        "",
        &set.decorators,
        &format!("The `{name}` symbol vocabulary."),
    );
    out.push_str("#[derive(Debug, Clone, Copy, PartialEq, Eq)]\npub enum ");
    out.push_str(&name);
    out.push_str(" {\n");
    for symbol in &set.symbols {
        writeln!(out, "    /// `:{}`", symbol.name).unwrap();
        writeln!(out, "    {},", camel_case(&symbol.name)).unwrap();
    }
    out.push_str("}\n\n");

    writeln!(out, "impl {name} {{").unwrap();
    out.push_str("    /// The WCL symbol name this member was declared with.\n");
    out.push_str("    pub fn as_wcl(self) -> &'static str {\n        match self {\n");
    for symbol in &set.symbols {
        writeln!(
            out,
            "            {name}::{} => {:?},",
            camel_case(&symbol.name),
            symbol.name
        )
        .unwrap();
    }
    out.push_str("        }\n    }\n}\n\n");

    writeln!(out, "impl TryFrom<&Value> for {name} {{").unwrap();
    out.push_str("    type Error = ContentError;\n");
    out.push_str("    fn try_from(value: &Value) -> Result<Self, ContentError> {\n");
    writeln!(out, "        match symbol_name(value, {name:?})? {{").unwrap();
    for symbol in &set.symbols {
        writeln!(
            out,
            "            {:?} => Ok({name}::{}),",
            symbol.name,
            camel_case(&symbol.name)
        )
        .unwrap();
    }
    writeln!(
        out,
        "            other => Err(ContentError::UnknownSymbol {{ owner: {name:?}, symbol: other.to_string() }}),"
    )
    .unwrap();
    out.push_str("        }\n    }\n}\n\n");
    emit_owned_try_from(out, &name);
}

/// The by-value conversion, forwarding to the by-reference one so there
/// is a single body per type.
fn emit_owned_try_from(out: &mut String, name: &str) {
    writeln!(out, "impl TryFrom<Value> for {name} {{").unwrap();
    out.push_str("    type Error = ContentError;\n");
    out.push_str("    fn try_from(value: Value) -> Result<Self, ContentError> {\n");
    out.push_str("        Self::try_from(&value)\n    }\n}\n\n");
}

/// One `let <field> = …;` binding reading a field out of a payload map.
fn emit_field_read(out: &mut String, indent: &str, owner: &str, field: &TypeField) {
    let at = format!("At {{ owner: {owner:?}, field: {:?} }}", field.name);
    let ident = rust_ident(&field.name);
    if field.optional {
        writeln!(
            out,
            "{indent}let {ident} = match present(map, {:?}) {{",
            field.name
        )
        .unwrap();
        writeln!(
            out,
            "{indent}    Some(v) => Some({}?),",
            read_expr(&field.ty, &at, "v", 0)
        )
        .unwrap();
        writeln!(out, "{indent}    None => None,\n{indent}}};").unwrap();
    } else {
        writeln!(
            out,
            "{indent}let {ident} = {{ let v = required(map, {at})?; {}? }};",
            read_expr(&field.ty, &at, "v", 0)
        )
        .unwrap();
    }
}

/// A `Result<T, ContentError>`-typed expression reading `src` as `ty`.
/// `depth` names the closure binding of nested list readers so they
/// don't shadow each other.
fn read_expr(ty: &TypeRef, at: &str, src: &str, depth: usize) -> String {
    match ty {
        TypeRef::Builtin(b) => {
            let reader = match b {
                BuiltinType::Bool => "as_bool".to_string(),
                BuiltinType::Utf8 | BuiltinType::Ascii => "as_string".to_string(),
                BuiltinType::Identifier => "as_identifier".to_string(),
                BuiltinType::Symbol => "as_symbol".to_string(),
                BuiltinType::F32 => "as_f32".to_string(),
                BuiltinType::F64 => "as_f64".to_string(),
                other => format!("as_int::<{}>", int_rust_name(*other)),
            };
            format!("{reader}({src}, {at})")
        }
        TypeRef::List(inner) => {
            let binding = format!("it{depth}");
            format!(
                "as_seq({src}, {at}, |{binding}| {})",
                read_expr(inner, at, &binding, depth + 1)
            )
        }
        TypeRef::Named { path, args } => format!("{}::try_from({src})", named(path, args)),
        other => panic!(
            "content IR: a field typed `{other}` cannot cross the backend boundary — the IR carries scalars, lists, records, unions and symbol sets"
        ),
    }
}

/// The Rust spelling of a declared field type.
fn rust_type(ty: &TypeRef, optional: bool) -> String {
    let inner = match ty {
        TypeRef::Builtin(b) => match b {
            BuiltinType::Bool => "bool".to_string(),
            BuiltinType::Utf8 | BuiltinType::Ascii | BuiltinType::Identifier => {
                "String".to_string()
            }
            BuiltinType::Symbol => "String".to_string(),
            BuiltinType::F32 => "f32".to_string(),
            BuiltinType::F64 => "f64".to_string(),
            other => int_rust_name(*other).to_string(),
        },
        TypeRef::List(inner) => format!("Vec<{}>", rust_type(inner, false)),
        TypeRef::Named { path, args } => named(path, args),
        other => panic!("content IR: unsupported field type `{other}`"),
    };
    if optional {
        format!("Option<{inner}>")
    } else {
        inner
    }
}

fn int_rust_name(b: BuiltinType) -> &'static str {
    match b {
        BuiltinType::I8 => "i8",
        BuiltinType::I16 => "i16",
        BuiltinType::I32 => "i32",
        BuiltinType::I64 => "i64",
        BuiltinType::I128 => "i128",
        BuiltinType::Isize => "isize",
        BuiltinType::U8 => "u8",
        BuiltinType::U16 => "u16",
        BuiltinType::U32 => "u32",
        BuiltinType::U64 => "u64",
        BuiltinType::U128 => "u128",
        BuiltinType::Usize => "usize",
        other => panic!(
            "content IR: `{}` is not a numeric type",
            TypeRef::Builtin(other)
        ),
    }
}

// ── Naming and doc comments ───────────────────────────────────────

/// Rust keywords a WCL field name may legitimately collide with.
const RUST_KEYWORDS: &[&str] = &[
    "as", "box", "break", "const", "continue", "crate", "dyn", "else", "enum", "extern", "false",
    "fn", "for", "if", "impl", "in", "let", "loop", "match", "mod", "move", "mut", "pub", "ref",
    "return", "self", "static", "struct", "super", "trait", "true", "type", "union", "unsafe",
    "use", "where", "while",
];

fn rust_ident(name: &str) -> String {
    if RUST_KEYWORDS.contains(&name) {
        format!("r#{name}")
    } else {
        name.to_string()
    }
}

fn camel_case(name: &str) -> String {
    name.split('_')
        .filter(|p| !p.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect()
}

/// Emit the declaration's `@doc("…")` text as a Rust doc comment,
/// falling back to `fallback` when it declares none (and emitting
/// nothing when there is no fallback either).
fn doc_lines(out: &mut String, indent: &str, decorators: &[Decorator], fallback: &str) {
    let text = doc_text(decorators).unwrap_or_else(|| fallback.to_string());
    if text.is_empty() {
        return;
    }
    for line in text.lines() {
        writeln!(out, "{indent}/// {line}").unwrap();
    }
}

fn doc_text(decorators: &[Decorator]) -> Option<String> {
    decorators
        .iter()
        .find(|d| bare(&d.name) == "doc")
        .and_then(|d| d.positional.first())
        .and_then(|e| match e {
            Expr::Utf8(s) | Expr::Ascii(s) => Some(s.clone()),
            _ => None,
        })
}
