use std::path::Path;

pub fn run(file: &Path, strict: bool, schema: Option<&Path>) -> Result<(), String> {
    let source = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read {}: {}", file.display(), e))?;

    let mut options = wcl::ParseOptions::default();
    options.root_dir = file.parent().unwrap_or(Path::new(".")).to_path_buf();

    let doc = wcl::parse(&source, options);

    // Log the schema path if provided (not yet implemented)
    if let Some(schema_path) = schema {
        eprintln!("note: schema validation against {} is not yet implemented", schema_path.display());
    }

    let errors: Vec<_> = doc.diagnostics.iter().filter(|d| {
        d.is_error() || (strict && matches!(d.severity, wcl::Severity::Warning))
    }).collect();

    if errors.is_empty() {
        println!("{} is valid", file.display());
        Ok(())
    } else {
        for diag in &doc.diagnostics {
            let prefix = match diag.severity {
                wcl::Severity::Error => "error",
                wcl::Severity::Warning => "warning",
                wcl::Severity::Info => "info",
                wcl::Severity::Hint => "hint",
            };
            let code = diag.code.as_deref().unwrap_or("");
            if code.is_empty() {
                eprintln!("{}: {}", prefix, diag.message);
            } else {
                eprintln!("{}[{}]: {}", prefix, code, diag.message);
            }
        }
        Err(format!("{} error(s) found", errors.len()))
    }
}
