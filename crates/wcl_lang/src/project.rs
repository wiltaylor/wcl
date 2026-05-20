//! Generic WCL project loading helpers.

use std::path::{Path, PathBuf};

pub fn load_files(
    files: &[PathBuf],
    options: crate::ParseOptions,
) -> Result<crate::Document, String> {
    if files.is_empty() {
        return Err("no input files".to_string());
    }

    let root_dir = options.root_dir.clone();
    let mut source = String::new();
    for file in files {
        let import_path = project_import_path(file, &root_dir);
        source.push_str("import \"");
        source.push_str(&escape_wcl_string(&import_path));
        source.push_str("\"\n");
    }

    let document = crate::parse(&source, options.clone());
    let errors: Vec<_> = document
        .diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.is_error())
        .collect();
    if !errors.is_empty() {
        let mut msg = String::new();
        for diagnostic in errors {
            msg.push_str(&format_diagnostic(
                diagnostic,
                &document.source_map,
                Path::new("<wcl-project>"),
            ));
            msg.push('\n');
        }
        return Err(msg);
    }

    Ok(document)
}

fn project_import_path(file: &Path, root_dir: &Path) -> String {
    let path = if file.is_absolute() {
        file.strip_prefix(root_dir).unwrap_or(file)
    } else {
        file
    };
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use crate::{InMemoryFs, ParseOptions};

    #[test]
    fn load_files_returns_document_for_root_files() {
        let mut fs = InMemoryFs::new();
        fs.add_file(PathBuf::from("/project/main.wcl"), "answer = 42");
        let doc = super::load_files(
            &[PathBuf::from("/project/main.wcl")],
            ParseOptions {
                root_dir: PathBuf::from("/project"),
                fs: Some(Arc::new(fs)),
                ..ParseOptions::default()
            },
        )
        .expect("project should load");

        assert!(!doc.has_errors(), "errors: {:?}", doc.errors());
        assert!(doc.values.contains_key("answer"));
        assert!(doc
            .imported_paths
            .contains(&PathBuf::from("/project/main.wcl")));
    }
}

fn escape_wcl_string(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn format_diagnostic(
    diagnostic: &crate::Diagnostic,
    source_map: &crate::SourceMap,
    fallback_path: &Path,
) -> String {
    let code = diagnostic
        .code
        .as_deref()
        .map(|code| format!("[{code}]"))
        .unwrap_or_default();
    let source_file = source_map.get_file(diagnostic.span.file);
    let path = if source_file.path.is_empty() || source_file.path == "<input>" {
        fallback_path.display().to_string()
    } else {
        source_file.path.clone()
    };
    let (line, col) = source_file.line_col(diagnostic.span.start);
    format!(
        "{:?}{code}: {}\n  --> {path}:{line}:{col}",
        diagnostic.severity, diagnostic.message
    )
}
