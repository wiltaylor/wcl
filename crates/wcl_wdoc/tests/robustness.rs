//! Regression tests for documents that used to panic, spin or lay out
//! wrongly: each builds a small hostile document through the public
//! `build` API and checks the build finishes with sane output.

use std::path::Path;

use tempfile::TempDir;
use wcl_wdoc::{BuildError, BuildOptions, BuildReport, PageSize, build, build_with_options, pdf};

/// Build `body` (with the stdlib import prepended) as a one-file site and
/// return the build result plus the output directory.
fn build_doc(body: &str) -> (Result<BuildReport, BuildError>, TempDir) {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    std::fs::write(&src, format!("import <wdoc.wcl>\n{body}")).expect("write fixture");
    let out = tmp.path().join("out");
    let result = build_with_options(&src, &out, None, &BuildOptions::default());
    (result, tmp)
}

/// Build `body` and return `index.html` plus the build's warnings,
/// failing the test on any error.
fn build_with_warnings(body: &str) -> (String, Vec<String>) {
    let (result, tmp) = build_doc(body);
    match result {
        Ok(report) => (read_index(tmp.path()), report.warnings),
        Err(e) => panic!("build failed: {}", describe(&e)),
    }
}

/// Build `body` and return `index.html`, failing the test on any error.
fn build_html(body: &str) -> String {
    build_with_warnings(body).0
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
        other => format!("{other:?}"),
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

#[test]
fn oversized_terminal_is_clamped_with_a_warning() {
    // `cols * rows` cells used to be allocated as asked: 10^9 x 10^9
    // overflowed or exhausted memory.
    let (html, warnings) = build_with_warnings(
        r##"
page index {
  terminal {
    cols = 1000000000  rows = 1000000000
    text = "hello"
  }
}
"##,
    );
    // 500 columns of 8-unit cells plus the 6-unit window margins.
    assert!(html.contains("viewBox=\"0 0 4012 "), "{html}");
    assert!(html.contains(">h</text>"), "{html}");
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("exceeds the 500x200 maximum")),
        "{warnings:?}"
    );
}

#[test]
fn terminal_positions_at_the_i64_limits_do_not_overflow() {
    // `row - 1` on i64::MIN, and `base + pos` for a widget at i64::MAX,
    // overflowed in the cell-offset arithmetic.
    build_html(
        r##"
page index {
  terminal {
    cols = 20  rows = 5
    term_text "a" { row = -9223372036854775807 - 1  col = 9223372036854775807 }
    term_box { row = 9223372036854775807  col = 9223372036854775807  width = 5  height = 3  title = "t" }
  }
}
"##,
    );
}

/// Every `height="…"` on a bar rect of the first series.
fn bar_heights(html: &str) -> Vec<f64> {
    html.split("<rect class=\"wdoc-series-1\"")
        .skip(1)
        .filter_map(|r| r.split("height=\"").nth(1)?.split('"').next()?.parse().ok())
        .collect()
}

#[test]
fn negative_bar_values_extend_the_scale_below_zero() {
    // The auto scale folded from 0 up, so a negative value got a
    // negative-height rect and vanished.
    let html = build_html(
        r##"
page index {
  diagram { width = 400  height = 240
    bar_chart { width = 400.0  height = 240.0
      categories = ["a", "b", "c"]
      series = [ { name: "s", values: [10.0, -5.0, 20.0] } ]
    }
  }
}
"##,
    );
    // Plot height 206 over the auto scale -5..20: the -5 bar hangs 41.2
    // below the zero line, and the 20 bar stands 164.8 above it.
    assert!(
        html.contains("y=\"172.8\" width=\"92.80000000000001\" height=\"41.2\""),
        "{html}"
    );
    assert!(
        html.contains("y=\"8\" width=\"92.80000000000001\" height=\"164.8\""),
        "{html}"
    );
    assert!(bar_heights(&html).iter().all(|h| *h >= 0.0));
}

