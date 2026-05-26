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
        .stdout(predicate::str::contains("wrote 9 pages"));

    // Landing page lives in main.wcl directly.
    let index = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");
    assert!(index.contains("<style>"), "{index}");
    assert!(
        index.contains("<p class=\"heading-1\">"),
        "missing landing heading:\n{index}"
    );

    // The richer content (text, SVG, classes) sits on the overview
    // page that main.wcl pulls in via `import`.
    let overview =
        std::fs::read_to_string(out.path().join("overview.html")).expect("read overview.html");
    assert!(overview.contains("<p><span>"), "{overview}");
    assert!(overview.contains("<svg"), "{overview}");
    assert!(overview.contains("class=\""), "{overview}");
    assert!(overview.contains("<p class=\"heading-1\">"), "{overview}");
}
