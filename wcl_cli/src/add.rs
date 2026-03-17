use std::path::Path;

pub fn run(file: &Path, block_spec: &str, _file_auto: bool) -> Result<(), String> {
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

    // Parse block_spec: "block_type block_id" e.g. "service svc-new"
    let parts: Vec<&str> = block_spec.splitn(2, ' ').collect();
    let (block_kind, block_id) = match parts.as_slice() {
        [kind, id] => (*kind, Some(*id)),
        [kind] => (*kind, None),
        _ => return Err(format!("invalid block spec: {}", block_spec)),
    };

    // TODO: Implement AST-level insertion of a new block.
    // This requires appending a new block node to the document AST
    // and re-serializing to source text preserving existing formatting.
    let _ = (&doc, &source);

    let id_display = block_id
        .map(|id| format!(" {}", id))
        .unwrap_or_default();
    Err(format!(
        "add command is not yet implemented (would add {}{} {{}} to {})",
        block_kind, id_display, file.display()
    ))
}
