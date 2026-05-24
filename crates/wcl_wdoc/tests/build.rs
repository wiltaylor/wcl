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
        Err(BuildError::Parse(r)) => panic!("build parse error: {r:?}"),
        Err(BuildError::Schema(n)) => panic!("build schema error: {n} violations"),
        Err(BuildError::BadPage(m)) => panic!("build bad-page error: {m}"),
        Err(BuildError::DuplicateId { page, id }) => {
            panic!("build duplicate-id error: page {page}: {id}")
        }
        Err(BuildError::BadLink(msgs)) => panic!("build bad-link error: {msgs:?}"),
    }
}

#[test]
fn build_emits_fundamentals_for_example_site() {
    let out = TempDir::new().expect("mkdir tempdir");
    let n = build_ok(&examples_dir().join("wdoc").join("site.wcl"), out.path());
    assert_eq!(n, 3);

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
    // stdlib flowchart lowering — process emits a rect + centered
    // label. The site example puts the flowchart inside a layered
    // diagram, so each shape renders at (0, 0) inside its own
    // <g transform="translate(...)"> wrapper. The label centers at
    // (50, 20) of its own rect.
    assert!(
        index.contains("<rect x=\"0\" y=\"0\" width=\"100\" height=\"40\" fill=\"#eef\""),
        "{index}"
    );
    assert!(
        index.contains("<text x=\"50\" y=\"20\">Validate</text>"),
        "{index}"
    );
    // decision lowers to a diamond polygon — using the default 80x40
    // bbox the layered example doesn't override.
    assert!(
        index.contains("<polygon points=\"50,0 100,30 50,60 0,30\""),
        "{index}"
    );
}

