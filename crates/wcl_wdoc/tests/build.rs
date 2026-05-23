use std::path::{Path, PathBuf};

use tempfile::TempDir;
use wcl_wdoc::{BuildError, build};

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
}

fn build_ok(file: &Path, out: &Path) -> usize {
    match build(file, out) {
        Ok(n) => n,
        Err(BuildError::Io(e, ctx)) => panic!("build io error: {ctx}: {e}"),
        Err(BuildError::Parse(_)) => panic!("build parse error"),
        Err(BuildError::Schema(n)) => panic!("build schema error: {n} violations"),
        Err(BuildError::BadPage(m)) => panic!("build bad-page error: {m}"),
    }
}

#[test]
fn build_emits_one_html_per_page() {
    let out = TempDir::new().expect("mkdir tempdir");
    let n = build_ok(&examples_dir().join("wdoc").join("site.wcl"), out.path());
    assert_eq!(n, 2);

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
    build_ok(&src, out.path());

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
    match build(&src, out.path()) {
        Err(BuildError::Schema(n)) => assert!(n >= 1, "expected at least one violation, got {n}"),
        Err(BuildError::Io(e, ctx)) => panic!("expected Schema, got Io({ctx}: {e})"),
        Err(BuildError::Parse(_)) => panic!("expected Schema, got Parse"),
        Err(BuildError::BadPage(m)) => panic!("expected Schema, got BadPage({m})"),
        Ok(n) => panic!("expected Schema error, got Ok({n})"),
    }
}
