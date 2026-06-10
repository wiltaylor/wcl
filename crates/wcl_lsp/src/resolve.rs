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

use wcl_lang::{DeclName, Document, Span, SymbolKind, SymbolRecord, ast, parse_for_edit};

use crate::scan::is_ident_byte;
use crate::walk;

/// What was identified at the cursor. Variants carry the FQN of the
/// declaration we can point a client at.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum LocatedSymbol {
    Type(String),
    Decorator(String),
    BlockKind(String),
    UnionVariant {
        union: String,
        variant: String,
    },
    SymbolEntry {
        set: String,
        entry: String,
    },
    /// Top-level field. `name` is the FQN suitable for `SymbolIndex::lookup`.
    Field(String),
    /// Local binding (function parameter or `let`). The declaration
    /// span is carried inline since `SymbolIndex` doesn't index locals.
    Local {
        name: String,
        decl_span: Span,
    },
}

impl LocatedSymbol {
    /// The FQN for the variants that index a single declaration directly
    /// (`SymbolIndex::lookup`-able). `Local`/`UnionVariant`/`SymbolEntry`
    /// return `None` — they either carry their span inline or resolve
    /// through a composed parent path.
    pub(crate) fn simple_fqn(&self) -> Option<&str> {
        match self {
            LocatedSymbol::Type(f)
            | LocatedSymbol::Decorator(f)
            | LocatedSymbol::BlockKind(f)
            | LocatedSymbol::Field(f) => Some(f.as_str()),
            LocatedSymbol::Local { .. }
            | LocatedSymbol::UnionVariant { .. }
            | LocatedSymbol::SymbolEntry { .. } => None,
        }
    }

    /// One-word label for the hover header.
    pub(crate) fn kind_label(&self) -> &'static str {
        match self {
            LocatedSymbol::Type(_) => "type",
            LocatedSymbol::Decorator(_) => "decorator",
            LocatedSymbol::BlockKind(_) => "block kind",
            LocatedSymbol::UnionVariant { .. } => "variant",
            LocatedSymbol::SymbolEntry { .. } => "symbol",
            LocatedSymbol::Field(_) => "field",
            LocatedSymbol::Local { .. } => "local",
        }
    }

    /// Human-facing name for display (dotted for nested variants/entries).
    pub(crate) fn display_name(&self) -> String {
        match self {
            LocatedSymbol::Type(f)
            | LocatedSymbol::Decorator(f)
            | LocatedSymbol::BlockKind(f)
            | LocatedSymbol::Field(f) => f.clone(),
            LocatedSymbol::UnionVariant { union, variant } => format!("{union}.{variant}"),
            LocatedSymbol::SymbolEntry { set, entry } => format!("{set}.{entry}"),
            LocatedSymbol::Local { name, .. } => name.clone(),
        }
    }
}

/// Resolve the identifier under `offset`. Returns the discovered
/// symbol plus the span of the on-screen identifier (so callers can
/// echo it back as the LSP `selectionRange` / range).
pub(crate) fn locate(
    doc: &Document,
    ast: &ast::Source,
    source: &str,
    offset: usize,
) -> Option<(LocatedSymbol, Span)> {
    let (word, span) = word_at(source, offset)?;
    // If the cursor sits on the last segment of a dotted reference
    // (e.g. the `Color` in `shared.Color`), reconstruct the full
    // dotted form so we can resolve it against `find_symbol`
    // directly. The on-screen span stays on the bare segment so
    // editors highlight only what the user clicked.
    if let Some(dot) = dotted_form(source, span).as_deref()
        && let Some(hit) = doc.find_symbol(dot)
        && let Some(located) = classify(hit.record)
    {
        return Some((located, span));
    }

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
    // share the FQN namespace of SymbolIndex. `find_symbol` walks
    // every imported file, so a bare identifier in one file can
    // resolve to a declaration in another namespace if a prefix
    // candidate matches.
    for fqn in candidate_fqns(&word, doc) {
        if let Some(hit) = doc.find_symbol(&fqn)
            && let Some(located) = classify(hit.record)
        {
            return Some((located, span));
        }
    }

    // Local-scope binding (function parameter / let-binding) in the
    // enclosing scope at `offset`. Inner shadowing wins by scanning
    // the outer→inner list in reverse.
    let scopes = walk::enclosing_scopes_at(&ast.items, offset);
    if let Some(p) = scopes.params.iter().rev().find(|p| p.name == word) {
        return Some((
            LocatedSymbol::Local {
                name: p.name.clone(),
                decl_span: p.span,
            },
            span,
        ));
    }
    if let Some(lb) = scopes.lets.iter().rev().find(|l| l.name == word) {
        return Some((
            LocatedSymbol::Local {
                name: lb.name.clone(),
                decl_span: lb.span,
            },
            span,
        ));
    }
    // Match-arm / if-let pattern bindings, innermost first.
    if let Some((name, decl_span)) = scopes.bindings.iter().rev().find(|(n, _)| *n == word) {
        return Some((
            LocatedSymbol::Local {
                name: (*name).to_string(),
                decl_span: *decl_span,
            },
            span,
        ));
    }

    // Top-level field as a last resort. SymbolIndex carries fields,
    // but `classify()` deliberately ignores them so we only treat them
    // as a navigation target here.
    for fqn in candidate_fqns(&word, doc) {
        if let Some(rec) = doc.symbols().lookup(&fqn)
            && matches!(rec.kind, SymbolKind::Field)
        {
            return Some((LocatedSymbol::Field(rec.fqn.clone()), span));
        }
    }

    None
}

