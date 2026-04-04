use crate::lang::ast::*;
use crate::lang::diagnostic::DiagnosticBag;
use crate::lang::span::Span;

use crate::eval::value::Value;

/// Expands `for` loops and `if/else` conditionals in a WCL document.
///
/// Control flow expansion occurs after macro expansion and before final scope
/// construction. The expander evaluates iterable/condition expressions via a
/// caller-provided callback, then replaces `ForLoop` and `Conditional` AST
/// nodes with the concrete body items they expand to.
pub struct ControlFlowExpander {
    max_depth: u32,
    max_iterations: u32,
    total_iterations: u32,
    diagnostics: DiagnosticBag,
}

impl ControlFlowExpander {
    pub fn new(max_depth: u32, max_iterations: u32) -> Self {
        ControlFlowExpander {
            max_depth,
            max_iterations,
            total_iterations: 0,
            diagnostics: DiagnosticBag::new(),
        }
    }

    /// Expand all for loops and if/else conditionals in the document.
    ///
    /// The `eval_expr` callback is used to evaluate expressions (iterable
    /// expressions in for loops and condition expressions in if/else).
    /// Non-body `DocItem`s (imports, exports) are preserved as-is.
    pub fn expand(
        &mut self,
        doc: &mut Document,
        eval_expr: &dyn Fn(&Expr) -> Result<Value, String>,
    ) {
        let original_items = std::mem::take(&mut doc.items);
        let mut new_items: Vec<DocItem> = Vec::new();

        for item in original_items {
            match item {
                DocItem::Body(body_item) => {
                    let expanded = self.expand_single_item(body_item, eval_expr, 0);
                    for exp_item in expanded {
                        new_items.push(DocItem::Body(exp_item));
                    }
                }
                other => {
                    new_items.push(other);
                }
            }
        }

        doc.items = new_items;
    }

    /// Expand a single body item, returning the list of items it expands to.
    fn expand_single_item(
        &mut self,
        item: BodyItem,
        eval_expr: &dyn Fn(&Expr) -> Result<Value, String>,
        depth: u32,
    ) -> Vec<BodyItem> {
        if depth > self.max_depth {
            self.diagnostics.error_with_code(
                format!(
                    "control flow nesting depth limit exceeded (max {})",
                    self.max_depth
                ),
                Span::dummy(),
                "E029",
            );
            return vec![];
        }

        match item {
            BodyItem::ForLoop(for_loop) => self.expand_for_loop(&for_loop, eval_expr, depth),
            BodyItem::Conditional(cond) => self.expand_conditional(&cond, eval_expr, depth),
            BodyItem::Block(mut block) => {
                // Recurse into block body
                let original_body = std::mem::take(&mut block.body);
                let mut new_body = Vec::new();
                for child in original_body {
                    new_body.extend(self.expand_single_item(child, eval_expr, depth));
                }
                block.body = new_body;
                vec![BodyItem::Block(block)]
            }
            other => vec![other],
        }
    }

    /// Expand a for loop by evaluating the iterable and replicating the body.
    fn expand_for_loop(
        &mut self,
        for_loop: &ForLoop,
        eval_expr: &dyn Fn(&Expr) -> Result<Value, String>,
        depth: u32,
    ) -> Vec<BodyItem> {
        // Evaluate the iterable expression
        let iterable_value = match eval_expr(&for_loop.iterable) {
            Ok(v) => v,
            Err(e) => {
                self.diagnostics.error(
                    format!("error evaluating for loop iterable: {}", e),
                    for_loop.span,
                );
                return vec![];
            }
        };

        // The iterable must be a list
        let items = match &iterable_value {
            Value::List(items) => items.clone(),
            other => {
                self.diagnostics.error_with_code(
                    format!(
                        "for loop iterable must be a list, got {}",
                        other.type_name()
                    ),
                    for_loop.span,
                    "E025",
                );
                return vec![];
            }
        };

        // Check iteration limits
        let iteration_count = items.len() as u32;
        if iteration_count > 1000 {
            self.diagnostics.error_with_code(
                format!(
                    "for loop iteration limit exceeded: {} iterations (max 1000)",
                    iteration_count
                ),
                for_loop.span,
                "E028",
            );
            return vec![];
        }

        self.total_iterations += iteration_count;
        if self.total_iterations > self.max_iterations {
            self.diagnostics.error_with_code(
                format!(
                    "total iteration limit exceeded: {} (max {})",
                    self.total_iterations, self.max_iterations
                ),
                for_loop.span,
                "E028",
            );
            return vec![];
        }

        // Expand the body once per element
        let mut result = Vec::new();
        let iterator_name = &for_loop.iterator.name;
        let index_name = for_loop.index.as_ref().map(|i| &i.name);

        for (idx, element) in items.into_iter().enumerate() {
            // For each iteration, we clone the body and substitute the iterator variable.
            // Full substitution requires the evaluator to handle scope properly.
            // For now, we produce the body items as-is — the evaluator will bind
            // the iterator variable in the appropriate scope at evaluation time.
            //
            // However, we do need to handle identifier interpolation in inline IDs.
            for body_item in &for_loop.body {
                let mut cloned = body_item.clone();
                // Substitute iterator references in the cloned item
                substitute_value_in_body_item(
                    &mut cloned,
                    iterator_name,
                    &element,
                    index_name,
                    idx,
                );
                // Recursively expand any nested control flow
                let expanded = self.expand_single_item(cloned, eval_expr, depth + 1);
                result.extend(expanded);
            }
        }

        result
    }