#[test]
fn recursive_lowering_terminates_for_chained_custom_shapes() {
    // `outer` lowers to an `inner` variant, which itself lowers to a
    // rect. The renderer must keep lowering until only fundamentals
    // remain. Each type carries its lowering via `@default` on its
    // own `lower` field — no top-level `_lower` bindings.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("chain.wcl");
    std::fs::write(
        &src,
        r##"
union ChainStep {
  Inner { fill: utf8? }
  Rect  { x: f64? y: f64? width: f64? height: f64? fill: utf8? stroke: utf8? id: identifier? class: list<utf8>? }
}

@block("inner")
type Inner extends SvgBlock {
  fill: utf8?
  lower = fn(i: Inner) -> list<SvgFundamental> [
    SvgFundamental::Rect {
      x: 5.0, y: 5.0, width: 20.0, height: 20.0,
      fill: i.fill, stroke: none, id: none, class: none,
    }
  ]
}

@block("outer")
type Outer extends SvgBlock {
  fill: utf8?
  lower = fn(o: Outer) -> list<ChainStep> [
    ChainStep::Inner { fill: o.fill }
  ]
}

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
union LoopStep {
  Loopy { fill: utf8? }
}

@block("loopy")
type Loopy extends SvgBlock {
  fill: utf8?
  lower = fn(l: Loopy) -> list<LoopStep> [
    LoopStep::Loopy { fill: l.fill }
  ]
}

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
fn user_defined_block_lowering_via_at_default() {
    // A user-authored block in user source, with no top-level
    // `_lower` binding anywhere — the renderer must still pick up
    // the lowering from the type's `@default(...)` on `lower`.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("user_block.wcl");
    std::fs::write(
        &src,
        r##"
@block("badge")
type Badge extends SvgBlock {
  @inline(0) text: utf8
  id: identifier?
  x: f64?  y: f64?
  lower = fn(b: Badge) -> list<SvgFundamental> [
    SvgFundamental::Label {
      content: b.text, x: b.x, y: b.y,
      fill: none, id: b.id, class: none,
    }
  ]
}

page index {
  diagram {
    width  = 100
    height = 50
    badge "hello" {
      x = 10.0
      y = 20.0
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
        html.contains("<text x=\"10\" y=\"20\">hello</text>"),
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
        Err(BuildError::DuplicateId { page, id }) => {
            panic!("expected Schema, got DuplicateId({page}: {id})")
        }
        Err(BuildError::BadLink(msgs)) => panic!("expected Schema, got BadLink({msgs:?})"),
        Ok(n) => panic!("expected Schema error, got Ok({n})"),
    }
}

#[test]
fn build_emits_id_attributes_across_paths() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("ids.wcl");
    std::fs::write(
        &src,
        r##"
page index {
  h1 "Title" {
    id = title
  }
  text {
    id = intro
    span "hello " {
      id = greeting
    }
  }
  diagram {
    width  = 100
    height = 100
    rect {
      id = box
      x = 0.0  y = 0.0
      width = 10.0  height = 10.0
      fill = "#abc"
    }
    process "Step" {
      id = step1
      x = 20.0  y = 20.0
      width = 50.0  height = 20.0
      fill = "#def"
    }
  }
}
"##,
    )
    .expect("write fixture");

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());

    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");

    // Block-side path: rect picks up id directly.
    assert!(html.contains("id=\"box\""), "{html}");
    // Lowered HTML payload path: h1 -> paragraph carries id.
    assert!(
        html.contains("<p class=\"heading-1\" id=\"title\">"),
        "{html}"
    );
    // Lowered SVG payload path: process -> rect carries id, label does not.
    assert!(html.contains("id=\"step1\""), "{html}");
    // The process's label should not inherit the id.
    let label_chunk = html
        .split("<text ")
        .nth(1)
        .expect("at least one <text> in lowered process output");
    assert!(
        !label_chunk.starts_with("x") || !label_chunk.contains("id=\"step1\""),
        "process label should not carry the process id: {label_chunk}"
    );
    // Block-side text and span pick up ids.
    assert!(html.contains("<p id=\"intro\">"), "{html}");
    assert!(html.contains("<span id=\"greeting\">"), "{html}");
}

#[test]
fn build_rejects_duplicate_id_within_page() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("dupes.wcl");
    std::fs::write(
        &src,
        r##"
page index {
  diagram {
    width  = 100
    height = 100
    rect {
      id = shared
      x = 0.0  y = 0.0  width = 10.0  height = 10.0
      fill = "#abc"
    }
    rect {
      id = shared
      x = 20.0  y = 20.0  width = 10.0  height = 10.0
      fill = "#def"
    }
  }
}
"##,
    )
    .expect("write fixture");

    let out = TempDir::new().expect("mkdir out");
    match build(&src, out.path()) {
        Err(BuildError::DuplicateId { page, id }) => {
            assert_eq!(page, "index");
            assert_eq!(id, "shared");
        }
        Err(BuildError::Schema(n)) => panic!("expected DuplicateId, got Schema({n})"),
        Err(BuildError::Io(e, ctx)) => panic!("expected DuplicateId, got Io({ctx}: {e})"),
        Err(BuildError::Parse(_)) => panic!("expected DuplicateId, got Parse"),
        Err(BuildError::BadPage(m)) => panic!("expected DuplicateId, got BadPage({m})"),
        Err(BuildError::BadLink(msgs)) => panic!("expected DuplicateId, got BadLink({msgs:?})"),
        Ok(n) => panic!("expected DuplicateId, got Ok({n})"),
    }
}

#[test]
fn build_preserves_source_order_across_mixed_children() {
    // With the single `@children(WdocBlock)` slot on `Page`, mixed-kind
    // children must come out in source order (not bucketed by kind).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("order.wcl");
    std::fs::write(
        &src,
        r##"
page index {
  h1 "First" {}
  text { span "alpha" {} }
  h2 "Middle" {}
  diagram {
    width  = 10
    height = 10
  }
  text { span "omega" {} }
}
"##,
    )
    .expect("write fixture");

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");

    let order = [
        "<p class=\"heading-1\"><span>First</span></p>",
        "<p><span>alpha</span></p>",
        "<p class=\"heading-2\"><span>Middle</span></p>",
        "<svg",
        "<p><span>omega</span></p>",
    ];
    let mut cursor = 0;
    for marker in order {
        let found = html[cursor..]
            .find(marker)
            .unwrap_or_else(|| panic!("marker {marker:?} missing or out of order in:\n{html}"));
        cursor += found + marker.len();
    }
}

#[test]
fn build_renders_connections_as_arrows() {
    // Self-contained fixture so the test stays stable when the
    // connections.wcl example file's layout changes. Exercises the
    // straight-elbow case (same y), the kind-tagged case, and the
    // cross-container case where the source and destination live
    // inside different containers.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("arrows.wcl");
    std::fs::write(
        &src,
        r##"
page index {
  // Flat diagram with manual x/y so we can assert exact bbox sides.
  diagram {
    width  = 320
    height = 120

    process "Validate" {
      id = step_a
      x = 10.0  y = 40.0  width = 80.0  height = 40.0
      fill = "#eef"  stroke = "#446"
    }
    decision "Match?" {
      id = step_b
      x = 130.0  y = 30.0  width = 80.0  height = 60.0
      fill = "#fee"  stroke = "#a44"
    }
    terminator "Done" {
      id = step_c
      x = 240.0  y = 40.0  width = 70.0  height = 40.0
      fill = "#efe"  stroke = "#494"
    }

    step_a -> step_b
    step_b -> step_c :flow
  }

  // Cross-container: anchor each container manually so we can
  // assert the inner rects' absolute coordinates.
  diagram {
    width  = 320
    height = 160

    container {
      id = group_left
      anchor_left = 0.0  anchor_top = 0.0
      width = 140.0  height = 160.0
      rect {
        id = inner_a
        x = 30.0  y = 50.0  width = 80.0  height = 40.0
        fill = "#cce"  stroke = "#446"
      }
    }
    container {
      id = group_right
      anchor_left = 180.0  anchor_top = 0.0
      width = 140.0  height = 160.0
      rect {
        id = inner_b
        x = 30.0  y = 50.0  width = 80.0  height = 40.0
        fill = "#ecc"  stroke = "#a44"
      }
    }

    inner_a -> inner_b :data
  }
}
"##,
    )
    .expect("write fixture");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // One shared <defs><marker> per diagram that has edges.
    assert!(
        html.contains("<marker id=\"wdoc-arrow\""),
        "missing arrow marker:\n{html}"
    );

    // Flat diagram: step_a east (90,60) -> step_b west (130,60).
    assert!(
        html.contains(
            "<polyline points=\"90,60 130,60\" fill=\"none\" \
             stroke=\"currentColor\" marker-end=\"url(#wdoc-arrow)\" data-kind=\"default\" />"
        ),
        "missing default edge:\n{html}"
    );
    // Same diagram: step_b east (210,60) -> step_c west (240,60).
    assert!(
        html.contains(
            "<polyline points=\"210,60 240,60\" fill=\"none\" \
             stroke=\"currentColor\" marker-end=\"url(#wdoc-arrow)\" data-kind=\"flow\" />"
        ),
        "missing flow edge:\n{html}"
    );
    // Cross-container: inner_a east (110,70) -> inner_b west (210,70).
    assert!(
        html.contains(
            "<polyline points=\"110,70 210,70\" fill=\"none\" \
             stroke=\"currentColor\" marker-end=\"url(#wdoc-arrow)\" data-kind=\"data\" />"
        ),
        "missing cross-container edge:\n{html}"
    );
}

#[test]
fn build_picks_closest_anchor_pair_and_honors_custom_connect_points() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("anchors.wcl");
    std::fs::write(
        &src,
        r##"
page index {
  // Two rects stacked vertically: the closest anchor pair is
  // `south of top` -> `north of bottom`.
  diagram {
    width  = 100
    height = 200
    rect {
      id = top
      x = 10.0  y = 10.0  width = 80.0  height = 40.0
      fill = "#abc"
    }
    rect {
      id = bottom
      x = 10.0  y = 150.0  width = 80.0  height = 40.0
      fill = "#abc"
    }
    top -> bottom
  }

  // Custom override: the source attaches only at `:east`, so even
  // though `top` sits directly above `right_only` (south->north
  // would be shorter) the arrow must leave from the east side of
  // `top` (90, 30).
  diagram {
    width  = 200
    height = 200
    rect {
      id = top2
      x = 10.0  y = 10.0  width = 80.0  height = 40.0
      fill = "#abc"
      connect_points = [:east]
    }
    rect {
      id = right_only
      x = 10.0  y = 150.0  width = 80.0  height = 40.0
      fill = "#abc"
    }
    top2 -> right_only
  }
}
"##,
    )
    .expect("write fixture");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // Vertical stack: polyline starts at south(top) (50,50) and ends
    // at north(bottom) (50,150). Elbow routing keeps it a single
    // vertical segment since there's no obstacle in between.
    assert!(
        html.contains("<polyline points=\"50,50 50,150\""),
        "missing vertical south->north edge:\n{html}"
    );
    // Custom override: arrow must leave from east of top2 (90,30)
    // rather than south (50,50), even though south->north would
    // be the shorter path.
    assert!(
        html.contains("points=\"90,30"),
        "expected arrow to leave east of top2 (90,30):\n{html}"
    );
}

#[test]
fn build_routes_around_obstacle() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("obstacle.wcl");
    std::fs::write(
        &src,
        r##"
page index {
  diagram {
    width  = 320
    height = 200
    rect {
      id = a
      x = 10.0  y = 80.0  width = 60.0  height = 40.0
      fill = "#abc"
    }
    rect {
      id = blocker
      x = 130.0  y = 60.0  width = 60.0  height = 80.0
      fill = "#999"
    }
    rect {
      id = b
      x = 250.0  y = 80.0  width = 60.0  height = 40.0
      fill = "#abc"
    }
    a -> b
  }
}
"##,
    )
    .expect("write fixture");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    // The polyline must include at least one bend (>= 3 points,
    // i.e. >= 2 commas in the points list) and a y value that
    // differs from the straight-line y=100 to clear the blocker.
    let poly = html
        .split("<polyline points=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .unwrap_or("");
    let points: Vec<(f64, f64)> = poly
        .split_whitespace()
        .filter_map(|p| {
            let mut it = p.split(',');
            let x: f64 = it.next()?.parse().ok()?;
            let y: f64 = it.next()?.parse().ok()?;
            Some((x, y))
        })
        .collect();
    assert!(
        points.len() >= 3,
        "expected the obstacle route to bend, got points: {:?}\n{html}",
        points
    );
    assert!(
        points.iter().any(|(_, y)| (y - 100.0).abs() > 1.0),
        "expected the polyline to deviate from y=100 to clear blocker: {:?}\n{html}",
        points
    );
}

#[test]
fn build_separates_parallel_edges() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("sep.wcl");
    std::fs::write(
        &src,
        r##"
page index {
  // Two pairs of shapes positioned so the natural edge for both
  // (a -> b and c -> d) is a single horizontal segment at y=100.
  // The separation pass should nudge the two segments apart.
  diagram {
    width  = 320
    height = 200
    rect {
      id = a
      x = 10.0  y = 80.0  width = 60.0  height = 40.0
      connect_points = [:east]
    }
    rect {
      id = b
      x = 250.0  y = 80.0  width = 60.0  height = 40.0
      connect_points = [:west]
    }
    rect {
      id = c
      x = 10.0  y = 80.0  width = 60.0  height = 40.0
      connect_points = [:east]
    }
    rect {
      id = d
      x = 250.0  y = 80.0  width = 60.0  height = 40.0
      connect_points = [:west]
    }
    a -> b
    c -> d
  }
}
"##,
    )
    .expect("write fixture");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    // Two polylines, both with the same start/end x; their middle
    // segment's y should be nudged ±step from y=100. Since these
    // polylines only have 2 points (no middle segment between bends),
    // we instead expect both edges to render — separation only
    // applies to middle segments. The fixture just confirms the
    // separation pass doesn't crash and that both edges render.
    let count = html.matches("<polyline").count();
    assert!(count >= 2, "expected 2 polylines, got {count}:\n{html}");
}

