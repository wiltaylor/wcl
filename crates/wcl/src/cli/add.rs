use std::path::Path;

pub fn run(file: &Path, block_spec: &str, _file_auto: bool) -> Result<(), String> {
    let source = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read {}: {}", file.display(), e))?;

    // Validate the existing document parses cleanly
    let mut source_map = crate::lang::span::SourceMap::new();
    let file_id = source_map.add_file(file.display().to_string(), source.clone());
    let (_, diags) = crate::lang::parse(&source, file_id);
    if diags.has_errors() {
        for d in diags.diagnostics() {
            if d.is_error() {
                eprintln!("{}", super::format_diagnostic(d, &source_map, file));
            }
        }
        return Err(format!("parse errors in {}", file.display()));
    }

    // Parse block_spec: "block_type block_id" or "block_type" e.g. "service svc-new"
    let parts: Vec<&str> = block_spec.splitn(2, ' ').collect();
    let (block_kind, block_id) = match parts.as_slice() {
        [kind, id] => (*kind, Some(*id)),
        [kind] => (*kind, None),
        _ => return Err(format!("invalid block spec: {}", block_spec)),
    };

    // Build the new block text
    let block_text = match block_id {
        Some(id) => format!("{} {} {{\n}}\n", block_kind, id),
        None => format!("{} {{\n}}\n", block_kind),
    };

    // Append to the file, ensuring a blank line separator
    let mut result = source.clone();
    if !result.ends_with('\n') {
        result.push('\n');
    }
    if !result.ends_with("\n\n") {
        result.push('\n');
    }
    result.push_str(&block_text);

    // Validate the result still parses
    let (_, new_diags) = crate::lang::parse(&result, file_id);
    if new_diags.has_errors() {
        return Err(format!(
            "generated block does not parse cleanly; check block spec: {}",
            block_spec
        ));
    }

    std::fs::write(file, &result).map_err(|e| format!("cannot write {}: {}", file.display(), e))?;

    let id_display = block_id.map(|id| format!(" {}", id)).unwrap_or_default();
    println!("added {}{} to {}", block_kind, id_display, file.display());
    Ok(())
}
