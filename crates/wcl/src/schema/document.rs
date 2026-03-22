use crate::eval::evaluator::Evaluator;
use crate::eval::value::Value;
use crate::lang::ast::*;
use crate::lang::diagnostic::DiagnosticBag;

/// Execute all `validation` blocks in the document.
///
/// Each validation block is evaluated in a fresh document context so that
/// let-bindings inside the validation have access to the outer document scope
/// (the evaluator was already run over the full document).  For each block we:
///
///  1. Evaluate the `check` expression.
///  2. If false, evaluate the `message` expression and emit an error (or
///     warning when the block carries `@warning`).
///
/// Because `Evaluator` internalises its scope arena we cannot inject
/// per-validation let-bindings without re-evaluating the whole document.
/// The simpler approach taken here is: create a new standalone sub-document
/// containing only the validation's own let-bindings plus the validation
/// check/message expressions, and run a fresh evaluator over it.  For
/// production use the caller should wire up a richer scope; the interface
/// here keeps the crate free from assuming the evaluator's internal layout.
pub fn validate_document(
    doc: &Document,
    _evaluator: &mut Evaluator,
    diagnostics: &mut DiagnosticBag,
) {
    for item in &doc.items {
        if let DocItem::Body(BodyItem::Validation(validation)) = item {
            validate_rule(validation, _evaluator, diagnostics);
        }
    }
}

fn validate_rule(
    validation: &Validation,
    _evaluator: &mut Evaluator,
    diagnostics: &mut DiagnosticBag,
) {
    let is_warning = validation
        .decorators
        .iter()
        .any(|d| d.name.name == "warning");

    // Build a tiny synthetic document that contains only the let-bindings from
    // this validation block so we can evaluate them in isolation.
    let mut items: Vec<DocItem> = validation
        .lets
        .iter()
        .map(|lb| DocItem::Body(BodyItem::LetBinding(lb.clone())))
        .collect();

    // Append a synthetic attribute for `check` and `message` so the evaluator
    // registers them and we can retrieve their values from the output map.
    items.push(DocItem::Body(BodyItem::Attribute(Attribute {
        decorators: vec![],
        name: crate::lang::ast::Ident {
            name: "__wcl_schema_check__".to_string(),
            span: validation.span,
        },
        value: validation.check.clone(),
        trivia: crate::lang::trivia::Trivia::default(),
        span: validation.span,
    })));
    items.push(DocItem::Body(BodyItem::Attribute(Attribute {
        decorators: vec![],
        name: crate::lang::ast::Ident {
            name: "__wcl_schema_message__".to_string(),
            span: validation.span,
        },
        value: validation.message.clone(),
        trivia: crate::lang::trivia::Trivia::default(),
        span: validation.span,
    })));

    let sub_doc = Document {
        items,
        trivia: crate::lang::trivia::Trivia::default(),
        span: validation.span,
    };

    // Run a fresh evaluator so we don't taint the caller's state.
    let mut sub_eval = Evaluator::new();
    let values = sub_eval.evaluate(&sub_doc);

    // Propagate any evaluation errors from the sub-evaluation.
    let sub_diags = sub_eval.into_diagnostics();
    if sub_diags.has_errors() {
        diagnostics.merge(sub_diags);
        return;
    }

    let check_val = values.get("__wcl_schema_check__");
    let name = crate::schema::schema::string_lit_to_string(&validation.name);

    match check_val {
        Some(Value::Bool(true)) => {
            // Validation passed — nothing to do.
        }
        Some(Value::Bool(false)) => {
            let message = values
                .get("__wcl_schema_message__")
                .and_then(|v| match v {
                    Value::String(s) => Some(s.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| "validation failed".to_string());

            if is_warning {
                diagnostics.warning(
                    format!("validation '{}': {}", name, message),
                    validation.span,
                );
            } else {
                diagnostics.error_with_code(
                    format!("validation '{}': {}", name, message),
                    validation.span,
                    "E080",
                );
            }
        }
        Some(other) => {
            diagnostics.error_with_code(
                format!(
                    "validation '{}' check must return bool, got {}",
                    name,
                    other.type_name()
                ),
                validation.span,
                "E050",
            );
        }
        None => {
            diagnostics.error_with_code(
                format!(
                    "validation '{}' check expression did not produce a value",
                    name
                ),
                validation.span,
                "E050",
            );
        }
    }
}
