use std::path::Path;

use crate::cli::vars::parse_var_args;
use crate::cli::LibraryArgs;

pub fn run(
    file: &Path,
    strict: bool,
    schema: Option<&Path>,
    vars: &[String],
    lib_args: &LibraryArgs,
) -> Result<(), String> {
    let source = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read {}: {}", file.display(), e))?;

    let variables = parse_var_args(vars)?;

    let mut options = crate::ParseOptions {
        root_dir: file.parent().unwrap_or(Path::new(".")).to_path_buf(),
        variables,
        ..Default::default()
    };
    lib_args.apply(&mut options);

    let mut doc = crate::parse(&source, options);

    // If an external schema file was provided, parse it and validate against it
    if let Some(schema_path) = schema {
        let schema_source = std::fs::read_to_string(schema_path)
            .map_err(|e| format!("cannot read schema {}: {}", schema_path.display(), e))?;
        let schema_file_id = crate::lang::FileId(1000);
        let (schema_doc, schema_parse_diags) = crate::lang::parse(&schema_source, schema_file_id);

        // Add any parse errors from the schema file
        doc.diagnostics
            .extend(schema_parse_diags.into_diagnostics());

        // Collect schemas from the external file and validate the main document
        let mut external_schemas = crate::SchemaRegistry::new();
        let mut diag_bag = crate::DiagnosticBag::new();
        external_schemas.collect(&schema_doc, &mut diag_bag);
        external_schemas.validate(&doc.ast, &doc.values, &doc.symbol_sets, &mut diag_bag);
        doc.diagnostics.extend(diag_bag.into_diagnostics());
    }

    let errors: Vec<_> = doc
        .diagnostics
        .iter()
        .filter(|d| d.is_error() || (strict && matches!(d.severity, crate::Severity::Warning)))
        .collect();

    if errors.is_empty() {
        println!("{} is valid", file.display());
        Ok(())
    } else {
        for diag in &doc.diagnostics {
            let prefix = match diag.severity {
                crate::Severity::Error => "error",
                crate::Severity::Warning => "warning",
                crate::Severity::Info => "info",
                crate::Severity::Hint => "hint",
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
