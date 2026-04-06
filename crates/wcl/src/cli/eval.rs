use std::path::Path;

use crate::cli::vars::parse_var_args;
use crate::cli::LibraryArgs;
use wcl_lang::fmt_value::{blockref_to_json, document_values_to_wcl, value_to_json, value_to_wcl};

pub fn run(
    file: &Path,
    expression: Option<&str>,
    format: &str,
    vars: &[String],
    lib_args: &LibraryArgs,
) -> Result<(), String> {
    let file_source = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read {}: {}", file.display(), e))?;

    // When an expression is supplied, append it as a synthetic top-level
    // binding so the full document scope (let bindings, imports, functions)
    // is available during expression evaluation. We then pluck that binding
    // out of the evaluated document.
    const EXPR_BINDING: &str = "__wcl_eval_expr__";
    let source = if let Some(expr) = expression {
        format!("{}\n{} = ({})\n", file_source, EXPR_BINDING, expr)
    } else {
        file_source
    };

    let variables = parse_var_args(vars)?;

    let mut options = crate::ParseOptions {
        root_dir: file.parent().unwrap_or(Path::new(".")).to_path_buf(),
        variables,
        ..Default::default()
    };
    lib_args.apply(&mut options);

    let doc = crate::parse(&source, options);

    if doc.has_errors() {
        for diag in doc.errors() {
            eprintln!("{}", super::format_diagnostic(diag, &doc.source_map, file));
        }
        return Err("document has errors".to_string());
    }

    if format != "wcl" && format != "json" {
        return Err(format!("unsupported format '{}', use wcl or json", format));
    }

    if expression.is_some() {
        let value = doc
            .values
            .get(EXPR_BINDING)
            .ok_or_else(|| "expression did not produce a value".to_string())?;
        match format {
            "json" => {
                let json = value_to_json(value);
                println!("{}", serde_json::to_string_pretty(&json).unwrap());
            }
            _ => {
                print!("{}", value_to_wcl(value));
            }
        }
        return Ok(());
    }

    // No expression — print the whole document.
    match format {
        "json" => {
            let mut json_map = serde_json::Map::new();
            for (key, val) in &doc.values {
                if let crate::Value::BlockRef(br) = val {
                    let kind_entry = json_map
                        .entry(&br.kind)
                        .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
                    if let serde_json::Value::Object(kind_map) = kind_entry {
                        let block_key = br.id.as_deref().unwrap_or(key);
                        kind_map.insert(block_key.to_string(), blockref_to_json(br));
                    }
                } else {
                    json_map.insert(key.clone(), value_to_json(val));
                }
            }
            let json = serde_json::Value::Object(json_map);
            println!("{}", serde_json::to_string_pretty(&json).unwrap());
        }
        _ => {
            print!("{}", document_values_to_wcl(&doc.values));
        }
    }

    Ok(())
}
