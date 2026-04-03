use crate::lang::ast::*;
use crate::lang::trivia::CommentStyle;
use async_lsp::lsp_types::{Hover, HoverContents, MarkupContent, MarkupKind};
use ropey::Rope;

use crate::lsp::ast_utils::{find_node_at_offset, NodeAtOffset};
use crate::lsp::convert::span_to_lsp_range;
use crate::lsp::state::AnalysisResult;

pub fn hover(analysis: &AnalysisResult, offset: usize, rope: &Rope) -> Option<Hover> {
    let node = find_node_at_offset(&analysis.ast, offset);

    match node {
        NodeAtOffset::IdentRef(ident) => hover_ident_ref(analysis, ident, rope),
        NodeAtOffset::AttributeName(attr) => hover_attribute(analysis, attr, rope),
        NodeAtOffset::BlockKind(block) | NodeAtOffset::BlockId(block) => hover_block(block, rope),
        NodeAtOffset::LetBindingName(lb) => hover_let_binding(analysis, lb, rope),
        NodeAtOffset::DecoratorName(dec) => hover_decorator(dec, rope),
        NodeAtOffset::MacroDefName(md) => hover_macro_def(md, rope),
        NodeAtOffset::MacroCallName(mc) => hover_macro_call(analysis, mc, rope),
        NodeAtOffset::SchemaName(schema) => hover_schema(schema, rope),
        NodeAtOffset::TypeExpr(te) => hover_type_expr(te, rope),
        NodeAtOffset::FnCall(expr, _) => hover_fn_call(expr, rope),
        NodeAtOffset::ImportPath(import) => hover_import(import, rope),
        NodeAtOffset::Keyword(_) | NodeAtOffset::None => None,
    }
}

fn hover_ident_ref(analysis: &AnalysisResult, ident: &Ident, rope: &Rope) -> Option<Hover> {
    // Try to resolve via scope arena
    // Walk all scopes to find an entry matching this name
    for scope in analysis.scopes.all_scopes() {
        if let Some(entry) = scope.entries.get(&ident.name) {
            let value_str = entry
                .value
                .as_ref()
                .map(|v| format!("{}", v))
                .unwrap_or_else(|| "<unevaluated>".to_string());
            let kind = match entry.kind {
                crate::eval::ScopeEntryKind::LetBinding => "let",
                crate::eval::ScopeEntryKind::ExportLet => "export let",
                crate::eval::ScopeEntryKind::Attribute => "attribute",
                crate::eval::ScopeEntryKind::BlockChild => "block",
                crate::eval::ScopeEntryKind::TableEntry => "table",
                crate::eval::ScopeEntryKind::IteratorVar => "iterator",
            };
            let content = format!("```wcl\n{} {} = {}\n```", kind, ident.name, value_str);
            return Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: content,
                }),
                range: Some(span_to_lsp_range(ident.span, rope)),
            });
        }
    }

    // Fallback: just show the identifier name
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("```wcl\n{}\n```", ident.name),
        }),
        range: Some(span_to_lsp_range(ident.span, rope)),
    })
}

fn hover_attribute(analysis: &AnalysisResult, attr: &Attribute, rope: &Rope) -> Option<Hover> {
    // Look up evaluated value
    let value_str = analysis
        .values
        .get(&attr.name.name)
        .map(|v| format!("{}", v))
        .unwrap_or_else(|| "<unevaluated>".to_string());

    let content = format!("```wcl\n{} = {}\n```", attr.name.name, value_str);
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: content,
        }),
        range: Some(span_to_lsp_range(attr.name.span, rope)),
    })
}

fn hover_block(block: &Block, rope: &Rope) -> Option<Hover> {
    let mut content = String::new();
    let id_str = block.inline_id.as_ref().map(|id| match id {
        InlineId::Literal(lit) => format!(" #{}", lit.value),
        InlineId::Interpolated(_) => " #<interpolated>".to_string(),
    });
    content.push_str(&format!(
        "```wcl\n{}{}\n```",
        block.kind.name,
        id_str.unwrap_or_default()
    ));

    // Extract doc comments
    let doc = extract_doc_comments(&block.trivia);
    if !doc.is_empty() {
        content.push_str("\n\n---\n\n");
        content.push_str(&doc);
    }

    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: content,
        }),
        range: Some(span_to_lsp_range(block.kind.span, rope)),
    })
}