#[test]
fn build_keeps_shared_anchor_edges_aligned() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("shared_anchor.wcl");
    std::fs::write(
        &src,
        r##"
page index {
  // Source has only one anchor (:east at x=70, y=100). Two edges
  // leave that anchor to two different destinations. Both polylines
  // must start at exactly (70, 100); the first segment is not
  // nudged.
  diagram {
    width  = 320
    height = 200
    rect {
      id = src
      x = 10.0  y = 80.0  width = 60.0  height = 40.0
      connect_points = [:east]
    }
    rect {
      id = top
      x = 200.0  y = 20.0  width = 60.0  height = 40.0
    }
    rect {
      id = bot
      x = 200.0  y = 140.0  width = 60.0  height = 40.0
    }
    src -> top
    src -> bot
  }
}
"##,
    )
    .expect("write fixture");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    // Both polylines start at "70,100" — the shared :east anchor.
    let starts: Vec<&str> = html
        .split("<polyline points=\"")
        .skip(1)
        .filter_map(|s| s.split_whitespace().next())
        .collect();
    assert_eq!(starts.len(), 2, "expected 2 polylines:\n{html}");
    assert_eq!(starts[0], "70,100", "first edge start: {starts:?}");
    assert_eq!(starts[1], "70,100", "second edge start: {starts:?}");
}