    /// Expand a conditional by evaluating the condition and returning the
    /// matching branch's body.
    fn expand_conditional(
        &mut self,
        cond: &Conditional,
        eval_expr: &dyn Fn(&Expr) -> Result<Value, String>,
        depth: u32,
    ) -> Vec<BodyItem> {
        // Evaluate the condition
        let condition_value = match eval_expr(&cond.condition) {
            Ok(v) => v,
            Err(e) => {
                self.diagnostics
                    .error(format!("error evaluating if condition: {}", e), cond.span);
                return vec![];
            }
        };

        // Condition must be a bool
        let is_true = match condition_value.is_truthy() {
            Some(b) => b,
            None => {
                self.diagnostics.error_with_code(
                    format!(
                        "if condition must be bool, got {}",
                        condition_value.type_name()
                    ),
                    cond.span,
                    "E026",
                );
                return vec![];
            }
        };

        if is_true {
            // Expand the then branch
            let mut result = Vec::new();
            for item in &cond.then_body {
                result.extend(self.expand_single_item(item.clone(), eval_expr, depth + 1));
            }
            result
        } else {
            // Expand the else branch (if any)
            match &cond.else_branch {
                Some(ElseBranch::ElseIf(else_cond)) => {
                    self.expand_conditional(else_cond, eval_expr, depth)
                }
                Some(ElseBranch::Else(body, _, _)) => {
                    let mut result = Vec::new();
                    for item in body {
                        result.extend(self.expand_single_item(item.clone(), eval_expr, depth + 1));
                    }
                    result
                }
                None => vec![],
            }
        }
    }

    /// Consume the expander and return accumulated diagnostics.
    pub fn into_diagnostics(self) -> DiagnosticBag {
        self.diagnostics
    }
}

/// Substitute iterator/index values in a body item for identifier interpolation.
///
/// This handles the case where a for loop iterator appears in inline IDs,
/// string interpolations, and identifier references within the body.
fn substitute_value_in_body_item(
    item: &mut BodyItem,
    iterator_name: &str,
    value: &Value,
    index_name: Option<&String>,
    index: usize,
) {
    match item {
        BodyItem::Block(block) => {
            // Substitute in inline ID if it's interpolated
            if let Some(InlineId::Interpolated(parts)) = &mut block.inline_id {
                substitute_in_string_parts(parts, iterator_name, value, index_name, index);
            }
            try_resolve_interpolated_id(&mut block.inline_id);
            // Substitute in inline args
            for arg in &mut block.inline_args {
                substitute_in_expr(arg, iterator_name, value, index_name, index);
            }
            // Substitute in text content
            if let Some(ref mut tc) = block.text_content {
                substitute_in_string_parts(&mut tc.parts, iterator_name, value, index_name, index);
            }
            // Recurse into block body
            for child in &mut block.body {
                substitute_value_in_body_item(child, iterator_name, value, index_name, index);
            }
        }
        BodyItem::Attribute(attr) => {
            substitute_in_expr(&mut attr.value, iterator_name, value, index_name, index);
        }
        BodyItem::LetBinding(lb) => {
            substitute_in_expr(&mut lb.value, iterator_name, value, index_name, index);
        }
        BodyItem::Table(table) => {
            if let Some(InlineId::Interpolated(parts)) = &mut table.inline_id {
                substitute_in_string_parts(parts, iterator_name, value, index_name, index);
            }
            try_resolve_interpolated_id(&mut table.inline_id);
            for row in &mut table.rows {
                for cell in &mut row.cells {
                    substitute_in_expr(cell, iterator_name, value, index_name, index);
                }
            }
            if let Some(ref mut expr) = table.import_expr {
                substitute_in_expr(expr, iterator_name, value, index_name, index);
            }
        }
        _ => {
            // Other body items: no substitution needed at this level
        }
    }
}

