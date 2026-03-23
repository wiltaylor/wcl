use crate::lang::ast::*;
use async_lsp::lsp_types::{DocumentSymbol, SymbolKind};
use ropey::Rope;

use crate::lsp::convert::span_to_lsp_range;

#[allow(deprecated)] // DocumentSymbol::deprecated is deprecated in the struct but we need to set it
pub fn document_symbols(doc: &Document, rope: &Rope) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();
    for item in &doc.items {
        if let Some(sym) = doc_item_symbol(item, rope) {
            symbols.push(sym);
        }
    }
    symbols
}

#[allow(deprecated)]
fn doc_item_symbol(item: &DocItem, rope: &Rope) -> Option<DocumentSymbol> {
    match item {
        DocItem::Import(import) => {
            let path = import
                .path
                .parts
                .iter()
                .filter_map(|p| match p {
                    StringPart::Literal(s) => Some(s.as_str()),
                    _ => None,
                })
                .collect::<String>();
            Some(DocumentSymbol {
                name: format!("import \"{}\"", path),
                detail: None,
                kind: SymbolKind::PACKAGE,
                tags: None,
                deprecated: None,
                range: span_to_lsp_range(import.span, rope),
                selection_range: span_to_lsp_range(import.path.span, rope),
                children: None,
            })
        }
        DocItem::ExportLet(el) => Some(DocumentSymbol {
            name: el.name.name.clone(),
            detail: Some("export let".to_string()),
            kind: SymbolKind::VARIABLE,
            tags: None,
            deprecated: None,
            range: span_to_lsp_range(el.span, rope),
            selection_range: span_to_lsp_range(el.name.span, rope),
            children: None,
        }),
        DocItem::ReExport(re) => Some(DocumentSymbol {
            name: re.name.name.clone(),
            detail: Some("export".to_string()),
            kind: SymbolKind::VARIABLE,
            tags: None,
            deprecated: None,
            range: span_to_lsp_range(re.span, rope),
            selection_range: span_to_lsp_range(re.name.span, rope),
            children: None,
        }),
        DocItem::Body(body_item) => body_item_symbol(body_item, rope),
        #[allow(deprecated)]
        DocItem::FunctionDecl(decl) => Some(DocumentSymbol {
            name: decl.name.name.clone(),
            detail: Some("declare".to_string()),
            kind: SymbolKind::FUNCTION,
            tags: None,
            deprecated: None,
            range: span_to_lsp_range(decl.span, rope),
            selection_range: span_to_lsp_range(decl.name.span, rope),
            children: None,
        }),
    }
}

