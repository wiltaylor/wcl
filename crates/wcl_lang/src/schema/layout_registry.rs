//! WCL Layout Registry — collects and resolves layout definitions from AST.
//!
//! A layout block defines how structs compose into a complete binary or text
//! format. Each child block within a layout is a section that references a
//! struct definition and carries encoding configuration via decorators.

use crate::lang::ast::{Block, BodyItem, DecoratorArg, DocItem, Document, Expr, InlineId};
use crate::lang::diagnostic::DiagnosticBag;
use crate::transform::layout::encoding_from_decorators;
use crate::transform::layout::{CountSpec, LayoutDef, LayoutSection, SectionKind};
use indexmap::IndexMap;

/// Registry of all layout definitions in a document.
#[derive(Debug, Clone, Default)]
pub struct LayoutRegistry {
    pub layouts: IndexMap<String, LayoutDef>,
}

impl LayoutRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Collect all layout definitions from a parsed document.
    pub fn collect(&mut self, doc: &Document, diagnostics: &mut DiagnosticBag) {
        for item in &doc.items {
            if let DocItem::Body(BodyItem::Block(block)) = item {
                if block.kind.name == "layout" {
                    self.register_layout(block, diagnostics);
                }
            }
        }
    }

    /// Look up a layout definition by name.
    pub fn get(&self, name: &str) -> Option<&LayoutDef> {
        self.layouts.get(name)
    }

    fn register_layout(&mut self, block: &Block, diagnostics: &mut DiagnosticBag) {
        let name = match &block.inline_id {
            Some(InlineId::Literal(lit)) => lit.value.clone(),
            Some(InlineId::Interpolated(_)) => {
                diagnostics.error("layout name must be a plain identifier", block.span);
                return;
            }
            None => {
                diagnostics.error("layout block must have a name", block.span);
                return;
            }
        };

        if self.layouts.contains_key(&name) {
            diagnostics.error(format!("duplicate layout name '{}'", name), block.span);
            return;
        }

        let mut sections = Vec::new();

        for item in &block.body {
            if let BodyItem::Block(child) = item {
                let struct_name = child.kind.name.clone();
                let section_name = match &child.inline_id {
                    Some(InlineId::Literal(lit)) => lit.value.clone(),
                    _ => struct_name.clone(),
                };

                let kind = if child.decorators.iter().any(|d| d.name.name == "stream") {
                    SectionKind::Stream
                } else {
                    SectionKind::Structured
                };

                let count = extract_count_spec(&child.decorators);
                let encoding = encoding_from_decorators(&child.decorators);

                sections.push(LayoutSection {
                    name: section_name,
                    struct_name,
                    kind,
                    encoding,
                    count,
                });
            }
        }

        self.layouts
            .insert(name.clone(), LayoutDef { name, sections });
    }
}

