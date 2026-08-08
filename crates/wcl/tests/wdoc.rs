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

    relocatable::assert_relocatable(&nested, 3, 8);
}
