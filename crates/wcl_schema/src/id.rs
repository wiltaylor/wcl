use std::collections::HashMap;
use wcl_core::ast::*;
use wcl_core::diagnostic::{Diagnostic, DiagnosticBag};
use wcl_core::span::Span;

/// Track block IDs for uniqueness enforcement (spec Section 23).
///
/// Within a given scope path the same ID may appear twice only if *both*
/// occurrences carry the `partial` flag — those two blocks will be merged
/// rather than being treated as duplicates.
#[derive(Debug, Default)]
pub struct IdRegistry {
    /// (scope_path, id) → (first_span, first_is_partial)
    ids: HashMap<(String, String), (Span, bool)>,
}

impl IdRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Walk the whole document and report any ID uniqueness violations.
    pub fn check_document(&mut self, doc: &Document, diagnostics: &mut DiagnosticBag) {
        self.check_items(&doc.items, "", diagnostics);
    }

    fn check_items(
        &mut self,
        items: &[DocItem],
        scope_path: &str,
        diagnostics: &mut DiagnosticBag,
    ) {
        for item in items {
            if let DocItem::Body(body_item) = item {
                self.check_body_item(body_item, scope_path, diagnostics);
            }
        }
    }

    fn check_body_item(
        &mut self,
        item: &BodyItem,
        scope_path: &str,
        diagnostics: &mut DiagnosticBag,
    ) {
        if let BodyItem::Block(block) = item {
            self.check_block(block, scope_path, diagnostics);
        }
    }

    fn check_block(&mut self, block: &Block, scope_path: &str, diagnostics: &mut DiagnosticBag) {
        // Determine the child scope path for nested blocks.
        let child_path;

        if let Some(inline_id) = &block.inline_id {
            match inline_id {
                InlineId::Literal(lit) => {
                    let id_str = &lit.value;
                    let key = (scope_path.to_string(), id_str.clone());

                    if let Some((first_span, first_partial)) = self.ids.get(&key).copied() {
                        // Two non-partial blocks with the same ID — error.
                        // Two partial blocks — allowed (they will be merged).
                        if !block.partial || !first_partial {
                            diagnostics.error_with_code(
                                format!("duplicate id '{}' in scope", id_str),
                                block.span,
                                "E030",
                            );
                            diagnostics.add(
                                Diagnostic::error("first defined here", first_span)
                                    .with_code("E030"),
                            );
                        }
                        // Keep the first registration; do not update.
                    } else {
                        self.ids.insert(key, (block.span, block.partial));
                    }

                    child_path = format!("{}/{}.{}", scope_path, block.kind.name, id_str);
                }
                InlineId::Interpolated(_) => {
                    // Interpolated IDs are dynamic — we cannot check them statically.
                    child_path = scope_path.to_string();
                }
            }
        } else {
            child_path = scope_path.to_string();
        }

        // Recurse into nested body items.
        for child in &block.body {
            self.check_body_item(child, &child_path, diagnostics);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wcl_core::span::{FileId, Span};
    use wcl_core::trivia::Trivia;

    fn sp() -> Span {
        Span::new(FileId(0), 0, 1)
    }

    fn make_ident(name: &str) -> Ident {
        Ident {
            name: name.to_string(),
            span: sp(),
        }
    }

    fn make_inline_id(value: &str) -> InlineId {
        InlineId::Literal(IdentifierLit {
            value: value.to_string(),
            span: sp(),
        })
    }

    fn make_block(kind: &str, id: Option<&str>, partial: bool) -> Block {
        Block {
            decorators: vec![],
            partial,
            kind: make_ident(kind),
            inline_id: id.map(make_inline_id),
            labels: vec![],
            body: vec![],
            text_content: None,
            trivia: Trivia::default(),
            span: Span::new(FileId(0), 0, 10),
        }
    }

    fn make_doc(blocks: Vec<Block>) -> Document {
        Document {
            items: blocks
                .into_iter()
                .map(|b| DocItem::Body(BodyItem::Block(b)))
                .collect(),
            trivia: Trivia::default(),
            span: sp(),
        }
    }

    #[test]
    fn unique_ids_no_error() {
        let doc = make_doc(vec![
            make_block("service", Some("alpha"), false),
            make_block("service", Some("beta"), false),
        ]);
        let mut reg = IdRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.check_document(&doc, &mut diags);
        assert!(!diags.has_errors());
    }

    #[test]
    fn duplicate_non_partial_is_error() {
        let doc = make_doc(vec![
            make_block("service", Some("alpha"), false),
            make_block("service", Some("alpha"), false),
        ]);
        let mut reg = IdRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.check_document(&doc, &mut diags);
        // Two errors: one for the duplicate block, one for "first defined here"
        assert!(diags.has_errors());
        assert!(diags.error_count() >= 1);
        // The primary error carries code E030
        let primary = diags.diagnostics().iter().find(|d| {
            d.code.as_deref() == Some("E030") && d.message.contains("duplicate id 'alpha'")
        });
        assert!(
            primary.is_some(),
            "expected an E030 'duplicate id' diagnostic"
        );
    }

    #[test]
    fn two_partial_blocks_same_id_allowed() {
        let doc = make_doc(vec![
            make_block("service", Some("alpha"), true),
            make_block("service", Some("alpha"), true),
        ]);
        let mut reg = IdRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.check_document(&doc, &mut diags);
        assert!(!diags.has_errors());
    }

    #[test]
    fn partial_and_non_partial_same_id_is_error() {
        let doc = make_doc(vec![
            make_block("service", Some("alpha"), false),
            make_block("service", Some("alpha"), true),
        ]);
        let mut reg = IdRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.check_document(&doc, &mut diags);
        assert!(diags.has_errors());
    }

    #[test]
    fn blocks_without_ids_never_conflict() {
        // Blocks with no inline ID should not conflict with each other.
        let doc = make_doc(vec![
            make_block("service", None, false),
            make_block("service", None, false),
        ]);
        let mut reg = IdRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.check_document(&doc, &mut diags);
        assert!(!diags.has_errors());
    }

    #[test]
    fn same_id_different_scope_no_error() {
        // "alpha" appears in two different top-level blocks; they live in
        // different child scopes so there is no conflict.
        let mut svc_alpha = make_block("service", Some("alpha"), false);
        svc_alpha.body = vec![BodyItem::Block(make_block("port", Some("http"), false))];

        let mut svc_beta = make_block("service", Some("beta"), false);
        svc_beta.body = vec![BodyItem::Block(make_block("port", Some("http"), false))];

        let doc = make_doc(vec![svc_alpha, svc_beta]);
        let mut reg = IdRegistry::new();
        let mut diags = DiagnosticBag::new();
        reg.check_document(&doc, &mut diags);
        assert!(!diags.has_errors());
    }
}
