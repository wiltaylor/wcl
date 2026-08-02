//! `textDocument/completion` handler. The cursor's preceding byte
//! decides which catalogue to propose:
//!
//!   - `@` → every type carrying `@decorator(...)` plus the builtins
//!   - `:` (in type-ref position) → every declared type/union + builtin types
//!   - `&` → same list as `:`
//!   - anything else (manual invoke / identifier letter) → locals in the
//!     enclosing scope, then top-level fields, then registered builtins.

use std::collections::HashSet;

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind};
use wcl_lang::{DeclName, Document, SymbolKind, parse_for_edit};

use crate::resolve::preceding_non_ws;
use crate::walk;

/// Push a completion item, skipping it when a same-`label` item was
/// already proposed. Centralises the `seen`-set dedup + the
/// `CompletionItem { .. ..Default }` literal repeated by every builder.
fn push_unique(
    out: &mut Vec<CompletionItem>,
    seen: &mut HashSet<String>,
    label: String,
    kind: CompletionItemKind,
    detail: String,
) {
    if seen.insert(label.clone()) {
        out.push(CompletionItem {
            label,
            kind: Some(kind),
            detail: Some(detail),
            ..Default::default()
        });
    }
}

/// Builtin decorator names (registered by `Environment`, not declared
/// in source). Listed here because `Environment` exposes no
/// enumeration accessor.
const BUILTIN_DECORATORS: &[&str] = &[
    "document",
    "schemaless",
    "block",
    "table",
    "child",
    "children",
    "inline",
    "default",
    "decorator",
    "contextual",
    "declares_kind",
];

/// Builtin scalar / string type names the user can write in a type
/// position. Mirrors the set parsed by `value::BuiltinType`.
const BUILTIN_TYPES: &[&str] = &[
    "bool", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
    "f32", "f64", "utf8", "ascii", "utf16", "utf32",
];

pub(crate) fn completions(
    source: &str,
    uri: &str,
    offset: usize,
    root_doc: Option<&Document>,
) -> Vec<CompletionItem> {
    // Parse the source on a best-effort basis: even if it fails (the
    // user is mid-typing), we still emit builtins. The per-file
    // `doc` is what gives us the right local scope for *this* file
    // (namespace, top-level fields). When a root document is also
    // available it contributes cross-file type / decorator / field
    // completions.
    let local_doc = Document::open(source, uri).ok();
    match preceding_non_ws(source, offset) {
        Some(b'@') => decorator_items(local_doc.as_ref(), root_doc),
        Some(b':') | Some(b'&') => type_items(local_doc.as_ref(), root_doc),
        _ => identifier_items(local_doc.as_ref(), root_doc, source, uri, offset),
    }
}

fn decorator_items(
    local_doc: Option<&Document>,
    root_doc: Option<&Document>,
) -> Vec<CompletionItem> {
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for name in BUILTIN_DECORATORS {
        push_unique(
            &mut out,
            &mut seen,
            (*name).to_string(),
            CompletionItemKind::FUNCTION,
            "builtin decorator".to_string(),
        );
    }
    for doc in [root_doc, local_doc].into_iter().flatten() {
        // Types carrying `@decorator("foo")` declare decorator `foo`.
        for td in doc.type_decls() {
            for d in td.decorators() {
                if d.full_name() != "decorator" {
                    continue;
                }
                let Ok(args) = d.positional() else { continue };
                let Some(first) = args.into_iter().next() else {
                    continue;
                };
                let label = match first {
                    wcl_lang::Value::Utf8(s) | wcl_lang::Value::Ascii(s) => s,
                    _ => continue,
                };
                push_unique(
                    &mut out,
                    &mut seen,
                    label,
                    CompletionItemKind::FUNCTION,
                    format!("decorator (schema: {})", td.name_segments().join(".")),
                );
            }
        }
    }
    out
}

fn type_items(local_doc: Option<&Document>, root_doc: Option<&Document>) -> Vec<CompletionItem> {
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for name in BUILTIN_TYPES {
        push_unique(
            &mut out,
            &mut seen,
            (*name).to_string(),
            CompletionItemKind::STRUCT,
            "builtin type".to_string(),
        );
    }
    for doc in [root_doc, local_doc].into_iter().flatten() {
        for td in doc.type_decls() {
            push_unique(
                &mut out,
                &mut seen,
                td.name_segments().join("."),
                CompletionItemKind::CLASS,
                "type".to_string(),
            );
        }
        for u in doc.union_decls() {
            push_unique(
                &mut out,
                &mut seen,
                u.name_segments().join("."),
                CompletionItemKind::ENUM,
                "union".to_string(),
            );
        }
        for i in doc.interfaces() {
            push_unique(
                &mut out,
                &mut seen,
                i.name_segments().join("."),
                CompletionItemKind::INTERFACE,
                "interface".to_string(),
            );
        }
    }
    out
}

