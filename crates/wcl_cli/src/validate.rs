use std::path::Path;

pub fn run(file: &Path, strict: bool, schema: Option<&Path>) -> Result<(), String> {
    let source = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read {}: {}", file.display(), e))?;

    let options = wcl::ParseOptions {
        root_dir: file.parent().unwrap_or(Path::new(".")).to_path_buf(),
        ..Default::default()
    };

    let mut doc = wcl::parse(&source, options);

    // If an external schema file was provided, parse it and validate against it
    if let Some(schema_path) = schema {
        let schema_source = std::fs::read_to_string(schema_path)
            .map_err(|e| format!("cannot read schema {}: {}", schema_path.display(), e))?;
        let schema_file_id = wcl_core::FileId(1000);
        let (schema_doc, schema_parse_diags) = wcl_core::parse(&schema_source, schema_file_id);

        // Add any parse errors from the schema file
        doc.diagnostics
            .extend(schema_parse_diags.into_diagnostics());

        // Collect schemas from the external file and validate the main document
        let mut external_schemas = wcl::SchemaRegistry::new();
        let mut diag_bag = wcl::DiagnosticBag::new();
        external_schemas.collect(&schema_doc, &mut diag_bag);
        external_schemas.validate(&doc.ast, &doc.values, &mut diag_bag);
        doc.diagnostics.extend(diag_bag.into_diagnostics());
    }

    let errors: Vec<_> = doc
        .diagnostics
        .iter()
        .filter(|d| d.is_error() || (strict && matches!(d.severity, wcl::Severity::Warning)))
        .collect();

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