/// Extract a `CountSpec` from decorator list, looking for `@count(expr)`.
fn extract_count_spec(decorators: &[crate::lang::ast::Decorator]) -> Option<CountSpec> {
    for dec in decorators {
        if dec.name.name == "count" {
            if let Some(DecoratorArg::Positional(expr)) = dec.args.first() {
                return match expr {
                    Expr::IntLit(n, _) => Some(CountSpec::Fixed(*n as usize)),
                    Expr::MemberAccess(obj, field, _) => {
                        if let Expr::Ident(section_ident) = obj.as_ref() {
                            Some(CountSpec::FieldRef {
                                section: section_ident.name.clone(),
                                field: field.name.clone(),
                            })
                        } else {
                            None
                        }
                    }
                    _ => None,
                };
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::ast::*;
    use crate::lang::span::Span;
    use crate::lang::trivia::Trivia;

    fn sp() -> Span {
        Span::dummy()
    }

    fn make_ident(name: &str) -> Ident {
        Ident {
            name: name.to_string(),
            span: sp(),
        }
    }

    fn make_id_lit(name: &str) -> InlineId {
        InlineId::Literal(IdentifierLit {
            value: name.to_string(),
            span: sp(),
        })
    }

    fn make_decorator(name: &str, args: Vec<DecoratorArg>) -> Decorator {
        Decorator {
            name: make_ident(name),
            args,
            span: sp(),
        }
    }

    #[test]
    fn collect_simple_layout() {
        // layout my_format {
        //     Header header { ... }
        //     @stream
        //     @count(header.count)
        //     Record records { ... }
        // }
        let layout_block = Block {
            decorators: vec![],
            partial: false,
            kind: make_ident("layout"),
            inline_id: Some(make_id_lit("my_format")),
            arrow_target: None,
            inline_args: vec![],
            body: vec![
                BodyItem::Block(Block {
                    decorators: vec![],
                    partial: false,
                    kind: make_ident("Header"),
                    inline_id: Some(make_id_lit("header")),
                    arrow_target: None,
                    inline_args: vec![],
                    body: vec![],
                    text_content: None,
                    trivia: Trivia::default(),
                    span: sp(),
                }),
                BodyItem::Block(Block {
                    decorators: vec![
                        make_decorator("stream", vec![]),
                        make_decorator(
                            "count",
                            vec![DecoratorArg::Positional(Expr::MemberAccess(
                                Box::new(Expr::Ident(make_ident("header"))),
                                make_ident("count"),
                                sp(),
                            ))],
                        ),
                    ],
                    partial: false,
                    kind: make_ident("Record"),
                    inline_id: Some(make_id_lit("records")),
                    arrow_target: None,
                    inline_args: vec![],
                    body: vec![],
                    text_content: None,
                    trivia: Trivia::default(),
                    span: sp(),
                }),
            ],
            text_content: None,
            trivia: Trivia::default(),
            span: sp(),
        };

        let doc = Document {
            items: vec![DocItem::Body(BodyItem::Block(layout_block))],
            trivia: Trivia::default(),
            span: sp(),
        };

        let mut registry = LayoutRegistry::new();
        let mut diag = DiagnosticBag::new();
        registry.collect(&doc, &mut diag);

        assert!(diag.into_diagnostics().is_empty());
        assert_eq!(registry.layouts.len(), 1);

        let layout = registry.get("my_format").unwrap();
        assert_eq!(layout.sections.len(), 2);

        let header_sec = &layout.sections[0];
        assert_eq!(header_sec.name, "header");
        assert_eq!(header_sec.struct_name, "Header");
        assert_eq!(header_sec.kind, SectionKind::Structured);
        assert!(header_sec.count.is_none());

        let records_sec = &layout.sections[1];
        assert_eq!(records_sec.name, "records");
        assert_eq!(records_sec.struct_name, "Record");
        assert_eq!(records_sec.kind, SectionKind::Stream);
        match &records_sec.count {
            Some(CountSpec::FieldRef { section, field }) => {
                assert_eq!(section, "header");
                assert_eq!(field, "count");
            }
            other => panic!("expected FieldRef, got {:?}", other),
        }
    }

    #[test]
    fn section_name_defaults_to_struct_name() {
        let layout_block = Block {
            decorators: vec![],
            partial: false,
            kind: make_ident("layout"),
            inline_id: Some(make_id_lit("simple")),
            arrow_target: None,
            inline_args: vec![],
            body: vec![BodyItem::Block(Block {
                decorators: vec![],
                partial: false,
                kind: make_ident("Header"),
                inline_id: None,
                arrow_target: None,
                inline_args: vec![],
                body: vec![],
                text_content: None,
                trivia: Trivia::default(),
                span: sp(),
            })],
            text_content: None,
            trivia: Trivia::default(),
            span: sp(),
        };

        let doc = Document {
            items: vec![DocItem::Body(BodyItem::Block(layout_block))],
            trivia: Trivia::default(),
            span: sp(),
        };

        let mut registry = LayoutRegistry::new();
        let mut diag = DiagnosticBag::new();
        registry.collect(&doc, &mut diag);

        let layout = registry.get("simple").unwrap();
        assert_eq!(layout.sections[0].name, "Header");
        assert_eq!(layout.sections[0].struct_name, "Header");
    }

    #[test]
    fn fixed_count_spec() {
        let layout_block = Block {
            decorators: vec![],
            partial: false,
            kind: make_ident("layout"),
            inline_id: Some(make_id_lit("fixed")),
            arrow_target: None,
            inline_args: vec![],
            body: vec![BodyItem::Block(Block {
                decorators: vec![
                    make_decorator("stream", vec![]),
                    make_decorator(
                        "count",
                        vec![DecoratorArg::Positional(Expr::IntLit(10, sp()))],
                    ),
                ],
                partial: false,
                kind: make_ident("Row"),
                inline_id: None,
                arrow_target: None,
                inline_args: vec![],
                body: vec![],
                text_content: None,
                trivia: Trivia::default(),
                span: sp(),
            })],
            text_content: None,
            trivia: Trivia::default(),
            span: sp(),
        };

        let doc = Document {
            items: vec![DocItem::Body(BodyItem::Block(layout_block))],
            trivia: Trivia::default(),
            span: sp(),
        };

        let mut registry = LayoutRegistry::new();
        let mut diag = DiagnosticBag::new();
        registry.collect(&doc, &mut diag);

        let layout = registry.get("fixed").unwrap();
        match &layout.sections[0].count {
            Some(CountSpec::Fixed(10)) => {}
            other => panic!("expected Fixed(10), got {:?}", other),
        }
    }
}
