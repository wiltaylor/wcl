//! Shared "what's under the cursor?" logic used by go-to-definition,
//! find-references, and hover.
//!
//! Strategy: pull the identifier substring out of the raw source at
//! the cursor offset, look at the preceding non-whitespace byte to
//! disambiguate (`@` ⇒ decorator, anything else ⇒ try block-kind /
//! type-name), and consult `Document::symbols()` / `block_schema()` /
//! `decorator_schema()` for a declaration to point at.
//!
//! The AST's `Expr::Identifier` carries no span (see `ast.rs:76`), so
//! we deliberately work off raw bytes rather than AST nodes for this
//! slice. That keeps us correct for the inputs we can resolve (type
//! refs, decorators, block kinds) and silent for the ones we can't
//! (bare identifiers inside expressions).

use wcl_lang::{DeclName, Document, Span, SymbolKind, SymbolRecord};

/// What was identified at the cursor. Variants carry the FQN of the
/// declaration we can point a client at.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LocatedSymbol {
    Type(String),
    Decorator(String),
    BlockKind(String),
    UnionVariant { union: String, variant: String },
    SymbolEntry { set: String, entry: String },
}

/// Resolve the identifier under `offset`. Returns the discovered
/// symbol plus the span of the on-screen identifier (so callers can
/// echo it back as the LSP `selectionRange` / range).
pub(crate) fn locate(doc: &Document, source: &str, offset: usize) -> Option<(LocatedSymbol, Span)> {
    let (word, span) = word_at(source, offset)?;

    // Decorator name → '@' immediately precedes the word.
    if preceding_non_ws(source, span.start) == Some(b'@') {
        if let Some(td) = doc.decorator_schema(&word) {
            return Some((LocatedSymbol::Decorator(td.name_segments().join(".")), span));
        }
        return None;
    }

    // Block kinds are unique at the start of a Block; `block_schema`
    // returns the @block(...) type decl when one is registered.
    if let Some(td) = doc.block_schema(&word) {
        return Some((LocatedSymbol::BlockKind(td.name_segments().join(".")), span));
    }

    // Type / interface / union / connection / symbol-set declarations
    // share the FQN namespace of SymbolIndex.
    for fqn in candidate_fqns(&word, doc) {
        if let Some(rec) = doc.symbols().lookup(&fqn)
            && let Some(located) = classify(rec)
        {
            return Some((located, span));
        }
    }

    None
}

/// Slice the identifier (`[A-Za-z_][A-Za-z0-9_]*`) that contains
/// `offset`. Returns `None` when the offset doesn't land on an
/// identifier byte (whitespace, punctuation, EOF).
pub(crate) fn word_at(source: &str, offset: usize) -> Option<(String, Span)> {
    let bytes = source.as_bytes();
    if offset > bytes.len() {
        return None;
    }
    let mut start = offset;
    // The cursor often sits *just after* the last char of a word
    // (e.g. when the editor passes the position at the end of an
    // identifier). Step back one byte in that case.
    if start == bytes.len() || !is_ident_byte(bytes[start]) {
        if start == 0 {
            return None;
        }
        if !is_ident_byte(bytes[start - 1]) {
            return None;
        }
        start -= 1;
    }
    while start > 0 && is_ident_byte(bytes[start - 1]) {
        start -= 1;
    }
    let mut end = start;
    while end < bytes.len() && is_ident_byte(bytes[end]) {
        end += 1;
    }
    if start == end {
        return None;
    }
    let s = std::str::from_utf8(&bytes[start..end]).ok()?.to_string();
    // Identifiers must not start with a digit.
    if s.as_bytes().first().is_some_and(|b| b.is_ascii_digit()) {
        return None;
    }
    Some((s, Span::new(start, end)))
}

