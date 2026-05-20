use std::hint::black_box;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion};
use indexmap::IndexMap;
use tempfile::tempdir;
use wcl_lang::transform::codec::custom::{
    encode_custom_value_with_registry_and_builtins_and_file_access, CustomCodecRegistry,
};
use wcl_lang::transform::codec::native::OutputTarget;
use wcl_lang::transform::codec::CodecOptions;
use wcl_lang::{Document, ParseOptions, Value};

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("crate should live under workspace/crates/wcl")
        .to_path_buf()
}

fn example_site() -> PathBuf {
    workspace_root().join("examples/wdoc/site.wcl")
}

fn wdoc_options() -> ParseOptions {
    ParseOptions {
        root_dir: workspace_root(),
        functions: wcl_lang::wdoc::source::wdoc_functions(),
        ..ParseOptions::default()
    }
}

fn load_example_project() -> Document {
    wcl_lang::project::load_files(&[example_site()], wdoc_options())
        .expect("example wdoc project should load")
}

fn registry(document: &Document) -> CustomCodecRegistry {
    document
        .codec_registry()
        .expect("codec registry should build from example wdoc project")
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

fn find_wdoc_document_value(document: &Document) -> Value {
    document
        .values
        .values()
        .find(|value| matches!(value, Value::BlockRef(block) if block.kind == "wdoc::doc"))
        .expect("example wdoc project should have a wdoc::doc block")
        .clone()
}

fn wdoc_project_values(document: &Document) -> IndexMap<String, Value> {
    document
        .values
        .iter()
        .filter_map(|(name, value)| match value {
            Value::BlockRef(_) | Value::Function(_) => Some((name.clone(), value.clone())),
            _ => None,
        })
        .collect()
}

fn wdoc_project_value(document: &Document, source_dirs: &[PathBuf]) -> Value {
    let mut map = IndexMap::new();
    map.insert("root".to_string(), find_wdoc_document_value(document));
    map.insert(
        "values".to_string(),
        Value::Map(wdoc_project_values(document)),
    );
    map.insert("metadata".to_string(), document.schema_metadata_value());
    map.insert(
        "source_dirs".to_string(),
        Value::List(
            source_dirs
                .iter()
                .map(|path| Value::String(path.display().to_string()))
                .collect(),
        ),
    );
    Value::Map(map)
}

fn render_example_project(document: &Document, registry: CustomCodecRegistry, output: &Path) {
    let files = [example_site()];
    let imported_files = document.imported_file_paths();
    let source_dirs = source_dirs(&files, &imported_files);
    let doc_value = wdoc_project_value(document, &source_dirs);
    let codec = registry
        .get("wdoc-html")
        .expect("example wdoc project should define wdoc-html codec")
        .clone();
    encode_custom_value_with_registry_and_builtins_and_file_access(
        &doc_value,
        &codec,
        &CodecOptions::new(),
        OutputTarget::Directory(output),
        Arc::new(registry),
        wcl_lang::wdoc::source::wdoc_functions().functions,
        files[0]
            .parent()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".")),
    )
    .expect("example wdoc project should render");
}

fn bench_wdoc_project_load(c: &mut Criterion) {
    let mut group = c.benchmark_group("wdoc");
    group.sample_size(10);
    group.bench_function("load example project", |b| {
        b.iter(|| black_box(load_example_project()))
    });
    group.finish();
}

fn bench_wdoc_codec_registry(c: &mut Criterion) {
    let document = load_example_project();
    let mut group = c.benchmark_group("wdoc");
    group.sample_size(10);
    group.bench_function("codec registry", |b| {
        b.iter(|| black_box(registry(black_box(&document))))
    });
    group.finish();
}

fn bench_wdoc_render(c: &mut Criterion) {
    let document = load_example_project();
    let registry = registry(&document);
    let mut group = c.benchmark_group("wdoc");
    group.sample_size(10);
    group.bench_function("render example project", |b| {
        b.iter_batched(
            || tempdir().expect("tempdir"),
            |dir| render_example_project(&document, registry.clone(), black_box(dir.path())),
            BatchSize::SmallInput,
        )
    });
    group.finish();
}

fn bench_wdoc_full_build(c: &mut Criterion) {
    let mut group = c.benchmark_group("wdoc");
    group.sample_size(10);
    group.bench_function("full example build", |b| {
        b.iter_batched(
            || (load_example_project(), tempdir().expect("tempdir")),
            |(document, dir)| {
                let registry = registry(&document);
                render_example_project(&document, registry, black_box(dir.path()));
            },
            BatchSize::SmallInput,
        )
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_wdoc_project_load,
    bench_wdoc_codec_registry,
    bench_wdoc_render,
    bench_wdoc_full_build,
);
criterion_main!(benches);
