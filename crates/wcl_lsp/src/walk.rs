//! Static AST walker that finds every local-scope binding (function
//! parameter, `let` binding) whose enclosing scope spans a given byte
//! offset.
//!
//! The wcl_lang library doesn't expose a generic visitor; the LSP only
//! needs this one slice of it, so we hand-roll a recursive descent that
//! prunes subtrees not containing the offset and accumulates bindings
//! in outer-to-inner order. Callers can dedup with later (inner)
//! entries shadowing earlier ones.

use wcl_lang::Span;
use wcl_lang::ast::{
    Expr, Field, FunctionLit, Item, LetBinding, Parameter, Pattern, TemplatePart, VariantArgs,
    VariantPatArgs,
};

#[derive(Default)]
pub(crate) struct EnclosingScopes<'a> {
    pub params: Vec<&'a Parameter>,
    pub lets: Vec<&'a LetBinding>,
    /// Names bound by `match` / `if let` patterns whose arm body spans
    /// the offset, as `(name, declaration-span)`. Distinct from `lets`
    /// because pattern bindings aren't `LetBinding`s.
    pub bindings: Vec<(&'a str, Span)>,
}

pub(crate) fn enclosing_scopes_at<'a>(items: &'a [Item], offset: usize) -> EnclosingScopes<'a> {
    let mut out = EnclosingScopes::default();
    for item in items {
        walk_item(item, offset, &mut out);
    }
    out
}

fn walk_item<'a>(item: &'a Item, offset: usize, out: &mut EnclosingScopes<'a>) {
    match item {
        Item::Field(f) => walk_field(f, offset, out),
        Item::Block(b) => {
            if !contains(b.span, offset) {
                return;
            }
            for lbl in &b.labels {
                walk_expr(lbl, offset, out);
            }
            for inner in &b.items {
                walk_item(inner, offset, out);
            }
        }
        // Top-level decls don't have user-writable expression bodies
        // that contain function/block scopes (decorator args do, but
        // those are exprs covered by walk_field on a separate path).
        _ => {}
    }
}

fn walk_field<'a>(f: &'a Field, offset: usize, out: &mut EnclosingScopes<'a>) {
    if !contains(f.span, offset) {
        return;
    }
    walk_expr(&f.expr, offset, out);
}

fn walk_expr<'a>(expr: &'a Expr, offset: usize, out: &mut EnclosingScopes<'a>) {
    match expr {
        Expr::Block { lets, tail, span } => {
            if !contains(*span, offset) {
                return;
            }
            // Only let-bindings whose `=`-rhs ends *before* the offset
            // are in scope at the offset. Including a let whose value
            // span overlaps the cursor would suggest the binding
            // inside its own initialiser.
            for lb in lets {
                if lb.span.end <= offset {
                    out.lets.push(lb);
                }
                walk_expr(&lb.value, offset, out);
            }
            walk_expr(tail, offset, out);
        }
        Expr::Function(FunctionLit {
            params, body, span, ..
        }) => {
            if !contains(*span, offset) {
                return;
            }
            for p in params {
                out.params.push(p);
            }
            walk_expr(body, offset, out);
        }
        Expr::Call { callee, args, span } => {
            if !contains(*span, offset) {
                return;
            }
            walk_expr(callee, offset, out);
            for a in args {
                walk_expr(a, offset, out);
            }
        }
        Expr::Binary { lhs, rhs, span, .. } => {
            if !contains(*span, offset) {
                return;
            }
            walk_expr(lhs, offset, out);
            walk_expr(rhs, offset, out);
        }
        Expr::Unary { operand, span, .. } => {
            if !contains(*span, offset) {
                return;
            }
            walk_expr(operand, offset, out);
        }
        Expr::Member { recv, span, .. } => {
            if !contains(*span, offset) {
                return;
            }
            walk_expr(recv, offset, out);
        }
        Expr::Paren { inner, span } => {
            if !contains(*span, offset) {
                return;
            }
            walk_expr(inner, offset, out);
        }
        Expr::ListLit { elements, span } => {
            if !contains(*span, offset) {
                return;
            }
            for e in elements {
                walk_expr(e, offset, out);
            }
        }
        Expr::If {
            cond,
            then_block,
            else_block,
            span,
        } => {
            if !contains(*span, offset) {
                return;
            }
            walk_expr(cond, offset, out);
            walk_expr(then_block, offset, out);
            walk_expr(else_block, offset, out);
        }
        Expr::IfLet {
            pattern,
            scrut,
            then_block,
            else_block,
            span,
        } => {
            if !contains(*span, offset) {
                return;
            }
            walk_expr(scrut, offset, out);
            // Pattern bindings are in scope inside `then_block`.
            if contains_in_expr(then_block, offset) {
                push_pattern_bindings(pattern, out);
            }
            walk_expr(then_block, offset, out);
            walk_expr(else_block, offset, out);
        }
        Expr::Match { scrut, arms, span } => {
            if !contains(*span, offset) {
                return;
            }
            walk_expr(scrut, offset, out);
            for arm in arms {
                if !contains(arm.span, offset) {
                    continue;
                }
                for pat in &arm.patterns {
                    push_pattern_bindings(pat, out);
                }
                if let Some(g) = &arm.guard {
                    walk_expr(g, offset, out);
                }
                walk_expr(&arm.body, offset, out);
            }
        }
        Expr::Variant { args, span, .. } => {
            if !contains(*span, offset) {
                return;
            }
            match args {
                VariantArgs::Unit => {}
                VariantArgs::Positional(e) => walk_expr(e, offset, out),
                VariantArgs::Record(named) => {
                    for n in named {
                        walk_expr(&n.value, offset, out);
                    }
                }
            }
        }
        Expr::InterpolatedString { parts, span, .. } => {
            if !contains(*span, offset) {
                return;
            }
            for p in parts {
                if let TemplatePart::Expr(e) = p {
                    walk_expr(e, offset, out);
                }
            }
        }
        // Atoms — no sub-expressions to descend into.
        _ => {}
    }
}

