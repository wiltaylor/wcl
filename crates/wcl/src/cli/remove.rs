use std::path::Path;

use crate::cli::path::{self, Resolved};

pub fn run(file: &Path, path_str: &str) -> Result<(), String> {
    let source = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read {}: {}", file.display(), e))?;

    let file_id = crate::lang::FileId(0);
    let (doc, diags) = crate::lang::parse(&source, file_id);
    if diags.has_errors() {
        for d in diags.diagnostics() {
            if d.is_error() {
                eprintln!("error: {}", d.message);
            }
        }
        return Err(format!("parse errors in {}", file.display()));
    }

    let segments = path::parse_path(path_str)?;
    let resolved = path::resolve(&doc, &segments)?;

    let (remove_start, remove_end) = match resolved {
        Resolved::Block { block } => path::block_full_span(&source, block),
        Resolved::Attribute { attr } => path::attr_full_span(&source, attr),
    };

    let mut result = String::with_capacity(source.len());
    result.push_str(&source[..remove_start]);
    result.push_str(&source[remove_end..]);

    std::fs::write(file, &result).map_err(|e| format!("cannot write {}: {}", file.display(), e))?;
    println!("removed {} from {}", path_str, file.display());
    Ok(())
}
