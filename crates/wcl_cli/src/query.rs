use std::path::{Path, PathBuf};

pub fn run(
    file: &Path,
    query_str: &str,
    format: &str,
    count: bool,
    recursive: bool,
) -> Result<(), String> {
    if recursive && file.is_dir() {
        let files = collect_wcl_files(file)?;
        if files.is_empty() {
            return Err(format!("no .wcl files found in {}", file.display()));
        }

        let mut all_results: Vec<wcl::Value> = Vec::new();

        for wcl_file in &files {
            let source = std::fs::read_to_string(wcl_file)
                .map_err(|e| format!("cannot read {}: {}", wcl_file.display(), e))?;

            let options = wcl::ParseOptions {
                root_dir: wcl_file.parent().unwrap_or(Path::new(".")).to_path_buf(),
                ..Default::default()
            };

            let doc = wcl::parse(&source, options);
            if doc.has_errors() {
                for diag in doc.errors() {
                    eprintln!("error [{}]: {}", wcl_file.display(), diag.message);
                }
                continue;
            }

            match doc.query(query_str) {
                Ok(wcl::Value::List(items)) => {
                    all_results.extend(items);
                }
                Ok(val) => {
                    all_results.push(val);
                }
                Err(e) => {
                    eprintln!("query error [{}]: {}", wcl_file.display(), e);
                }
            }
        }

        let aggregated = wcl::Value::List(all_results);

        if count {
            match &aggregated {
                wcl::Value::List(items) => println!("{}", items.len()),
                _ => println!("1"),
            }
        } else {
            match format {
                "json" => {
                    let json = value_to_json(&aggregated);
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&json).unwrap_or_default()
                    );
                }
                "csv" => {
                    print_csv(&aggregated);
                }
                "wcl" => {
                    print_wcl(&aggregated, 0);
                }
                _ => {
                    // "text" or any other format
                    println!("{}", aggregated);
                }
            }
        }

        return Ok(());
    }

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
            "csv" => {
                print_csv(&result);
            }
            "wcl" => {
                print_wcl(&result, 0);
            }
            _ => {
                // "text" or any other format
                println!("{}", result);
            }
        }
    }

    Ok(())
}

fn collect_wcl_files(dir: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    collect_wcl_files_recursive(dir, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_wcl_files_recursive(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = std::fs::read_dir(dir)
        .map_err(|e| format!("cannot read directory {}: {}", dir.display(), e))?;
    for entry in entries {
        let entry = entry.map_err(|e| format!("directory entry error: {}", e))?;
        let path = entry.path();
        if path.is_dir() {
            collect_wcl_files_recursive(&path, files)?;
        } else if path.extension().is_some_and(|ext| ext == "wcl") {
            files.push(path);
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

fn print_csv(val: &wcl::Value) {
    match val {
        wcl::Value::List(items) => {
            // Collect all keys from map/block items for headers
            let mut headers: Vec<String> = Vec::new();
            let rows: Vec<_> = items
                .iter()
                .filter_map(|item| match item {
                    wcl::Value::Map(m) => Some(m),
                    wcl::Value::BlockRef(br) => Some(&br.attributes),
                    _ => None,
                })
                .collect();

            // Gather unique headers preserving order
            for row in &rows {
                for key in row.keys() {
                    if !headers.contains(key) {
                        headers.push(key.clone());
                    }
                }
            }

            if headers.is_empty() {
                // Simple list of scalars, one per line
                for item in items {
                    println!("{}", item);
                }
            } else {
                // Print header row
                println!("{}", headers.join(","));
                // Print data rows
                for row in &rows {
                    let cells: Vec<String> = headers
                        .iter()
                        .map(|h| {
                            row.get(h)
                                .map(|v| csv_escape(&format!("{}", v)))
                                .unwrap_or_default()
                        })
                        .collect();
                    println!("{}", cells.join(","));
                }
            }
        }
        _ => println!("{}", val),
    }
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn print_wcl(val: &wcl::Value, indent: usize) {
    let pad = "    ".repeat(indent);
    match val {
        wcl::Value::List(items) => {
            for item in items {
                print_wcl(item, indent);
            }
        }
        wcl::Value::BlockRef(br) => {
            print!("{}{}", pad, br.kind);
            if let Some(id) = &br.id {
                print!(" {}", id);
            }
            println!(" {{");
            for (key, value) in &br.attributes {
                print!("{}    {} = ", pad, key);
                print_wcl_value(value);
                println!();
            }
            for child in &br.children {
                print_wcl(&wcl::Value::BlockRef(child.clone()), indent + 1);
            }
            println!("{}}}", pad);
        }
        wcl::Value::Map(m) => {
            for (key, value) in m {
                print!("{}{} = ", pad, key);
                print_wcl_value(value);
                println!();
            }
        }
        other => {
            print_wcl_value(other);
            println!();
        }
    }
}

fn print_wcl_value(val: &wcl::Value) {
    match val {
        wcl::Value::String(s) => print!("\"{}\"", s),
        wcl::Value::Int(n) => print!("{}", n),
        wcl::Value::Float(f) => print!("{}", f),
        wcl::Value::Bool(b) => print!("{}", b),
        wcl::Value::Null => print!("null"),
        wcl::Value::List(items) => {
            print!("[");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    print!(", ");
                }
                print_wcl_value(item);
            }
            print!("]");
        }
        other => print!("{}", other),
    }
}