fn hover_let_binding(analysis: &AnalysisResult, lb: &LetBinding, rope: &Rope) -> Option<Hover> {
    let value_str = analysis
        .scopes
        .all_entries()
        .find(|(_, entry)| entry.name == lb.name.name)
        .and_then(|(_, entry)| entry.value.as_ref())
        .map(|v| format!("{}", v))
        .unwrap_or_else(|| "<unevaluated>".to_string());

    let content = format!("```wcl\nlet {} = {}\n```", lb.name.name, value_str);
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: content,
        }),
        range: Some(span_to_lsp_range(lb.name.span, rope)),
    })
}

fn hover_decorator(dec: &Decorator, rope: &Rope) -> Option<Hover> {
    let content = if dec.args.is_empty() {
        format!("```wcl\n@{}\n```", dec.name.name)
    } else {
        let args: Vec<String> = dec
            .args
            .iter()
            .map(|arg| match arg {
                DecoratorArg::Positional(expr) => format_expr_brief(expr),
                DecoratorArg::Named(ident, expr) => {
                    format!("{} = {}", ident.name, format_expr_brief(expr))
                }
            })
            .collect();
        format!("```wcl\n@{}({})\n```", dec.name.name, args.join(", "))
    };
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: content,
        }),
        range: Some(span_to_lsp_range(dec.name.span, rope)),
    })
}

/// Brief formatting of an expression for hover display.
fn format_expr_brief(expr: &Expr) -> String {
    match expr {
        Expr::IntLit(n, _) => n.to_string(),
        Expr::FloatLit(f, _) => f.to_string(),
        Expr::BoolLit(b, _) => b.to_string(),
        Expr::NullLit(_) => "null".to_string(),
        Expr::StringLit(s) => {
            let val: String = s
                .parts
                .iter()
                .filter_map(|p| match p {
                    StringPart::Literal(lit) => Some(lit.as_str()),
                    _ => None,
                })
                .collect();
            format!("\"{}\"", val)
        }
        Expr::Ident(i) => i.name.clone(),
        _ => "...".to_string(),
    }
}

fn hover_macro_def(md: &MacroDef, rope: &Rope) -> Option<Hover> {
    let params: Vec<String> = md
        .params
        .iter()
        .map(|p| {
            if let Some(ref tc) = p.type_constraint {
                format!("{}: {}", p.name.name, type_expr_str(tc))
            } else {
                p.name.name.clone()
            }
        })
        .collect();
    let kind = match md.kind {
        MacroKind::Function => "macro",
        MacroKind::Attribute => "macro @",
    };
    let content = format!(
        "```wcl\n{}{}({})\n```",
        kind,
        md.name.name,
        params.join(", ")
    );
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: content,
        }),
        range: Some(span_to_lsp_range(md.name.span, rope)),
    })
}

fn hover_schema(schema: &Schema, rope: &Rope) -> Option<Hover> {
    let name: String = schema
        .name
        .parts
        .iter()
        .filter_map(|p| match p {
            StringPart::Literal(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();

    let fields: Vec<String> = schema
        .fields
        .iter()
        .map(|f| format!("  {}: {}", f.name.name, type_expr_str(&f.type_expr)))
        .collect();

    let content = format!(
        "```wcl\nschema \"{}\" {{\n{}\n}}\n```",
        name,
        fields.join("\n")
    );
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: content,
        }),
        range: Some(span_to_lsp_range(schema.name.span, rope)),
    })
}

fn hover_type_expr(te: &TypeExpr, rope: &Rope) -> Option<Hover> {
    let content = format!("```wcl\n{}\n```", type_expr_str(te));
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: content,
        }),
        range: Some(span_to_lsp_range(te.span(), rope)),
    })
}

