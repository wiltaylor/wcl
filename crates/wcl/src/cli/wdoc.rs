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
    let project = load_project(files, vars, lib_args)?;
    render_project(&project, output)?;
    println!("wdoc: built to {}", output.display());
    Ok(())
}

pub fn run_validate(
    files: &[PathBuf],
    vars: &[String],
    lib_args: &LibraryArgs,
) -> Result<(), String> {
    load_project(files, vars, lib_args)?;
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

pub(crate) struct LoadedWdocCliProject {
    pub document: wcl_lang::Document,
    pub registry: wcl_lang::transform::codec::custom::CustomCodecRegistry,
    pub doc_value: wcl_lang::Value,
    pub watch_paths: Vec<PathBuf>,
    pub source_dirs: Vec<PathBuf>,
}

pub(crate) fn load_project(
    files: &[PathBuf],
    vars: &[String],
    lib_args: &LibraryArgs,
) -> Result<LoadedWdocCliProject, String> {
    let project = wcl_lang::project::load_files(files, source_options(vars, lib_args)?)?;
    let registry = project.codecs;
    if !registry.contains(wcl_lang::wdoc::codec::HTML_CODEC) {
        let names = registry.names().join(", ");
        return Err(format!(
            "loaded WCL project does not define codec '{}'. Please add `import <wdoc.wcl>` to your files. Loaded codecs: {}",
            wcl_lang::wdoc::codec::HTML_CODEC,
            if names.is_empty() { "(none)" } else { &names }
        ));
    }
    let doc_value = find_wdoc_document_value(&project.document)?.clone();
    let source_dirs = source_dirs(files, &project.loaded_files);

    Ok(LoadedWdocCliProject {
        document: project.document,
        registry,
        doc_value,
        watch_paths: project.loaded_files,
        source_dirs,
    })
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

pub(crate) fn render_project(project: &LoadedWdocCliProject, output: &Path) -> Result<(), String> {
    let mut options = wcl_lang::transform::codec::CodecOptions::new();
    options.insert(
        "asset_dirs".to_string(),
        wcl_lang::Value::List(
            project
                .source_dirs
                .iter()
                .map(|path| wcl_lang::Value::String(path.display().to_string()))
                .collect(),
        ),
    );
    wcl_lang::wdoc::source::with_loaded_project_context(
        &project.document,
        &project.source_dirs,
        || {
            wcl_lang::transform::encode_value_with_custom_to_directory(
                &project.doc_value,
                wcl_lang::wdoc::codec::HTML_CODEC,
                output,
                &options,
                Some(&project.registry),
            )
        },
    )
    .map(|_| ())
    .map_err(|e| e.to_string())
}