/// Catalog for an identifier-position cursor (no trigger char).
/// Combines locals (params + let-bindings) from the enclosing scope,
/// top-level field names, and registered builtin functions.
fn identifier_items(
    local_doc: Option<&Document>,
    root_doc: Option<&Document>,
    source: &str,
    uri: &str,
    offset: usize,
) -> Vec<CompletionItem> {
    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // Locals — only computable when we have a parseable AST.
    if let Ok(ast) = parse_for_edit(source, uri) {
        let scopes = walk::enclosing_scopes_at(&ast.items, offset);
        // Inner-most first so they outrank outer same-name entries.
        for p in scopes.params.iter().rev() {
            push_unique(
                &mut out,
                &mut seen,
                p.name.clone(),
                CompletionItemKind::VARIABLE,
                "parameter".to_string(),
            );
        }
        for lb in scopes.lets.iter().rev() {
            push_unique(
                &mut out,
                &mut seen,
                lb.name.clone(),
                CompletionItemKind::VARIABLE,
                "let binding".to_string(),
            );
        }
        // Match-arm / if-let pattern bindings (inner-most first).
        for (name, _) in scopes.bindings.iter().rev() {
            push_unique(
                &mut out,
                &mut seen,
                (*name).to_string(),
                CompletionItemKind::VARIABLE,
                "pattern binding".to_string(),
            );
        }
    }

    for doc in [root_doc, local_doc].into_iter().flatten() {
        for rec in doc.symbols().iter() {
            if !matches!(rec.kind, SymbolKind::Field) {
                continue;
            }
            let short = rec.fqn.rsplit('.').next().unwrap_or(&rec.fqn).to_string();
            push_unique(
                &mut out,
                &mut seen,
                short,
                CompletionItemKind::FIELD,
                format!("field — {}", rec.fqn),
            );
        }
        for (name, f) in doc.environment().builtins() {
            let detail = match f.signature() {
                Some(sig) => format!("builtin {sig}"),
                None => format!("builtin fn ({} args)", f.arity()),
            };
            push_unique(
                &mut out,
                &mut seen,
                name.to_string(),
                CompletionItemKind::FUNCTION,
                detail,
            );
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(items: Vec<CompletionItem>) -> Vec<String> {
        items.into_iter().map(|c| c.label).collect()
    }

    #[test]
    fn at_prefix_lists_builtin_decorators_and_declared_ones() {
        // Cursor sits right after the standalone `@` on the last line.
        // Trailing newline keeps the document parseable so user-declared
        // decorators show up alongside the builtin ones.
        let src =
            "@decorator(\"max_len\")\ntype MaxLen {\n  value: u64\n}\n@\ntype Trailing {\n}\n";
        let cursor = src.find("\n@\n").unwrap() + 2; // just past the `@`
        let labs = labels(completions(src, "test.wcl", cursor, None));
        assert!(labs.iter().any(|l| l == "block"), "{labs:?}");
        assert!(labs.iter().any(|l| l == "max_len"), "{labs:?}");
    }

    #[test]
    fn colon_prefix_lists_types() {
        // Field type slot is empty (`v: \n`) — still parses as the
        // parser tolerates a missing type? If not, the test relies on
        // the builtins-only fallback. Use complete source to be safe:
        let src = "@document\ntype Root {\n  v: utf8\n}\ntype Other {\n  v: Root\n}\n";
        let cursor = src.find("v: Root").unwrap() + 2; // just past the `:`
        let labs = labels(completions(src, "test.wcl", cursor, None));
        assert!(labs.iter().any(|l| l == "utf8"), "{labs:?}");
        assert!(labs.iter().any(|l| l == "Root"), "{labs:?}");
    }

    #[test]
    fn no_trigger_lists_locals_fields_and_builtins() {
        // Cursor sits in an expression body so `helper` (a let) is in
        // scope; `host` is a top-level field; `len` is a builtin.
        let src = "host = \"a\"\nx = {\n  let helper = 1;\n  he\n}\n";
        let cursor = src.find("  he\n").unwrap() + 4;
        let labs = labels(completions(src, "test.wcl", cursor, None));
        assert!(labs.iter().any(|l| l == "helper"), "{labs:?}");
        assert!(labs.iter().any(|l| l == "host"), "{labs:?}");
        assert!(labs.iter().any(|l| l == "len"), "{labs:?}");
    }

    #[test]
    fn no_trigger_lists_function_params() {
        let src = "x = fn (input: i32) -> i32 { in }\n";
        let cursor = src.find("{ in ").unwrap() + 2;
        let labs = labels(completions(src, "test.wcl", cursor, None));
        assert!(labs.iter().any(|l| l == "input"), "{labs:?}");
    }
}
