//! `textDocument/hover` handler. Returns a small markdown chunk
//! describing the identifier under the cursor: what kind of thing it
//! is, plus a fenced snippet of its source.

use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind};
use wcl_lang::{Document, parse_for_edit};

use crate::convert::span_to_range;
use crate::resolve::{self, LocatedSymbol};

pub(crate) fn hover(
    source: &str,
    uri: &str,
    offset: usize,
    root_doc: Option<&Document>,
) -> Option<Hover> {
    let local_doc = Document::open(source, uri).ok();
    let ast = parse_for_edit(source, uri).ok()?;
    let (sym, span) = local_doc
        .as_ref()
        .and_then(|d| resolve::locate(d, &ast, source, offset))
        .or_else(|| root_doc.and_then(|d| resolve::locate(d, &ast, source, offset)))?;
    // The declaration's source text is needed for the hover snippet.
    // Prefer the per-file doc (cheap, in-memory); fall back to reading
    // the declaring file off disk via the root doc's symbol hit.
    let snippet = hover_snippet(local_doc.as_ref(), root_doc, &sym, source);
    let body = format!(
        "**{kind}** `{name}`\n\n```wcl\n{snippet}\n```",
        kind = kind_label(&sym),
        name = display_name(&sym),
        snippet = snippet.as_deref().unwrap_or("<no source>"),
    );
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: body,
        }),
        range: Some(span_to_range(source, span)),
    })
}

/// Slice the declaration text out of either the request file's
/// source (local symbol) or the declaring imported file on disk
/// (cross-file symbol).
fn hover_snippet(
    local_doc: Option<&Document>,
    root_doc: Option<&Document>,
    sym: &LocatedSymbol,
    source: &str,
) -> Option<String> {
    if let Some(d) = local_doc
        && let Some(s) = resolve::declaration_span(d, sym)
        && let Some(text) = source.get(s.start..s.end)
    {
        return Some(text.to_string());
    }
    let root = root_doc?;
    let fqn = match sym {
        LocatedSymbol::Type(f)
        | LocatedSymbol::Decorator(f)
        | LocatedSymbol::BlockKind(f)
        | LocatedSymbol::Field(f) => f.as_str(),
        _ => return None,
    };
    let hit = root.find_symbol(fqn)?;
    let path = hit.source_path?;
    let text = std::fs::read_to_string(path).ok()?;
    text.get(hit.record.span.start..hit.record.span.end)
        .map(str::to_string)
}

fn kind_label(sym: &LocatedSymbol) -> &'static str {
    match sym {
        LocatedSymbol::Type(_) => "type",
        LocatedSymbol::Decorator(_) => "decorator",
        LocatedSymbol::BlockKind(_) => "block kind",
        LocatedSymbol::UnionVariant { .. } => "variant",
        LocatedSymbol::SymbolEntry { .. } => "symbol",
        LocatedSymbol::Field(_) => "field",
        LocatedSymbol::Local { .. } => "local",
    }
}

fn display_name(sym: &LocatedSymbol) -> String {
    match sym {
        LocatedSymbol::Type(f)
        | LocatedSymbol::Decorator(f)
        | LocatedSymbol::BlockKind(f)
        | LocatedSymbol::Field(f) => f.clone(),
        LocatedSymbol::UnionVariant { union, variant } => format!("{union}.{variant}"),
        LocatedSymbol::SymbolEntry { set, entry } => format!("{set}.{entry}"),
        LocatedSymbol::Local { name, .. } => name.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hover_on_block_kind_includes_decl_snippet() {
        let src = "@document\ntype Root {\n  c: Config\n}\n@block(\"config\")\ntype Config {\n  region: utf8\n}\nconfig {\n  region = \"x\"\n}\n";
        let cursor = src.find("config {").unwrap() + 2;
        let h = hover(src, "test.wcl", cursor, None).expect("hover present");
        let HoverContents::Markup(m) = h.contents else {
            panic!("expected markup")
        };
        assert!(m.value.contains("block kind"));
        assert!(m.value.contains("type Config"));
    }

    #[test]
    fn hover_returns_none_on_whitespace() {
        let src = "type Foo {\n}\n";
        let cursor = 4; // a space
        assert!(hover(src, "test.wcl", cursor, None).is_none());
    }
}
