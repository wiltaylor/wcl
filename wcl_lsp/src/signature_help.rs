use tower_lsp::lsp_types::{
    Documentation, ParameterInformation, ParameterLabel, SignatureHelp, SignatureInformation,
};

/// Static signature info for built-in functions.
struct BuiltinSig {
    name: &'static str,
    params: &'static [&'static str],
    doc: &'static str,
}

const BUILTIN_SIGS: &[BuiltinSig] = &[
    BuiltinSig { name: "upper", params: &["s: string"], doc: "Convert string to uppercase" },
    BuiltinSig { name: "lower", params: &["s: string"], doc: "Convert string to lowercase" },
    BuiltinSig { name: "trim", params: &["s: string"], doc: "Trim whitespace" },
    BuiltinSig { name: "trim_prefix", params: &["s: string", "prefix: string"], doc: "Remove prefix" },
    BuiltinSig { name: "trim_suffix", params: &["s: string", "suffix: string"], doc: "Remove suffix" },
    BuiltinSig { name: "replace", params: &["s: string", "from: string", "to: string"], doc: "Replace occurrences" },
    BuiltinSig { name: "split", params: &["s: string", "sep: string"], doc: "Split string by separator" },
    BuiltinSig { name: "join", params: &["list: list", "sep: string"], doc: "Join list elements" },
    BuiltinSig { name: "starts_with", params: &["s: string", "prefix: string"], doc: "Check prefix" },
    BuiltinSig { name: "ends_with", params: &["s: string", "suffix: string"], doc: "Check suffix" },
    BuiltinSig { name: "contains", params: &["s: string", "sub: string"], doc: "Check substring" },
    BuiltinSig { name: "length", params: &["s: string"], doc: "String length" },
    BuiltinSig { name: "substr", params: &["s: string", "start: int", "end: int"], doc: "Substring" },
    BuiltinSig { name: "format", params: &["fmt: string", "...args"], doc: "Format string" },
    BuiltinSig { name: "abs", params: &["n: number"], doc: "Absolute value" },
    BuiltinSig { name: "min", params: &["a: number", "b: number"], doc: "Minimum" },
    BuiltinSig { name: "max", params: &["a: number", "b: number"], doc: "Maximum" },
    BuiltinSig { name: "floor", params: &["n: float"], doc: "Floor" },
    BuiltinSig { name: "ceil", params: &["n: float"], doc: "Ceiling" },
    BuiltinSig { name: "round", params: &["n: float"], doc: "Round" },
    BuiltinSig { name: "sqrt", params: &["n: float"], doc: "Square root" },
    BuiltinSig { name: "pow", params: &["base: float", "exp: float"], doc: "Power" },
    BuiltinSig { name: "len", params: &["collection"], doc: "Collection length" },
    BuiltinSig { name: "keys", params: &["m: map"], doc: "Map keys" },
    BuiltinSig { name: "values", params: &["m: map"], doc: "Map values" },
    BuiltinSig { name: "flatten", params: &["list: list"], doc: "Flatten nested lists" },
    BuiltinSig { name: "concat", params: &["a: list", "b: list"], doc: "Concatenate lists" },
    BuiltinSig { name: "distinct", params: &["list: list"], doc: "Remove duplicates" },
    BuiltinSig { name: "sort", params: &["list: list"], doc: "Sort list" },
    BuiltinSig { name: "reverse", params: &["list: list"], doc: "Reverse list" },
    BuiltinSig { name: "index_of", params: &["list: list", "elem"], doc: "Find element index" },
    BuiltinSig { name: "range", params: &["start: int", "end: int"], doc: "Integer range" },
    BuiltinSig { name: "zip", params: &["a: list", "b: list"], doc: "Zip two lists" },
    BuiltinSig { name: "map", params: &["list: list", "fn: lambda"], doc: "Map over list" },
    BuiltinSig { name: "filter", params: &["list: list", "fn: lambda"], doc: "Filter list" },
    BuiltinSig { name: "every", params: &["list: list", "fn: lambda"], doc: "All match predicate" },
    BuiltinSig { name: "some", params: &["list: list", "fn: lambda"], doc: "Any matches predicate" },
    BuiltinSig { name: "reduce", params: &["list: list", "init", "fn: lambda"], doc: "Reduce list" },
    BuiltinSig { name: "sum", params: &["list: list(number)"], doc: "Sum numbers" },
    BuiltinSig { name: "avg", params: &["list: list(number)"], doc: "Average" },
    BuiltinSig { name: "sha256", params: &["s: string"], doc: "SHA-256 hash" },
    BuiltinSig { name: "base64_encode", params: &["s: string"], doc: "Base64 encode" },
    BuiltinSig { name: "base64_decode", params: &["s: string"], doc: "Base64 decode" },
    BuiltinSig { name: "json_encode", params: &["value"], doc: "Encode as JSON string" },
    BuiltinSig { name: "to_string", params: &["value"], doc: "Convert to string" },
    BuiltinSig { name: "to_int", params: &["value"], doc: "Convert to int" },
    BuiltinSig { name: "to_float", params: &["value"], doc: "Convert to float" },
    BuiltinSig { name: "to_bool", params: &["value"], doc: "Convert to bool" },
    BuiltinSig { name: "type_of", params: &["value"], doc: "Get type name" },
    BuiltinSig { name: "has", params: &["value", "key: string"], doc: "Check if key exists" },
    BuiltinSig { name: "has_decorator", params: &["block", "name: string"], doc: "Check decorator" },
];

pub fn signature_help(
    source: &str,
    offset: usize,
    analysis: Option<&crate::state::AnalysisResult>,
) -> Option<SignatureHelp> {
    let before = &source[..offset.min(source.len())];
    let (fn_name, active_param) = find_call_context(before)?;

    // Try builtins first
    if let Some(sig) = BUILTIN_SIGS.iter().find(|s| s.name == fn_name) {
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

fn type_expr_label(te: &wcl_core::ast::TypeExpr) -> String {
    match te {
        wcl_core::ast::TypeExpr::String(_) => "string".to_string(),
        wcl_core::ast::TypeExpr::Int(_) => "int".to_string(),
        wcl_core::ast::TypeExpr::Float(_) => "float".to_string(),
        wcl_core::ast::TypeExpr::Bool(_) => "bool".to_string(),
        wcl_core::ast::TypeExpr::Any(_) => "any".to_string(),
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
        use crate::analysis::analyze;
        let source = "let x = upper(";
        let analysis = analyze(source, &wcl::ParseOptions::default());
        let offset = source.len();
        let help = signature_help(source, offset, Some(&analysis)).unwrap();
        assert!(help.signatures[0].label.contains("upper"));
    }

    #[test]
    fn test_signature_help_with_macro() {
        use crate::analysis::analyze;
        // Macros are collected into macro_registry during analysis
        let source = "macro greet(name, greeting) { msg = greeting }\nlet x = greet(";
        let analysis = analyze(source, &wcl::ParseOptions::default());
        let offset = source.len();
        let help = signature_help(source, offset, Some(&analysis));
        assert!(help.is_some(), "should find macro signature");
        let h = help.unwrap();
        assert!(h.signatures[0].label.contains("greet"));
        assert!(h.signatures[0].label.contains("name"));
        assert!(h.signatures[0].label.contains("greeting"));
    }
}
