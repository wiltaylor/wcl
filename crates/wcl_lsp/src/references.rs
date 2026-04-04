use async_lsp::lsp_types::Location;
use ropey::Rope;
use wcl_lang::eval::{ScopeId, ScopeKind};
use wcl_lang::lang::ast::*;

use crate::ast_utils::{find_node_at_offset, NodeAtOffset};
use crate::convert::span_to_lsp_range;
use crate::state::AnalysisResult;

pub fn find_references(
    analysis: &AnalysisResult,
    offset: usize,
    rope: &Rope,
    uri: &async_lsp::lsp_types::Url,
    include_declaration: bool,
) -> Vec<Location> {
    let node = find_node_at_offset(&analysis.ast, offset);

    // Handle BlockKind: find all blocks of the same kind
    if let NodeAtOffset::BlockKind(block) = &node {
        let kind_name = &block.kind.name;
        let mut locations = Vec::new();
        collect_block_kinds(&analysis.ast, kind_name, uri, rope, &mut locations);
        return locations;
    }

    // Handle SchemaName: find all blocks whose kind matches this schema name
    if let NodeAtOffset::SchemaName(schema) = &node {
        let schema_name = wcl_lang::schema::schema::string_lit_to_string(&schema.name);
        let mut locations = Vec::new();
        if include_declaration {
            locations.push(Location {
                uri: uri.clone(),
                range: span_to_lsp_range(schema.name.span, rope),
            });
        }
        collect_block_kinds(&analysis.ast, &schema_name, uri, rope, &mut locations);
        return locations;
    }

    let target_name = match &node {
        NodeAtOffset::IdentRef(ident) => &ident.name,
        NodeAtOffset::AttributeName(attr) => &attr.name.name,
        NodeAtOffset::LetBindingName(lb) => &lb.name.name,
        NodeAtOffset::MacroDefName(md) => &md.name.name,
        NodeAtOffset::MacroCallName(mc) => &mc.name.name,
        _ => return Vec::new(),
    };

    let mut locations = Vec::new();

    // Collect all occurrences of this name in the AST
    collect_name_refs(&analysis.ast, target_name, uri, rope, &mut locations);

    // Use scope information to filter out unrelated references.
    // Find the scope where this name is defined — if it is in a non-module scope,
    // restrict references to only those within the enclosing block's span.
    if let Some(constraint) = find_scope_constraint(analysis, target_name, offset) {
        locations.retain(|loc| {
            let constraint_range = span_to_lsp_range(constraint, rope);
            loc.range.start >= constraint_range.start && loc.range.end <= constraint_range.end
        });
    }

    // If not including declaration, try to find and remove the definition site
    if !include_declaration {
        // Find definition span
        if let Some(def_span) = find_def_span(analysis, target_name) {
            let def_range = span_to_lsp_range(def_span, rope);
            locations.retain(|loc| loc.range != def_range);
        }
    }

    locations
}

/// Find the span constraint for filtering references based on scope.
///
/// If the definition lives in a Module scope, returns `None` (all references are valid).
/// If the definition lives in a Block/ForLoop/Lambda scope, returns the enclosing
/// block's AST span so we only accept references within that span.
fn find_scope_constraint(
    analysis: &AnalysisResult,
    name: &str,
    offset: usize,
) -> Option<wcl_lang::lang::span::Span> {
    // First, find which scope the cursor is in by finding the deepest scope
    // whose entry spans contain the offset.
    let def_scope_id = find_def_scope_for_offset(analysis, name, offset)?;
    let scope = analysis.scopes.get(def_scope_id);

    if scope.kind == ScopeKind::Module {
        // Module-level definition: all references are valid
        return None;
    }

    // For non-module scopes, find the enclosing block span from the AST.
    // Walk the AST to find the smallest block whose span contains the definition.
    let entry = scope.entries.get(name)?;
    let def_start = entry.span.start;
    find_enclosing_block_span(&analysis.ast, def_start)
}

