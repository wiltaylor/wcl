use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::cli::vars::parse_var_args;
use crate::cli::LibraryArgs;
use indexmap::IndexMap;

struct WdocProfiler {
    enabled: bool,
    label: &'static str,
    started: Instant,
    last: Instant,
    entries: Vec<(&'static str, Duration)>,
}

impl WdocProfiler {
    fn from_env(label: &'static str) -> Self {
        let now = Instant::now();
        Self {
            enabled: std::env::var_os("WCL_PROFILE").is_some(),
            label,
            started: now,
            last: now,
            entries: Vec::new(),
        }
    }

    fn checkpoint(&mut self, name: &'static str) {
        if !self.enabled {
            return;
        }
        let now = Instant::now();
        self.entries.push((name, now.duration_since(self.last)));
        self.last = now;
    }

    fn finish(&self) {
        if !self.enabled {
            return;
        }
        eprintln!("WCL_PROFILE {label}", label = self.label);
        for (name, duration) in &self.entries {
            eprintln!("  {name:<36} {duration:>10.3?}");
        }
        eprintln!(
            "  {name:<36} {duration:>10.3?}",
            name = "total",
            duration = self.started.elapsed()
        );
    }
}

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
    let mut profiler = WdocProfiler::from_env("wdoc load_project");
    let options = source_options(vars, lib_args)?;
    profiler.checkpoint("build parse options");
    let document = wcl_lang::project::load_files(files, options)?;
    profiler.checkpoint("load files");
    profiler.finish();
    Ok(document)
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
    let mut profiler = WdocProfiler::from_env("wdoc render_project");
    let registry = require_wdoc_html_codec(document)?;
    profiler.checkpoint("codec registry");
    let root = find_wdoc_document_value(document)?.clone();
    let imported_files = document.imported_file_paths();
    let source_dirs = source_dirs(files, &imported_files);
    let file_base_dir = files
        .first()
        .and_then(|path| path.parent())
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    let doc_value = wdoc_project_value(root, document, &source_dirs);
    profiler.checkpoint("prepare project value");
    let options = wcl_lang::transform::codec::CodecOptions::new();
    let codec = registry
        .get("wdoc-html")
        .ok_or_else(|| "loaded WCL project does not define codec 'wdoc-html'".to_string())?
        .clone();
    wcl_lang::transform::codec::custom::encode_custom_value_with_registry_and_builtins_and_file_access(
        &doc_value,
        &codec,
        &options,
        wcl_lang::transform::codec::native::OutputTarget::Directory(output),
        Arc::new(registry),
        wcl_lang::wdoc::source::wdoc_functions().functions,
        file_base_dir,
    )
    .map_err(|e| e.to_string())?;
    profiler.checkpoint("encode wdoc-html");
    profiler.finish();
    Ok(())
}

fn wdoc_project_value(
    root: wcl_lang::Value,
    document: &wcl_lang::Document,
    source_dirs: &[PathBuf],
) -> wcl_lang::Value {
    let mut map = IndexMap::new();
    map.insert("root".to_string(), root);
    map.insert(
        "values".to_string(),
        wcl_lang::Value::Map(wdoc_project_values(document)),
    );
    map.insert("metadata".to_string(), document.schema_metadata_value());
    map.insert(
        "source_dirs".to_string(),
        wcl_lang::Value::List(
            source_dirs
                .iter()
                .map(|path| wcl_lang::Value::String(path.display().to_string()))
                .collect(),
        ),
    );
    wcl_lang::Value::Map(map)
}

fn wdoc_project_values(document: &wcl_lang::Document) -> IndexMap<String, wcl_lang::Value> {
    document
        .values
        .iter()
        .filter_map(|(name, value)| match value {
            wcl_lang::Value::BlockRef(_) | wcl_lang::Value::Function(_) => {
                Some((name.clone(), value.clone()))
            }
            _ => None,
        })
        .collect()
}
