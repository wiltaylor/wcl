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
        // showcase (11) + docs (3) + blog (3) across three sites.
        .stdout(predicate::str::contains("wrote 17 pages"));

    // `showcase` is the `root` site, so the root index is its landing
    // demo (not a chooser), with cross-site links into the subdir sites.
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
    assert!(overview.contains("<p><span>"), "{overview}");
    assert!(overview.contains("<svg"), "{overview}");
    assert!(overview.contains("class=\""), "{overview}");
    assert!(overview.contains("<p class=\"heading-1\">"), "{overview}");
}