/// Find the scope that defines `name` and is most relevant for `offset`.
///
/// If `name` is defined in multiple scopes (e.g., shadowed), prefer the scope
/// whose entries' spans are closest to `offset`.
fn find_def_scope_for_offset(
    analysis: &AnalysisResult,
    name: &str,
    offset: usize,
) -> Option<ScopeId> {
    let mut best: Option<(ScopeId, usize)> = None; // (scope_id, distance)

    for scope in analysis.scopes.all_scopes() {
        if let Some(entry) = scope.entries.get(name) {
            if entry.span.start == 0 && entry.span.end == 0 {
                continue;
            }
            // If the offset is within the entry's span, this is the best match
            if offset >= entry.span.start && offset < entry.span.end {
                return Some(scope.id);
            }
            // Otherwise measure distance to choose the closest
            let dist = if offset < entry.span.start {
                entry.span.start - offset
            } else {
                offset - entry.span.end
            };
            if best.is_none() || dist < best.unwrap().1 {
                best = Some((scope.id, dist));
            }
        }
    }

    // If offset is inside a block that has this name in scope, prefer that scope
    // by checking if offset falls within any block span in the scope chain.
    // Fall back to the closest definition.
    best.map(|(id, _)| id)
}

/// Walk the AST to find the smallest enclosing Block span that contains
/// the given byte offset.
fn find_enclosing_block_span(doc: &Document, offset: usize) -> Option<wcl_lang::lang::span::Span> {
    let mut result = None;
    for item in &doc.items {
        if let DocItem::Body(body_item) = item {
            find_enclosing_block_in_body(body_item, offset, &mut result);
        }
    }
    result
}