/// Collect the names a pattern binds into `out.bindings`. Recurses
/// through `name @ inner` and into variant sub-patterns so a binding
/// nested inside a destructuring (`Some(x)`, `Point { x, y }`) is
/// surfaced too. Wildcards and literals bind nothing.
fn push_pattern_bindings<'a>(pat: &'a Pattern, out: &mut EnclosingScopes<'a>) {
    match pat {
        Pattern::Binding { name, span } => out.bindings.push((name.as_str(), *span)),
        Pattern::At { name, inner, span } => {
            out.bindings.push((name.as_str(), *span));
            push_pattern_bindings(inner, out);
        }
        Pattern::Variant { args, .. } => match args {
            VariantPatArgs::Positional(inner) => push_pattern_bindings(inner, out),
            VariantPatArgs::Record { fields, .. } => {
                for (_, sub) in fields {
                    push_pattern_bindings(sub, out);
                }
            }
            VariantPatArgs::Unit => {}
        },
        Pattern::Wildcard(_)
        | Pattern::LiteralBool(_, _)
        | Pattern::LiteralNumber { .. }
        | Pattern::LiteralUtf8(_, _)
        | Pattern::LiteralAscii(_, _)
        | Pattern::LiteralSymbol(_, _)
        | Pattern::LiteralNone(_) => {}
    }
}

fn contains(span: wcl_lang::ast::Span, offset: usize) -> bool {
    offset >= span.start && offset <= span.end
}

/// Cheap span check that pulls a span out of any `Expr` variant.
fn contains_in_expr(expr: &Expr, offset: usize) -> bool {
    let span = expr_span(expr);
    offset >= span.start && offset <= span.end
}