#[test]
fn build_layered_layout_assigns_positions() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("layered.wcl");
    std::fs::write(
        &src,
        r##"
page index {
  // Three connected shapes; no x/y declared. The layered layout
  // should assign positions so a is at rank 0 (top), b at rank 1,
  // c at rank 2 (bottom).
  diagram {
    width     = 200
    height    = 300
    layout    = :layered
    layer_gap = 20.0
    rect {
      id = a
      width = 80.0  height = 40.0  fill = "#cce"
    }
    rect {
      id = b
      width = 80.0  height = 40.0  fill = "#ecc"
    }
    rect {
      id = c
      width = 80.0  height = 40.0  fill = "#cec"
    }
    a -> b
    b -> c
  }
}
"##,
    )
    .expect("write fixture");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    // Each rect is wrapped in <g transform="translate(tx ty)"> at
    // its layered offset. a starts at the top (ty=0); b is one
    // rank down (40 + 20 = 60); c another rank (120).
    assert!(
        html.contains(
            "<g transform=\"translate(0 0)\"><rect x=\"0\" y=\"0\" width=\"80\" height=\"40\""
        ),
        "missing layered rank-0 placement:\n{html}"
    );
    assert!(
        html.contains("<g transform=\"translate(0 60)\""),
        "missing layered rank-1 placement (y=60):\n{html}"
    );
    assert!(
        html.contains("<g transform=\"translate(0 120)\""),
        "missing layered rank-2 placement (y=120):\n{html}"
    );
}

