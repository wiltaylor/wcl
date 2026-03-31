use std::path::Path;

use crate::cli::vars::parse_var_args;
use crate::cli::LibraryArgs;

pub fn run(
    file: &Path,
    format: &str,
    vars: &[String],
    lib_args: &LibraryArgs,
) -> Result<(), String> {
    let source = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read {}: {}", file.display(), e))?;

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

    let mut json_map = serde_json::Map::new();
    // Group block refs by kind (e.g., all "server" blocks under "server" key)
    for (key, val) in &doc.values {
        if let crate::Value::BlockRef(br) = val {
            let kind_entry = json_map
                .entry(&br.kind)
                .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
            if let serde_json::Value::Object(kind_map) = kind_entry {
                // Use inline ID, then first label, then the raw key
                let block_key = br.id.as_deref().unwrap_or(key);
                kind_map.insert(block_key.to_string(), blockref_to_json(br));
            }
        } else {
            json_map.insert(key.clone(), value_to_json(val));
        }
    }
    let json = serde_json::Value::Object(json_map);

    match format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&json).unwrap());
        }
        "yaml" | "yml" => {
            let yaml = serde_yaml_ng::to_string(&json).map_err(|e| format!("YAML error: {}", e))?;
            print!("{}", yaml);
        }
        "toml" => {
            let toml_val: toml::Value = serde_json::from_value(json)
                .map_err(|e| format!("TOML conversion error: {}", e))?;
            let toml_str =
                toml::to_string_pretty(&toml_val).map_err(|e| format!("TOML error: {}", e))?;
            print!("{}", toml_str);
        }
        _ => {
            return Err(format!(
                "unsupported format '{}', use json, yaml, or toml",
                format
            ))
        }
    }

    Ok(())
}

fn value_to_json(val: &crate::Value) -> serde_json::Value {
    match val {
        crate::Value::String(s) => serde_json::Value::String(s.clone()),
        crate::Value::Int(i) => serde_json::json!(i),
        crate::Value::Float(f) => serde_json::json!(f),
        crate::Value::Bool(b) => serde_json::Value::Bool(*b),
        crate::Value::Null => serde_json::Value::Null,
        crate::Value::Identifier(s) => serde_json::Value::String(s.clone()),
        crate::Value::Symbol(s) => serde_json::Value::String(s.clone()),
        crate::Value::List(items) => {
            serde_json::Value::Array(items.iter().map(value_to_json).collect())
        }
        crate::Value::Map(m) => {
            let obj: serde_json::Map<String, serde_json::Value> = m
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        crate::Value::BlockRef(br) => blockref_to_json(br),
        _ => serde_json::Value::String(format!("{}", val)),
    }
}

fn blockref_to_json(br: &crate::BlockRef) -> serde_json::Value {
    let mut obj = serde_json::Map::new();
    for (k, v) in &br.attributes {
        obj.insert(k.clone(), value_to_json(v));
    }
    for child in &br.children {
        let key = child.id.as_deref().unwrap_or(&child.kind);
        obj.insert(key.to_string(), blockref_to_json(child));
    }
    serde_json::Value::Object(obj)
}