fn type_expr_str(te: &TypeExpr) -> String {
    match te {
        TypeExpr::String(_) => "string".to_string(),
        TypeExpr::I8(_) => "i8".to_string(),
        TypeExpr::U8(_) => "u8".to_string(),
        TypeExpr::I16(_) => "i16".to_string(),
        TypeExpr::U16(_) => "u16".to_string(),
        TypeExpr::I32(_) => "i32".to_string(),
        TypeExpr::U32(_) => "u32".to_string(),
        TypeExpr::I64(_) => "i64".to_string(),
        TypeExpr::U64(_) => "u64".to_string(),
        TypeExpr::I128(_) => "i128".to_string(),
        TypeExpr::U128(_) => "u128".to_string(),
        TypeExpr::F32(_) => "f32".to_string(),
        TypeExpr::F64(_) => "f64".to_string(),
        TypeExpr::Date(_) => "date".to_string(),
        TypeExpr::Duration(_) => "duration".to_string(),
        TypeExpr::Bool(_) => "bool".to_string(),
        TypeExpr::Null(_) => "null".to_string(),
        TypeExpr::Identifier(_) => "identifier".to_string(),
        TypeExpr::Any(_) => "any".to_string(),
        TypeExpr::List(inner, _) => format!("list({})", type_expr_str(inner)),
        TypeExpr::Map(k, v, _) => format!("map({}, {})", type_expr_str(k), type_expr_str(v)),
        TypeExpr::Set(inner, _) => format!("set({})", type_expr_str(inner)),
        TypeExpr::Ref(schema_name, _) => {
            let name: String = schema_name
                .parts
                .iter()
                .filter_map(|p| match p {
                    StringPart::Literal(s) => Some(s.as_str()),
                    _ => None,
                })
                .collect();
            format!("ref(\"{}\")", name)
        }
        TypeExpr::Union(types, _) => {
            let parts: Vec<String> = types.iter().map(type_expr_str).collect();
            format!("union({})", parts.join(", "))
        }
        TypeExpr::Symbol(_) => "symbol".to_string(),
        TypeExpr::StructType(ident, _) => ident.name.clone(),
        TypeExpr::Pattern(_) => "pattern".to_string(),
    }
}

fn hover_macro_call(analysis: &AnalysisResult, mc: &MacroCall, rope: &Rope) -> Option<Hover> {
    // Find the macro definition to show its signature
    for item in &analysis.ast.items {
        if let DocItem::Body(BodyItem::MacroDef(md)) = item {
            if md.name.name == mc.name.name {
                return hover_macro_def(md, rope);
            }
        }
    }
    // Fallback: just show the call name
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("```wcl\n{}(...)\n```", mc.name.name),
        }),
        range: Some(span_to_lsp_range(mc.name.span, rope)),
    })
}

fn hover_fn_call(expr: &Expr, rope: &Rope) -> Option<Hover> {
    if let Expr::FnCall(callee, _, span) = expr {
        let name = match callee.as_ref() {
            Expr::Ident(i) => i.name.clone(),
            Expr::MemberAccess(_, field, _) => field.name.clone(),
            _ => return None,
        };
        Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: format!("```wcl\n{}(...)\n```\n\nBuiltin function", name),
            }),
            range: Some(span_to_lsp_range(*span, rope)),
        })
    } else {
        None
    }
}

fn hover_import(import: &Import, rope: &Rope) -> Option<Hover> {
    let path: String = import
        .path
        .parts
        .iter()
        .filter_map(|p| match p {
            StringPart::Literal(s) => Some(s.as_str()),
            _ => None,
        })
        .collect();
    Some(Hover {
        contents: HoverContents::Markup(MarkupContent {
            kind: MarkupKind::Markdown,
            value: format!("```wcl\nimport \"{}\"\n```", path),
        }),
        range: Some(span_to_lsp_range(import.span, rope)),
    })
}

