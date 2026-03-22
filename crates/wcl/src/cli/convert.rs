use std::path::Path;

use crate::cli::LibraryArgs;

pub fn run(
    file: &Path,
    to: Option<&str>,
    from: Option<&str>,
    lib_args: &LibraryArgs,
) -> Result<(), String> {
    let source = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read {}: {}", file.display(), e))?;

    match to {
        Some("json") => {
            let mut options = crate::ParseOptions {
                root_dir: file.parent().unwrap_or(Path::new(".")).to_path_buf(),
                ..Default::default()
            };
            lib_args.apply(&mut options);
            let doc = crate::parse(&source, options);
            if doc.has_errors() {
                for diag in doc.errors() {
                    eprintln!("error: {}", diag.message);
                }
                return Err("document has errors".to_string());
            }
            let mut json_map = serde_json::Map::new();
            for (key, val) in &doc.values {
                json_map.insert(key.clone(), value_to_json(val));
            }
            let json = serde_json::Value::Object(json_map);
            println!("{}", serde_json::to_string_pretty(&json).unwrap());
            Ok(())
        }
        Some("yaml") | Some("yml") => {
            let mut options = crate::ParseOptions {
                root_dir: file.parent().unwrap_or(Path::new(".")).to_path_buf(),
                ..Default::default()
            };
            lib_args.apply(&mut options);
            let doc = crate::parse(&source, options);
            if doc.has_errors() {
                for diag in doc.errors() {
                    eprintln!("error: {}", diag.message);
                }
                return Err("document has errors".to_string());
            }
            let mut json_map = serde_json::Map::new();
            for (key, val) in &doc.values {
                json_map.insert(key.clone(), value_to_json(val));
            }
            let json = serde_json::Value::Object(json_map);
            let yaml = serde_yaml::to_string(&json).map_err(|e| format!("YAML error: {}", e))?;
            print!("{}", yaml);
            Ok(())
        }
        Some("toml") => {
            let mut options = crate::ParseOptions {
                root_dir: file.parent().unwrap_or(Path::new(".")).to_path_buf(),
                ..Default::default()
            };
            lib_args.apply(&mut options);
            let doc = crate::parse(&source, options);
            if doc.has_errors() {
                for diag in doc.errors() {
                    eprintln!("error: {}", diag.message);
                }
                return Err("document has errors".to_string());
            }
            let mut json_map = serde_json::Map::new();
            for (key, val) in &doc.values {
                json_map.insert(key.clone(), value_to_json(val));
            }
            let json = serde_json::Value::Object(json_map);
            let toml_val: toml::Value = serde_json::from_value(json.clone())
                .map_err(|e| format!("TOML conversion error: {}", e))?;
            let toml_str =
                toml::to_string_pretty(&toml_val).map_err(|e| format!("TOML error: {}", e))?;
            print!("{}", toml_str);
            Ok(())
        }
        Some(fmt) => Err(format!("output format '{}' not yet supported", fmt)),
        None => match from {
            Some("json") => {
                let json: serde_json::Value =
                    serde_json::from_str(&source).map_err(|e| format!("invalid JSON: {}", e))?;
                print_json_as_wcl(&json, 0);
                Ok(())
            }
            Some("yaml") | Some("yml") => {
                let yaml: serde_json::Value =
                    serde_yaml::from_str(&source).map_err(|e| format!("invalid YAML: {}", e))?;
                print_json_as_wcl(&yaml, 0);
                Ok(())
            }
            Some("toml") => {
                let toml_val: toml::Value = source
                    .parse::<toml::Value>()
                    .map_err(|e| format!("invalid TOML: {}", e))?;
                let json: serde_json::Value = serde_json::to_value(&toml_val)
                    .map_err(|e| format!("conversion error: {}", e))?;
                print_json_as_wcl(&json, 0);
                Ok(())
            }
            Some(fmt) => Err(format!("input format '{}' not yet supported", fmt)),
            None => Err("specify --to or --from".to_string()),
        },
    }
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
        _ => serde_json::Value::String(format!("{}", val)),
    }
}

fn print_json_as_wcl(json: &serde_json::Value, indent: usize) {
    let indent_str = "    ".repeat(indent);
    match json {
        serde_json::Value::Object(map) => {
            for (key, val) in map {
                match val {
                    serde_json::Value::Object(_) => {
                        println!("{}{} {{", indent_str, key);
                        print_json_as_wcl(val, indent + 1);
                        println!("{}}}", indent_str);
                    }
                    _ => {
                        print!("{}{} = ", indent_str, key);
                        print_json_value(val);
                        println!();
                    }
                }
            }
        }
        _ => {
            print_json_value(json);
            println!();
        }
    }
}

fn print_json_value(val: &serde_json::Value) {
    match val {
        serde_json::Value::String(s) => print!("\"{}\"", s),
        serde_json::Value::Number(n) => print!("{}", n),
        serde_json::Value::Bool(b) => print!("{}", b),
        serde_json::Value::Null => print!("null"),
        serde_json::Value::Array(items) => {
            print!("[");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    print!(", ");
                }
                print_json_value(item);
            }
            print!("]");
        }
        serde_json::Value::Object(_) => print!("{{...}}"),
    }
}
