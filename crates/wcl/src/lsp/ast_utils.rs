use crate::lang::ast::*;
use crate::lang::span::Span;

/// Describes which AST node was found at a given byte offset.
#[derive(Debug)]
pub enum NodeAtOffset<'a> {
    IdentRef(&'a Ident),
    AttributeName(&'a Attribute),
    BlockKind(&'a Block),
    BlockId(&'a Block),
    LetBindingName(&'a LetBinding),
    DecoratorName(&'a Decorator),
    MacroDefName(&'a MacroDef),
    MacroCallName(&'a MacroCall),
    SchemaName(&'a Schema),
    FnCall(&'a Expr, usize),
    TypeExpr(&'a TypeExpr),
    ImportPath(&'a Import),
    Keyword(Span),
    None,
}

fn contains(span: Span, offset: usize) -> bool {
    span.start <= offset && offset < span.end
}

/// Find the most specific AST node at the given byte offset.
pub fn find_node_at_offset(doc: &Document, offset: usize) -> NodeAtOffset<'_> {
    for item in &doc.items {
        let result = find_in_doc_item(item, offset);
        if !matches!(result, NodeAtOffset::None) {
            return result;
        }
    }
    NodeAtOffset::None
}

fn find_in_doc_item<'a>(item: &'a DocItem, offset: usize) -> NodeAtOffset<'a> {
    match item {
        DocItem::Import(import) => {
            if contains(import.span, offset) {
                // "import" keyword: between item start and path start
                if offset < import.path.span.start {
                    return NodeAtOffset::Keyword(Span {
                        start: import.span.start,
                        end: import.path.span.start,
                        file: import.span.file,
                    });
                }
                return NodeAtOffset::ImportPath(import);
            }
        }
        DocItem::ExportLet(el) => {
            if contains(el.span, offset) {
                // "export let" keyword region: between item start and name start
                if offset < el.name.span.start {
                    return NodeAtOffset::Keyword(Span {
                        start: el.span.start,
                        end: el.name.span.start,
                        file: el.span.file,
                    });
                }
                if contains(el.name.span, offset) {
                    return NodeAtOffset::IdentRef(&el.name);
                }
                let result = find_in_expr(&el.value, offset);
                if !matches!(result, NodeAtOffset::None) {
                    return result;
                }
            }
        }
        DocItem::ReExport(re) => {
            if contains(re.name.span, offset) {
                return NodeAtOffset::IdentRef(&re.name);
            }
        }
        DocItem::Body(body_item) => {
            return find_in_body_item(body_item, offset);
        }
        DocItem::FunctionDecl(_) => {}
    }
    NodeAtOffset::None
}

fn find_in_body_item<'a>(item: &'a BodyItem, offset: usize) -> NodeAtOffset<'a> {
    match item {
        BodyItem::Attribute(attr) => {
            if contains(attr.span, offset) {
                // Check decorators first
                for dec in &attr.decorators {
                    let r = find_in_decorator(dec, offset);
                    if !matches!(r, NodeAtOffset::None) {
                        return r;
                    }
                }
                // Check name
                if contains(attr.name.span, offset) {
                    return NodeAtOffset::AttributeName(attr);
                }
                // Check value expression
                return find_in_expr(&attr.value, offset);
            }
        }
        BodyItem::Block(block) => {
            if contains(block.span, offset) {
                // Check decorators
                for dec in &block.decorators {
                    let r = find_in_decorator(dec, offset);
                    if !matches!(r, NodeAtOffset::None) {
                        return r;
                    }
                }
                // Check kind
                if contains(block.kind.span, offset) {
                    return NodeAtOffset::BlockKind(block);
                }
                // Check inline id
                if let Some(InlineId::Literal(lit)) = &block.inline_id {
                    if contains(lit.span, offset) {
                        return NodeAtOffset::BlockId(block);
                    }
                }
                // Check body items
                for child in &block.body {
                    let r = find_in_body_item(child, offset);
                    if !matches!(r, NodeAtOffset::None) {
                        return r;
                    }
                }
            }
        }
        BodyItem::LetBinding(lb) => {
            if contains(lb.span, offset) {
                for dec in &lb.decorators {
                    let r = find_in_decorator(dec, offset);
                    if !matches!(r, NodeAtOffset::None) {
                        return r;
                    }
                }
                // "let" keyword: between item start and name start
                if offset < lb.name.span.start {
                    return NodeAtOffset::Keyword(Span {
                        start: lb.span.start,
                        end: lb.name.span.start,
                        file: lb.span.file,
                    });
                }
                if contains(lb.name.span, offset) {
                    return NodeAtOffset::LetBindingName(lb);
                }
                return find_in_expr(&lb.value, offset);
            }
        }
        BodyItem::MacroDef(md) => {
            if contains(md.span, offset) {
                // "macro" keyword: between item start and name start
                if offset < md.name.span.start {
                    return NodeAtOffset::Keyword(Span {
                        start: md.span.start,
                        end: md.name.span.start,
                        file: md.span.file,
                    });
                }
                if contains(md.name.span, offset) {
                    return NodeAtOffset::MacroDefName(md);
                }
            }
        }
        BodyItem::MacroCall(mc) => {
            if contains(mc.span, offset) && contains(mc.name.span, offset) {
                return NodeAtOffset::MacroCallName(mc);
            }
        }
        BodyItem::Schema(schema) => {
            if contains(schema.span, offset) {
                // "schema" keyword: between item start and name start
                if offset < schema.name.span.start {
                    return NodeAtOffset::Keyword(Span {
                        start: schema.span.start,
                        end: schema.name.span.start,
                        file: schema.span.file,
                    });
                }
                if contains(schema.name.span, offset) {
                    return NodeAtOffset::SchemaName(schema);
                }
                for field in &schema.fields {
                    if contains(field.span, offset) {
                        if contains(field.name.span, offset) {
                            return NodeAtOffset::IdentRef(&field.name);
                        }
                        if contains(field.type_expr.span(), offset) {
                            return NodeAtOffset::TypeExpr(&field.type_expr);
                        }
                    }
                }
                for variant in &schema.variants {
                    if contains(variant.span, offset) {
                        for field in &variant.fields {
                            if contains(field.span, offset) {
                                if contains(field.name.span, offset) {
                                    return NodeAtOffset::IdentRef(&field.name);
                                }
                                if contains(field.type_expr.span(), offset) {
                                    return NodeAtOffset::TypeExpr(&field.type_expr);
                                }
                            }
                        }
                    }
                }
            }
        }
        BodyItem::Table(table) => {
            if contains(table.span, offset) {
                // "table" keyword: between item start and first child
                let first_child_start = table
                    .inline_id
                    .as_ref()
                    .map(|id| match id {
                        InlineId::Literal(lit) => lit.span.start,
                        InlineId::Interpolated(_) => table.span.end, // fallback
                    })
                    .or_else(|| table.columns.first().map(|c| c.span.start))
                    .unwrap_or(table.span.end);
                if offset < first_child_start {
                    return NodeAtOffset::Keyword(Span {
                        start: table.span.start,
                        end: first_child_start,
                        file: table.span.file,
                    });
                }
                for col in &table.columns {
                    if contains(col.name.span, offset) {
                        return NodeAtOffset::IdentRef(&col.name);
                    }
                    if contains(col.type_expr.span(), offset) {
                        return NodeAtOffset::TypeExpr(&col.type_expr);
                    }
                }
                for row in &table.rows {
                    for cell in &row.cells {
                        let r = find_in_expr(cell, offset);
                        if !matches!(r, NodeAtOffset::None) {
                            return r;
                        }
                    }
                }
            }
        }
        BodyItem::ForLoop(fl) => {
            if contains(fl.span, offset) {
                // "for" keyword: between item start and iterator start
                if offset < fl.iterator.span.start {
                    return NodeAtOffset::Keyword(Span {
                        start: fl.span.start,
                        end: fl.iterator.span.start,
                        file: fl.span.file,
                    });
                }
                if contains(fl.iterator.span, offset) {
                    return NodeAtOffset::IdentRef(&fl.iterator);
                }
                if let Some(idx) = &fl.index {
                    if contains(idx.span, offset) {
                        return NodeAtOffset::IdentRef(idx);
                    }
                }
                let r = find_in_expr(&fl.iterable, offset);
                if !matches!(r, NodeAtOffset::None) {
                    return r;
                }
                for child in &fl.body {
                    let r = find_in_body_item(child, offset);
                    if !matches!(r, NodeAtOffset::None) {
                        return r;
                    }
                }
            }
        }
        BodyItem::Conditional(cond) => {
            if contains(cond.span, offset) {
                // "if" keyword: between item start and condition start
                if offset < cond.condition.span().start {
                    return NodeAtOffset::Keyword(Span {
                        start: cond.span.start,
                        end: cond.condition.span().start,
                        file: cond.span.file,
                    });
                }
                return find_in_conditional(cond, offset);
            }
        }
        BodyItem::Validation(val) => {
            if contains(val.span, offset) {
                // "validation" keyword: between item start and name start
                if offset < val.name.span.start {
                    return NodeAtOffset::Keyword(Span {
                        start: val.span.start,
                        end: val.name.span.start,
                        file: val.span.file,
                    });
                }
                let r = find_in_expr(&val.check, offset);
                if !matches!(r, NodeAtOffset::None) {
                    return r;
                }
                let r = find_in_expr(&val.message, offset);
                if !matches!(r, NodeAtOffset::None) {
                    return r;
                }
            }
        }
        BodyItem::DecoratorSchema(_) => {}
        BodyItem::SymbolSetDecl(_) => {}
    }
    NodeAtOffset::None
}

fn find_in_conditional<'a>(cond: &'a Conditional, offset: usize) -> NodeAtOffset<'a> {
    let r = find_in_expr(&cond.condition, offset);
    if !matches!(r, NodeAtOffset::None) {
        return r;
    }
    for child in &cond.then_body {
        let r = find_in_body_item(child, offset);
        if !matches!(r, NodeAtOffset::None) {
            return r;
        }
    }
    if let Some(else_branch) = &cond.else_branch {
        match else_branch {
            ElseBranch::ElseIf(inner) => {
                return find_in_conditional(inner, offset);
            }
            ElseBranch::Else(body, _, _) => {
                for child in body {
                    let r = find_in_body_item(child, offset);
                    if !matches!(r, NodeAtOffset::None) {
                        return r;
                    }
                }
            }
        }
    }
    NodeAtOffset::None
}

fn find_in_decorator<'a>(dec: &'a Decorator, offset: usize) -> NodeAtOffset<'a> {
    if contains(dec.span, offset) && contains(dec.name.span, offset) {
        return NodeAtOffset::DecoratorName(dec);
    }
    NodeAtOffset::None
}

fn find_in_expr<'a>(expr: &'a Expr, offset: usize) -> NodeAtOffset<'a> {
    if !contains(expr.span(), offset) {
        return NodeAtOffset::None;
    }
    match expr {
        Expr::Ident(ident) => NodeAtOffset::IdentRef(ident),
        Expr::BinaryOp(lhs, _, rhs, _) => {
            let r = find_in_expr(lhs, offset);
            if !matches!(r, NodeAtOffset::None) {
                return r;
            }
            find_in_expr(rhs, offset)
        }
        Expr::UnaryOp(_, inner, _) | Expr::Paren(inner, _) => find_in_expr(inner, offset),
        Expr::Ternary(cond, then_e, else_e, _) => {
            for e in [cond.as_ref(), then_e.as_ref(), else_e.as_ref()] {
                let r = find_in_expr(e, offset);
                if !matches!(r, NodeAtOffset::None) {
                    return r;
                }
            }
            NodeAtOffset::None
        }
        Expr::MemberAccess(obj, field, _) => {
            let r = find_in_expr(obj, offset);
            if !matches!(r, NodeAtOffset::None) {
                return r;
            }
            if contains(field.span, offset) {
                return NodeAtOffset::IdentRef(field);
            }
            NodeAtOffset::None
        }
        Expr::IndexAccess(obj, idx, _) => {
            let r = find_in_expr(obj, offset);
            if !matches!(r, NodeAtOffset::None) {
                return r;
            }
            find_in_expr(idx, offset)
        }
        Expr::FnCall(callee, args, _) => {
            let r = find_in_expr(callee, offset);
            if !matches!(r, NodeAtOffset::None) {
                return r;
            }
            for (i, arg) in args.iter().enumerate() {
                let arg_expr = match arg {
                    CallArg::Positional(e) => e,
                    CallArg::Named(_, e) => e,
                };
                if contains(arg_expr.span(), offset) {
                    let r = find_in_expr(arg_expr, offset);
                    if !matches!(r, NodeAtOffset::None) {
                        return r;
                    }
                    return NodeAtOffset::FnCall(expr, i);
                }
            }
            NodeAtOffset::FnCall(expr, 0)
        }
        Expr::List(items, _) => {
            for item in items {
                let r = find_in_expr(item, offset);
                if !matches!(r, NodeAtOffset::None) {
                    return r;
                }
            }
            NodeAtOffset::None
        }
        Expr::Map(entries, _) => {
            for (_, val) in entries {
                let r = find_in_expr(val, offset);
                if !matches!(r, NodeAtOffset::None) {
                    return r;
                }
            }
            NodeAtOffset::None
        }
        Expr::Lambda(_, body, _) => find_in_expr(body, offset),
        Expr::BlockExpr(_, final_expr, _) => find_in_expr(final_expr, offset),
        _ => NodeAtOffset::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::span::FileId;

    fn parse_and_find(source: &str, offset: usize) -> String {
        let file = FileId(0);
        let (doc, _) = crate::lang::parse(source, file);
        let node = find_node_at_offset(&doc, offset);
        match node {
            NodeAtOffset::IdentRef(i) => format!("IdentRef({})", i.name),
            NodeAtOffset::AttributeName(a) => format!("AttributeName({})", a.name.name),
            NodeAtOffset::BlockKind(b) => format!("BlockKind({})", b.kind.name),
            NodeAtOffset::BlockId(_) => "BlockId".to_string(),
            NodeAtOffset::LetBindingName(lb) => format!("LetBindingName({})", lb.name.name),
            NodeAtOffset::DecoratorName(d) => format!("DecoratorName({})", d.name.name),
            NodeAtOffset::MacroDefName(m) => format!("MacroDefName({})", m.name.name),
            NodeAtOffset::MacroCallName(m) => format!("MacroCallName({})", m.name.name),
            NodeAtOffset::SchemaName(_) => "SchemaName".to_string(),
            NodeAtOffset::FnCall(_, _) => "FnCall".to_string(),
            NodeAtOffset::TypeExpr(_) => "TypeExpr".to_string(),
            NodeAtOffset::ImportPath(_) => "ImportPath".to_string(),
            NodeAtOffset::Keyword(_) => "Keyword".to_string(),
            NodeAtOffset::None => "None".to_string(),
        }
    }

    #[test]
    fn test_find_block_kind() {
        // "config { port = 8080 }"
        //  ^--- offset 0 = 'c' of "config"
        let result = parse_and_find("config { port = 8080 }", 0);
        assert_eq!(result, "BlockKind(config)");
    }

    #[test]
    fn test_find_attribute_name() {
        // "config { port = 8080 }"
        //           ^--- offset 9 = 'p' of "port"
        let result = parse_and_find("config { port = 8080 }", 9);
        assert_eq!(result, "AttributeName(port)");
    }

    #[test]
    fn test_find_let_binding() {
        // "let x = 42"
        //      ^--- offset 4 = 'x'
        let result = parse_and_find("let x = 42", 4);
        assert_eq!(result, "LetBindingName(x)");
    }

    #[test]
    fn test_find_ident_ref_in_expr() {
        // "config { port = x }"
        //                  ^--- offset 16 = 'x'
        let result = parse_and_find("config { port = x }", 16);
        assert_eq!(result, "IdentRef(x)");
    }

    #[test]
    fn test_find_none_outside() {
        let result = parse_and_find("config { port = 8080 }", 100);
        assert_eq!(result, "None");
    }

    #[test]
    fn test_find_keyword_let() {
        // "let x = 42"
        //  ^--- offset 0 = 'l' of "let"
        let result = parse_and_find("let x = 42", 0);
        assert_eq!(result, "Keyword");
    }

    #[test]
    fn test_find_keyword_if() {
        // "if true { x = 1 }"
        //  ^--- offset 0 = 'i' of "if"
        let result = parse_and_find("if true { x = 1 }", 0);
        assert_eq!(result, "Keyword");
    }

    #[test]
    fn test_find_keyword_for() {
        // "for x in [1, 2] { }"
        //  ^--- offset 0 = 'f' of "for"
        let result = parse_and_find("for x in [1, 2] { }", 0);
        assert_eq!(result, "Keyword");
    }

    #[test]
    fn test_find_keyword_import() {
        // "import \"./foo.wcl\""
        //  ^--- offset 0 = 'i' of "import"
        let result = parse_and_find("import \"./foo.wcl\"", 0);
        assert_eq!(result, "Keyword");
    }
}
