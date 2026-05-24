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
    // class system: <style> with both class rules + class= attributes
    assert!(
        index.contains(".accent { color:#003a8c;font-weight:bold; }"),
        "{index}"
    );
    assert!(
        index.contains(".boxed { padding:0.5rem;border:1px solid #999; }"),
        "{index}"
    );
    assert!(
        index.contains("<span class=\"accent\">— now with classes.</span>"),
        "{index}"
    );
    // column carries `class` AND its inline grid style
    assert!(
        index.contains(
            "<div class=\"boxed\" style=\"display:grid;grid-template-columns:50% 50%;\">"
        ),
        "{index}"
    );
    // diagram SVG wrapper
    assert!(
        index.contains(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"280\" height=\"130\" \
             viewBox=\"0 0 280 130\">"
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
    // grid-laid container: outer <g class="badge"> with translate to
    // its declared position, then per-cell translates spaced by
    // (cell_width + gap) = 85.
    assert!(
        index.contains("<g class=\"badge\" transform=\"translate(10 90)\">"),
        "{index}"
    );
    assert!(
        index.contains("<g transform=\"translate(0 0)\">"),
        "{index}"
    );
    assert!(
        index.contains("<g transform=\"translate(85 0)\">"),
        "{index}"
    );
    assert!(
        index.contains("<g transform=\"translate(170 0)\">"),
        "{index}"
    );
    // anchored-rect inside the first cell stretches to cell size.
    assert!(
        index.contains("<rect x=\"0\" y=\"0\" width=\"80\" height=\"30\" fill=\"#eef\""),
        "{index}"
    );
    // stdlib heading lowering — h1 reduces to a paragraph with the
    // matching heading class.
    assert!(
        index.contains("<p class=\"heading-1\"><span>Pipeline overview</span></p>"),
        "{index}"
    );
    // stdlib flowchart lowering — process emits a rect + centered label.
    assert!(
        index.contains("<rect x=\"10\" y=\"10\" width=\"100\" height=\"40\" fill=\"#eef\""),
        "{index}"
    );
    assert!(
        index.contains("<text x=\"60\" y=\"30\">Validate</text>"),
        "{index}"
    );
    // decision lowers to a diamond polygon.
    assert!(
        index.contains("<polygon points=\"60,70 110,100 60,130 10,100\""),
        "{index}"
    );
}

#[test]
fn recursive_lowering_terminates_for_chained_custom_shapes() {
    // `outer` lowers to an `inner` variant, which itself lowers to a
    // rect. The renderer must keep lowering until only fundamentals
    // remain.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("chain.wcl");
    std::fs::write(
        &src,
        r##"
@block("inner")
type Inner { fill: utf8? }

union ChainStep {
  Inner { fill: utf8? }
  Rect  { x: f64? y: f64? width: f64? height: f64? fill: utf8? stroke: utf8? class: list<utf8>? }
}

@schemaless
inner_lower = fn(i: Inner) -> list<SvgFundamental> [
  SvgFundamental::Rect {
    x: 5.0, y: 5.0, width: 20.0, height: 20.0,
    fill: i.fill, stroke: none, class: none,
  }
]

@block("outer")
type Outer { fill: utf8? }

@schemaless
outer_lower = fn(o: Outer) -> list<ChainStep> [
  ChainStep::Inner { fill: o.fill }
]

page index {
  diagram {
    width  = 100
    height = 100
    outer {
      fill = "#abc"
    }
  }
}
"##,
    )
    .expect("write fixture");

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    // The chain `outer -> inner -> Rect` resolves all the way through.
    assert!(
        html.contains("<rect x=\"5\" y=\"5\" width=\"20\" height=\"20\" fill=\"#abc\""),
        "{html}"
    );
}

#[test]
fn lowering_depth_limit_emits_marker() {
    // A pathological lowering that emits its own kind — must bail at
    // the depth limit rather than recursing forever.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("loop.wcl");
    std::fs::write(
        &src,
        r##"
@block("loopy")
type Loopy { fill: utf8? }

union LoopStep {
  Loopy { fill: utf8? }
}

@schemaless
loopy_lower = fn(l: Loopy) -> list<LoopStep> [
  LoopStep::Loopy { fill: l.fill }
]

page index {
  diagram {
    width  = 50
    height = 50
    loopy {
      fill = "#fff"
    }
  }
}
"##,
    )
    .expect("write fixture");

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("wdoc: lowering depth limit reached"),
        "{html}"
    );
}

#[test]
fn build_resolves_anchor_stretch_without_layout() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("anchors.wcl");
    std::fs::write(
        &src,
        r##"
page index {
  diagram {
    width  = 0
    height = 0
    container {
      anchor_left = 0.0
      anchor_top  = 0.0
      width  = 200.0
      height = 100.0
      rect {
        anchor_left   = 10.0
        anchor_right  = 10.0
        anchor_top    = 20.0
        anchor_bottom = 20.0
        fill = "#abc"
      }
    }
  }
}
"##,
    )
    .expect("write fixture");

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());

    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");
    // Container declares 200×100; child rect stretched against that
    // with 10/10 horizontal and 20/20 vertical anchors.
    assert!(
        html.contains("<rect x=\"10\" y=\"20\" width=\"180\" height=\"60\" fill=\"#abc\""),
        "{html}"
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