/// Open `source` per-file and resolve the identifier at `offset`,
/// falling back to `root_doc` when the per-file open can't resolve it
/// (common when the file references cross-file types). Returns the
/// located symbol, its on-screen span, and the owned per-file
/// [`Document`] (which several callers still need for follow-up
/// lookups). Shared by go-to-definition, find-references, and hover.
pub(crate) fn locate_at(
    source: &str,
    uri: &str,
    offset: usize,
    root_doc: Option<&Document>,
) -> Option<(LocatedSymbol, Span, Option<Document>)> {
    let local_doc = Document::open(source, uri).ok();
    // `parse_for_edit` is purely syntactic, so the AST is available even
    // when neither doc type-checks.
    let ast = parse_for_edit(source, uri).ok()?;
    let (sym, span) = local_doc
        .as_ref()
        .and_then(|d| locate(d, &ast, source, offset))
        .or_else(|| root_doc.and_then(|d| locate(d, &ast, source, offset)))?;
    Some((sym, span, local_doc))
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

/// If the bare identifier at `span` is the tail of a dotted path
/// (`foo.bar.baz`), return the full dotted form. Otherwise `None`.
/// Walks left from `span.start` collecting `.<ident>` prefixes; stops
/// at any non-identifier / non-dot byte.
fn dotted_form(source: &str, span: Span) -> Option<String> {
    let bytes = source.as_bytes();
    if span.start == 0 || bytes.get(span.start - 1) != Some(&b'.') {
        return None;
    }
    let mut segments: Vec<&str> = Vec::new();
    segments.push(&source[span.start..span.end]);
    let mut i = span.start;
    while i >= 2 && bytes[i - 1] == b'.' {
        let mut start = i - 1;
        while start > 0 && is_ident_byte(bytes[start - 1]) {
            start -= 1;
        }
        if start == i - 1 {
            break;
        }
        segments.push(&source[start..i - 1]);
        i = start;
    }
    segments.reverse();
    Some(segments.join("."))
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
        | SymbolKind::SymbolSetDecl
        | SymbolKind::FnDecl => Some(LocatedSymbol::Type(rec.fqn.clone())),
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
    match sym {
        LocatedSymbol::Local { decl_span, .. } => Some(*decl_span),
        LocatedSymbol::UnionVariant { union, variant } => symbols
            .lookup(&format!("{union}.{variant}"))
            .map(|r| r.span),
        LocatedSymbol::SymbolEntry { set, entry } => {
            symbols.lookup(&format!("{set}.{entry}")).map(|r| r.span)
        }
        // Type / Decorator / BlockKind / Field — direct FQN lookup.
        _ => symbols.lookup(sym.simple_fqn()?).map(|r| r.span),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wcl_lang::parse_for_edit;

    fn doc(src: &str) -> Document {
        Document::open(src, "test.wcl").expect("parse ok")
    }

    fn ast(src: &str) -> wcl_lang::ast::Source {
        parse_for_edit(src, "test.wcl").expect("parse ok")
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
        let a = ast(src);
        let cursor = src.find("config {").unwrap() + 2;
        let (sym, _) = locate(&d, &a, src, cursor).expect("locate found something");
        assert_eq!(sym, LocatedSymbol::BlockKind("Config".to_string()));
    }

    #[test]
    fn locate_resolves_decorator() {
        let src = "@decorator(\"max_len\")\ntype MaxLen {\n  value: u64\n}\n@max_len(value = 5u64)\ntype X {\n  v: utf8\n}\n";
        let d = doc(src);
        let a = ast(src);
        let cursor = src.find("@max_len").unwrap() + 2;
        let (sym, _) = locate(&d, &a, src, cursor).expect("locate found something");
        assert_eq!(sym, LocatedSymbol::Decorator("MaxLen".to_string()));
    }

    #[test]
    fn locate_resolves_type_name() {
        let src = "@document\ntype Root {\n  name: utf8\n}\ntype Other {\n  v: Root\n}\n";
        let d = doc(src);
        let a = ast(src);
        let cursor = src.find("v: Root").unwrap() + 3;
        let (sym, _) = locate(&d, &a, src, cursor).expect("locate found something");
        assert_eq!(sym, LocatedSymbol::Type("Root".to_string()));
    }

    #[test]
    fn locate_resolves_local_let_binding() {
        let src = "x = {\n  let helper = 1;\n  helper + 2\n}\n";
        let d = doc(src);
        let a = ast(src);
        let cursor = src.find("helper + 2").unwrap() + 2;
        let (sym, _) = locate(&d, &a, src, cursor).expect("locate found something");
        assert!(matches!(sym, LocatedSymbol::Local { ref name, .. } if name == "helper"));
    }

    #[test]
    fn locate_resolves_match_pattern_binding() {
        let src = "x = match c1 {\n  Shape::Circle { radius, .. } => radius,\n  _ => 0.0,\n}\n";
        let d = doc(src);
        let a = ast(src);
        let cursor = src.find("=> radius").unwrap() + 3;
        let (sym, _) = locate(&d, &a, src, cursor).expect("locate found something");
        assert!(matches!(sym, LocatedSymbol::Local { ref name, .. } if name == "radius"));
    }

    #[test]
    fn locate_falls_back_to_top_level_field() {
        let src = "host = \"a\"\nport = 80u16\nwhere = host\n";
        let d = doc(src);
        let a = ast(src);
        // Cursor on the `host` reference in the RHS of `where = host`.
        let cursor = src.find("= host").unwrap() + 2;
        let (sym, _) = locate(&d, &a, src, cursor).expect("locate found something");
        assert!(matches!(sym, LocatedSymbol::Field(ref f) if f == "host"));
    }
}