#[allow(deprecated)]
fn body_item_symbol(item: &BodyItem, rope: &Rope) -> Option<DocumentSymbol> {
    match item {
        BodyItem::Attribute(attr) => Some(DocumentSymbol {
            name: attr.name.name.clone(),
            detail: None,
            kind: SymbolKind::PROPERTY,
            tags: None,
            deprecated: None,
            range: span_to_lsp_range(attr.span, rope),
            selection_range: span_to_lsp_range(attr.name.span, rope),
            children: None,
        }),
        BodyItem::Block(block) => {
            let name = block_display_name(block);
            let children: Vec<DocumentSymbol> = block
                .body
                .iter()
                .filter_map(|child| body_item_symbol(child, rope))
                .collect();
            Some(DocumentSymbol {
                name,
                detail: None,
                kind: SymbolKind::CLASS,
                tags: None,
                deprecated: None,
                range: span_to_lsp_range(block.span, rope),
                selection_range: span_to_lsp_range(block.kind.span, rope),
                children: if children.is_empty() {
                    None
                } else {
                    Some(children)
                },
            })
        }
        BodyItem::LetBinding(lb) => Some(DocumentSymbol {
            name: lb.name.name.clone(),
            detail: Some("let".to_string()),
            kind: SymbolKind::VARIABLE,
            tags: None,
            deprecated: None,
            range: span_to_lsp_range(lb.span, rope),
            selection_range: span_to_lsp_range(lb.name.span, rope),
            children: None,
        }),
        BodyItem::MacroDef(md) => Some(DocumentSymbol {
            name: md.name.name.clone(),
            detail: Some("macro".to_string()),
            kind: SymbolKind::FUNCTION,
            tags: None,
            deprecated: None,
            range: span_to_lsp_range(md.span, rope),
            selection_range: span_to_lsp_range(md.name.span, rope),
            children: None,
        }),
        BodyItem::Schema(schema) => {
            let name = schema
                .name
                .parts
                .iter()
                .filter_map(|p| match p {
                    StringPart::Literal(s) => Some(s.as_str()),
                    _ => None,
                })
                .collect::<String>();
            Some(DocumentSymbol {
                name,
                detail: Some("schema".to_string()),
                kind: SymbolKind::INTERFACE,
                tags: None,
                deprecated: None,
                range: span_to_lsp_range(schema.span, rope),
                selection_range: span_to_lsp_range(schema.name.span, rope),
                children: None,
            })
        }
        BodyItem::Table(table) => {
            let name = table
                .inline_id
                .as_ref()
                .map(|id| match id {
                    InlineId::Literal(lit) => format!("table {}", lit.value),
                    InlineId::Interpolated(_) => "table <interpolated>".to_string(),
                })
                .unwrap_or_else(|| "table".to_string());
            Some(DocumentSymbol {
                name,
                detail: None,
                kind: SymbolKind::STRUCT,
                tags: None,
                deprecated: None,
                range: span_to_lsp_range(table.span, rope),
                selection_range: span_to_lsp_range(table.span, rope),
                children: None,
            })
        }
        BodyItem::Validation(val) => {
            let name = val
                .name
                .parts
                .iter()
                .filter_map(|p| match p {
                    StringPart::Literal(s) => Some(s.as_str()),
                    _ => None,
                })
                .collect::<String>();
            Some(DocumentSymbol {
                name,
                detail: Some("validation".to_string()),
                kind: SymbolKind::EVENT,
                tags: None,
                deprecated: None,
                range: span_to_lsp_range(val.span, rope),
                selection_range: span_to_lsp_range(val.name.span, rope),
                children: None,
            })
        }
        BodyItem::MacroCall(_)
        | BodyItem::ForLoop(_)
        | BodyItem::Conditional(_)
        | BodyItem::DecoratorSchema(_)
        | BodyItem::SymbolSetDecl(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::span::FileId;

    fn get_symbols(source: &str) -> Vec<DocumentSymbol> {
        let file_id = FileId(0);
        let (doc, _) = crate::lang::parse(source, file_id);
        let rope = Rope::from_str(source);
        document_symbols(&doc, &rope)
    }

    #[test]
    fn test_block_symbol() {
        let syms = get_symbols("config { port = 8080 }");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "config");
        assert_eq!(syms[0].kind, SymbolKind::CLASS);
    }

    #[test]
    fn test_block_with_id() {
        let syms = get_symbols("server main { port = 80 }");
        assert_eq!(syms[0].name, "server #main");
    }

    #[test]
    fn test_let_binding_symbol() {
        let syms = get_symbols("let x = 42");
        assert_eq!(syms.len(), 1);
        assert_eq!(syms[0].name, "x");
        assert_eq!(syms[0].kind, SymbolKind::VARIABLE);
    }

    #[test]
    fn test_nested_children() {
        let syms = get_symbols("server { inner { x = 1 } }");
        assert_eq!(syms.len(), 1);
        let children = syms[0].children.as_ref().unwrap();
        assert!(children.iter().any(|c| c.name == "inner"));
    }

    #[test]
    fn test_attribute_symbol() {
        let syms = get_symbols("config { port = 8080\nhost = \"localhost\" }");
        let children = syms[0].children.as_ref().unwrap();
        assert_eq!(children.len(), 2);
        assert!(children.iter().all(|c| c.kind == SymbolKind::PROPERTY));
    }

    #[test]
    fn test_multiple_top_level() {
        let syms = get_symbols("let x = 1\nserver { port = 80 }\nclient { timeout = 30 }");
        assert_eq!(syms.len(), 3);
    }
}

fn block_display_name(block: &Block) -> String {
    let mut name = block.kind.name.clone();
    if let Some(id) = &block.inline_id {
        match id {
            InlineId::Literal(lit) => {
                name.push_str(&format!(" #{}", lit.value));
            }
            InlineId::Interpolated(_) => {
                name.push_str(" #<interpolated>");
            }
        }
    }
    name
}
