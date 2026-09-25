//! The `examples/` corpus, run through the built `wcl` binary.
//!
//! Every example document must pass `wcl check`; every fixture under
//! `examples/errors/` must fail with the exit code it declares. The
//! declaration is a comment line in the fixture itself:
//!
//! - `// expect-exit: N` — the exit code of `wcl check <fixture>`
//!   (required on every error fixture);
//! - `// expect-get: <path> N` — optionally, the exit code of
//!   `wcl get <fixture> <path>`, for fixtures that only fail once a
//!   value is evaluated.
//!
//! Fragments — files that are only meaningful when imported by another
//! document — are listed in [`FRAGMENTS`] and skipped. Anything else
//! that fails is a stale example.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
use tempfile::TempDir;

/// Files under `examples/` that are imported by another example rather
/// than checked on their own, relative to `examples/`. A directory entry
/// (ending in `/`) covers every file beneath it.
const FRAGMENTS: &[&str] = &["wdoc/pages/", "imports/web-defaults.wcl"];

/// The wdoc sites under `examples/`, each built to a scratch directory.
const WDOC_SITES: &[&str] = &["wdoc/main.wcl", "wdoc_template.wcl", "wdoc_website.wcl"];

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
}

fn wcl() -> Command {
    Command::cargo_bin("wcl").expect("wcl binary built")
}

/// Every `.wcl` file beneath `dir`, sorted, as paths relative to
/// `examples/` with `/` separators.
fn wcl_files(dir: &Path) -> Vec<String> {
    fn walk(root: &Path, dir: &Path, out: &mut Vec<String>) {
        let entries =
            std::fs::read_dir(dir).unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()));
        for entry in entries {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                walk(root, &path, out);
            } else if path.extension().is_some_and(|e| e == "wcl") {
                let rel = path.strip_prefix(root).expect("path under examples/");
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

fn is_fragment(rel: &str) -> bool {
    FRAGMENTS.iter().any(|f| {
        if f.ends_with('/') {
            rel.starts_with(f)
        } else {
            rel == *f
        }
    })
}

/// Run `wcl <args>` and return its exit code, with stderr for the
/// failure message.
fn exit_code(args: &[&str]) -> (i32, String) {
    let out = wcl().args(args).output().expect("run wcl");
    (
        out.status.code().expect("wcl exited with a code"),
        String::from_utf8_lossy(&out.stderr).into_owned(),
    )
}

#[test]
fn every_example_document_checks_clean() {
    let root = examples_dir();
    let mut failures = Vec::new();
    let mut checked = 0usize;
    // `union_dispatch.wcl` is expected to pass like the rest; it
    // depends on the union-dispatch fix landing alongside this test.
    for rel in wcl_files(&root) {
        if rel.starts_with("errors/") || is_fragment(&rel) {
            continue;
        }
        let path = root.join(&rel);
        let (code, stderr) = exit_code(&["check", path.to_str().expect("utf-8 path")]);
        if code != 0 {
            failures.push(format!("{rel}: exit {code}\n{stderr}"));
        }
        checked += 1;
    }
    assert!(checked > 0, "no example documents found");
    assert!(
        failures.is_empty(),
        "examples failing `wcl check`:\n{}",
        failures.join("\n")
    );
}

/// The value of `// <key>: <value>` in `src`, if present.
fn directive<'a>(src: &'a str, key: &str) -> Option<&'a str> {
    let prefix = format!("// {key}:");
    src.lines()
        .find_map(|l| l.trim().strip_prefix(prefix.as_str()).map(str::trim))
}

#[test]
fn every_error_fixture_exits_with_its_declared_code() {
    let root = examples_dir();
    let mut failures = Vec::new();
    let fixtures: Vec<String> = wcl_files(&root)
        .into_iter()
        .filter(|rel| rel.starts_with("errors/"))
        .collect();
    assert!(!fixtures.is_empty(), "no fixtures under examples/errors/");
    for rel in fixtures {
        let path = root.join(&rel);
        let path_str = path.to_str().expect("utf-8 path");
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));

        let expected: i32 = directive(&src, "expect-exit")
            .unwrap_or_else(|| panic!("{rel}: missing `// expect-exit: N` line"))
            .parse()
            .unwrap_or_else(|e| panic!("{rel}: bad expect-exit: {e}"));
        let (code, stderr) = exit_code(&["check", path_str]);
        if code != expected {
            failures.push(format!(
                "{rel}: `wcl check` exit {code}, expected {expected}\n{stderr}"
            ));
        }

        if let Some(get) = directive(&src, "expect-get") {
            let (field, want) = get
                .split_once(' ')
                .unwrap_or_else(|| panic!("{rel}: expect-get wants `<path> <code>`"));
            let want: i32 = want
                .trim()
                .parse()
                .unwrap_or_else(|e| panic!("{rel}: bad expect-get code: {e}"));
            let (code, stderr) = exit_code(&["get", path_str, field]);
            if code != want {
                failures.push(format!(
                    "{rel}: `wcl get {field}` exit {code}, expected {want}\n{stderr}"
                ));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "error fixtures with the wrong exit code:\n{}",
        failures.join("\n")
    );
}

#[test]
fn fragment_list_names_real_files() {
    let files = wcl_files(&examples_dir());
    for f in FRAGMENTS {
        assert!(
            files
                .iter()
                .any(|rel| is_fragment(rel) && rel.starts_with(f)),
            "fragment entry {f:?} matches no file under examples/"
        );
    }
}

#[test]
fn every_wdoc_example_site_builds() {
    for site in WDOC_SITES {
        let out = TempDir::new().expect("mkdir tempdir");
        wcl()
            .arg("wdoc")
            .arg("build")
            .arg(examples_dir().join(site))
            .arg("--out")
            .arg(out.path())
            .assert()
            .success();
        assert!(
            out.path().join("index.html").exists(),
            "{site}: build wrote no index.html"
        );
    }
}