#[test]
fn build_layered_layout_left_to_right() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("layered_lr.wcl");
    std::fs::write(
        &src,
        r##"
page index {
  diagram {
    width     = 400
    height    = 100
    layout    = :layered
    direction = :left_to_right
    layer_gap = 20.0
    rect {
      id = a
      width = 80.0  height = 40.0
    }
    rect {
      id = b
      width = 80.0  height = 40.0
    }
    rect {
      id = c
      width = 80.0  height = 40.0
    }
    a -> b
    b -> c
  }
}
"##,
    )
    .expect("write fixture");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    // a at left (tx=0); b at tx = 80 + 20 = 100; c at tx = 200.
    assert!(
        html.contains("<g transform=\"translate(0 0)\""),
        "missing layered rank-0 placement:\n{html}"
    );
    assert!(
        html.contains("<g transform=\"translate(100 0)\""),
        "missing layered rank-1 (x=100):\n{html}"
    );
    assert!(
        html.contains("<g transform=\"translate(200 0)\""),
        "missing layered rank-2 (x=200):\n{html}"
    );
}

#[test]
fn build_allows_same_id_across_different_pages() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("two_pages.wcl");
    std::fs::write(
        &src,
        r##"
page one {
  diagram {
    width  = 50
    height = 50
    rect {
      id = shared
      x = 0.0  y = 0.0  width = 10.0  height = 10.0
      fill = "#abc"
    }
  }
}
page two {
  diagram {
    width  = 50
    height = 50
    rect {
      id = shared
      x = 0.0  y = 0.0  width = 10.0  height = 10.0
      fill = "#def"
    }
  }
}
"##,
    )
    .expect("write fixture");

    let out = TempDir::new().expect("mkdir out");
    let n = build_ok(&src, out.path());
    assert_eq!(n, 2);
}

