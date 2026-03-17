use std::path::Path;

pub fn run(file: &Path, path: &str, value: &str) -> Result<(), String> {
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

    // Parse the path: "block_type#id.attribute" or "attribute"
    let _segments = parse_path(path)?;

    // TODO: Implement AST-level mutation to set the value at the given path.
    // This requires:
    //   1. Locating the target node in the AST by path
    //   2. Replacing the expression with the new value
    //   3. Re-serializing the AST back to source text preserving formatting
    //
    // For now, report that this feature is not yet implemented.
    let _ = (value, &doc, &source);
    Err(format!(
        "set command is not yet implemented (would set {} = {} in {})",
        path,
        value,
        file.display()
    ))
}

/// Parse a WCL path like "service#svc-api.port" into segments.
fn parse_path(path: &str) -> Result<Vec<PathSegment>, String> {
    let mut segments = Vec::new();

    // Split on '.' for nested access
    for part in path.split('.') {
        if part.is_empty() {
            return Err("empty segment in path".to_string());
        }
        if let Some(hash_pos) = part.find('#') {
            let block_type = &part[..hash_pos];
            let block_id = &part[hash_pos + 1..];
            if block_type.is_empty() || block_id.is_empty() {
                return Err(format!("invalid block reference: {}", part));
            }
            segments.push(PathSegment::Block {
                kind: block_type.to_string(),
                id: block_id.to_string(),
            });
        } else {
            segments.push(PathSegment::Attribute(part.to_string()));
        }
    }

    if segments.is_empty() {
        return Err("empty path".to_string());
    }
    Ok(segments)
}

#[derive(Debug)]
#[allow(dead_code)]
enum PathSegment {
    Block { kind: String, id: String },
    Attribute(String),
}
