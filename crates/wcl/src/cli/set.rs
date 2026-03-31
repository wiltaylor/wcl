use std::path::Path;

use crate::cli::path::{self, Resolved};

pub fn run(file: &Path, path_str: &str, value: &str) -> Result<(), String> {
    let source = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read {}: {}", file.display(), e))?;

    let mut source_map = crate::lang::span::SourceMap::new();
    let file_id = source_map.add_file(file.display().to_string(), source.clone());
    let (doc, diags) = crate::lang::parse(&source, file_id);
    if diags.has_errors() {
        for d in diags.diagnostics() {
            if d.is_error() {
                eprintln!("{}", super::format_diagnostic(d, &source_map, file));
            }
        }
        return Err(format!("parse errors in {}", file.display()));
    }

    let segments = path::parse_path(path_str)?;
    let resolved = path::resolve(&doc, &segments)?;

    match resolved {
        Resolved::Attribute { attr } => {
            // Replace just the value expression in the source text.
            // The value span covers from after `=` to end of expression.
            let val_span = attr.value.span();
            let mut result = String::with_capacity(source.len());
            result.push_str(&source[..val_span.start]);
            result.push_str(value);
            result.push_str(&source[val_span.end..]);

            std::fs::write(file, &result)
                .map_err(|e| format!("cannot write {}: {}", file.display(), e))?;
            println!("set {} = {} in {}", path_str, value, file.display());
            Ok(())
        }
        Resolved::Block { .. } => {
            Err(format!(
                "path '{}' resolves to a block, not an attribute; use a dotted path to target an attribute (e.g. {}.attribute_name)",
                path_str, path_str
            ))
        }
    }
}
