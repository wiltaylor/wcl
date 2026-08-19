//! The `wcl parse` document-tree dump and `WCL_PROFILE` JSON rendering.
//!
//! Split out of `main.rs` so the CLI entry point stays thin. All text
//! goes through [`Out`], which centralises the two-space indentation and
//! absorbs the (infallible) writes into a `String`, keeping the dumpers
//! free of `.unwrap()` noise.

use std::fmt::Write as _;

use wcl_lang::{
    Block, ConnectionDecl, DeclName, Decorator, Document, Field, Profile, ProfileKey, ProfileNode,
    SymbolSetDecl, TypeDecl, UnionDecl, UnionVariant, UseDeclView, UseFormView, Value,
    VariantBodyView,
};

/// Text sink for the parse dump. Owns the buffer, indents by levels of
/// two spaces, and never surfaces a write error (writing to a `String`
/// is infallible).
#[derive(Default)]
struct Out {
    buf: String,
}

impl Out {
    fn into_string(self) -> String {
        self.buf
    }

    fn indent(&mut self, depth: usize) {
        for _ in 0..depth {
            self.buf.push_str("  ");
        }
    }

    /// A full line: `depth` indents, the formatted text, then a newline.
    fn line(&mut self, depth: usize, args: std::fmt::Arguments<'_>) {
        self.indent(depth);
        let _ = self.buf.write_fmt(args);
        self.buf.push('\n');
    }

    /// The start of a line: `depth` indents then text, no newline (the
    /// caller continues it with [`cont`](Self::cont) / [`endln`](Self::endln)).
    fn frag(&mut self, depth: usize, args: std::fmt::Arguments<'_>) {
        self.indent(depth);
        let _ = self.buf.write_fmt(args);
    }

    /// Mid-line continuation: no indent, no newline.
    fn cont(&mut self, args: std::fmt::Arguments<'_>) {
        let _ = self.buf.write_fmt(args);
    }

    /// End the current line: text (no indent) then a newline.
    fn endln(&mut self, args: std::fmt::Arguments<'_>) {
        let _ = self.buf.write_fmt(args);
        self.buf.push('\n');
    }
}

macro_rules! line {
    ($o:expr, $d:expr, $($a:tt)*) => {{ $o.line($d, format_args!($($a)*)); }};
}
macro_rules! frag {
    ($o:expr, $d:expr, $($a:tt)*) => {{ $o.frag($d, format_args!($($a)*)); }};
}
macro_rules! cont {
    ($o:expr, $($a:tt)*) => {{ $o.cont(format_args!($($a)*)); }};
}
macro_rules! endln {
    ($o:expr, $($a:tt)*) => {{ $o.endln(format_args!($($a)*)); }};
}

/// Render the whole document tree as the textual form `wcl parse` prints.
pub(crate) fn document(doc: &Document) -> String {
    let mut out = Out::default();
    dump_document(doc, &mut out);
    out.into_string()
}

fn dump_document(doc: &Document, out: &mut Out) {
    if !doc.namespace().is_empty() {
        line!(out, 0, "namespace {}", doc.namespace().join("."));
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
    for c in doc.connection_decls() {
        dump_connection_decl(&c, out);
    }
    for f in doc.fields() {
        dump_field(&f, 0, out);
    }
    for b in doc.blocks() {
        dump_block(&b, 0, out);
    }
    for c in doc.connection_stmts() {
        match c.kind() {
            Some(k) => line!(out, 0, "{} -> {} :{}", c.source(), c.destination(), k),
            None => line!(out, 0, "{} -> {}", c.source(), c.destination()),
        }
    }
}

fn dump_connection_decl(c: &ConnectionDecl<'_>, out: &mut Out) {
    line!(
        out,
        0,
        "connection {}: {} -> {} : {}",
        c.full_name(),
        c.source_type(),
        c.destination_type(),
        c.kind_set_path().join("."),
    );
}

fn dump_use_decl(u: &UseDeclView<'_>, out: &mut Out) {
    let prefix = u.path().join(".");
    match u.form() {
        UseFormView::Bare(None) => line!(out, 0, "use {prefix}"),
        UseFormView::Bare(Some(alias)) => line!(out, 0, "use {prefix} as {alias}"),
        UseFormView::List => {
            let parts: Vec<String> = u
                .items()
                .map(|it| match it.alias() {
                    Some(a) => format!("{} as {a}", it.name()),
                    None => it.name().to_string(),
                })
                .collect();
            line!(out, 0, "use {prefix}.{{{}}}", parts.join(", "));
        }
    }
}

fn dump_symbol_set_decl(s: &SymbolSetDecl<'_>, out: &mut Out) {
    dump_decorators(s.decorators(), 0, out);
    line!(out, 0, "symbol_set {} {{", s.name_segments().join("."));
    for entry in s.symbols() {
        dump_decorators(entry.decorators(), 1, out);
        line!(out, 1, "{}", entry.name());
    }
    line!(out, 0, "}}");
}

fn dump_decorators<'a>(decs: impl Iterator<Item = Decorator<'a>>, depth: usize, out: &mut Out) {
    for d in decs {
        let name = d.full_name();
        let positional: Vec<String> = match d.positional() {
            Ok(vals) => vals.iter().map(Value::to_string).collect(),
            Err(e) => vec![format!("<error: {e}>")],
        };
        let named: Vec<String> = d
            .named()
            .map(|n| {
                let val = match n.value() {
                    Ok(v) => v.to_string(),
                    Err(e) => format!("<error: {e}>"),
                };
                format!("{} = {}", n.name(), val)
            })
            .collect();
        let args = if positional.is_empty() && named.is_empty() {
            String::new()
        } else {
            let combined: Vec<String> = positional.into_iter().chain(named).collect();
            format!("({})", combined.join(", "))
        };
        line!(out, depth, "@{name}{args}");
    }
}

