use std::env;
use std::fs;
use std::path::{Path, PathBuf};

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let source_dir = manifest_dir.join("src").join("wdoc");
    let manifest_path = source_dir.join("manifest.txt");

    println!("cargo:rerun-if-changed={}", manifest_path.display());

    let manifest = fs::read_to_string(&manifest_path)
        .unwrap_or_else(|err| panic!("failed to read {}: {err}", manifest_path.display()));

    let mut bundled = String::new();
    let wcl_lang_std = manifest_dir
        .parent()
        .and_then(Path::parent)
        .map(|root| root.join("crates").join("wcl_lang").join("src").join("std"))
        .unwrap_or_else(|| manifest_dir.join("../wcl_lang/src/std"));
    for entry in ["html.wcl", "svg.wcl", "css.wcl"] {
        let path = wcl_lang_std.join(entry);
        println!("cargo:rerun-if-changed={}", path.display());
        let fragment = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
        bundled.push_str(&fragment);
        if !bundled.ends_with('\n') {
            bundled.push('\n');
        }
        bundled.push('\n');
    }

    for line in manifest.lines() {
        let entry = line.trim();
        if entry.is_empty() || entry.starts_with('#') {
            continue;
        }

        let path = source_dir.join(entry);
        ensure_manifest_entry_is_local(&source_dir, &path);
        println!("cargo:rerun-if-changed={}", path.display());

        let fragment = fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("failed to read {}: {err}", path.display()));
        bundled.push_str(&fragment);
        if !bundled.ends_with('\n') {
            bundled.push('\n');
        }
        bundled.push('\n');
    }

    while bundled.ends_with('\n') {
        bundled.pop();
    }
    bundled.push('\n');

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    fs::write(out_dir.join("wdoc.wcl"), bundled).expect("failed to write bundled wdoc.wcl");
}

fn ensure_manifest_entry_is_local(source_dir: &Path, path: &Path) {
    let source_dir = source_dir
        .canonicalize()
        .unwrap_or_else(|err| panic!("failed to canonicalize {}: {err}", source_dir.display()));

    if let Some(parent) = path.parent() {
        let parent = parent
            .canonicalize()
            .unwrap_or_else(|err| panic!("failed to canonicalize {}: {err}", parent.display()));
        if parent.starts_with(&source_dir) {
            return;
        }
    }

    panic!(
        "manifest entry {} must stay inside {}",
        path.display(),
        source_dir.display()
    );
}
