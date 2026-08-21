use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
}

fn wcl() -> Command {
    Command::cargo_bin("wcl").expect("wcl binary built")
}

#[test]
fn wcl_wdoc_build_renders_fundamental_blocks() {
    let out = TempDir::new().expect("mkdir tempdir");
    wcl()
        .arg("wdoc")
        .arg("build")
        .arg(examples_dir().join("wdoc").join("main.wcl"))
        .arg("--out")
        .arg(out.path())
        .assert()
        .success()
        // showcase (15) + docs (3) + blog (3) + talk deck (1) across four sites.
        .stdout(predicate::str::contains("wrote 22 pages"));

    // `showcase` is the `root` site and its `overview` page sets
    // `start = true`, so the root index is that page (not a chooser),
    // with cross-site links into the subdir sites.
    let index = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");
    assert!(index.contains("<style>"), "{index}");
    assert!(
        index.contains("wdoc showcase"),
        "root index should be the showcase demo:\n{index}"
    );
    assert!(
        index.contains("href=\"docs/getting_started.html\""),
        "missing cross-site link:\n{index}"
    );

    // The richer content (text, SVG, classes) sits on the showcase
    // overview page, flat at the root since showcase is the root site.
    let overview =
        std::fs::read_to_string(out.path().join("overview.html")).expect("read overview.html");
    assert!(overview.contains("<p class=\"accent\">"), "{overview}");
    assert!(overview.contains("<svg"), "{overview}");
    assert!(overview.contains("class=\""), "{overview}");
    assert!(
        overview.contains("<h1 class=\"heading-1\" id="),
        "{overview}"
    );
}

// ---------------------------------------------------------------------------
// Relocatable output, through the CLI.
//
// `crates/wcl_wdoc/tests/build.rs` runs this same walk over this same fixture
// book through the library. This is the flag path — `wcl wdoc build --out
// <nested>`, the invocation `just docs-build` and the deploy workflow use.
// ---------------------------------------------------------------------------

#[path = "../../wcl_wdoc/tests/support/relocatable.rs"]
mod relocatable;

#[test]
fn wcl_wdoc_build_into_a_nested_out_dir_is_relocatable() {
    let out = TempDir::new().expect("mkdir tempdir");
    // Nested, and nothing tells the build where it sits — exactly how
    // `docs-build` renders the reference book into `docs/_site/reference`.
    let nested = out.path().join("site").join("reference");
    wcl()
        .arg("wdoc")
        .arg("build")
        .arg(examples_dir().join("wdoc_relocatable").join("main.wcl"))
        .arg("--out")
        .arg(&nested)
        .assert()
        .success()
        .stdout(predicate::str::contains("wrote 3 pages"));

    // Eight attribute URLs and the book typography's thirteen font faces are
    // a floor each side of the walk, so neither half can rot into a no-op.
    relocatable::assert_relocatable(&nested, 3, 8, 13);
}

// ---------------------------------------------------------------------------
// `wcl wdoc build --type` — the consolidated renderer selector that replaced
// the separate `wdoc markdown` and `wdoc pdf` subcommands.
// ---------------------------------------------------------------------------

/// A small single-site fixture; the showcase document is heavy enough that
/// rendering it three times would dominate the suite's runtime.
fn relocatable_main() -> PathBuf {
    examples_dir().join("wdoc_relocatable").join("main.wcl")
}

#[test]
fn build_type_markdown_writes_md_files() {
    let out = TempDir::new().expect("mkdir tempdir");
    wcl()
        .args(["wdoc", "build"])
        .arg(relocatable_main())
        .arg("--out")
        .arg(out.path())
        .args(["--type", "markdown"])
        .assert()
        .success()
        .stdout(predicate::str::contains("wrote 3 pages"));
    let mds: Vec<_> = std::fs::read_dir(out.path())
        .expect("read out dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "md"))
        .collect();
    assert!(!mds.is_empty(), "expected .md output in {:?}", out.path());
}

/// `md` is kept as a value alias, so the old `wdoc md` muscle memory still
/// lands somewhere sensible.
#[test]
fn build_type_accepts_md_alias() {
    let out = TempDir::new().expect("mkdir tempdir");
    wcl()
        .args(["wdoc", "build"])
        .arg(relocatable_main())
        .arg("--out")
        .arg(out.path())
        .args(["--type", "md"])
        .assert()
        .success()
        .stdout(predicate::str::contains("wrote 3 pages"));
}

#[test]
fn build_type_pdf_writes_a_pdf_and_counts_sites() {
    let out = TempDir::new().expect("mkdir tempdir");
    wcl()
        .args(["wdoc", "build"])
        .arg(relocatable_main())
        .arg("--out")
        .arg(out.path())
        .args(["--type", "pdf", "--page-size", "letter"])
        .assert()
        .success()
        // pdf counts sites (one file each), not pages — the same fixture
        // renders 3 pages as markdown but 2 PDFs, which is exactly why the
        // success message stayed per-type rather than being unified.
        .stdout(predicate::str::contains("wrote 2 pdfs"));
    let pdfs: Vec<_> = std::fs::read_dir(out.path())
        .expect("read out dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|x| x == "pdf"))
        .collect();
    assert_eq!(pdfs.len(), 2, "expected two .pdf files in {:?}", out.path());
}

/// `--page-size` is pdf-only, and passing it elsewhere is an error rather
/// than a silently ignored flag.
#[test]
fn page_size_outside_pdf_is_an_error() {
    let out = TempDir::new().expect("mkdir tempdir");
    wcl()
        .args(["wdoc", "build"])
        .arg(relocatable_main())
        .arg("--out")
        .arg(out.path())
        .args(["--type", "html", "--page-size", "a4"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("--page-size"));
}

/// The three-command split is gone: `pdf` and `markdown` are no longer
/// subcommands of `wdoc`.
#[test]
fn removed_wdoc_subcommands_are_rejected() {
    for removed in ["pdf", "markdown", "md"] {
        wcl()
            .args(["wdoc", removed])
            .arg(relocatable_main())
            .assert()
            .failure();
    }
}

#[test]
fn wdoc_build_fails_with_exit_3_when_a_code_listing_is_missing() {
    // A file-backed listing is a promise the build keeps: if the file it
    // names is gone, the build stops rather than shipping a page that
    // quietly disagrees with the code. Exit 3 is the document-fault code,
    // the same one a failed block lowering gives.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    std::fs::write(
        &src,
        "import <wdoc.wcl>\npage index {\n  code rust { source_file = \"gone.rs\" }\n}\n",
    )
    .expect("write doc");
    let out = TempDir::new().expect("mkdir out");
    wcl()
        .arg("wdoc")
        .arg("build")
        .arg(&src)
        .arg("--out")
        .arg(out.path())
        .assert()
        .code(3)
        .stderr(predicate::str::contains("gone.rs"));
}

#[test]
fn wdoc_build_reads_a_code_listing_from_a_file() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    std::fs::write(tmp.path().join("lib.rs"), "pub const ANSWER: u8 = 42;\n")
        .expect("write listing");
    let src = tmp.path().join("doc.wcl");
    std::fs::write(
        &src,
        "import <wdoc.wcl>\npage index {\n  code rust { source_file = \"lib.rs\" }\n}\n",
    )
    .expect("write doc");
    let out = TempDir::new().expect("mkdir out");
    wcl()
        .arg("wdoc")
        .arg("build")
        .arg(&src)
        .arg("--out")
        .arg(out.path())
        .assert()
        .success();
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");
    assert!(html.contains("ANSWER"), "listing not rendered:\n{html}");
}