fn dump_union_decl(u: &UnionDecl<'_>, out: &mut Out) {
    dump_decorators(u.decorators(), 0, out);
    line!(out, 0, "union {} {{", u.name_segments().join("."));
    for v in u.variants() {
        dump_variant(&v, out);
    }
    line!(out, 0, "}}");
}

fn dump_variant(v: &UnionVariant<'_>, out: &mut Out) {
    dump_decorators(v.decorators(), 1, out);
    match v.body() {
        VariantBodyView::Record => {
            line!(out, 1, "{} {{", v.name());
            for f in v.fields() {
                dump_decorators(f.decorators(), 2, out);
                let ty = f.type_ref();
                let q = if f.optional() { "?" } else { "" };
                line!(out, 2, "{}: {ty}{q}", f.name());
            }
            line!(out, 1, "}}");
        }
        VariantBodyView::TypeRef(t) => {
            line!(out, 1, "{} {}", v.name(), t);
        }
        VariantBodyView::InterfaceRef(path) => {
            line!(out, 1, "{} &{}", v.name(), path.join("."));
        }
        VariantBodyView::Unit => {
            line!(out, 1, "{} none", v.name());
        }
    }
}

fn dump_type_decl(t: &TypeDecl<'_>, out: &mut Out) {
    dump_decorators(t.decorators(), 0, out);
    line!(out, 0, "type {} {{", t.name_segments().join("."));
    for field in t.fields() {
        dump_decorators(field.decorators(), 1, out);
        let ty = field.type_ref();
        let q = if field.optional() { "?" } else { "" };
        line!(out, 1, "{}: {ty}{q}", field.name());
    }
    line!(out, 0, "}}");
}

fn dump_field(f: &Field<'_>, depth: usize, out: &mut Out) {
    dump_decorators(f.decorators(), depth, out);
    frag!(out, depth, "{} = ", f.name());
    if let Some(r) = f.reference() {
        match r {
            Ok(dr) => endln!(out, "&{}", dataref_label(&dr)),
            Err(e) => endln!(out, "<error: {e}>"),
        }
        return;
    }
    match f.value() {
        Ok(v) => endln!(out, "{v}"),
        Err(e) => endln!(out, "<error: {e}>"),
    }
}

fn dataref_label(dr: &wcl_lang::DataRef<'_>) -> String {
    if let Some(b) = dr.as_block()
        && let Ok(labels) = b.labels()
        && let Some(first) = labels.first()
    {
        return format!("{}({first})", dr.kind());
    }
    dr.kind().to_string()
}

fn dump_block(b: &Block<'_>, depth: usize, out: &mut Out) {
    dump_decorators(b.decorators(), depth, out);
    frag!(out, depth, "{}", b.kind());
    match b.labels() {
        Ok(labels) => {
            for label in labels {
                cont!(out, " {label}");
            }
        }
        Err(e) => {
            cont!(out, " <label error: {e}>");
        }
    }
    endln!(out, " {{");
    for f in b.fields() {
        dump_field(&f, depth + 1, out);
    }
    for inner in b.blocks() {
        dump_block(&inner, depth + 1, out);
    }
    for t in b.tables() {
        dump_table(&t, depth + 1, out);
    }
    line!(out, depth, "}}");
}

fn dump_table(t: &wcl_lang::TableView<'_>, depth: usize, out: &mut Out) {
    line!(out, depth, "{}:", t.field_name());
    for r in t.rows() {
        frag!(out, depth + 1, "|");
        match r.values() {
            Ok(vs) => {
                for v in vs {
                    cont!(out, " {} |", v);
                }
            }
            Err(e) => {
                cont!(out, " <row error: {e}> |");
            }
        }
        endln!(out, "");
    }
}

pub(crate) fn profile_to_json(p: &Profile) -> serde_json::Value {
    profile_node_to_json(p.root())
}

fn profile_node_to_json(n: &ProfileNode) -> serde_json::Value {
    serde_json::json!({
        "key": profile_key_to_json(&n.key),
        "count": n.count,
        "total_ns": n.total.as_nanos() as u64,
        "min_ns": if n.count == 0 { 0 } else { n.min.as_nanos() as u64 },
        "max_ns": n.max.as_nanos() as u64,
        "mean_ns": n.mean().as_nanos() as u64,
        "children": n
            .children
            .values()
            .map(profile_node_to_json)
            .collect::<Vec<_>>(),
    })
}

fn profile_key_to_json(k: &ProfileKey) -> serde_json::Value {
    match k {
        ProfileKey::Root => serde_json::json!({ "kind": "root" }),
        ProfileKey::Field { path } => serde_json::json!({ "kind": "field", "path": path }),
        ProfileKey::UserFn { name } => serde_json::json!({ "kind": "user_fn", "name": name }),
        ProfileKey::Builtin { name } => serde_json::json!({ "kind": "builtin", "name": name }),
    }
}
