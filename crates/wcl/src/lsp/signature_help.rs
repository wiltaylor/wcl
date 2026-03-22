use tower_lsp::lsp_types::{
    Documentation, ParameterInformation, ParameterLabel, SignatureHelp, SignatureInformation,
};

pub fn signature_help(
    source: &str,
    offset: usize,
    analysis: Option<&crate::lsp::state::AnalysisResult>,
) -> Option<SignatureHelp> {
    let before = &source[..offset.min(source.len())];
    let (fn_name, active_param) = find_call_context(before)?;

    // Try function signatures (builtins + custom) from analysis, or fallback to builtins
    let builtin_sigs;
    let sigs: &[crate::eval::FunctionSignature] = if let Some(analysis) = analysis {
        &analysis.function_signatures
    } else {
        builtin_sigs = crate::eval::builtin_signatures();
        &builtin_sigs
    };
    if let Some(sig) = sigs.iter().find(|s| s.name == fn_name) {
        let params: Vec<ParameterInformation> = sig
            .params
            .iter()
            .map(|p| ParameterInformation {
                label: ParameterLabel::Simple(p.to_string()),
                documentation: None,
            })
            .collect();

        let label = format!("{}({})", sig.name, sig.params.join(", "));

        return Some(SignatureHelp {
            signatures: vec![SignatureInformation {
                label,
                documentation: Some(Documentation::String(sig.doc.to_string())),
                parameters: Some(params),
                active_parameter: Some(active_param),
            }],
            active_signature: Some(0),
            active_parameter: Some(active_param),
        });
    }

    // Try user-defined macros from the macro registry
    if let Some(analysis) = analysis {
        if let Some(md) = analysis.macro_registry.function_macros.get(&fn_name) {
            let params: Vec<ParameterInformation> = md
                .params
                .iter()
                .map(|p| {
                    let label = if let Some(te) = &p.type_constraint {
                        format!("{}: {}", p.name.name, type_expr_label(te))
                    } else {
                        p.name.name.clone()
                    };
                    ParameterInformation {
                        label: ParameterLabel::Simple(label),
                        documentation: None,
                    }
                })
                .collect();

            let param_strs: Vec<String> = md
                .params
                .iter()
                .map(|p| {
                    if let Some(te) = &p.type_constraint {
                        format!("{}: {}", p.name.name, type_expr_label(te))
                    } else {
                        p.name.name.clone()
                    }
                })
                .collect();

            let label = format!("{}({})", md.name.name, param_strs.join(", "));

            return Some(SignatureHelp {
                signatures: vec![SignatureInformation {
                    label,
                    documentation: Some(Documentation::String("user-defined macro".to_string())),
                    parameters: Some(params),
                    active_parameter: Some(active_param),
                }],
                active_signature: Some(0),
                active_parameter: Some(active_param),
            });
        }
    }

    None
}

fn type_expr_label(te: &crate::lang::ast::TypeExpr) -> String {
    match te {
        crate::lang::ast::TypeExpr::String(_) => "string".to_string(),
        crate::lang::ast::TypeExpr::Int(_) => "int".to_string(),
        crate::lang::ast::TypeExpr::Float(_) => "float".to_string(),
        crate::lang::ast::TypeExpr::Bool(_) => "bool".to_string(),
        crate::lang::ast::TypeExpr::Any(_) => "any".to_string(),
        _ => "any".to_string(),
    }
}

/// Find the function name and active parameter index from text before cursor.
fn find_call_context(before: &str) -> Option<(String, u32)> {
    let mut depth = 0i32;
    let mut commas = 0u32;

    // Walk backwards to find the matching '('
    for (i, ch) in before.char_indices().rev() {
        match ch {
            ')' => depth += 1,
            '(' => {
                if depth == 0 {
                    // Found the opening paren, extract the function name before it
                    let prefix = before[..i].trim_end();
                    let name = prefix
                        .rsplit(|c: char| !c.is_alphanumeric() && c != '_')
                        .next()?;
                    if name.is_empty() {
                        return None;
                    }
                    return Some((name.to_string(), commas));
                }
                depth -= 1;
            }
            ',' if depth == 0 => commas += 1,
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_call_context_simple() {
        let (name, param) = find_call_context("upper(").unwrap();
        assert_eq!(name, "upper");
        assert_eq!(param, 0);
    }

    #[test]
    fn test_find_call_context_second_param() {
        let (name, param) = find_call_context("replace(s, ").unwrap();
        assert_eq!(name, "replace");
        assert_eq!(param, 1);
    }

    #[test]
    fn test_signature_help_upper() {
        let help = signature_help("upper(", 6, None).unwrap();
        assert_eq!(help.signatures[0].label, "upper(s: string)");
        assert_eq!(help.active_parameter, Some(0));
    }

    #[test]
    fn test_signature_help_unknown_returns_none() {
        let result = signature_help("unknown_fn(", 11, None);
        assert!(result.is_none());
    }

    #[test]
    fn test_find_call_context_nested() {
        let (name, param) = find_call_context("join(split(x, \".\"), ").unwrap();
        assert_eq!(name, "join");
        assert_eq!(param, 1);
    }

    #[test]
    fn test_signature_help_builtin_with_analysis() {
        use crate::lsp::analysis::analyze;
        let source = "let x = upper(";
        let analysis = analyze(source, &crate::ParseOptions::default());
        let offset = source.len();
        let help = signature_help(source, offset, Some(&analysis)).unwrap();
        assert!(help.signatures[0].label.contains("upper"));
    }

    #[test]
    fn test_signature_help_with_macro() {
        use crate::lsp::analysis::analyze;
        // Macros are collected into macro_registry during analysis
        let source = "macro greet(name, greeting) { msg = greeting }\nlet x = greet(";
        let analysis = analyze(source, &crate::ParseOptions::default());
        let offset = source.len();
        let help = signature_help(source, offset, Some(&analysis));
        assert!(help.is_some(), "should find macro signature");
        let h = help.unwrap();
        assert!(h.signatures[0].label.contains("greet"));
        assert!(h.signatures[0].label.contains("name"));
        assert!(h.signatures[0].label.contains("greeting"));
    }
}
