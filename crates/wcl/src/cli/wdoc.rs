use std::path::{Path, PathBuf};

use crate::cli::vars::parse_var_args;
use crate::cli::LibraryArgs;

pub(crate) fn source_options(
    vars: &[String],
    lib_args: &LibraryArgs,
) -> Result<wcl_lang::ParseOptions, String> {
    let mut options = wcl_lang::ParseOptions {
        root_dir: std::env::current_dir().map_err(|e| format!("cannot read cwd: {e}"))?,
        variables: parse_var_args(vars)?,
        functions: wcl_lang::wdoc::source::wdoc_functions(),
        ..Default::default()
    };
    options.lib_paths.clone_from(&lib_args.lib_paths);
    options.no_default_lib_paths = lib_args.no_default_lib_paths;
    Ok(options)
}

pub fn run_build(
    files: &[PathBuf],
    output: &Path,
    vars: &[String],
    lib_args: &LibraryArgs,
) -> Result<(), String> {
    let document = load_project(files, vars, lib_args)?;
    render_project(files, &document, output)?;
    println!("wdoc: built to {}", output.display());
    Ok(())
}

pub fn run_validate(
    files: &[PathBuf],
    vars: &[String],
    lib_args: &LibraryArgs,
) -> Result<(), String> {
    let document = load_project(files, vars, lib_args)?;
    require_wdoc_html_codec(&document)?;
    find_wdoc_document_value(&document)?;
    println!("wdoc: valid");
    Ok(())
}

pub fn run_serve(
    files: &[PathBuf],
    port: u16,
    open: bool,
    vars: &[String],
    lib_args: &LibraryArgs,
) -> Result<(), String> {
    crate::cli::wdoc_serve::run_serve(files, port, open, vars, lib_args)
}

pub(crate) fn load_project(
    files: &[PathBuf],
    vars: &[String],
    lib_args: &LibraryArgs,
) -> Result<wcl_lang::Document, String> {
    wcl_lang::project::load_files(files, source_options(vars, lib_args)?)
}

fn require_wdoc_html_codec(
    document: &wcl_lang::Document,
) -> Result<wcl_lang::transform::codec::custom::CustomCodecRegistry, String> {
    let registry = document.codec_registry().map_err(|e| e.to_string())?;
    if !registry.contains("wdoc-html") {
        let names = registry.names().join(", ");
        return Err(format!(
            "loaded WCL project does not define codec '{}'. Please add `import <wdoc.wcl>` to your files. Loaded codecs: {}",
            "wdoc-html",
            if names.is_empty() { "(none)" } else { &names }
        ));
    }
    Ok(registry)
}

fn find_wdoc_document_value(document: &wcl_lang::Document) -> Result<&wcl_lang::Value, String> {
    let docs: Vec<&wcl_lang::Value> = document
        .values
        .values()
        .filter(
            |value| matches!(value, wcl_lang::Value::BlockRef(block) if block.kind == "wdoc::doc"),
        )
        .collect();
    match docs.as_slice() {
        [value] => Ok(*value),
        [] => Err("no top-level wdoc::doc block found in loaded WCL project".to_string()),
        _ => Err("multiple top-level wdoc::doc blocks found in loaded WCL project".to_string()),
    }
}

fn source_dirs(files: &[PathBuf], loaded_files: &[PathBuf]) -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = files
        .iter()
        .chain(loaded_files.iter())
        .filter_map(|path| path.parent().map(PathBuf::from))
        .collect();
    dirs.sort();
    dirs.dedup();
    dirs
}

pub(crate) fn render_project(
    files: &[PathBuf],
    document: &wcl_lang::Document,
    output: &Path,
) -> Result<(), String> {
    let registry = require_wdoc_html_codec(document)?;
    let doc_value = find_wdoc_document_value(document)?.clone();
    let imported_files = document.imported_file_paths();
    let source_dirs = source_dirs(files, &imported_files);
    let file_base_dir = files
        .first()
        .and_then(|path| path.parent())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let options = wcl_lang::transform::codec::CodecOptions::new();
    wcl_lang::wdoc::source::with_loaded_project_context(document, &source_dirs, || {
        wcl_lang::transform::encode_value_with_custom_to_directory_with_file_access(
            &doc_value,
            "wdoc-html",
            output,
            &options,
            Some(&registry),
            file_base_dir,
        )
    })
    .map(|_| ())
    .map_err(|e| e.to_string())
}
