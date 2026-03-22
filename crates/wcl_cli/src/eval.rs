use std::path::Path;

use crate::vars::parse_var_args;

pub fn run(file: &Path, format: &str, vars: &[String]) -> Result<(), String> {
    let source = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read {}: {}", file.display(), e))?;

    let variables = parse_var_args(vars)?;

    let options = wcl::ParseOptions {
        root_dir: file.parent().unwrap_or(Path::new(".")).to_path_buf(),
        variables,
        ..Default::default()
    };

    let doc = wcl::parse(&source, options);

    if doc.has_errors() {
        for diag in doc.errors() {
            eprintln!("error: {}", diag.message);
        }
        return Err("document has errors".to_string());
    }

    let mut json_map = serde_json::Map::new();
    // Group block refs by kind (e.g., all "server" blocks under "server" key)
    for (key, val) in &doc.values {
        if let wcl::Value::BlockRef(br) = val {
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
            let yaml = serde_yaml::to_string(&json).map_err(|e| format!("YAML error: {}", e))?;
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

fn value_to_json(val: &wcl::Value) -> serde_json::Value {
    match val {
        wcl::Value::String(s) => serde_json::Value::String(s.clone()),
        wcl::Value::Int(i) => serde_json::json!(i),
        wcl::Value::Float(f) => serde_json::json!(f),
        wcl::Value::Bool(b) => serde_json::Value::Bool(*b),
        wcl::Value::Null => serde_json::Value::Null,
        wcl::Value::Identifier(s) => serde_json::Value::String(s.clone()),
        wcl::Value::Symbol(s) => serde_json::Value::String(s.clone()),
        wcl::Value::List(items) => {
            serde_json::Value::Array(items.iter().map(value_to_json).collect())
        }
        wcl::Value::Map(m) => {
            let obj: serde_json::Map<String, serde_json::Value> = m
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        wcl::Value::BlockRef(br) => blockref_to_json(br),
        _ => serde_json::Value::String(format!("{}", val)),
    }
}

fn blockref_to_json(br: &wcl::BlockRef) -> serde_json::Value {
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