fn expr_span(expr: &Expr) -> wcl_lang::ast::Span {
    use wcl_lang::ast::Span;
    match expr {
        Expr::InterpolatedString { span, .. }
        | Expr::Call { span, .. }
        | Expr::Binary { span, .. }
        | Expr::Unary { span, .. }
        | Expr::Block { span, .. }
        | Expr::Paren { span, .. }
        | Expr::ListLit { span, .. }
        | Expr::Member { span, .. }
        | Expr::If { span, .. }
        | Expr::IfLet { span, .. }
        | Expr::Match { span, .. }
        | Expr::Variant { span, .. } => *span,
        Expr::SelfKw(s) | Expr::ParentKw(s) => *s,
        Expr::Function(f) => f.span,
        Expr::Identifier(_, s) => *s,
        _ => Span::new(0, 0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wcl_lang::parse_for_edit;

    fn ast(src: &str) -> wcl_lang::ast::Source {
        parse_for_edit(src, "test.wcl").expect("parse ok")
    }

    #[test]
    fn let_binding_visible_inside_block() {
        let src = "x = {\n  let a = 1;\n  let b = 2;\n  a + b\n}\n";
        let s = ast(src);
        let cursor = src.find("a + b").unwrap() + 1;
        let scopes = enclosing_scopes_at(&s.items, cursor);
        let names: Vec<_> = scopes.lets.iter().map(|l| l.name.as_str()).collect();
        assert!(names.contains(&"a"), "{names:?}");
        assert!(names.contains(&"b"), "{names:?}");
    }

    #[test]
    fn let_binding_not_visible_in_its_own_initialiser() {
        let src = "x = {\n  let a = 1;\n  let b = a + 1;\n  b\n}\n";
        let s = ast(src);
        // Cursor sits on the `a` in `let b = a + 1`.
        let cursor = src.find("let b = a").unwrap() + "let b = ".len();
        let scopes = enclosing_scopes_at(&s.items, cursor);
        let names: Vec<_> = scopes.lets.iter().map(|l| l.name.as_str()).collect();
        assert!(names.contains(&"a"), "{names:?}");
        assert!(!names.contains(&"b"), "{names:?}");
    }

    #[test]
    fn fn_params_visible_in_body() {
        let src = "x = fn (a: i32, b: i32) -> i32 { a + b }\n";
        let s = ast(src);
        let cursor = src.find("a + b").unwrap() + 1;
        let scopes = enclosing_scopes_at(&s.items, cursor);
        let names: Vec<_> = scopes.params.iter().map(|p| p.name.as_str()).collect();
        assert!(names.contains(&"a"), "{names:?}");
        assert!(names.contains(&"b"), "{names:?}");
    }

    #[test]
    fn nothing_visible_outside_any_scope() {
        let src = "x = {\n  let a = 1;\n  a\n}\ny = 2\n";
        let s = ast(src);
        let cursor = src.find("y = 2").unwrap() + 1;
        let scopes = enclosing_scopes_at(&s.items, cursor);
        assert!(scopes.lets.is_empty());
        assert!(scopes.params.is_empty());
    }

    fn binding_names<'a>(scopes: &EnclosingScopes<'a>) -> Vec<&'a str> {
        scopes.bindings.iter().map(|(n, _)| *n).collect()
    }

    #[test]
    fn match_record_binding_visible_in_arm_body() {
        let src = "x = match c1 {\n  Shape::Circle { radius, .. } => radius,\n  _ => 0.0,\n}\n";
        let s = ast(src);
        let cursor = src.find("=> radius").unwrap() + 3;
        let scopes = enclosing_scopes_at(&s.items, cursor);
        assert!(
            binding_names(&scopes).contains(&"radius"),
            "{:?}",
            binding_names(&scopes)
        );
    }

    #[test]
    fn match_binding_arm_visible_in_body() {
        let src = "x = match tag {\n  :red => :stop,\n  other => other,\n}\n";
        let s = ast(src);
        let cursor = src.find("=> other").unwrap() + 3;
        let scopes = enclosing_scopes_at(&s.items, cursor);
        assert!(
            binding_names(&scopes).contains(&"other"),
            "{:?}",
            binding_names(&scopes)
        );
    }

    #[test]
    fn match_at_binding_visible_in_body() {
        let src = "x = match c1 {\n  shape @ Shape::Circle { .. } => shape,\n  _ => 0,\n}\n";
        let s = ast(src);
        let cursor = src.find("=> shape").unwrap() + 3;
        let scopes = enclosing_scopes_at(&s.items, cursor);
        assert!(
            binding_names(&scopes).contains(&"shape"),
            "{:?}",
            binding_names(&scopes)
        );
    }

    #[test]
    fn if_let_binding_visible_in_then_block() {
        let src = "x = if let Shape::Circle { radius, .. } = c1 { radius } else { 0.0 }\n";
        let s = ast(src);
        let cursor = src.find("{ radius }").unwrap() + 2;
        let scopes = enclosing_scopes_at(&s.items, cursor);
        assert!(
            binding_names(&scopes).contains(&"radius"),
            "{:?}",
            binding_names(&scopes)
        );
    }

    #[test]
    fn match_binding_not_visible_outside() {
        let src = "x = match tag {\n  other => other,\n}\ny = 2\n";
        let s = ast(src);
        let cursor = src.find("y = 2").unwrap() + 1;
        let scopes = enclosing_scopes_at(&s.items, cursor);
        assert!(scopes.bindings.is_empty(), "{:?}", binding_names(&scopes));
    }
}
