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

fn wdoc() -> Command {
    Command::cargo_bin("wdoc").expect("wdoc binary built")
}

#[test]
fn build_emits_one_html_per_page() {
    let out = TempDir::new().expect("mkdir tempdir");
    wdoc()
        .arg("build")
        .arg(examples_dir().join("wdoc").join("site.wcl"))
        .arg("--out")
        .arg(out.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("wrote 2 pages"));

    let index = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");
    assert!(index.contains("<h1>Welcome</h1>"), "{index}");
    assert!(index.contains("<p>Hello, world.</p>"), "{index}");
    assert!(index.contains("<title>index</title>"), "{index}");

    let about = std::fs::read_to_string(out.path().join("about.html")).expect("read about.html");
    assert!(about.contains("<h1>About</h1>"), "{about}");
    assert!(about.contains("<h2>What is wdoc?</h2>"), "{about}");
    assert!(
        about.contains("<p>A small WCL-driven static site generator.</p>"),
        "{about}"
    );
}

#[test]
fn build_html_escapes_text() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("escape.wcl");
    std::fs::write(
        &src,
        r#"
page index {
  h1 "A & B <c>" {}
  p  "say \"hi\"" {}
}
"#,
    )
    .expect("write fixture");

    let out = TempDir::new().expect("mkdir out");
    wdoc()
        .arg("build")
        .arg(&src)
        .arg("--out")
        .arg(out.path())
        .assert()
        .success();

    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");
    assert!(html.contains("<h1>A &amp; B &lt;c&gt;</h1>"), "{html}");
    assert!(html.contains("<p>say &quot;hi&quot;</p>"), "{html}");
}

#[test]
fn build_reports_schema_error_for_unknown_block() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("bad.wcl");
    std::fs::write(
        &src,
        r#"
page index {
  h7 "nope" {}
}
"#,
    )
    .expect("write fixture");

    let out = TempDir::new().expect("mkdir out");
    wdoc()
        .arg("build")
        .arg(&src)
        .arg("--out")
        .arg(out.path())
        .assert()
        .failure()
        .stderr(predicate::str::contains("h7"));
}
