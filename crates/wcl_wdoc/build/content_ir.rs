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
    /// Names the stdlib declares more than once (`Image` is both a page
    /// block and a typedoc one). Harmless until the IR reaches one, at
    /// which point which declaration it meant is a coin toss — so
    /// reaching an ambiguous name fails the build.
    ambiguous: BTreeSet<String>,
}

/// Parse `lib/*.wcl` and emit the content IR module source.
pub(crate) fn generate(lib_dir: &Path) -> String {
    let mut files: Vec<_> = std::fs::read_dir(lib_dir)
        .unwrap_or_else(|e| panic!("read {}: {e}", lib_dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("wcl"))
        .collect();
    files.sort();

    let sources: Vec<(String, String)> = files
        .into_iter()
        .map(|path| {
            let src = std::fs::read_to_string(&path)
                .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
            (path.display().to_string(), src)
        })
        .collect();
    generate_from(&sources)
}

/// The body of [`generate`], over already-read sources — the seam its
/// tests drive with a synthetic stdlib.
pub(crate) fn generate_from(sources: &[(String, String)]) -> String {
    emit(&parse_all(sources))
}

fn parse_all(sources: &[(String, String)]) -> Decls {
    let mut decls = Decls::default();
    for (name, src) in sources {
        let source = wcl_lang::parse_for_edit(src, name.clone())
            .unwrap_or_else(|e| panic!("parse {name}: {e}"));
        for item in source.items {
            match item {
                Item::UnionDecl(u) => decls.declare_union(u),
                Item::TypeDecl(t) => decls.declare_record(t),
                Item::SymbolSetDecl(s) => decls.declare_symbol_set(s),
                _ => {}
            }
        }
    }
    decls
}

impl Decls {
    fn note(&mut self, name: &str, taken: bool) {
        if taken {
            self.ambiguous.insert(name.to_string());
        }
    }

    fn declares(&self, name: &str) -> bool {
        self.unions.contains_key(name)
            || self.records.contains_key(name)
            || self.symbol_sets.contains_key(name)
    }

    fn declare_union(&mut self, decl: UnionDecl) {
        let name = bare(&decl.name);
        self.note(&name, self.declares(&name));
        self.unions.insert(name, decl);
    }

    fn declare_record(&mut self, decl: TypeDecl) {
        let name = bare(&decl.name);
        self.note(&name, self.declares(&name));
        self.records.insert(name, decl);
    }

    fn declare_symbol_set(&mut self, decl: SymbolSetDecl) {
        let name = bare(&decl.name);
        self.note(&name, self.declares(&name));
        self.symbol_sets.insert(name, decl);
    }
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
    let mut queue: std::collections::VecDeque<String> =
        std::collections::VecDeque::from([ROOT_UNION.to_string()]);
    while let Some(name) = queue.pop_front() {
        if !seen.insert(name.clone()) {
            continue;
        }
        assert!(
            !decls.ambiguous.contains(&name),
            "content IR: the wdoc stdlib declares `{name}` more than once, so which declaration the IR means is ambiguous"
        );
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
    emit_try_from_header(out, &name);
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

    emit_try_from_header(out, &name);
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

    emit_try_from_header(out, &name);
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

/// The opening of a `TryFrom<&Value>` impl — every generated type reads
/// from a borrowed value and fails with the one error type.
fn emit_try_from_header(out: &mut String, name: &str) {
    writeln!(out, "impl TryFrom<&Value> for {name} {{").unwrap();
    out.push_str("    type Error = ContentError;\n");
    out.push_str("    fn try_from(value: &Value) -> Result<Self, ContentError> {\n");
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
        TypeRef::Builtin(b) => format!("{}({src}, {at})", builtin(*b).reader),
        TypeRef::List(inner) => {
            let binding = format!("it{depth}");
            format!(
                "as_seq({src}, {at}, |{binding}| {})",
                read_expr(inner, at, &binding, depth + 1)
            )
        }
        TypeRef::Named { path, args } => format!("{}::try_from({src})", named(path, args)),
        other => panic!("content IR: {}", uncarryable(other)),
    }
}

/// The one wording for a field type the IR cannot carry — the same
/// refusal whether the emitter met it while spelling the Rust type or
/// while writing the reader.
fn uncarryable(ty: &TypeRef) -> String {
    format!(
        "a field typed `{ty}` cannot cross the backend boundary — the IR carries scalars, lists, records, unions and symbol sets"
    )
}

/// The Rust spelling of a declared field type.
fn rust_type(ty: &TypeRef, optional: bool) -> String {
    let inner = match ty {
        TypeRef::Builtin(b) => builtin(*b).rust.to_string(),
        TypeRef::List(inner) => format!("Vec<{}>", rust_type(inner, false)),
        TypeRef::Named { path, args } => named(path, args),
        other => panic!("content IR: {}", uncarryable(other)),
    };
    if optional {
        format!("Option<{inner}>")
    } else {
        inner
    }
}

/// How a WCL builtin crosses into Rust.
struct Crossing {
    /// The Rust type the generated field carries.
    rust: &'static str,
    /// The `src/content.rs` reader that pulls it out of a `Value`.
    reader: &'static str,
}

/// The one table pairing a builtin with its Rust type and its reader —
/// declared together so the two can't disagree about a builtin, and so
/// teaching the IR a new one is a single row.
fn builtin(b: BuiltinType) -> Crossing {
    let (rust, reader) = match b {
        BuiltinType::Bool => ("bool", "as_bool"),
        BuiltinType::Utf8 | BuiltinType::Ascii => ("String", "as_string"),
        BuiltinType::Identifier => ("String", "as_identifier"),
        BuiltinType::Symbol => ("String", "as_symbol"),
        BuiltinType::F32 => ("f32", "as_f32"),
        BuiltinType::F64 => ("f64", "as_f64"),
        BuiltinType::I8 => ("i8", "as_int::<i8>"),
        BuiltinType::I16 => ("i16", "as_int::<i16>"),
        BuiltinType::I32 => ("i32", "as_int::<i32>"),
        BuiltinType::I64 => ("i64", "as_int::<i64>"),
        BuiltinType::I128 => ("i128", "as_int::<i128>"),
        BuiltinType::Isize => ("isize", "as_int::<isize>"),
        BuiltinType::U8 => ("u8", "as_int::<u8>"),
        BuiltinType::U16 => ("u16", "as_int::<u16>"),
        BuiltinType::U32 => ("u32", "as_int::<u32>"),
        BuiltinType::U64 => ("u64", "as_int::<u64>"),
        BuiltinType::U128 => ("u128", "as_int::<u128>"),
        BuiltinType::Usize => ("usize", "as_int::<usize>"),
        // `utf16` / `utf32` have no reader: a document's prose is utf8,
        // and a backend that wanted another encoding would convert.
        other => panic!(
            "content IR: a field typed `{}` has no Rust crossing",
            TypeRef::Builtin(other)
        ),
    };
    Crossing { rust, reader }
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
