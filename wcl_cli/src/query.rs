use std::path::Path;

pub fn run(
    file: &Path,
    query_str: &str,
    format: &str,
    count: bool,
    _recursive: bool,
) -> Result<(), String> {
    let source = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read {}: {}", file.display(), e))?;

    let options = wcl::ParseOptions {
        root_dir: file.parent().unwrap_or(Path::new(".")).to_path_buf(),
        ..Default::default()
    };

    let doc = wcl::parse(&source, options);
    if doc.has_errors() {
        for diag in doc.errors() {
            eprintln!("error: {}", diag.message);
        }
        return Err("document has errors".to_string());
    }

    let result = doc.query(query_str)?;

    if count {
        match &result {
            wcl::Value::List(items) => println!("{}", items.len()),
            _ => println!("1"),
        }
    } else {
        match format {
            "json" => {
                let json = value_to_json(&result);
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json).unwrap_or_default()
                );
            }
            _ => {
                println!("{}", result);
            }
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
        wcl::Value::List(items) => {
            serde_json::Value::Array(items.iter().map(value_to_json).collect())
        }
        wcl::Value::Map(m) => {
            let obj: serde_json::Map<String, serde_json::Value> =
                m.iter().map(|(k, v)| (k.clone(), value_to_json(v))).collect();
            serde_json::Value::Object(obj)
        }
        _ => serde_json::Value::String(format!("{}", val)),
    }
}
