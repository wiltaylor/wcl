//! `textDocument/hover` handler. Returns a small markdown chunk
//! describing the identifier under the cursor: what kind of thing it
//! is, plus a fenced snippet of its source.

use tower_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind};
use wcl_lang::Document;

use crate::convert::span_to_range;
use crate::resolve::{self, LocatedSymbol};

pub(crate) fn hover(
    source: &str,
    uri: &str,
    offset: usize,
    root_doc: Option<&Document>,
) -> Option<Hover> {
    let (sym, span, local_doc) = resolve::locate_at(source, uri, offset, root_doc)?;
    // The declaration's source text is needed for the hover snippet.
    // Prefer the per-file doc (cheap, in-memory); fall back to reading
    // the declaring file off disk via the root doc's symbol hit.
    let snippet = hover_snippet(local_doc.as_ref(), root_doc, &sym, source);
    let docs = hover_doc_comment(local_doc.as_ref(), root_doc, &sym)
        .map(|comment| format!("\n\n{comment}"))
        .unwrap_or_default();
    let body = format!(
        "**{kind}** `{name}`{docs}\n\n```wcl\n{snippet}\n```",
        kind = sym.kind_label(),
        name = sym.display_name(),
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
    let fqn = sym.simple_fqn()?;
    let hit = root.find_symbol(fqn)?;
    let path = hit.source_path?;
    let text = std::fs::read_to_string(path).ok()?;
    text.get(hit.record.span.start..hit.record.span.end)
        .map(str::to_string)
}

fn hover_doc_comment(
    local_doc: Option<&Document>,
    root_doc: Option<&Document>,
    sym: &LocatedSymbol,
) -> Option<String> {
    let LocatedSymbol::Decorator(fqn) = sym else {
        return None;
    };
    local_doc
        .and_then(|doc| doc.type_decl(fqn))
        .and_then(|declaration| declaration.doc_comment())
        .or_else(|| {
            root_doc
                .and_then(|doc| doc.type_decl(fqn))
                .and_then(|declaration| declaration.doc_comment())
        })
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
    fn hover_on_decorator_includes_doc_comment_and_argument_shape() {
        let src = r#"# Deployment metadata.
@decorator("deploy")
type Deploy { @inline(0) target: utf8 }
@deploy("prod")
type Target {}
"#;
        let cursor = src.rfind("deploy").expect("decorator use") + 2;
        let h = hover(src, "test.wcl", cursor, None).expect("hover present");
        let HoverContents::Markup(markup) = h.contents else {
            panic!("expected markup")
        };

        assert!(markup.value.contains("Deployment metadata."), "{markup:#?}");
        assert!(markup.value.contains("target: utf8"), "{markup:#?}");
    }

    #[test]
    fn hover_returns_none_on_whitespace() {
        let src = "type Foo {\n}\n";
        let cursor = 4; // a space
        assert!(hover(src, "test.wcl", cursor, None).is_none());
    }
}