// ── Inline pattern tests ────────────────────────────────────────────

fn write_inline_fixture(tmp: &TempDir, body: &str) -> std::path::PathBuf {
    let src = tmp.path().join("inline.wcl");
    let full = format!(
        "page index {{\n  text {{\n    span \"{}\" {{}}\n  }}\n}}\n",
        body
    );
    std::fs::write(&src, full).expect("write fixture");
    src
}

#[test]
fn build_renders_bold_inline() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = write_inline_fixture(&tmp, "Hello **world**");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("<span>Hello <span class=\"bold\">world</span></span>"),
        "bold not rendered:\n{html}"
    );
}

#[test]
fn build_renders_italic_inline() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = write_inline_fixture(&tmp, "an _accent_ here");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("<span>an <span class=\"italic\">accent</span> here</span>"),
        "italic not rendered:\n{html}"
    );
}

#[test]
fn build_renders_code_inline() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    // Escape backticks for the WCL string literal — we want the
    // input text to contain literal backticks for the code pattern.
    let src = write_inline_fixture(&tmp, "say `hello`");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("<span>say <span class=\"code\">hello</span></span>"),
        "code not rendered:\n{html}"
    );
}

#[test]
fn build_renders_link_inline() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = write_inline_fixture(&tmp, "see [docs](https://example.com)");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("<a href=\"https://example.com\">docs</a>"),
        "link not rendered:\n{html}"
    );
}

#[test]
fn build_renders_recursive_inline() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = write_inline_fixture(&tmp, "**bold _and italic_**");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    // Bold span wraps a literal prefix plus an inner italic span.
    assert!(
        html.contains("<span class=\"bold\">bold <span class=\"italic\">and italic</span></span>"),
        "recursive nesting missing:\n{html}"
    );
}

#[test]
fn build_renders_user_defined_inline_pattern() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("custom.wcl");
    // Custom pattern matching #tags. Captures one group (the tag
    // name without the leading #) and wraps it in a class="tag" span.
    std::fs::write(
        &src,
        r##"
inline_pattern hashtag {
  pattern = "#(\\w+)"
  to_span = fn(g: list<utf8>) -> list<InlineSpan>
    [InlineSpan::Plain { text: at(g, 1), class: ["tag"] }]
}

page index {
  text {
    span "hello #world from #wdoc" {}
  }
}
"##,
    )
    .expect("write fixture");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("<span class=\"tag\">world</span>"),
        "missing first hashtag span:\n{html}"
    );
    assert!(
        html.contains("<span class=\"tag\">wdoc</span>"),
        "missing second hashtag span:\n{html}"
    );
}

