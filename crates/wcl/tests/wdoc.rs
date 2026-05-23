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
fn wcl_wdoc_build_writes_index_html() {
    let out = TempDir::new().expect("mkdir tempdir");
    wcl()
        .arg("wdoc")
        .arg("build")
        .arg(examples_dir().join("wdoc").join("site.wcl"))
        .arg("--out")
        .arg(out.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("wrote 2 pages"));

    let index = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");
    assert!(index.contains("<h1>Welcome</h1>"), "{index}");
}