/// Substitute iterator references in string parts (for identifier interpolation).
fn substitute_in_string_parts(
    parts: &mut [StringPart],
    iterator_name: &str,
    value: &Value,
    index_name: Option<&String>,
    index: usize,
) {
    for part in parts.iter_mut() {
        if let StringPart::Interpolation(expr) = part {
            substitute_in_expr(expr, iterator_name, value, index_name, index);
        }
    }
}

/// Substitute iterator/index references in an expression.
///
/// Replaces `Expr::Ident` nodes whose name matches the iterator or index
/// variable with an appropriate literal expression derived from the value.
fn substitute_in_expr(
    expr: &mut Expr,
    iterator_name: &str,
    value: &Value,
    index_name: Option<&String>,
    index: usize,
) {
    match expr {
        Expr::Ident(ident) => {
            if ident.name == iterator_name {
                if let Some(replacement) = value_to_expr(value, ident.span) {
                    *expr = replacement;
                }
            } else if let Some(idx_name) = index_name {
                if ident.name == *idx_name {
                    *expr = Expr::IntLit(index as i64, ident.span);
                }
            }
        }
        Expr::BinaryOp(lhs, _, rhs, _) => {
            substitute_in_expr(lhs, iterator_name, value, index_name, index);
            substitute_in_expr(rhs, iterator_name, value, index_name, index);
        }
        Expr::UnaryOp(_, operand, _) => {
            substitute_in_expr(operand, iterator_name, value, index_name, index);
        }
        Expr::Ternary(cond, then_e, else_e, _) => {
            substitute_in_expr(cond, iterator_name, value, index_name, index);
            substitute_in_expr(then_e, iterator_name, value, index_name, index);
            substitute_in_expr(else_e, iterator_name, value, index_name, index);
        }
        Expr::MemberAccess(obj, field, span) => {
            // Check if this is `iterator_name.field` — if so, and the iterator
            // value is a map, resolve the field directly to avoid leaving an
            // unevaluatable MemberAccess(Map(...), field) in the AST.
            if let Expr::Ident(ident) = obj.as_ref() {
                if ident.name == iterator_name {
                    if let Value::Map(map) = value {
                        if let Some(field_val) = map.get(&field.name) {
                            if let Some(replacement) = value_to_expr(field_val, *span) {
                                *expr = replacement;
                                return;
                            }
                        }
                    }
                }
            }
            substitute_in_expr(obj, iterator_name, value, index_name, index);
        }
        Expr::IndexAccess(obj, idx_expr, _) => {
            substitute_in_expr(obj, iterator_name, value, index_name, index);
            substitute_in_expr(idx_expr, iterator_name, value, index_name, index);
        }
        Expr::FnCall(callee, args, _) => {
            substitute_in_expr(callee, iterator_name, value, index_name, index);
            for arg in args {
                match arg {
                    CallArg::Positional(e) => {
                        substitute_in_expr(e, iterator_name, value, index_name, index);
                    }
                    CallArg::Named(_, e) => {
                        substitute_in_expr(e, iterator_name, value, index_name, index);
                    }
                }
            }
        }
        Expr::List(items, _) => {
            for item in items {
                substitute_in_expr(item, iterator_name, value, index_name, index);
            }
        }
        Expr::Map(entries, _) => {
            for (_, v) in entries {
                substitute_in_expr(v, iterator_name, value, index_name, index);
            }
        }
        Expr::StringLit(string_lit) => {
            for part in &mut string_lit.parts {
                if let StringPart::Interpolation(inner) = part {
                    substitute_in_expr(inner, iterator_name, value, index_name, index);
                }
            }
        }
        Expr::Paren(inner, _) => {
            substitute_in_expr(inner, iterator_name, value, index_name, index);
        }
        Expr::Lambda(_, body, _) => {
            substitute_in_expr(body, iterator_name, value, index_name, index);
        }
        _ => {
            // Literals and other leaf expressions: nothing to substitute
        }
    }
}

