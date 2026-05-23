//! `textDocument/completion` handler. Trigger-character driven; the
//! cursor's preceding byte (`@`, `:`, `&`) decides which catalogue to
//! propose. v1 covers the suggestions backed directly by the document's
//! own declarations:
//!
//!   - `@` → every type carrying `@decorator(...)` plus the builtins
//!   - `:` (in type-ref position) → every declared type/union + builtin types
//!   - `&` → same list as `:`
//!
//! Bare-identifier completion in expression bodies needs scope info
//! the symbol index doesn't carry yet; that's deferred.

use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind};
use wcl_lang::{DeclName, Document};

use crate::resolve::preceding_non_ws;

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
];

/// Builtin scalar / string type names the user can write in a type
/// position. Mirrors the set parsed by `value::BuiltinType`.
const BUILTIN_TYPES: &[&str] = &[
    "bool", "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize",
    "f32", "f64", "utf8", "ascii", "utf16", "utf32",
];

pub(crate) fn completions(source: &str, uri: &str, offset: usize) -> Vec<CompletionItem> {
    // Parse the source on a best-effort basis: even if it fails (the
    // user is mid-typing), we still emit builtins.
    let doc = Document::open(source, uri).ok();
    match preceding_non_ws(source, offset) {
        Some(b'@') => decorator_items(doc.as_ref()),
        Some(b':') | Some(b'&') => type_items(doc.as_ref()),
        _ => Vec::new(),
    }
}

fn decorator_items(doc: Option<&Document>) -> Vec<CompletionItem> {
    let mut out = Vec::new();
    for name in BUILTIN_DECORATORS {
        out.push(CompletionItem {
            label: (*name).to_string(),
            kind: Some(CompletionItemKind::FUNCTION),
            detail: Some("builtin decorator".to_string()),
            ..Default::default()
        });
    }
    let Some(doc) = doc else { return out };
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
            out.push(CompletionItem {
                label,
                kind: Some(CompletionItemKind::FUNCTION),
                detail: Some(format!(
                    "decorator (schema: {})",
                    td.name_segments().join(".")
                )),
                ..Default::default()
            });
        }
    }
    out
}

fn type_items(doc: Option<&Document>) -> Vec<CompletionItem> {
    let mut out = Vec::new();
    for name in BUILTIN_TYPES {
        out.push(CompletionItem {
            label: (*name).to_string(),
            kind: Some(CompletionItemKind::STRUCT),
            detail: Some("builtin type".to_string()),
            ..Default::default()
        });
    }
    let Some(doc) = doc else { return out };
    for td in doc.type_decls() {
        out.push(CompletionItem {
            label: td.name_segments().join("."),
            kind: Some(CompletionItemKind::CLASS),
            detail: Some("type".to_string()),
            ..Default::default()
        });
    }
    for u in doc.union_decls() {
        out.push(CompletionItem {
            label: u.name_segments().join("."),
            kind: Some(CompletionItemKind::ENUM),
            detail: Some("union".to_string()),
            ..Default::default()
        });
    }
    for i in doc.interfaces() {
        out.push(CompletionItem {
            label: i.name_segments().join("."),
            kind: Some(CompletionItemKind::INTERFACE),
            detail: Some("interface".to_string()),
            ..Default::default()
        });
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
        let labs = labels(completions(src, "test.wcl", cursor));
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
        let labs = labels(completions(src, "test.wcl", cursor));
        assert!(labs.iter().any(|l| l == "utf8"), "{labs:?}");
        assert!(labs.iter().any(|l| l == "Root"), "{labs:?}");
    }

    #[test]
    fn no_trigger_returns_empty() {
        let src = "@document\ntype Root {\n  v: utf8\n}\n";
        let labs = labels(completions(src, "test.wcl", 0));
        assert!(labs.is_empty());
    }
}
