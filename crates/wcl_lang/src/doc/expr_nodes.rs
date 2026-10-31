//! A walk over every `let` item and function literal a source holds.
//!
//! Two passes need the same reach: provenance indexes each node's
//! address to find the file that holds it, and the open-time
//! declaration checks resolve every function signature's types. The
//! walk descends nested blocks, tables, decorator arguments, declared
//! field defaults and every expression form that can contain a
//! function literal.

use crate::ast;

/// One node the walk reports.
#[derive(Clone, Copy)]
pub(super) enum ExprNode<'a> {
    /// A `let` item, including the `fn name(…)` item form.
    Let(&'a ast::LetItem),
    /// A function literal, at any depth.
    Function(&'a ast::FunctionLit),
}

impl ExprNode<'_> {
    /// The node's identity, as [`Document::source_of_expr_node`] keys
    /// it: the `let` item itself, or a function literal's shared body.
    ///
    /// [`Document::source_of_expr_node`]: super::Document::source_of_expr_node
    pub(super) fn address(self) -> usize {
        match self {
            ExprNode::Let(binding) => std::ptr::from_ref(binding) as usize,
            ExprNode::Function(function) => std::sync::Arc::as_ptr(&function.body) as usize,
        }
    }
}

/// Report every `let` item and function literal in `items`, nested
/// blocks and every expression included.
pub(super) fn visit_expr_nodes<'a>(items: &'a [ast::Item], visit: &mut impl FnMut(ExprNode<'a>)) {
    for item in items {
        match item {
            ast::Item::Field(field) => {
                visit_decorators(&field.decorators, visit);
                visit_expr(&field.expr, visit);
            }
            ast::Item::Let(binding) => {
                visit(ExprNode::Let(binding));
                visit_decorators(&binding.decorators, visit);
                visit_expr(&binding.value, visit);
            }
            ast::Item::Block(block) => {
                visit_decorators(&block.decorators, visit);
                block
                    .labels
                    .iter()
                    .for_each(|label| visit_expr(label, visit));
                visit_expr_nodes(&block.items, visit);
            }
            ast::Item::Table(table) => {
                for row in &table.rows {
                    row.values.iter().for_each(|value| visit_expr(value, visit));
                }
            }
            ast::Item::TypeDecl(declaration) => {
                visit_decorators(&declaration.decorators, visit);
                visit_type_fields(&declaration.fields, visit);
            }
            ast::Item::InterfaceDecl(declaration) => {
                visit_decorators(&declaration.decorators, visit);
                visit_type_fields(&declaration.fields, visit);
            }
            ast::Item::UnionDecl(declaration) => {
                visit_decorators(&declaration.decorators, visit);
                for variant in &declaration.variants {
                    visit_decorators(&variant.decorators, visit);
                    if let ast::VariantBody::Record { fields, .. } = &variant.body {
                        visit_type_fields(fields, visit);
                    }
                }
            }
            ast::Item::SymbolSetDecl(declaration) => {
                visit_decorators(&declaration.decorators, visit);
            }
            ast::Item::ConnectionDecl(declaration) => {
                visit_decorators(&declaration.decorators, visit);
            }
            _ => {}
        }
    }
}

/// [`visit_expr_nodes`] over declared fields: their decorators and
/// inline defaults.
fn visit_type_fields<'a>(fields: &'a [ast::TypeField], visit: &mut impl FnMut(ExprNode<'a>)) {
    for field in fields {
        visit_decorators(&field.decorators, visit);
        if let Some(default) = &field.default_expr {
            visit_expr(default, visit);
        }
    }
}

/// [`visit_expr_nodes`] over decorator arguments.
fn visit_decorators<'a>(decorators: &'a [ast::Decorator], visit: &mut impl FnMut(ExprNode<'a>)) {
    for decorator in decorators {
        decorator
            .positional
            .iter()
            .for_each(|arg| visit_expr(arg, visit));
        decorator
            .named
            .iter()
            .for_each(|arg| visit_expr(&arg.value, visit));
    }
}

/// [`visit_expr_nodes`] within one expression: the body of every
/// function literal it contains, at any depth.
fn visit_expr<'a>(expr: &'a ast::Expr, visit: &mut impl FnMut(ExprNode<'a>)) {
    use ast::Expr as E;
    match expr {
        E::Function(function) => {
            visit(ExprNode::Function(function));
            visit_expr(&function.body, visit);
        }
        E::InterpolatedString { parts, .. } => {
            for part in parts {
                if let ast::TemplatePart::Expr(inner) = part {
                    visit_expr(inner, visit);
                }
            }
        }
        E::Call { callee, args, .. } => {
            visit_expr(callee, visit);
            args.iter().for_each(|arg| visit_expr(arg, visit));
        }
        E::Binary { lhs, rhs, .. } => {
            visit_expr(lhs, visit);
            visit_expr(rhs, visit);
        }
        E::Unary { operand: inner, .. }
        | E::Paren { inner, .. }
        | E::Member { recv: inner, .. } => visit_expr(inner, visit),
        E::Block { lets, tail, .. } => {
            lets.iter()
                .for_each(|binding| visit_expr(&binding.value, visit));
            visit_expr(tail, visit);
        }
        E::ListLit { elements, .. } => elements.iter().for_each(|e| visit_expr(e, visit)),
        E::If {
            cond,
            then_block,
            else_block,
            ..
        } => {
            visit_expr(cond, visit);
            visit_expr(then_block, visit);
            if let Some(else_block) = else_block {
                visit_expr(else_block, visit);
            }
        }
        E::IfLet {
            scrut,
            then_block,
            else_block,
            ..
        } => {
            visit_expr(scrut, visit);
            visit_expr(then_block, visit);
            visit_expr(else_block, visit);
        }
        E::Match { scrut, arms, .. } => {
            visit_expr(scrut, visit);
            for arm in arms {
                if let Some(guard) = &arm.guard {
                    visit_expr(guard, visit);
                }
                visit_expr(&arm.body, visit);
            }
        }
        E::Variant { args, .. } => match args {
            ast::VariantArgs::Positional(inner) => visit_expr(inner, visit),
            ast::VariantArgs::Record { fields, .. } => {
                fields.iter().for_each(|arg| visit_expr(&arg.value, visit));
            }
            ast::VariantArgs::Unit => {}
        },
        E::Try { body, handler, .. } => {
            visit_expr(body, visit);
            visit_expr(handler, visit);
        }
        E::Record { fields, .. } => fields.iter().for_each(|arg| visit_expr(&arg.value, visit)),
        _ => {}
    }
}