fn is_ident_byte(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

/// Last non-whitespace byte strictly before `offset`. Used to detect
/// the `@` / `&` / `:` prefix that disambiguates the cursor context.
pub(crate) fn preceding_non_ws(source: &str, offset: usize) -> Option<u8> {
    let bytes = source.as_bytes();
    let mut i = offset.min(bytes.len());
    while i > 0 {
        i -= 1;
        if !bytes[i].is_ascii_whitespace() {
            return Some(bytes[i]);
        }
    }
    None
}

/// Plausible fully-qualified names for a bare identifier. We try
/// the namespace-prefixed form first (it's the more specific match),
/// then the bare name as a fallback.
fn candidate_fqns(word: &str, doc: &Document) -> Vec<String> {
    let mut out = Vec::with_capacity(2);
    let ns = doc.namespace();
    if !ns.is_empty() {
        out.push(format!("{}.{}", ns.join("."), word));
    }
    out.push(word.to_string());
    out
}

fn classify(rec: &SymbolRecord) -> Option<LocatedSymbol> {
    match &rec.kind {
        SymbolKind::TypeDecl
        | SymbolKind::InterfaceDecl
        | SymbolKind::UnionDecl
        | SymbolKind::ConnectionDecl
        | SymbolKind::SymbolSetDecl => Some(LocatedSymbol::Type(rec.fqn.clone())),
        SymbolKind::UnionVariant { parent_fqn } => Some(LocatedSymbol::UnionVariant {
            union: parent_fqn.clone(),
            variant: short_name(&rec.fqn).to_string(),
        }),
        SymbolKind::SymbolEntry { parent_fqn } => Some(LocatedSymbol::SymbolEntry {
            set: parent_fqn.clone(),
            entry: short_name(&rec.fqn).to_string(),
        }),
        // Top-level fields and type/interface members aren't useful
        // navigation targets in this slice — they resolve to
        // themselves at the cursor.
        SymbolKind::Field | SymbolKind::TypeField { .. } | SymbolKind::InterfaceField { .. } => {
            None
        }
    }
}

fn short_name(fqn: &str) -> &str {
    fqn.rsplit('.').next().unwrap_or(fqn)
}

/// Look up the declaration span that backs a `LocatedSymbol`. Returns
/// `None` for symbols (builtin decorators) that have no AST site.
pub(crate) fn declaration_span(doc: &Document, sym: &LocatedSymbol) -> Option<Span> {
    let symbols = doc.symbols();
    let fqn = match sym {
        LocatedSymbol::Type(f) | LocatedSymbol::Decorator(f) | LocatedSymbol::BlockKind(f) => {
            f.as_str()
        }
        LocatedSymbol::UnionVariant { union, variant } => {
            return symbols
                .lookup(&format!("{union}.{variant}"))
                .map(|r| r.span);
        }
        LocatedSymbol::SymbolEntry { set, entry } => {
            return symbols.lookup(&format!("{set}.{entry}")).map(|r| r.span);
        }
    };
    symbols.lookup(fqn).map(|r| r.span)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(src: &str) -> Document {
        Document::open(src, "test.wcl").expect("parse ok")
    }

    #[test]
    fn word_at_finds_identifier_inside() {
        let src = "hello world";
        let (w, s) = word_at(src, 2).unwrap();
        assert_eq!(w, "hello");
        assert_eq!((s.start, s.end), (0, 5));
    }

    #[test]
    fn word_at_steps_back_at_word_end() {
        let src = "hello world";
        // offset 5 is the space after "hello" — should still find "hello".
        let (w, _) = word_at(src, 5).unwrap();
        assert_eq!(w, "hello");
    }

    #[test]
    fn word_at_returns_none_on_punctuation() {
        // offset 2 is the '+' itself; both neighbours are non-ident.
        assert!(word_at("a + b", 2).is_none());
    }

    #[test]
    fn word_at_handles_underscores_and_digits() {
        let (w, _) = word_at("my_var2 = 1", 3).unwrap();
        assert_eq!(w, "my_var2");
    }

    #[test]
    fn locate_resolves_block_kind() {
        let src = "@document\ntype Root {\n  config: Config\n}\n@block(\"config\")\ntype Config {\n  region: utf8\n}\nconfig {\n  region = \"x\"\n}\n";
        let d = doc(src);
        let cursor = src.find("config {").unwrap() + 2;
        let (sym, _) = locate(&d, src, cursor).expect("locate found something");
        assert_eq!(sym, LocatedSymbol::BlockKind("Config".to_string()));
    }

    #[test]
    fn locate_resolves_decorator() {
        let src = "@decorator(\"max_len\")\ntype MaxLen {\n  value: u64\n}\n@max_len(value = 5u64)\ntype X {\n  v: utf8\n}\n";
        let d = doc(src);
        let cursor = src.find("@max_len").unwrap() + 2;
        let (sym, _) = locate(&d, src, cursor).expect("locate found something");
        assert_eq!(sym, LocatedSymbol::Decorator("MaxLen".to_string()));
    }

    #[test]
    fn locate_resolves_type_name() {
        let src = "@document\ntype Root {\n  name: utf8\n}\ntype Other {\n  v: Root\n}\n";
        let d = doc(src);
        let cursor = src.find("v: Root").unwrap() + 3;
        let (sym, _) = locate(&d, src, cursor).expect("locate found something");
        assert_eq!(sym, LocatedSymbol::Type("Root".to_string()));
    }
}