#[test]
fn bars_are_clamped_to_an_explicit_scale() {
    // Values outside an explicit y_min..y_max stay inside the plot area
    // rather than drawing off it or with a negative height.
    let html = build_html(
        r##"
page index {
  diagram { width = 400  height = 240
    bar_chart { width = 400.0  height = 240.0
      y_min = 0.0  y_max = 10.0
      categories = ["a", "b"]
      series = [ { name: "s", values: [-50.0, 50.0] } ]
    }
  }
}
"##,
    );
    let heights = bar_heights(&html);
    assert_eq!(heights.len(), 2, "{html}");
    assert_eq!(heights[0], 0.0);
    assert_eq!(heights[1], 206.0);
}

/// A 256x256 PNG header: signature plus IHDR, all wdoc reads for sizing.
fn fake_png() -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    v.extend_from_slice(&[0, 0, 0, 13]);
    v.extend_from_slice(b"IHDR");
    v.extend_from_slice(&256u32.to_be_bytes());
    v.extend_from_slice(&256u32.to_be_bytes());
    v.extend_from_slice(&[8, 6, 0, 0, 0]);
    v
}

#[test]
fn sprite_sheet_geometry_at_the_i64_limits_does_not_overflow() {
    // `img_w - 2 * margin + spacing` (tileset) and `offset + col * stride`
    // / `columns * rows` (dopesheet) overflowed on extreme fields.
    let tmp = TempDir::new().expect("mkdir tempdir");
    std::fs::write(tmp.path().join("sheet.png"), fake_png()).expect("write sheet");
    let src = tmp.path().join("doc.wcl");
    std::fs::write(
        &src,
        r##"import <wdoc.wcl>
tileset world {
  source       = "sheet.png"
  tile_width   = 64
  tile_height  = 64
  image_width  = 256
  image_height = 256
  margin       = 9223372036854775807
  spacing      = 9223372036854775807
}
page index {
  diagram {
    width = 128  height = 64
    tilemap { set = "world"  tiles = [ [ 0, 1 ] ] }
    dopesheet {
      source = "sheet.png"
      frame_width = 1  frame_height = 1
      columns = 9223372036854775807
      stride_x = 9223372036854775807
      offset_x = 9223372036854775807
      from = 5
    }
  }
}
"##,
    )
    .expect("write fixture");
    if let Err(e) = build(&src, &tmp.path().join("out"), None) {
        panic!("build failed: {}", describe(&e));
    }
    assert!(read_index(tmp.path()).contains("wdoc-dopesheet"));
}

#[test]
fn extreme_aspect_inline_math_does_not_stall_the_pdf() {
    // The PDF reserves an inline object's width as placeholder spaces;
    // a 10^8-em-wide equation asked for hundreds of millions of them.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    std::fs::write(
        &src,
        "import <wdoc.wcl>\npage index {\n  p <<'TEX'\nWide: $a\\hspace{99999999em}b$ end.\nTEX\n}\n",
    )
    .expect("write fixture");
    let out = tmp.path().join("out");
    assert!(
        matches!(
            pdf(&src, &out, None, PageSize::A4),
            Ok(BuildReport { count: 1, .. })
        ),
        "pdf build failed"
    );
}

#[test]
fn self_referential_component_that_fans_out_is_a_build_error() {
    // A component instantiating itself twice stayed inside the 32-level
    // depth cap while generating 2^32 blocks, so the build never ended.
    // The render path now counts every block one expansion tree
    // generates against the language's 100,000-block limit.
    let (result, _tmp) = build_doc(
        r##"
wdoc_component fan {
  wdoc_body {
    p "level"
    fan { }
    fan { }
  }
}
page index {
  fan { }
}
"##,
    );
    match result {
        Err(BuildError::Eval(report)) => {
            let report = format!("{report:?}");
            assert!(
                report.contains("wcl::eval::expansion_limit") && report.contains("100000"),
                "{report}"
            );
        }
        Ok(_) => panic!("fan-out build succeeded"),
        Err(e) => panic!("wrong error: {}", describe(&e)),
    }
}