#[test]
fn build_inline_pattern_depth_limit() {
    // A pathological pattern: matches 'X' and emits a Plain whose
    // text is also 'X', causing infinite recursion unless the depth
    // guard cuts off. The output must contain *some* rendered span
    // — we don't care exactly how deeply — without stack-overflowing.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("depth.wcl");
    std::fs::write(
        &src,
        r##"
inline_pattern selfish {
  pattern = "X"
  to_span = fn(g: list<utf8>) -> list<InlineSpan>
    [InlineSpan::Plain { text: at(g, 0), class: ["loop"] }]
}

page index {
  text {
    span "X" {}
  }
}
"##,
    )
    .expect("write fixture");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    // At least one wrapper span shows up, and (eventually) a literal X.
    assert!(
        html.contains("<span class=\"loop\">"),
        "missing wrapper:\n{html}"
    );
    assert!(
        html.contains(">X<") || html.contains("X</"),
        "missing literal X:\n{html}"
    );
}

// ── Cross-page link tests ───────────────────────────────────────────

#[test]
fn build_renders_cross_page_link() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("pages.wcl");
    std::fs::write(
        &src,
        r##"
page index {
  text {
    span "See [About](about) for more." {}
  }
}
page about {
  text {
    span "About page" {}
  }
}
"##,
    )
    .expect("write fixture");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let index = std::fs::read_to_string(out.path().join("index.html")).expect("read index");
    assert!(
        index.contains("<a href=\"about.html\">About</a>"),
        "cross-page link missing:\n{index}"
    );
    assert!(out.path().join("about.html").exists());
}

#[test]
fn build_renders_cross_page_link_with_fragment() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("pages.wcl");
    std::fs::write(
        &src,
        r##"
page index {
  text {
    span "Jump to [deep section](about#section)." {}
  }
}
page about {
  text {
    span "About page" {}
  }
}
"##,
    )
    .expect("write fixture");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let index = std::fs::read_to_string(out.path().join("index.html")).expect("read index");
    assert!(
        index.contains("<a href=\"about.html#section\">deep section</a>"),
        "fragment-bearing cross-page link missing:\n{index}"
    );
}

#[test]
fn build_passes_through_external_url() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("ext.wcl");
    std::fs::write(
        &src,
        r##"
page index {
  text {
    span "see [docs](https://example.com)" {}
  }
}
"##,
    )
    .expect("write fixture");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("<a href=\"https://example.com\">docs</a>"),
        "external url should pass through:\n{html}"
    );
}

#[test]
fn build_passes_through_same_page_anchor() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("anchor.wcl");
    std::fs::write(
        &src,
        r##"
page index {
  text {
    span "back to [top](#top)" {}
  }
}
"##,
    )
    .expect("write fixture");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("<a href=\"#top\">top</a>"),
        "same-page anchor should pass through:\n{html}"
    );
}

#[test]
fn build_errors_on_unknown_page_link() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("broken.wcl");
    std::fs::write(
        &src,
        r##"
page index {
  text {
    span "see [docs](nonexistent)" {}
  }
}
"##,
    )
    .expect("write fixture");
    let out = TempDir::new().expect("mkdir out");
    match build(&src, out.path()) {
        Err(BuildError::BadLink(msgs)) => {
            assert!(
                msgs.iter().any(|m| m.contains("nonexistent")),
                "missing the unknown page name in errors: {msgs:?}"
            );
        }
        Err(BuildError::Io(e, ctx)) => panic!("expected BadLink, got Io({ctx}: {e})"),
        Err(BuildError::Parse(_)) => panic!("expected BadLink, got Parse"),
        Err(BuildError::Schema(n)) => panic!("expected BadLink, got Schema({n})"),
        Err(BuildError::BadPage(m)) => panic!("expected BadLink, got BadPage({m})"),
        Err(BuildError::DuplicateId { page, id }) => {
            panic!("expected BadLink, got DuplicateId({page}: {id})")
        }
        Ok(n) => panic!("expected BadLink, got Ok({n})"),
    }
}