fn extract_doc_comments(trivia: &crate::lang::trivia::Trivia) -> String {
    trivia
        .comments
        .iter()
        .filter(|c| c.style == CommentStyle::Doc)
        .map(|c| {
            c.text
                .strip_prefix("/// ")
                .or_else(|| c.text.strip_prefix("///"))
                .unwrap_or(&c.text)
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::analysis::analyze;

    fn get_hover(source: &str, offset: usize) -> Option<Hover> {
        let analysis = analyze(source, &crate::ParseOptions::default());
        let rope = ropey::Rope::from_str(source);
        hover(&analysis, offset, &rope)
    }

    fn hover_value(h: &Hover) -> &str {
        match &h.contents {
            HoverContents::Markup(m) => &m.value,
            _ => panic!("expected markup"),
        }
    }

    #[test]
    fn test_hover_block_kind() {
        let h = get_hover("config { port = 8080 }", 0).unwrap();
        assert!(hover_value(&h).contains("config"));
    }

    #[test]
    fn test_hover_attribute_name() {
        let source = "config { port = 8080 }";
        let offset = source.find("port").unwrap();
        let h = get_hover(source, offset).unwrap();
        assert!(hover_value(&h).contains("port"));
    }

    #[test]
    fn test_hover_let_binding() {
        let source = "let x = 42";
        let offset = source.find("x").unwrap();
        let h = get_hover(source, offset).unwrap();
        let val = hover_value(&h);
        assert!(val.contains("let"));
        assert!(val.contains("x"));
        assert!(val.contains("42"));
    }

    #[test]
    fn test_hover_ident_ref() {
        let source = "let x = 42\nconfig { port = x }";
        let offset = source.rfind("x").unwrap();
        let h = get_hover(source, offset).unwrap();
        let val = hover_value(&h);
        assert!(val.contains("x"));
    }

    #[test]
    fn test_hover_import() {
        let source = "import \"./other.wcl\"";
        // Offset at the string path, not the keyword
        let offset = source.find("./other").unwrap();
        let h = get_hover(source, offset).unwrap();
        let val = hover_value(&h);
        assert!(val.contains("import"));
        assert!(val.contains("other.wcl"));
    }

    #[test]
    fn test_hover_keyword_returns_none() {
        let source = "let x = 42";
        // Offset at "let" keyword — hover returns None
        let h = get_hover(source, 0);
        assert!(h.is_none());
    }

    #[test]
    fn test_hover_none_outside() {
        assert!(get_hover("config { port = 8080 }", 100).is_none());
    }

    #[test]
    fn test_type_expr_str() {
        assert_eq!(
            type_expr_str(&TypeExpr::String(crate::lang::Span::dummy())),
            "string"
        );
        assert_eq!(
            type_expr_str(&TypeExpr::I64(crate::lang::Span::dummy())),
            "i64"
        );
        assert_eq!(
            type_expr_str(&TypeExpr::Any(crate::lang::Span::dummy())),
            "any"
        );
    }

    #[test]
    fn test_hover_decorator_with_args() {
        // Test hover_decorator directly with a constructed Decorator
        let dec = Decorator {
            name: Ident {
                name: "deprecated".to_string(),
                span: crate::lang::Span::dummy(),
            },
            args: vec![DecoratorArg::Positional(Expr::StringLit(StringLit {
                parts: vec![StringPart::Literal("use v2".to_string())],
                heredoc: None,
                span: crate::lang::Span::dummy(),
            }))],
            span: crate::lang::Span::dummy(),
        };
        let rope = ropey::Rope::from_str("@deprecated(\"use v2\")");
        let h = hover_decorator(&dec, &rope).unwrap();
        let val = hover_value(&h);
        assert!(
            val.contains("@deprecated"),
            "should show decorator name, got: {}",
            val,
        );
        assert!(
            val.contains("use v2"),
            "should show decorator args, got: {}",
            val,
        );
    }

    #[test]
    fn test_type_expr_str_ref() {
        let ref_type = TypeExpr::Ref(
            StringLit {
                parts: vec![StringPart::Literal("my_schema".to_string())],
                heredoc: None,
                span: crate::lang::Span::dummy(),
            },
            crate::lang::Span::dummy(),
        );
        assert_eq!(type_expr_str(&ref_type), "ref(\"my_schema\")");
    }

    #[test]
    fn test_hover_macro_with_types() {
        // Test hover_macro_def directly with a constructed MacroDef
        let md = MacroDef {
            decorators: vec![],
            kind: MacroKind::Function,
            name: Ident {
                name: "greet".to_string(),
                span: crate::lang::Span::dummy(),
            },
            params: vec![
                MacroParam {
                    name: Ident {
                        name: "name".to_string(),
                        span: crate::lang::Span::dummy(),
                    },
                    type_constraint: Some(TypeExpr::String(crate::lang::Span::dummy())),
                    default: None,
                    span: crate::lang::Span::dummy(),
                },
                MacroParam {
                    name: Ident {
                        name: "count".to_string(),
                        span: crate::lang::Span::dummy(),
                    },
                    type_constraint: Some(TypeExpr::I64(crate::lang::Span::dummy())),
                    default: None,
                    span: crate::lang::Span::dummy(),
                },
            ],
            body: MacroBody::Function(vec![]),
            trivia: crate::lang::Trivia::empty(),
            span: crate::lang::Span::dummy(),
        };
        let rope = ropey::Rope::from_str("macro greet(name: string, count: i64) {}");
        let h = hover_macro_def(&md, &rope).unwrap();
        let val = hover_value(&h);
        assert!(
            val.contains("name: string"),
            "should show param type constraint, got: {}",
            val,
        );
        assert!(
            val.contains("count: i64"),
            "should show param type constraint, got: {}",
            val,
        );
    }
}
