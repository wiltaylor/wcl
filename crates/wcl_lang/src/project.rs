//! Generic WCL project loading helpers.

use std::path::{Path, PathBuf};

use crate::transform::codec::custom::{self, CustomCodecRegistry};

/// A WCL project loaded from one or more root files.
pub struct LoadedProject {
    pub document: crate::Document,
    pub loaded_files: Vec<PathBuf>,
    pub codecs: CustomCodecRegistry,
}

pub fn load_files(
    files: &[PathBuf],
    options: crate::ParseOptions,
) -> Result<LoadedProject, String> {
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

    let mut loaded_files: Vec<PathBuf> = document
        .imported_paths
        .iter()
        .filter(|path| !path.starts_with(crate::eval::imports::EMBEDDED_LIBRARY_ROOT))
        .cloned()
        .collect();
    loaded_files.sort();
    loaded_files.dedup();

    let codecs = custom::registry_from_document(&document, true).map_err(|e| e.to_string())?;

    Ok(LoadedProject {
        document,
        loaded_files,
        codecs,
    })
}

pub fn codec_names(project: &LoadedProject) -> Vec<String> {
    project.codecs.names()
}

fn project_import_path(file: &Path, root_dir: &Path) -> String {
    let path = if file.is_absolute() {
        file.strip_prefix(root_dir).unwrap_or(file)
    } else {
        file
    };
    path.to_string_lossy().replace('\\', "/")
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