/// Try to collapse an `InlineId::Interpolated` into `InlineId::Literal`
/// by statically evaluating the string parts after substitution.
///
/// If all interpolation expressions have been replaced with string literals
/// (or other simple literals), the result is concatenated into a single
/// `IdentifierLit`. Otherwise the ID is left as `Interpolated`.
fn try_resolve_interpolated_id(id: &mut Option<InlineId>) {
    let parts = match id {
        Some(InlineId::Interpolated(parts)) => parts,
        _ => return,
    };

    let mut result = String::new();
    let mut span = Span::dummy();

    for part in parts.iter() {
        match part {
            StringPart::Literal(s) => result.push_str(s),
            StringPart::Interpolation(expr) => match expr.as_ref() {
                Expr::StringLit(s) => {
                    for p in &s.parts {
                        match p {
                            StringPart::Literal(t) => result.push_str(t),
                            StringPart::Interpolation(_) => return, // can't resolve
                        }
                    }
                    span = s.span;
                }
                Expr::IntLit(i, s) => {
                    result.push_str(&i.to_string());
                    span = *s;
                }
                Expr::FloatLit(f, s) => {
                    result.push_str(&f.to_string());
                    span = *s;
                }
                Expr::BoolLit(b, s) => {
                    result.push_str(&b.to_string());
                    span = *s;
                }
                Expr::IdentifierLit(lit) => {
                    result.push_str(&lit.value);
                    span = lit.span;
                }
                _ => return, // Can't resolve statically
            },
        }
    }

    *id = Some(InlineId::Literal(IdentifierLit {
        value: result,
        span,
    }));
}