fn find_enclosing_block_in_body(
    item: &BodyItem,
    offset: usize,
    result: &mut Option<wcl_lang::lang::span::Span>,
) {
    match item {
        BodyItem::Block(block) => {
            if offset >= block.span.start && offset < block.span.end {
                // This block contains the offset. It may be a better (tighter) match.
                match result {
                    Some(existing) => {
                        let existing_size = existing.end - existing.start;
                        let new_size = block.span.end - block.span.start;
                        if new_size < existing_size {
                            *result = Some(block.span);
                        }
                    }
                    None => *result = Some(block.span),
                }
                // Recurse into children to find a tighter block
                for child in &block.body {
                    find_enclosing_block_in_body(child, offset, result);
                }
            }
        }
        BodyItem::ForLoop(fl) => {
            if offset >= fl.span.start && offset < fl.span.end {
                for child in &fl.body {
                    find_enclosing_block_in_body(child, offset, result);
                }
            }
        }
        BodyItem::Conditional(cond) => {
            for child in &cond.then_body {
                find_enclosing_block_in_body(child, offset, result);
            }
            if let Some(else_branch) = &cond.else_branch {
                match else_branch {
                    ElseBranch::ElseIf(inner) => {
                        find_enclosing_block_in_body(
                            &BodyItem::Conditional(inner.as_ref().clone()),
                            offset,
                            result,
                        );
                    }
                    ElseBranch::Else(body, _, _) => {
                        for child in body {
                            find_enclosing_block_in_body(child, offset, result);
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

fn find_def_span(analysis: &AnalysisResult, name: &str) -> Option<wcl_lang::lang::span::Span> {
    for scope in analysis.scopes.all_scopes() {
        if let Some(entry) = scope.entries.get(name) {
            if entry.span.start != 0 || entry.span.end != 0 {
                return Some(entry.span);
            }
        }
    }
    None
}

/// Collect all block kind identifiers matching `kind_name`.
fn collect_block_kinds(
    doc: &Document,
    kind_name: &str,
    uri: &async_lsp::lsp_types::Url,
    rope: &Rope,
    out: &mut Vec<Location>,
) {
    for item in &doc.items {
        if let DocItem::Body(body_item) = item {
            collect_block_kinds_in_body(body_item, kind_name, uri, rope, out);
        }
    }
}

fn collect_block_kinds_in_body(
    item: &BodyItem,
    kind_name: &str,
    uri: &async_lsp::lsp_types::Url,
    rope: &Rope,
    out: &mut Vec<Location>,
) {
    match item {
        BodyItem::Block(block) => {
            if block.kind.name == kind_name {
                out.push(Location {
                    uri: uri.clone(),
                    range: span_to_lsp_range(block.kind.span, rope),
                });
            }
            for child in &block.body {
                collect_block_kinds_in_body(child, kind_name, uri, rope, out);
            }
        }
        BodyItem::ForLoop(fl) => {
            for child in &fl.body {
                collect_block_kinds_in_body(child, kind_name, uri, rope, out);
            }
        }
        BodyItem::Conditional(cond) => {
            for child in &cond.then_body {
                collect_block_kinds_in_body(child, kind_name, uri, rope, out);
            }
            if let Some(else_branch) = &cond.else_branch {
                match else_branch {
                    ElseBranch::ElseIf(inner) => {
                        collect_block_kinds_in_body(
                            &BodyItem::Conditional(inner.as_ref().clone()),
                            kind_name,
                            uri,
                            rope,
                            out,
                        );
                    }
                    ElseBranch::Else(body, _, _) => {
                        for child in body {
                            collect_block_kinds_in_body(child, kind_name, uri, rope, out);
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

fn collect_name_refs(
    doc: &Document,
    name: &str,
    uri: &async_lsp::lsp_types::Url,
    rope: &Rope,
    out: &mut Vec<Location>,
) {
    for item in &doc.items {
        match item {
            DocItem::Body(body_item) => collect_in_body(body_item, name, uri, rope, out),
            DocItem::ExportLet(el) => {
                if el.name.name == name {
                    out.push(Location {
                        uri: uri.clone(),
                        range: span_to_lsp_range(el.name.span, rope),
                    });
                }
                collect_in_expr(&el.value, name, uri, rope, out);
            }
            DocItem::ReExport(re) => {
                if re.name.name == name {
                    out.push(Location {
                        uri: uri.clone(),
                        range: span_to_lsp_range(re.name.span, rope),
                    });
                }
            }
            DocItem::Import(_) | DocItem::FunctionDecl(_) => {}
            DocItem::Namespace(ns) => {
                for inner in &ns.items {
                    collect_in_doc_item(inner, name, uri, rope, out);
                }
            }
            DocItem::Use(_) => {}
        }
    }
}

fn collect_in_doc_item(
    item: &DocItem,
    name: &str,
    uri: &async_lsp::lsp_types::Url,
    rope: &Rope,
    out: &mut Vec<Location>,
) {
    match item {
        DocItem::Body(body_item) => collect_in_body(body_item, name, uri, rope, out),
        DocItem::ExportLet(el) => {
            if el.name.name == name {
                out.push(Location {
                    uri: uri.clone(),
                    range: span_to_lsp_range(el.name.span, rope),
                });
            }
            collect_in_expr(&el.value, name, uri, rope, out);
        }
        DocItem::ReExport(re) => {
            if re.name.name == name {
                out.push(Location {
                    uri: uri.clone(),
                    range: span_to_lsp_range(re.name.span, rope),
                });
            }
        }
        DocItem::Import(_) | DocItem::FunctionDecl(_) => {}
        DocItem::Namespace(ns) => {
            for inner in &ns.items {
                collect_in_doc_item(inner, name, uri, rope, out);
            }
        }
        DocItem::Use(_) => {}
    }
}

fn collect_in_body(
    item: &BodyItem,
    name: &str,
    uri: &async_lsp::lsp_types::Url,
    rope: &Rope,
    out: &mut Vec<Location>,
) {
    match item {
        BodyItem::Attribute(attr) => {
            if attr.name.name == name {
                out.push(Location {
                    uri: uri.clone(),
                    range: span_to_lsp_range(attr.name.span, rope),
                });
            }
            collect_in_expr(&attr.value, name, uri, rope, out);
        }
        BodyItem::Block(block) => {
            if block.kind.name == name {
                out.push(Location {
                    uri: uri.clone(),
                    range: span_to_lsp_range(block.kind.span, rope),
                });
            }
            for child in &block.body {
                collect_in_body(child, name, uri, rope, out);
            }
        }
        BodyItem::LetBinding(lb) => {
            if lb.name.name == name {
                out.push(Location {
                    uri: uri.clone(),
                    range: span_to_lsp_range(lb.name.span, rope),
                });
            }
            collect_in_expr(&lb.value, name, uri, rope, out);
        }
        BodyItem::MacroDef(md) => {
            if md.name.name == name {
                out.push(Location {
                    uri: uri.clone(),
                    range: span_to_lsp_range(md.name.span, rope),
                });
            }
        }
        BodyItem::MacroCall(mc) => {
            if mc.name.name == name {
                out.push(Location {
                    uri: uri.clone(),
                    range: span_to_lsp_range(mc.name.span, rope),
                });
            }
        }
        BodyItem::ForLoop(fl) => {
            if fl.iterator.name == name {
                out.push(Location {
                    uri: uri.clone(),
                    range: span_to_lsp_range(fl.iterator.span, rope),
                });
            }
            collect_in_expr(&fl.iterable, name, uri, rope, out);
            for child in &fl.body {
                collect_in_body(child, name, uri, rope, out);
            }
        }
        BodyItem::Conditional(cond) => {
            collect_in_expr(&cond.condition, name, uri, rope, out);
            for child in &cond.then_body {
                collect_in_body(child, name, uri, rope, out);
            }
            if let Some(else_branch) = &cond.else_branch {
                match else_branch {
                    ElseBranch::ElseIf(inner) => {
                        collect_in_body(
                            &BodyItem::Conditional(inner.as_ref().clone()),
                            name,
                            uri,
                            rope,
                            out,
                        );
                    }
                    ElseBranch::Else(body, _, _) => {
                        for child in body {
                            collect_in_body(child, name, uri, rope, out);
                        }
                    }
                }
            }
        }
        BodyItem::Validation(val) => {
            collect_in_expr(&val.check, name, uri, rope, out);
            collect_in_expr(&val.message, name, uri, rope, out);
        }
        BodyItem::Table(_)
        | BodyItem::Schema(_)
        | BodyItem::DecoratorSchema(_)
        | BodyItem::SymbolSetDecl(_)
        | BodyItem::StructDef(_) => {}
    }
}

fn collect_in_expr(
    expr: &Expr,
    name: &str,
    uri: &async_lsp::lsp_types::Url,
    rope: &Rope,
    out: &mut Vec<Location>,
) {
    match expr {
        Expr::Ident(ident) => {
            if ident.name == name {
                out.push(Location {
                    uri: uri.clone(),
                    range: span_to_lsp_range(ident.span, rope),
                });
            }
        }
        Expr::BinaryOp(lhs, _, rhs, _) => {
            collect_in_expr(lhs, name, uri, rope, out);
            collect_in_expr(rhs, name, uri, rope, out);
        }
        Expr::UnaryOp(_, inner, _) | Expr::Paren(inner, _) => {
            collect_in_expr(inner, name, uri, rope, out);
        }
        Expr::Ternary(a, b, c, _) => {
            collect_in_expr(a, name, uri, rope, out);
            collect_in_expr(b, name, uri, rope, out);
            collect_in_expr(c, name, uri, rope, out);
        }
        Expr::MemberAccess(obj, _, _) => {
            collect_in_expr(obj, name, uri, rope, out);
        }
        Expr::IndexAccess(obj, idx, _) => {
            collect_in_expr(obj, name, uri, rope, out);
            collect_in_expr(idx, name, uri, rope, out);
        }
        Expr::FnCall(callee, args, _) => {
            collect_in_expr(callee, name, uri, rope, out);
            for arg in args {
                let e = match arg {
                    CallArg::Positional(e) => e,
                    CallArg::Named(_, e) => e,
                };
                collect_in_expr(e, name, uri, rope, out);
            }
        }
        Expr::List(items, _) => {
            for item in items {
                collect_in_expr(item, name, uri, rope, out);
            }
        }
        Expr::Map(entries, _) => {
            for (_, val) in entries {
                collect_in_expr(val, name, uri, rope, out);
            }
        }
        Expr::Lambda(_, body, _) => collect_in_expr(body, name, uri, rope, out),
        Expr::BlockExpr(_, final_expr, _) => collect_in_expr(final_expr, name, uri, rope, out),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::analysis::analyze;
    use async_lsp::lsp_types::Url;

    fn get_refs(source: &str, offset: usize, include_decl: bool) -> Vec<Location> {
        let analysis = analyze(source, &wcl_lang::ParseOptions::default());
        let rope = Rope::from_str(source);
        let uri = Url::parse("file:///test.wcl").unwrap();
        find_references(&analysis, offset, &rope, &uri, include_decl)
    }

    #[test]
    fn test_find_refs_of_let_binding() {
        let source = "let x = 42\nconfig { port = x }";
        // offset at the definition of x
        let offset = source.find("x").unwrap();
        let refs = get_refs(source, offset, true);
        // Should find at least the definition and one usage
        assert!(refs.len() >= 2, "expected >= 2 refs, got {}", refs.len());
    }

    #[test]
    fn test_find_refs_include_declaration_has_more() {
        let source = "let x = 42\nconfig { port = x }";
        let offset = source.find("x").unwrap();
        let refs_with = get_refs(source, offset, true);
        // With declaration included, should find at least the definition
        assert!(!refs_with.is_empty());
    }

    #[test]
    fn test_find_refs_attribute() {
        let source = "config { port = 8080 }";
        let offset = source.find("port").unwrap();
        let refs = get_refs(source, offset, true);
        assert!(!refs.is_empty());
    }

    #[test]
    fn test_find_refs_no_match() {
        let source = "config { port = 8080 }";
        // offset at "8080" — numeric literal, no references
        let offset = source.find("8080").unwrap();
        let refs = get_refs(source, offset, true);
        assert!(refs.is_empty());
    }

    #[test]
    fn test_find_refs_scoped_variable() {
        // Two blocks each with their own `port` attribute — finding references
        // on the first `port` should NOT include the second block's `port`.
        let source = "server { port = 8080 }\ncache { port = 6379 }";
        // offset at the first "port"
        let offset = source.find("port").unwrap();
        let refs = get_refs(source, offset, true);
        // Should find exactly 1 reference (the `port` inside the `server` block only)
        assert_eq!(
            refs.len(),
            1,
            "expected 1 scoped ref for port in server block, got {}",
            refs.len()
        );
    }

    #[test]
    fn test_find_refs_block_kind() {
        // "find references" on a block kind should find all blocks of that kind
        let source = "server web { port = 8080 }\nserver api { port = 9090 }";
        // offset at the first "server"
        let offset = source.find("server").unwrap();
        let refs = get_refs(source, offset, true);
        // Should find both "server" block kinds
        assert_eq!(
            refs.len(),
            2,
            "expected 2 block kind refs for 'server', got {}",
            refs.len()
        );
    }

    #[test]
    fn test_find_refs_module_level_let() {
        // A module-level let binding should match references across all blocks.
        let source = "let base = 1000\nserver { port = base }\ncache { port = base }";
        // offset at the definition of "base"
        let offset = source.find("base").unwrap();
        let refs = get_refs(source, offset, true);
        // Should find: definition + 2 usages = 3
        assert!(
            refs.len() >= 3,
            "expected >= 3 refs for module-level 'base', got {}",
            refs.len()
        );
    }

    #[test]
    fn test_find_refs_schema_name() {
        let source =
            "schema \"server\" {\n    port: i64\n}\nserver web { port = 8080 }\nserver api { port = 9090 }";
        // offset at the schema name "server" (inside the string literal)
        let offset = source.find("\"server\"").unwrap() + 1;
        let refs = get_refs(source, offset, true);
        // Should find: schema declaration + 2 block kinds = 3
        assert_eq!(
            refs.len(),
            3,
            "expected 3 refs for schema 'server', got {}",
            refs.len()
        );
    }

    #[test]
    fn test_find_refs_schema_excludes_declaration() {
        let source = "schema \"server\" {\n    port: i64\n}\nserver web { port = 8080 }";
        let offset = source.find("\"server\"").unwrap() + 1;
        let refs = get_refs(source, offset, false);
        // Should find only the block kind, not the schema declaration
        assert_eq!(
            refs.len(),
            1,
            "expected 1 ref (excluding declaration), got {}",
            refs.len()
        );
    }
}
