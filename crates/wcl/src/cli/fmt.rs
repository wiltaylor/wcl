use std::path::Path;

pub fn run(file: &Path, write: bool, check: bool) -> Result<(), String> {
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

    let formatted = crate::fmt::format_document(&doc);

    if check {
        if formatted == source {
            Ok(())
        } else {
            Err(format!("{} is not formatted", file.display()))
        }
    } else if write {
        std::fs::write(file, &formatted)
            .map_err(|e| format!("cannot write {}: {}", file.display(), e))?;
        println!("formatted {}", file.display());
        Ok(())
    } else {
        print!("{}", formatted);
        Ok(())
    }
}
