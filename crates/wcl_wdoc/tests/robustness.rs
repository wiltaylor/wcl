//! Regression tests for documents that used to panic, spin or lay out
//! wrongly: each builds a small hostile document through the public
//! `build` API and checks the build finishes with sane output.

use std::path::Path;

use tempfile::TempDir;
use wcl_wdoc::{BuildError, build};

/// Build `body` (with the stdlib import prepended) as a one-file site and
/// return the build result plus the output directory.
fn build_doc(body: &str) -> (Result<usize, BuildError>, TempDir) {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    std::fs::write(&src, format!("import <wdoc.wcl>\n{body}")).expect("write fixture");
    let out = tmp.path().join("out");
    let result = build(&src, &out, None);
    (result, tmp)
}

/// Build `body` and return `index.html`, failing the test on any error.
fn build_html(body: &str) -> String {
    let (result, tmp) = build_doc(body);
    if let Err(e) = result {
        panic!("build failed: {}", describe(&e));
    }
    read_index(tmp.path())
}

/// A one-line account of a build error, for test failure messages.
fn describe(e: &BuildError) -> String {
    match e {
        BuildError::Io(e, ctx) => format!("io: {ctx}: {e}"),
        BuildError::Parse(r) => format!("parse: {r:?}"),
        BuildError::Schema(n) => format!("schema: {n} violations"),
        BuildError::Eval(r) => format!("eval: {r:?}"),
        BuildError::BadPage(m) => format!("bad page: {m}"),
        BuildError::DuplicateId { page, id } => format!("duplicate id: {page}: {id}"),
        BuildError::DuplicatePage { site, name } => format!("duplicate page: {site}: {name}"),
        BuildError::BadLink(m) => format!("bad link: {m:?}"),
        BuildError::BadTemplate(m) => format!("bad template: {m}"),
        BuildError::Tileset(m) => format!("tileset: {m}"),
        BuildError::EdgeRouting(m) => format!("edge routing: {m}"),
        BuildError::CodeInclude(m) => format!("code include: {m}"),
    }
}

fn read_index(root: &Path) -> String {
    std::fs::read_to_string(root.join("out").join("index.html")).expect("read index.html")
}

#[test]
fn elbow_edges_converging_on_a_small_circle_do_not_panic() {
    // Three elbow edges arrive on the west side of a 10-unit circle. The
    // arrival spread used to clamp to `by + 8 ..= by + bh - 8`, an empty
    // range on a side shorter than 16 units.
    let html = build_html(
        r##"
page index {
  diagram {
    width = 400  height = 300
    rect { id = a  x = 10.0  y = 10.0   width = 60.0  height = 30.0 }
    rect { id = b  x = 10.0  y = 200.0  width = 60.0  height = 30.0 }
    rect { id = c  x = 10.0  y = 100.0  width = 60.0  height = 30.0 }
    circle { id = j  cx = 300.0  cy = 115.0  r = 5.0 }
    a -> j
    b -> j
    c -> j
  }
}
"##,
    );
    assert_eq!(html.matches("marker-end=\"url(#wdoc-arrow)\"").count(), 3);
}

#[test]
fn non_ascii_hex_colour_is_ignored_not_sliced() {
    // `"#é1"` is three bytes, so it passed the `#rgb` length check and
    // was then sliced through the middle of the `é`.
    let html = build_html(
        r##"
page index {
  terminal {
    cols = 20  rows = 3
    term_text "x" { row = 1  col = 1  fg = "#é1"  bg = "#aébcd" }
  }
}
"##,
    );
    assert!(html.contains("<svg"), "{html}");
}

/// A one-shape timeline page with `fields` spliced into the block.
fn timeline_doc(fields: &str) -> String {
    format!(
        r##"
page index {{
  diagram {{ width = 560  height = 220
    timeline {{ width = 560.0  height = 220.0
      {fields}
      items = [ {{ label: "A", on: "2026-02-20" }} ]
    }}
  }}
}}
"##
    )
}

#[test]
fn timeline_axis_stops_at_the_end_of_the_calendar() {
    // The tick walk stepped past 9999-12-31, which `time` cannot hold.
    let html = build_html(&timeline_doc(
        r#"start = "9999-12-01"  end = "9999-12-31"  unit = :days"#,
    ));
    assert!(html.contains("wdoc-axis"), "{html}");
}

#[test]
fn timeline_huge_every_does_not_overflow() {
    // `every` scaled into a Duration (minutes) or months (years * 12)
    // overflowed i64.
    for unit in [":minutes", ":hours", ":days", ":weeks", ":months", ":years"] {
        build_html(&timeline_doc(&format!(
            r#"start = "2026-01-01"  end = "2026-12-31"  unit = {unit}  every = 9223372036854775807"#
        )));
    }
}