/// Convert a `Value` to an `Expr` for substitution purposes.
pub(crate) fn value_to_expr(value: &Value, span: Span) -> Option<Expr> {
    match value {
        Value::String(s) => Some(Expr::StringLit(StringLit {
            parts: vec![StringPart::Literal(s.clone())],
            heredoc: None,
            span,
        })),
        Value::Int(i) => Some(Expr::IntLit(*i, span)),
        Value::Float(f) => Some(Expr::FloatLit(*f, span)),
        Value::Bool(b) => Some(Expr::BoolLit(*b, span)),
        Value::Null => Some(Expr::NullLit(span)),
        Value::Identifier(s) => Some(Expr::IdentifierLit(IdentifierLit {
            value: s.clone(),
            span,
        })),
        Value::List(items) => {
            let exprs: Vec<Expr> = items
                .iter()
                .filter_map(|v| value_to_expr(v, span))
                .collect();
            if exprs.len() == items.len() {
                Some(Expr::List(exprs, span))
            } else {
                None
            }
        }
        Value::Map(entries) => {
            let pairs: Vec<(MapKey, Expr)> = entries
                .iter()
                .filter_map(|(k, v)| {
                    value_to_expr(v, span).map(|e| {
                        (
                            MapKey::Ident(Ident {
                                name: k.clone(),
                                span,
                            }),
                            e,
                        )
                    })
                })
                .collect();
            if pairs.len() == entries.len() {
                Some(Expr::Map(pairs, span))
            } else {
                None
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::span::{FileId, Span};
    use crate::lang::trivia::Trivia;

    fn dummy_span() -> Span {
        Span::new(FileId(0), 0, 0)
    }

    fn make_ident(name: &str) -> Ident {
        Ident {
            name: name.to_string(),
            span: dummy_span(),
        }
    }

    #[test]
    fn expand_conditional_true_branch() {
        let mut expander = ControlFlowExpander::new(32, 10000);

        let cond = Conditional {
            condition: Expr::BoolLit(true, dummy_span()),
            then_body: vec![BodyItem::Attribute(Attribute {
                decorators: vec![],
                name: make_ident("enabled"),
                value: Expr::BoolLit(true, dummy_span()),
                assign_op: crate::lang::ast::AssignOp::Assign,
                trivia: Trivia::empty(),
                span: dummy_span(),
            })],
            else_branch: Some(ElseBranch::Else(
                vec![BodyItem::Attribute(Attribute {
                    decorators: vec![],
                    name: make_ident("enabled"),
                    value: Expr::BoolLit(false, dummy_span()),
                    assign_op: crate::lang::ast::AssignOp::Assign,
                    trivia: Trivia::empty(),
                    span: dummy_span(),
                })],
                Trivia::empty(),
                dummy_span(),
            )),
            trivia: Trivia::empty(),
            span: dummy_span(),
        };

        let eval = |expr: &Expr| -> Result<Value, String> {
            match expr {
                Expr::BoolLit(b, _) => Ok(Value::Bool(*b)),
                _ => Err("unsupported".to_string()),
            }
        };

        let result = expander.expand_conditional(&cond, &eval, 0);
        assert_eq!(result.len(), 1);
        match &result[0] {
            BodyItem::Attribute(attr) => {
                assert_eq!(attr.name.name, "enabled");
                match &attr.value {
                    Expr::BoolLit(true, _) => {}
                    _ => panic!("expected true"),
                }
            }
            _ => panic!("expected attribute"),
        }
    }

    #[test]
    fn expand_conditional_false_branch() {
        let mut expander = ControlFlowExpander::new(32, 10000);

        let cond = Conditional {
            condition: Expr::BoolLit(false, dummy_span()),
            then_body: vec![BodyItem::Attribute(Attribute {
                decorators: vec![],
                name: make_ident("then_attr"),
                value: Expr::IntLit(1, dummy_span()),
                assign_op: crate::lang::ast::AssignOp::Assign,
                trivia: Trivia::empty(),
                span: dummy_span(),
            })],
            else_branch: Some(ElseBranch::Else(
                vec![BodyItem::Attribute(Attribute {
                    decorators: vec![],
                    name: make_ident("else_attr"),
                    value: Expr::IntLit(2, dummy_span()),
                    assign_op: crate::lang::ast::AssignOp::Assign,
                    trivia: Trivia::empty(),
                    span: dummy_span(),
                })],
                Trivia::empty(),
                dummy_span(),
            )),
            trivia: Trivia::empty(),
            span: dummy_span(),
        };

        let eval = |expr: &Expr| -> Result<Value, String> {
            match expr {
                Expr::BoolLit(b, _) => Ok(Value::Bool(*b)),
                _ => Err("unsupported".to_string()),
            }
        };

        let result = expander.expand_conditional(&cond, &eval, 0);
        assert_eq!(result.len(), 1);
        match &result[0] {
            BodyItem::Attribute(attr) => assert_eq!(attr.name.name, "else_attr"),
            _ => panic!("expected attribute"),
        }
    }

    #[test]
    fn expand_conditional_no_else_false() {
        let mut expander = ControlFlowExpander::new(32, 10000);

        let cond = Conditional {
            condition: Expr::BoolLit(false, dummy_span()),
            then_body: vec![BodyItem::Attribute(Attribute {
                decorators: vec![],
                name: make_ident("attr"),
                value: Expr::IntLit(1, dummy_span()),
                assign_op: crate::lang::ast::AssignOp::Assign,
                trivia: Trivia::empty(),
                span: dummy_span(),
            })],
            else_branch: None,
            trivia: Trivia::empty(),
            span: dummy_span(),
        };

        let eval = |expr: &Expr| -> Result<Value, String> {
            match expr {
                Expr::BoolLit(b, _) => Ok(Value::Bool(*b)),
                _ => Err("unsupported".to_string()),
            }
        };

        let result = expander.expand_conditional(&cond, &eval, 0);
        assert!(result.is_empty());
    }

    #[test]
    fn expand_conditional_non_bool_condition_is_error() {
        let mut expander = ControlFlowExpander::new(32, 10000);

        let cond = Conditional {
            condition: Expr::IntLit(42, dummy_span()),
            then_body: vec![],
            else_branch: None,
            trivia: Trivia::empty(),
            span: dummy_span(),
        };

        let eval = |expr: &Expr| -> Result<Value, String> {
            match expr {
                Expr::IntLit(i, _) => Ok(Value::Int(*i)),
                _ => Err("unsupported".to_string()),
            }
        };

        let result = expander.expand_conditional(&cond, &eval, 0);
        assert!(result.is_empty());
        assert!(expander.diagnostics.has_errors());
    }

    #[test]
    fn expand_for_loop_basic() {
        let mut expander = ControlFlowExpander::new(32, 10000);

        let for_loop = ForLoop {
            iterator: make_ident("item"),
            index: None,
            iterable: Expr::Ident(make_ident("my_list")),
            body: vec![BodyItem::Attribute(Attribute {
                decorators: vec![],
                name: make_ident("value"),
                value: Expr::Ident(make_ident("item")),
                assign_op: crate::lang::ast::AssignOp::Assign,
                trivia: Trivia::empty(),
                span: dummy_span(),
            })],
            trivia: Trivia::empty(),
            span: dummy_span(),
        };

        let eval = |expr: &Expr| -> Result<Value, String> {
            match expr {
                Expr::Ident(ident) if ident.name == "my_list" => Ok(Value::List(vec![
                    Value::String("a".to_string()),
                    Value::String("b".to_string()),
                ])),
                _ => Err("unsupported".to_string()),
            }
        };

        let result = expander.expand_for_loop(&for_loop, &eval, 0);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn expand_for_loop_empty_list() {
        let mut expander = ControlFlowExpander::new(32, 10000);

        let for_loop = ForLoop {
            iterator: make_ident("item"),
            index: None,
            iterable: Expr::Ident(make_ident("empty")),
            body: vec![BodyItem::Attribute(Attribute {
                decorators: vec![],
                name: make_ident("value"),
                value: Expr::Ident(make_ident("item")),
                assign_op: crate::lang::ast::AssignOp::Assign,
                trivia: Trivia::empty(),
                span: dummy_span(),
            })],
            trivia: Trivia::empty(),
            span: dummy_span(),
        };

        let eval = |expr: &Expr| -> Result<Value, String> {
            match expr {
                Expr::Ident(ident) if ident.name == "empty" => Ok(Value::List(vec![])),
                _ => Err("unsupported".to_string()),
            }
        };

        let result = expander.expand_for_loop(&for_loop, &eval, 0);
        assert!(result.is_empty());
        assert!(!expander.diagnostics.has_errors());
    }

    #[test]
    fn expand_for_loop_non_list_is_error() {
        let mut expander = ControlFlowExpander::new(32, 10000);

        let for_loop = ForLoop {
            iterator: make_ident("item"),
            index: None,
            iterable: Expr::Ident(make_ident("not_a_list")),
            body: vec![],
            trivia: Trivia::empty(),
            span: dummy_span(),
        };

        let eval = |expr: &Expr| -> Result<Value, String> {
            match expr {
                Expr::Ident(ident) if ident.name == "not_a_list" => {
                    Ok(Value::String("hello".to_string()))
                }
                _ => Err("unsupported".to_string()),
            }
        };

        let result = expander.expand_for_loop(&for_loop, &eval, 0);
        assert!(result.is_empty());
        assert!(expander.diagnostics.has_errors());
    }

    #[test]
    fn value_to_expr_conversions() {
        let span = dummy_span();

        assert!(matches!(
            value_to_expr(&Value::String("hello".into()), span),
            Some(Expr::StringLit(_))
        ));
        assert!(matches!(
            value_to_expr(&Value::Int(42), span),
            Some(Expr::IntLit(42, _))
        ));
        assert!(matches!(
            value_to_expr(&Value::Bool(true), span),
            Some(Expr::BoolLit(true, _))
        ));
        assert!(matches!(
            value_to_expr(&Value::Null, span),
            Some(Expr::NullLit(_))
        ));
        // Empty list converts to Expr::List
        assert!(matches!(
            value_to_expr(&Value::List(vec![]), span),
            Some(Expr::List(items, _)) if items.is_empty()
        ));
        // List with convertible elements
        assert!(matches!(
            value_to_expr(&Value::List(vec![Value::Int(1), Value::Int(2)]), span),
            Some(Expr::List(items, _)) if items.len() == 2
        ));
        // Map converts to Expr::Map
        let mut map = indexmap::IndexMap::new();
        map.insert("key".to_string(), Value::String("val".to_string()));
        assert!(matches!(
            value_to_expr(&Value::Map(map), span),
            Some(Expr::Map(pairs, _)) if pairs.len() == 1
        ));
    }
}
