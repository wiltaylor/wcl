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
fn build_emits_fundamentals_for_example_site() {
    let out = TempDir::new().expect("mkdir tempdir");
    let n = build_ok(&examples_dir().join("wdoc").join("site.wcl"), out.path());
    assert_eq!(n, 1);

    let index = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");
    assert!(index.contains("<title>index</title>"), "{index}");
    // text + span
    assert!(
        index.contains("<p><span>Welcome to wdoc </span>"),
        "{index}"
    );
    // column grid CSS
    assert!(
        index.contains("<div style=\"display:grid;grid-template-columns:50% 50%;\">"),
        "{index}"
    );
    // diagram SVG wrapper
    assert!(
        index.contains(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"240\" height=\"80\" \
             viewBox=\"0 0 240 80\">"
        ),
        "{index}"
    );
    // each shape kind
    assert!(index.contains("<rect "), "{index}");
    assert!(index.contains("<circle "), "{index}");
    assert!(index.contains("<line "), "{index}");
    assert!(
        index.contains("<text x=\"110\" y=\"76\">halfway</text>"),
        "{index}"
    );
    assert!(
        index.contains("<polygon points=\"180,10 230,40 180,70\""),
        "{index}"
    );
}

#[test]
fn build_html_escapes_span_text() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("escape.wcl");
    std::fs::write(
        &src,
        r#"
page index {
  text {
    span "A & B <c>" {}
    span "say \"hi\"" {}
  }
}
"#,
    )
    .expect("write fixture");

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());

    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");
    assert!(html.contains("<span>A &amp; B &lt;c&gt;</span>"), "{html}");
    assert!(html.contains("<span>say &quot;hi&quot;</span>"), "{html}");
}

#[test]
fn build_reports_schema_error_for_unknown_block() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("bad.wcl");
    std::fs::write(
        &src,
        r#"
page index {
  h1 "nope" {}
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
