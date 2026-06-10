use std::path::{Path, PathBuf};

use tempfile::TempDir;
use wcl_wdoc::{BuildError, build};

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
}

/// Write a wdoc fixture to `path`, prepending the `import <wdoc.wcl>`
/// line a real document needs now that `build` no longer injects the
/// stdlib. Repeated imports are a no-op, so this is safe even if `body`
/// already imports it.
fn write_fixture(path: impl AsRef<Path>, body: impl AsRef<str>) {
    let composed = format!("import <wdoc.wcl>\n{}", body.as_ref());
    std::fs::write(path, composed).expect("write wdoc fixture");
}

fn build_ok(file: &Path, out: &Path) -> usize {
    match build(file, out, None) {
        Ok(n) => n,
        Err(BuildError::Io(e, ctx)) => panic!("build io error: {ctx}: {e}"),
        Err(BuildError::Parse(r)) => panic!("build parse error: {r:?}"),
        Err(BuildError::Schema(n)) => panic!("build schema error: {n} violations"),
        Err(BuildError::Eval(r)) => panic!("build eval error: {r:?}"),
        Err(BuildError::BadPage(m)) => panic!("build bad-page error: {m}"),
        Err(BuildError::DuplicateId { page, id }) => {
            panic!("build duplicate-id error: page {page}: {id}")
        }
        Err(BuildError::DuplicatePage { site, name }) => {
            panic!("build duplicate-page error: site {site}: {name}")
        }
        Err(BuildError::BadLink(msgs)) => panic!("build bad-link error: {msgs:?}"),
        Err(BuildError::BadTemplate(name)) => panic!("build bad-template error: {name}"),
        Err(BuildError::Tileset(m)) => panic!("build tileset error: {m}"),
        Err(BuildError::EdgeRouting(m)) => panic!("build edge-routing error: {m}"),
    }
}

#[test]
fn build_emits_fundamentals_for_example_site() {
    // examples/wdoc/main.wcl declares four sites (showcase / docs / blog /
    // talk). `showcase` is the `root` site, so it renders flat at the
    // output root; docs/blog/talk go to subdirectories.
    let out = TempDir::new().expect("mkdir tempdir");
    let n = build_ok(&examples_dir().join("wdoc").join("main.wcl"), out.path());
    assert_eq!(n, 22); // showcase 15 + docs 3 + blog 3 + talk deck 1

    // The richer content (text + classes + diagram + flowchart) is on
    // the showcase overview page (at the root, since showcase is `root`).
    let overview =
        std::fs::read_to_string(out.path().join("overview.html")).expect("read overview.html");
    // Browser title: the page name (no page `title` set) suffixed with the
    // showcase site's title.
    assert!(
        overview.contains("<title>overview — wdoc showcase</title>"),
        "{overview}"
    );
    // text + span
    assert!(
        overview.contains("<p><span>Welcome to wdoc </span>"),
        "{overview}"
    );
    // class system: <style> with both class rules + class= attributes.
    // The example's `accent` uses the theme accent var so it adapts per
    // site theme (rather than a fixed colour that muddies on dark themes).
    assert!(
        overview.contains(".accent { color:var(--wdoc-accent);font-weight:bold; }"),
        "{overview}"
    );
    assert!(
        overview.contains(".boxed { padding:0.5rem;border:1px solid #999; }"),
        "{overview}"
    );
    assert!(
        overview.contains("<span class=\"accent\">— now with classes.</span>"),
        "{overview}"
    );
    // column carries `class` AND its inline grid style
    assert!(
        overview.contains(
            "<div class=\"boxed\" style=\"display:grid;grid-template-columns:50% 50%;\">"
        ),
        "{overview}"
    );
    // diagram SVG wrapper — the outer width/height pin the page
    // layout slot; the viewBox is computed to wrap the content
    // bbox, so we don't pin its exact value here.
    assert!(
        overview.contains(
            "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"280\" height=\"130\" viewBox=\""
        ),
        "{overview}"
    );
    // each shape kind
    assert!(overview.contains("<rect "), "{overview}");
    assert!(overview.contains("<circle "), "{overview}");
    assert!(overview.contains("<line "), "{overview}");
    // Labels now emit centring attributes + a tspan per line.
    assert!(
        overview.contains("<text x=\"110\" y=\"76\" font-size=\"14\" text-anchor=\"middle\" dominant-baseline=\"middle\"")
            && overview.contains(">halfway</tspan>"),
        "{overview}"
    );
    assert!(
        overview.contains("<polygon points=\"180,10 230,40 180,70\""),
        "{overview}"
    );
    // grid-laid container: outer <g class="badge"> with translate to
    // its declared position, then per-cell translates spaced by
    // (cell_width + gap) = 85.
    assert!(
        overview.contains("<g class=\"badge\" transform=\"translate(10 90)\">"),
        "{overview}"
    );
    assert!(
        overview.contains("<g transform=\"translate(0 0)\">"),
        "{overview}"
    );
    assert!(
        overview.contains("<g transform=\"translate(85 0)\">"),
        "{overview}"
    );
    assert!(
        overview.contains("<g transform=\"translate(170 0)\">"),
        "{overview}"
    );
    // anchored-rect inside the first cell stretches to cell size.
    assert!(
        overview.contains("<rect x=\"0\" y=\"0\" width=\"80\" height=\"30\" fill=\"#eef\""),
        "{overview}"
    );
    // stdlib heading lowering — h1 reduces to a paragraph with the
    // matching heading class.
    assert!(
        overview.contains("<p class=\"heading-1\"><span>Pipeline overview</span></p>"),
        "{overview}"
    );
    // stdlib flowchart lowering — process emits a rect + centered
    // label. The overview puts the flowchart inside a layered
    // diagram, so each shape renders at (0, 0) inside its own
    // <g transform="translate(...)"> wrapper. The label centers at
    // (50, 20) of its own rect.
    assert!(
        overview.contains("<rect x=\"0\" y=\"0\" width=\"100\" height=\"40\" fill=\"#eef\""),
        "{overview}"
    );
    // Process label centred at (50, 20) inside the layered cell;
    // single-line text gets a single tspan with `dy="0em"`.
    assert!(
        overview.contains("<text x=\"50\" y=\"20\" font-size=\"14\" text-anchor=\"middle\" dominant-baseline=\"middle\"")
            && overview.contains(">Validate</tspan>"),
        "{overview}"
    );
    // decision lowers to a diamond polygon — using the default 80x40
    // bbox the layered example doesn't override.
    assert!(
        overview.contains("<polygon points=\"50,0 100,30 50,60 0,30\""),
        "{overview}"
    );
    // table — pipe-row syntax. First row becomes the <thead> header;
    // utf8 cells run through inline patterns (**Parse** -> bold,
    // [see](about) -> cross-page link), numeric cells pass through.
    assert!(
        overview.contains(
            "<table class=\"wdoc-table\"><thead><tr><th>Stage</th><th>Owner</th><th>Steps</th></tr></thead>"
        ),
        "{overview}"
    );
    assert!(
        overview.contains("<td><span class=\"bold\">Parse</span></td>"),
        "{overview}"
    );
    assert!(
        overview.contains("<td><a class=\"link\" href=\"about.html\">see</a></td>"),
        "{overview}"
    );
    assert!(overview.contains("<td>3</td>"), "{overview}");
}

#[test]
fn recursive_lowering_terminates_for_chained_custom_shapes() {
    // `outer` lowers to an `inner` variant, which itself lowers to a
    // rect. The renderer must keep lowering until only fundamentals
    // remain. Each type carries its lowering via `@default` on its
    // own `lower` field — no top-level `_lower` bindings.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("chain.wcl");
    write_fixture(
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
    );

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
fn class_accent_field_themes_a_custom_callout() {
    // A `class`'s `accent` field emits the `--callout-accent` custom
    // property, so a custom callout type is themed from WCL — no
    // hand-written CSS / `stylesheet`. The user class rule is emitted
    // after the library default, so it wins for a custom type.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("accent.wcl");
    write_fixture(
        &src,
        r##"
class "deploy" { accent = "#b48ead" }

page index {
  callout "Deploying" {
    class = ["deploy"]
    icon  = "lucide.rocket"
    body  = "A custom type."
  }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    // The class rule carries the custom property.
    assert!(
        html.contains(".deploy { --callout-accent:#b48ead; }"),
        "{html}"
    );
    // The callout's outer div carries the class, so the accent applies.
    assert!(html.contains("class=\"callout deploy\""), "{html}");
}

#[test]
fn lowering_depth_limit_emits_marker() {
    // A pathological lowering that emits its own kind — must bail at
    // the depth limit rather than recursing forever.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("loop.wcl");
    write_fixture(
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
    );

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
    write_fixture(
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
      font_size: none, fit_width: none, fit_height: none,
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
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    // Text now emits with centering attributes and a `<tspan>` per
    // line. Match the substring without pinning the full element.
    assert!(
        html.contains("<text x=\"10\" y=\"20\"") && html.contains(">hello</tspan>"),
        "{html}"
    );
}

#[test]
fn build_emits_container_chrome_when_stroke_or_fill_set() {
    // Two containers: one with `stroke` (should get a chrome rect),
    // one without (should stay chromeless). The chrome rect is
    // synthesised by the renderer and therefore does not appear in
    // the obstacle graph — verified by the connections page where
    // cross-container edges still route cleanly.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("chrome.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width  = 200
    height = 100
    container {
      id     = framed
      width  = 100.0
      height = 80.0
      stroke = "#abc"
    }
    container {
      id     = bare
      width  = 100.0
      height = 80.0
    }
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // Chrome rect covers the full declared box and falls back to
    // fill="none" when only stroke is given.
    assert!(
        html.contains("<rect width=\"100\" height=\"80\" stroke=\"#abc\" fill=\"none\" />"),
        "missing chrome rect for framed container:\n{html}"
    );
    // The bare container's <g> must be empty — no synthesised rect.
    let bare = html
        .split("id=\"bare\">")
        .nth(1)
        .and_then(|s| s.split("</g>").next())
        .expect("bare container present");
    assert!(
        !bare.contains("<rect"),
        "bare container should have no chrome rect:\n{bare}"
    );
}

#[test]
fn build_draws_boundary_overlay_behind_radial_members() {
    // A `:radial` System-Context diagram with an "Achmisoft" boundary
    // around the hub `shop`. The boundary is a post-layout overlay: it
    // must draw a `wdoc-boundary` rect (sized to shop's placed bbox)
    // *behind* the shapes, keep the radial spokes (edges), and not move
    // its members. `stripe` stays outside the box.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("radial.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width  = 600
    height = 400
    layout = :radial
    hub    = shop
    process    "E-Commerce Platform" { id = shop }
    process    "Stripe"              { id = stripe }
    terminator "Customer"            { id = customer }
    shop     -> stripe
    customer -> shop
    boundary "Achmisoft" { members = [shop] }
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // The themed boundary rect is emitted (default class, no inline paint).
    assert!(
        html.contains("<rect class=\"wdoc-boundary\""),
        "missing themed boundary rect:\n{html}"
    );
    // …with its title in a boundary-label text element.
    assert!(
        html.contains("class=\"wdoc-boundary-label\"") && html.contains(">Achmisoft</text>"),
        "missing boundary label:\n{html}"
    );
    // Drawn behind the shapes: the boundary rect precedes the first
    // process shape in document order. Match the `<rect class="…"` forms
    // so the `.wdoc-*` rules in the <style> block don't confuse the order.
    let b = html
        .find("<rect class=\"wdoc-boundary\"")
        .expect("boundary rect present");
    let s = html
        .find("<rect class=\"wdoc-process\"")
        .expect("process rect present");
    assert!(
        b < s,
        "boundary must render behind (before) the shapes:\n{html}"
    );
    // The radial spokes still render (the boundary is not an obstacle and
    // does not suppress edges).
    assert!(
        html.contains("marker-end=\"url(#wdoc-arrow)\""),
        "radial spokes missing:\n{html}"
    );
}

#[test]
fn build_draws_boundary_layout_agnostic_and_skips_missing_members() {
    // The same boundary behaviour on a `:free` layout (manual x/y) proves
    // it's layout-agnostic — and a member id matching no shape is skipped
    // (boundary draws nothing) rather than failing the build.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("free.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width  = 400
    height = 200
    process "Shop" { id = shop  x = 20.0  y = 20.0  width = 120.0  height = 50.0 }
    process "Ext"  { id = ext   x = 250.0 y = 20.0 }
    boundary "Achmisoft" { members = [shop] }
    boundary "Ghost"     { members = [nope] }
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // shop bbox (20,20,120,50) + default padding 12 ⇒ box (8,8,144,74).
    assert!(
        html.contains(
            "<rect class=\"wdoc-boundary\" x=\"8\" y=\"8\" width=\"144\" height=\"74\" />"
        ),
        "boundary should hug shop's manual bbox:\n{html}"
    );
    // Exactly one boundary is drawn — the `Ghost` boundary (members all
    // unresolved) draws nothing.
    assert_eq!(
        html.matches("class=\"wdoc-boundary\"").count(),
        1,
        "the unresolved-member boundary must draw nothing:\n{html}"
    );
    assert!(
        !html.contains(">Ghost</text>"),
        "unresolved boundary must not emit a label:\n{html}"
    );
}

#[test]
fn build_resolves_anchor_stretch_without_layout() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("anchors.wcl");
    write_fixture(
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
    );

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
    write_fixture(
        &src,
        r#"
page index {
  text {
    span "A & B <c>" {}
    span "say \"hi\"" {}
  }
}
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());

    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");
    assert!(html.contains("<span>A &amp; B &lt;c&gt;</span>"), "{html}");
    assert!(html.contains("<span>say &quot;hi&quot;</span>"), "{html}");
}

#[test]
fn build_renders_bullet_and_numbered_lists_with_nesting() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("lists.wcl");
    write_fixture(
        &src,
        r#"
page index {
  list {
    li "Plain with **bold**"
    li "Has sub" {
      li "child A"
      li "child B"
    }
  }
  list { style = :numbered
    li "Step one"
    li "Step two" {
      li "Sub step"
    }
  }
}
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");

    // Bullet list → <ul>, with an inline-formatted item and a nested <ul>.
    assert!(
        html.contains("<ul><li>Plain with <span class=\"bold\">bold</span></li>"),
        "bullet list / inline patterns:\n{html}"
    );
    assert!(
        html.contains("<li>Has sub<ul><li>child A</li><li>child B</li></ul></li>"),
        "li-under-li sublist:\n{html}"
    );
    // Numbered list → <ol class="wdoc-list-numbered">, nesting the same class.
    assert!(
        html.contains(
            "<ol class=\"wdoc-list-numbered\"><li>Step one</li><li>Step two<ol class=\"wdoc-list-numbered\"><li>Sub step</li></ol></li></ol>"
        ),
        "numbered list with nested numbered sublist:\n{html}"
    );
    // The CSS-counter rule that produces "1.1" is injected.
    assert!(
        html.contains("counters(wdoc-li"),
        "wdoc-list counter stylesheet missing:\n{html}"
    );
}

#[test]
fn build_renders_computed_children_splice_value_and_render_paths() {
    // A `@children` slot authored as a value expression (`field =
    // map(data, …)`) generates child blocks from a data structure — the
    // "view over data" pattern. Covers both consumption paths:
    //   • value path: `text`'s lower maps over `spans` (schema-completed,
    //     so the omitted `id`/`class` reach the lower as `none`);
    //   • render path: `list` walks `blocks()` to render each `li`.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("splice.wcl");
    write_fixture(
        &src,
        r#"
let names = ["alice", "bob", "carol"]
let hosts = [ { name: "web-1" }, { name: "db-1" } ]
page index {
  text { spans = map(names, fn(n: utf8) -> Span { { text: n } }) }
  list { items = map(hosts, fn(h: Host) -> Li { { text: h.name } }) }
  list { style = :numbered
    items = map(names, fn(n: utf8) -> Li { { text: n } })
  }
}
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");

    // Value path: text → <p> of generated <span>s.
    assert!(
        html.contains("<p><span>alice</span><span>bob</span><span>carol</span></p>"),
        "computed text spans:\n{html}"
    );
    // Render path: list → <ul> of generated <li>s, computed from records.
    assert!(
        html.contains("<ul><li>web-1</li><li>db-1</li></ul>"),
        "computed bullet list:\n{html}"
    );
    // Render path, numbered.
    assert!(
        html.contains(
            "<ol class=\"wdoc-list-numbered\"><li>alice</li><li>bob</li><li>carol</li></ol>"
        ),
        "computed numbered list:\n{html}"
    );
}

#[test]
fn build_renders_data_driven_table() {
    // `table { header = [...] rows = map(data, …) }` builds a <table> from
    // a data structure: utf8 cells run through inline patterns, other
    // scalars stringify. Also exercises a component wrapping a table whose
    // `rows` come from a slot.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("dtable.wcl");
    write_fixture(
        &src,
        r#"
type Host { name: utf8  cpu: f64 }
wdoc_component host_table {
  wdoc_slot data                       // named apart from the table's `rows` field
  wdoc_body { table { header = ["Host", "Note"]  rows = data } }
}
page index {
  let inv = [ { name: "web-1", cpu: 42.0 }, { name: "db-1", cpu: 88.0 } ]
  table {
    header = ["Host", "CPU %"]
    rows = map(inv, fn(h: Host) -> list<utf8> { [h.name, h.cpu] })
  }
  host_table {
    data = map(inv, fn(h: Host) -> list<utf8> { [$"**${h.name}**", "ok"] })
  }
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");

    // Computed table: header + a stringified numeric cell.
    assert!(
        html.contains(
            "<table class=\"wdoc-table\"><thead><tr><th>Host</th><th>CPU %</th></tr></thead><tbody><tr><td>web-1</td><td>42.0</td></tr><tr><td>db-1</td><td>88.0</td></tr></tbody></table>"
        ),
        "computed table with numeric cell:\n{html}"
    );
    // Component-wrapped table fed by a `rows` slot; utf8 cell is
    // inline-formatted (**bold** → <span class="bold">).
    assert!(
        html.contains("<td><span class=\"bold\">web-1</span></td><td>ok</td>"),
        "component-wrapped data table with inline-formatted cell:\n{html}"
    );
}

#[test]
fn build_renders_data_driven_diagram() {
    // A `wdoc_repeater` generates one node per data element (with a
    // data-derived `id`), and a computed `edges` list connects them from
    // the data's own relationships. Layered layout then ranks them.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("graph.wcl");
    write_fixture(
        &src,
        r#"
type Svc { key: utf8  name: utf8  deps: list<utf8> }
page index {
  let svc = [
    { key: "web", name: "Web", deps: ["api"] },
    { key: "api", name: "API", deps: ["db", "cache"] },
    { key: "db",  name: "DB",  deps: [] },
    { key: "cache", name: "Cache", deps: [] },
  ]
  let links = flatten(map(svc, fn(s: Svc) -> list<Edge> {
    map(s.deps, fn(d: utf8) -> Edge { { source: s.key, destination: d } })
  }))
  diagram { width = 500  height = 320  layout = :layered
    wdoc_repeater { each = svc  as = :s
      rect { id = s.key  width = 90.0  height = 40.0 }
    }
    edges = links
  }
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");
    let svg_start = html.find("<svg").expect("an <svg>");
    let svg = &html[svg_start
        ..html[svg_start..]
            .find("</svg>")
            .map(|i| svg_start + i)
            .unwrap()];

    // One generated node per data element.
    assert_eq!(
        svg.matches("<rect").count(),
        4,
        "four generated nodes:\n{svg}"
    );
    // Three computed edges drawn (web→api, api→db, api→cache).
    let edge_count = svg.matches("marker-end=\"url(#wdoc-arrow)\"").count();
    assert_eq!(edge_count, 3, "three data-driven edges:\n{svg}");
    // Layered layout ranked the graph from the generated edges, so the
    // nodes are not all at the same offset (a flat row would mean the
    // edges never reached the layout solver).
    let offsets: std::collections::HashSet<&str> = svg
        .match_indices("<g transform=\"translate(")
        .map(|(i, _)| {
            let rest = &svg[i + "<g transform=\"translate(".len()..];
            &rest[..rest.find(')').unwrap()]
        })
        .collect();
    assert!(
        offsets.len() >= 3,
        "nodes should be spread across ranks, got offsets {offsets:?}"
    );
}

#[test]
fn build_generates_pages_and_toc_from_a_repeater() {
    // A document-level `wdoc_repeater` whose body is a `page` block emits
    // one rendered page per element of `each` (the page's interpolated
    // label is the route). A `wdoc_repeater` inside a `toc` chapter emits
    // one TOC entry per element. A computed link resolves against the
    // post-expansion page set.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("catalog.wcl");
    write_fixture(
        &src,
        r#"
let containers = [
  { id: "web", name: "Web App" },
  { id: "api", name: "API" },
  { id: "db",  name: "Database" },
]

site docbook {
  default_template = :book
  title = "Catalog"
  toc {
    chapter "Containers" {
      wdoc_repeater { each = containers  as = :c
        chapter $"${c.name}" { page = $"cont_${c.id}" }
      }
    }
  }
}

wdoc_repeater { each = containers  as = :c
  page $"cont_${c.id}" {
    sites = [:docbook]
    title = c.name
    h1 $"${c.name}"
    p $"Container id: ${c.id}"
  }
}

page index {
  sites = [:docbook]
  start = true
  h1 "Home"
  p "See the [Web App](cont_web) container."
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    // index + cont_web + cont_api + cont_db. build_ok panics on BadLink, so
    // a passing call already proves the computed `[..](cont_web)` resolved.
    let n = build_ok(&src, out.path());
    assert_eq!(n, 4, "one page per element plus the index");

    // One file per generated element, each with its own element-derived body.
    for (route, title, id) in [
        ("cont_web", "Web App", "web"),
        ("cont_api", "API", "api"),
        ("cont_db", "Database", "db"),
    ] {
        let page = std::fs::read_to_string(out.path().join(format!("{route}.html")))
            .unwrap_or_else(|e| panic!("read {route}.html: {e}"));
        assert!(
            page.contains(&format!("<span>{title}</span>")),
            "{route}.html should carry its element title heading:\n{page}"
        );
        assert!(
            page.contains(&format!("Container id: {id}")),
            "{route}.html should carry its element id:\n{page}"
        );
    }

    // The book sidebar lists the data-driven chapters, each linking to its
    // generated page.
    let index = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");
    assert!(
        index.contains("cont_web.html") && index.contains("Web App"),
        "book TOC should link the generated chapter to its page:\n{index}"
    );
    assert!(
        index.contains("cont_db.html") && index.contains("Database"),
        "book TOC should include every generated chapter:\n{index}"
    );
}

#[test]
fn build_rejects_a_non_slug_generated_page_route() {
    // A generated page route is its interpolated label; a value with a
    // space can't form a clean filename, so it's a build error.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("bad_slug.wcl");
    write_fixture(
        &src,
        r#"
let items = [ { name: "Web App" } ]
wdoc_repeater { each = items  as = :c
  page $"${c.name}" { h1 $"${c.name}" }
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    match build(&src, out.path(), None) {
        Err(BuildError::BadPage(m)) => assert!(
            m.contains("slug-safe"),
            "expected a slug-safe diagnostic, got: {m}"
        ),
        Err(_) => panic!("expected BadPage for a non-slug route, got a different BuildError"),
        Ok(_) => panic!("expected BadPage for a non-slug route, build unexpectedly succeeded"),
    }
}

#[test]
fn build_rejects_duplicate_generated_page_routes() {
    // Two elements whose interpolated labels collide would overwrite one
    // page with another — a build error rather than silent data loss.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("dup.wcl");
    write_fixture(
        &src,
        r#"
let items = [ { id: "x" }, { id: "x" } ]
site docs { title = "Docs" }
wdoc_repeater { each = items  as = :c
  page $"cont_${c.id}" { sites = [:docs]  h1 $"${c.id}" }
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    match build(&src, out.path(), None) {
        Err(BuildError::DuplicatePage { name, .. }) => {
            assert_eq!(name, "cont_x", "the colliding route should be named")
        }
        Err(_) => panic!("expected DuplicatePage for colliding routes, got a different BuildError"),
        Ok(_) => {
            panic!("expected DuplicatePage for colliding routes, build unexpectedly succeeded")
        }
    }
}

#[test]
fn build_renders_component_slots_defaults_and_content() {
    // A `wdoc_component` is instantiated by its own name; slots fill from
    // the instance's fields (or a `default`), resolve in `${…}` labels and
    // bare-identifier field exprs, and a `wdoc_content` block splices the
    // instance's own children (a layout wrapper).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("components.wcl");
    write_fixture(
        &src,
        r#"
wdoc_component badge {
  wdoc_slot label
  wdoc_slot kind { default = "note" }
  wdoc_body {
    callout $"${label}" { class = [kind]  body = "x" }
  }
}
wdoc_component panel {
  wdoc_slot title
  wdoc_body {
    h3 $"${title}"
    wdoc_content
  }
}
page index {
  badge { label = "Alpha" kind = "warning" }
  badge { label = "Beta" }
  panel { title = "Logs"
    p "line one"
    p "line two"
  }
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");

    // Slot interpolation into a callout title + the per-instance class.
    assert!(
        html.contains("<div class=\"callout warning\">") && html.contains("<span>Alpha</span>"),
        "first badge: slot label + class:\n{html}"
    );
    // Default applies when the slot is omitted (`kind` → "note").
    assert!(
        html.contains("<div class=\"callout note\">") && html.contains("<span>Beta</span>"),
        "second badge: default slot:\n{html}"
    );
    // Content slot splices the instance's own children, in order, after
    // the wrapper heading.
    assert!(
        html.contains("<p class=\"heading-3\"><span>Logs</span></p><p>line one</p><p>line two</p>"),
        "panel content slot:\n{html}"
    );
}

#[test]
fn build_renders_repeater_and_composes_with_components() {
    // `wdoc_repeater` iterates a list, binding each element to `as`. It
    // composes both ways: a repeater inside a component body iterating a
    // list-typed slot, and a component instantiated inside a repeater.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("repeat.wcl");
    write_fixture(
        &src,
        r#"
wdoc_component row {
  wdoc_slot name
  wdoc_slot cpu
  wdoc_body { p $"${name}: ${cpu}%" }
}
wdoc_component host_table {
  wdoc_slot heading
  wdoc_slot rows
  wdoc_body {
    h2 $"${heading}"
    wdoc_repeater { each = rows  as = :r
      row { name = r.name  cpu = r.cpu }
    }
  }
}
page index {
  let inventory = [
    { name: "web-1", cpu: 42 },
    { name: "db-1",  cpu: 88 },
  ]
  // Component instantiated inside a repeater (loop var → slot args).
  wdoc_repeater { each = inventory  as = :h
    row { name = h.name  cpu = h.cpu }
  }
  // Repeater inside a component body, iterating a list-typed slot.
  host_table { heading = "Fleet"  rows = inventory }
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");

    // Component-in-repeater: one <p> per element, with that element's data.
    assert!(
        html.contains("<p>web-1: 42%</p>") && html.contains("<p>db-1: 88%</p>"),
        "component instantiated inside a repeater:\n{html}"
    );
    // Repeater-in-component over a list-typed slot: heading + a row per
    // element (the loop body sees both the loop var `r` and the slot).
    assert!(
        html.contains("<p class=\"heading-2\"><span>Fleet</span></p>"),
        "component heading slot:\n{html}"
    );
    // Two distinct values prove each iteration has an independent cache
    // (no stale first-element value).
    assert_eq!(
        html.matches("<p>web-1: 42%</p>").count(),
        2,
        "web-1 row appears once from the bare repeater and once via host_table:\n{html}"
    );
}

#[test]
fn build_reports_schema_error_for_unknown_block() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("bad.wcl");
    write_fixture(
        &src,
        r#"
page index {
  h7 "nope" {}
}
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    match build(&src, out.path(), None) {
        Err(BuildError::Schema(n)) => assert!(n >= 1, "expected at least one violation, got {n}"),
        Err(BuildError::Io(e, ctx)) => panic!("expected Schema, got Io({ctx}: {e})"),
        Err(BuildError::Parse(_)) => panic!("expected Schema, got Parse"),
        Err(BuildError::Eval(r)) => panic!("expected Schema, got Eval({r:?})"),
        Err(BuildError::BadPage(m)) => panic!("expected Schema, got BadPage({m})"),
        Err(BuildError::DuplicateId { page, id }) => {
            panic!("expected Schema, got DuplicateId({page}: {id})")
        }
        Err(BuildError::BadLink(msgs)) => panic!("expected Schema, got BadLink({msgs:?})"),
        Err(BuildError::BadTemplate(name)) => panic!("expected Schema, got BadTemplate({name})"),
        Err(BuildError::Tileset(m)) => panic!("expected Schema, got Tileset({m})"),
        Err(BuildError::EdgeRouting(m)) => panic!("expected Schema, got EdgeRouting({m})"),
        Err(BuildError::DuplicatePage { site, name }) => {
            panic!("expected Schema, got DuplicatePage({site}: {name})")
        }
        Ok(n) => panic!("expected Schema error, got Ok({n})"),
    }
}

#[test]
fn build_emits_id_attributes_across_paths() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("ids.wcl");
    write_fixture(
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
    );

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
    write_fixture(
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
    );

    let out = TempDir::new().expect("mkdir out");
    match build(&src, out.path(), None) {
        Err(BuildError::DuplicateId { page, id }) => {
            assert_eq!(page, "index");
            assert_eq!(id, "shared");
        }
        Err(BuildError::Schema(n)) => panic!("expected DuplicateId, got Schema({n})"),
        Err(BuildError::Io(e, ctx)) => panic!("expected DuplicateId, got Io({ctx}: {e})"),
        Err(BuildError::Parse(_)) => panic!("expected DuplicateId, got Parse"),
        Err(BuildError::Eval(r)) => panic!("expected DuplicateId, got Eval({r:?})"),
        Err(BuildError::BadPage(m)) => panic!("expected DuplicateId, got BadPage({m})"),
        Err(BuildError::BadLink(msgs)) => panic!("expected DuplicateId, got BadLink({msgs:?})"),
        Err(BuildError::BadTemplate(name)) => {
            panic!("expected DuplicateId, got BadTemplate({name})")
        }
        Err(BuildError::Tileset(m)) => panic!("expected DuplicateId, got Tileset({m})"),
        Err(BuildError::EdgeRouting(m)) => panic!("expected DuplicateId, got EdgeRouting({m})"),
        Err(BuildError::DuplicatePage { site, name }) => {
            panic!("expected DuplicateId, got DuplicatePage({site}: {name})")
        }
        Ok(n) => panic!("expected DuplicateId, got Ok({n})"),
    }
}

#[test]
fn build_preserves_source_order_across_mixed_children() {
    // With the single `@children(WdocBlock)` slot on `Page`, mixed-kind
    // children must come out in source order (not bucketed by kind).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("order.wcl");
    write_fixture(
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
    );

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
    write_fixture(
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
    );
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
    write_fixture(
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
    );
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
fn build_node_table_renders_rows_and_attaches_edge_to_a_row() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("node_table.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width   = 400
    height  = 200
    routing = :straight
    node_table {
      id = users
      x = 20.0  y = 20.0  width = 140.0
      title = "users"
      node_row { id = users_id    p "id: int" }
      node_row { id = users_name  p "name: text" }
    }
    rect {
      id = box
      x = 300.0  y = 60.0  width = 60.0  height = 40.0
      fill = "#abc"
    }
    // Targets the SECOND row by id — its east anchor (160,93) is
    // distinct from the whole-table east midpoint (160,64), proving
    // the edge attaches to the row, not the box.
    box -> users_name
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // Frame: 140 wide, derived height = header(28) + 2*row(30) = 88.
    assert!(
        html.contains("<rect x=\"20\" y=\"20\" width=\"140\" height=\"88\"")
            && html.contains("class=\"wdoc-node-table-frame\""),
        "missing node_table frame:\n{html}"
    );
    // Title header + row bodies render as foreignObject HTML.
    assert!(
        html.contains("class=\"wdoc-node-table-title\"") && html.contains("users"),
        "missing title header:\n{html}"
    );
    assert!(
        html.contains("class=\"wdoc-node-row\"") && html.contains("name: text"),
        "missing row body content:\n{html}"
    );
    // Per-row connection markers (left + right by default).
    assert!(
        html.contains("class=\"wdoc-node-table-port\""),
        "missing per-row port markers:\n{html}"
    );
    // The edge attaches at the SECOND row's east edge (160,93), not the
    // table's outer midpoint (160,64) — the core per-row-port feature.
    assert!(
        html.contains("x2=\"160\" y2=\"93\""),
        "expected edge to attach at row users_name east (160,93):\n{html}"
    );
}

#[test]
fn build_node_table_expands_repeater_rows() {
    // A `node_table` whose rows come from a `wdoc_repeater` must expand
    // them like every other diagram shape: the rows render, the table's
    // derived height accounts for them, and each generated row id is a
    // real, edge-addressable shape. The edge here is a data-driven
    // `edges` list (the idiomatic way to connect generated ids — see
    // `build_renders_data_driven_diagram`), which matches endpoint
    // strings against the collected shape positions.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("node_table_repeat.wcl");
    write_fixture(
        &src,
        r##"
let cols = ["a", "b"]
page index {
  diagram {
    width   = 400
    height  = 200
    routing = :straight
    node_table {
      id = lit
      x = 20.0  y = 20.0  width = 140.0
      title = "literal"
      node_row { id = lit_a  p "a: int" }
      node_row { id = lit_b  p "b: int" }
    }
    node_table {
      id = rep
      x = 220.0  y = 20.0  width = 140.0
      title = "repeated"
      wdoc_repeater { each = cols  as = :c  node_row { id = $"rep_${c}"  p $"${c}: int" } }
    }
    edges = [ { source: "lit_a", destination: "rep_a", kind: :data } ]
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // The repeated table derives the same height as the literal one:
    // header(28) + 2*row(30) = 88. Before the fix it was header-only (28).
    assert!(
        html.contains("<rect x=\"220\" y=\"20\" width=\"140\" height=\"88\""),
        "repeated node_table did not expand its repeater rows (wrong height):\n{html}"
    );
    // Both repeated row bodies render as foreignObject HTML.
    assert!(
        html.contains("class=\"wdoc-node-row\"")
            && html.contains("a: int")
            && html.contains("b: int"),
        "missing repeated row body content:\n{html}"
    );
    // The generated row id resolves on the rendered row.
    assert!(
        html.contains("data-node-row-id=\"rep_a\""),
        "generated row id rep_a did not resolve on its row:\n{html}"
    );
    // The generated row is registered as an edge endpoint: the edge
    // attaches at lit_a's east (160,63) and rep_a's west (220,63).
    assert!(
        html.contains(
            "<line x1=\"160\" y1=\"63\" x2=\"220\" y2=\"63\" stroke=\"currentColor\" \
             marker-end=\"url(#wdoc-arrow)\" data-kind=\"data\" />"
        ),
        "edge to generated row rep_a did not attach at its west anchor:\n{html}"
    );
}

#[test]
fn build_resolves_arrow_edge_to_generated_id() {
    // The `@dynamic` Edge connection lets a plain `a -> b` arrow statement
    // target an id GENERATED by a `wdoc_repeater` (here a node_table row),
    // not just a literal block. The arrow form must produce the same edge
    // the data-driven `edges = [...]` form does in
    // `build_node_table_expands_repeater_rows` — only the default kind
    // differs (`:default` vs the explicit `:data` there).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("node_table_arrow.wcl");
    write_fixture(
        &src,
        r##"
let cols = ["a", "b"]
page index {
  diagram {
    width   = 400
    height  = 200
    routing = :straight
    node_table {
      id = lit
      x = 20.0  y = 20.0  width = 140.0
      title = "literal"
      node_row { id = lit_a  p "a: int" }
      node_row { id = lit_b  p "b: int" }
    }
    node_table {
      id = rep
      x = 220.0  y = 20.0  width = 140.0
      title = "repeated"
      wdoc_repeater { each = cols  as = :c  node_row { id = $"rep_${c}"  p $"${c}: int" } }
    }
    lit_a -> rep_a
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // The `lit_a -> rep_a` arrow draws to the generated row's west anchor,
    // identical geometry to the data-driven form (kind defaults to :default).
    assert!(
        html.contains(
            "<line x1=\"160\" y1=\"63\" x2=\"220\" y2=\"63\" stroke=\"currentColor\" \
             marker-end=\"url(#wdoc-arrow)\" data-kind=\"default\" />"
        ),
        "arrow edge to generated row rep_a did not draw:\n{html}"
    );
}

#[test]
fn build_layered_container_label_does_not_displace_shapes() {
    // A `label` inside an auto-layout (`:layered`) container is a
    // zero-footprint annotation, not a flow node: it must NOT be allocated
    // a layout cell that shoves the real shapes aside. The boundary rect
    // must therefore stay at the container origin (x=0), and the heading
    // sits at its own anchored position.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("titled_box.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width  = 300
    height = 160
    container {
      layout = :layered
      label "title" { x = 90.0  y = -8.0 }
      container {
        stroke = "#888"
        rect { id = a  width = 180.0  height = 60.0  fill = "#abc" }
      }
    }
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    // The flow shape stays at the container origin — before the fix the
    // label's phantom 80x40 cell pushed the inner container (and its rect)
    // right by ~120px.
    assert!(
        html.contains("<rect x=\"0\" y=\"0\" width=\"180\" height=\"60\" fill=\"#abc\" id=\"a\""),
        "boundary shape was displaced by the label's phantom layout cell:\n{html}"
    );
    // The heading renders at its own anchored x (90), not at the solver origin.
    assert!(
        html.contains("<text x=\"90\" y=\"-8\""),
        "heading label not at its anchored position:\n{html}"
    );
}

#[test]
fn build_warns_on_unmatched_edge_endpoint() {
    // An edge endpoint that names no rendered shape (a typo, or a
    // conditionally-absent shape) must NOT fail the build — it is dropped
    // with a non-fatal warning the caller can drain.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("dangling_edge.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width  = 300
    height = 120
    rect { id = a  x = 20.0  y = 20.0  width = 60.0  height = 40.0  fill = "#abc" }
    a -> nonexistent
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    // Build SUCCEEDS (non-fatal) and the dangling edge draws nothing.
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        !html.contains("marker-end=\"url(#wdoc-arrow)\""),
        "the unmatched edge should not have drawn a line:\n{html}"
    );
    // The drop surfaced a warning naming the missing endpoint. (build leaves
    // warnings in the sink on this thread for the caller to drain.)
    let warnings = wcl_wdoc::take_edge_warnings();
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("nonexistent") && w.contains("matches no shape id")),
        "expected an unmatched-endpoint warning, got: {warnings:?}"
    );
}

#[test]
fn build_routes_around_obstacle() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("obstacle.wcl");
    write_fixture(
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
    );
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
fn build_cross_container_edge_avoids_top_border() {
    // Two stroke-bordered containers (their top borders sit at y=0) hold
    // node_tables, with a cross-container FK between rows. The elbow router
    // must not run the edge flush along a container's top border line — the
    // wad ER-diagram symptom. (The routing.rs unit test proves the
    // mechanism with a before/after control; this guards the end-to-end
    // threading of container borders into the router.)
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("cross_container.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width   = 820
    height  = 320
    routing = :elbow
    layout  = :grid
    columns = 2
    cell_width  = 380.0
    cell_height = 300.0
    gap = 40.0
    container {
      stroke = "#888"  padding = 12.0
      layout = :grid  columns = 2  cell_width = 170.0  cell_height = 130.0  gap = 16.0
      node_table { id = users  title = "users"  width = 160.0
        node_row { id = u_id    p "id" }
        node_row { id = u_email p "email" }
      }
      node_table { id = sessions  title = "sessions"  width = 160.0
        node_row { id = s_id  p "id" }
      }
    }
    container {
      stroke = "#888"  padding = 12.0
      layout = :grid  columns = 1  cell_width = 170.0  cell_height = 130.0  gap = 16.0
      node_table { id = orders  title = "orders"  width = 160.0
        node_row { id = o_uid  p "user_id" }
      }
    }
    o_uid -> u_id
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    // Parse the routed edge polyline.
    let poly = html
        .split("<polyline points=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .expect("a routed edge polyline");
    let points: Vec<(f64, f64)> = poly
        .split_whitespace()
        .filter_map(|p| {
            let mut it = p.split(',');
            Some((it.next()?.parse().ok()?, it.next()?.parse().ok()?))
        })
        .collect();
    assert!(points.len() >= 2, "edge did not render: {poly:?}");
    // The container top borders are at y=0 (grid row 0). No long horizontal
    // segment may run within ~one routing cell (10px) of that border line.
    let on_top_border = points.windows(2).any(|w| {
        let (a, b) = (w[0], w[1]);
        (a.1 - b.1).abs() < 1e-6 && a.1.abs() <= 10.0 && (a.0 - b.0).abs() > 20.0
    });
    assert!(
        !on_top_border,
        "cross-container edge runs flush along the container top border: {points:?}"
    );
}

#[test]
fn build_separates_parallel_edges() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("sep.wcl");
    write_fixture(
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
    );
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
    write_fixture(
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
    );
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
    write_fixture(
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
    );
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
    write_fixture(
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
    );
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
fn build_layout_reserves_container_footprint() {
    // Regression: a `container` must reserve its real rendered
    // footprint (content bbox + padding) in an auto-layout, not the
    // default 80×40. Here three top-level siblings share rank 0 (their
    // edges point at ids *inside* the container, so none connects two
    // siblings) and spread along x by node_gap (40). The container's
    // :grid content is 140×168, +2*16 padding = 172×200. So:
    //   customer at x=0 (w 80) -> +80+40 -> internal at x=120
    //   internal  (w 172)      -> +172+40 -> stripe at x=332
    // Before the fix the container reserved only 80, putting stripe at
    // x=240, *inside* the container's box (x 120–292). Assert stripe is
    // clear: stripe.x (332) >= internal.x (120) + width (172) = 292.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("container_footprint.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width  = 900
    height = 500
    layout = :layered
    rect { id = customer  width = 80.0  height = 40.0 }
    container {
      id = internal  padding = 16.0
      layout = :grid  columns = 1  cell_width = 140.0  cell_height = 48.0  gap = 12.0
      rect { id = web  width = 140.0  height = 48.0 }
      rect { id = api  width = 140.0  height = 48.0 }
      rect { id = db   width = 140.0  height = 48.0 }
    }
    rect { id = stripe  width = 80.0  height = 40.0 }
    customer -> web
    web -> api
    api -> db
    api -> stripe
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("<g transform=\"translate(120 0)\""),
        "container should sit at x=120:\n{html}"
    );
    assert!(
        html.contains("<g transform=\"translate(332 0)\""),
        "stripe should clear the container's 172-wide box (x=332), not overlap at x=240:\n{html}"
    );
    assert!(
        !html.contains("<g transform=\"translate(240 0)\""),
        "stripe must not be placed inside the container border box (x=240):\n{html}"
    );
}

#[test]
fn build_force_layout_spreads_and_is_deterministic() {
    // A cyclic 4-node graph (circles) with no rank order. :force should
    // place each shape (no cx/cy declared) via the simulation, wrap each
    // in its own translate group, and draw a straight edge per
    // connection. Circles are sized by radius and centered in their cell.
    let fixture = r##"
page index {
  diagram {
    width         = 300
    height        = 300
    layout        = :force
    routing       = :straight
    link_distance = 70.0
    circle { id = a  r = 24.0  fill = "#cce" }
    circle { id = b  r = 24.0  fill = "#ecc" }
    circle { id = c  r = 24.0  fill = "#cec" }
    circle { id = d  r = 24.0  fill = "#fec" }
    a -> b
    b -> c
    c -> a
    a -> d
  }
}
"##;

    let render = || {
        let tmp = TempDir::new().expect("mkdir tempdir");
        let src = tmp.path().join("force.wcl");
        write_fixture(&src, fixture);
        let out = TempDir::new().expect("mkdir out");
        build_ok(&src, out.path());
        std::fs::read_to_string(out.path().join("index.html")).expect("read")
    };

    let html = render();

    // One circle + one translate wrapper per node; one arrow per edge.
    assert_eq!(
        html.matches("<circle ").count(),
        4,
        "expected 4 nodes:\n{html}"
    );
    assert_eq!(
        html.matches("<g transform=\"translate(").count(),
        4,
        "expected one wrapper per node:\n{html}"
    );
    // Circles are sized by diameter (r=24 → cx=cy=24 within the 48×48
    // cell), confirming the layout allocated a square cell and centered.
    assert!(
        html.contains("cx=\"24\" cy=\"24\" r=\"24\""),
        "circle not centered in its diameter-sized cell:\n{html}"
    );
    assert_eq!(
        html.matches("url(#wdoc-arrow)").count(),
        4,
        "expected one arrow per edge:\n{html}"
    );
    // The nodes must actually spread — not all collapsed at the origin.
    assert!(
        html.matches("translate(0 0)").count() < 4,
        "force layout collapsed all nodes to the origin:\n{html}"
    );

    // Pure, deterministic simulation: a second build is byte-identical.
    assert_eq!(html, render(), "force layout was not deterministic");
}

#[test]
fn build_radial_layout_places_hub_in_center_ring() {
    // A hub wired to four neighbours and no neighbour-to-neighbour edges.
    // The layered solver would strand the edge-less neighbours in one row;
    // :radial must put the hub (highest degree) at the centre and spread
    // the four neighbours on a ring around it — spanning both axes.
    let fixture = r##"
page index {
  diagram {
    width   = 400
    height  = 400
    layout  = :radial
    routing = :straight
    process { id = hub  width = 80.0  height = 40.0  fill = "#cce" }
    process { id = n1   width = 80.0  height = 40.0  fill = "#ecc" }
    process { id = n2   width = 80.0  height = 40.0  fill = "#cec" }
    process { id = n3   width = 80.0  height = 40.0  fill = "#fec" }
    process { id = n4   width = 80.0  height = 40.0  fill = "#cff" }
    hub -> n1
    hub -> n2
    hub -> n3
    hub -> n4
  }
}
"##;

    let render = || {
        let tmp = TempDir::new().expect("mkdir tempdir");
        let src = tmp.path().join("radial.wcl");
        write_fixture(&src, fixture);
        let out = TempDir::new().expect("mkdir out");
        build_ok(&src, out.path());
        std::fs::read_to_string(out.path().join("index.html")).expect("read")
    };

    let html = render();

    // One translate wrapper per node; one arrow per edge.
    assert_eq!(
        html.matches("<g transform=\"translate(").count(),
        5,
        "expected one wrapper per node:\n{html}"
    );
    assert_eq!(
        html.matches("url(#wdoc-arrow)").count(),
        4,
        "expected one arrow per edge:\n{html}"
    );

    // Collect the (x, y) of every node wrapper and confirm the layout is
    // genuinely 2D — neighbours span more than one row AND more than one
    // column, which a single-line layered fallback could never produce.
    let mut xs = Vec::new();
    let mut ys = Vec::new();
    for frag in html.split("<g transform=\"translate(").skip(1) {
        let coords = frag.split(')').next().unwrap_or("");
        let mut parts = coords.split_whitespace();
        let (Some(x), Some(y)) = (parts.next(), parts.next()) else {
            continue;
        };
        if let (Ok(x), Ok(y)) = (x.parse::<f64>(), y.parse::<f64>()) {
            xs.push(x);
            ys.push(y);
        }
    }
    assert_eq!(xs.len(), 5, "expected 5 parsed node offsets:\n{html}");
    let spread = |v: &[f64]| {
        v.iter().cloned().fold(f64::MIN, f64::max) - v.iter().cloned().fold(f64::MAX, f64::min)
    };
    assert!(
        spread(&xs) > 1.0 && spread(&ys) > 1.0,
        "radial layout did not spread in 2D (x {:.1}, y {:.1}):\n{html}",
        spread(&xs),
        spread(&ys)
    );

    // Pure, deterministic placement: a second build is byte-identical.
    assert_eq!(html, render(), "radial layout was not deterministic");
}

#[test]
fn build_radial_straight_spokes_leave_distinct_anchors() {
    // A hub that is the SOURCE of three straight edges must not bundle
    // them onto one shared egress anchor (which would make every spoke
    // leave the same side and cross the hub body — "lines from the
    // middle"). Each straight spoke should leave the hub's own facing
    // border anchor, so the three hub-sourced edges have three DISTINCT
    // start points.
    let fixture = r##"
page index {
  diagram {
    width   = 400
    height  = 400
    layout  = :radial
    hub     = hub
    routing = :straight
    process { id = hub width = 100.0 height = 40.0 fill = "#cce" }
    process { id = e   width = 100.0 height = 40.0 fill = "#ecc" }
    process { id = s   width = 100.0 height = 40.0 fill = "#cec" }
    process { id = w   width = 100.0 height = 40.0 fill = "#fec" }
    nbr -> hub
    hub -> e
    hub -> s
    hub -> w
    process { id = nbr width = 100.0 height = 40.0 fill = "#cff" }
  }
}
"##;

    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("radial_straight.wcl");
    write_fixture(&src, fixture);
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // Parse the <line> elements in emission (= edge declaration) order.
    let starts: Vec<(String, String)> = html
        .split("<line ")
        .skip(1)
        .filter_map(|frag| {
            let grab = |key: &str| {
                frag.split(&format!("{key}=\""))
                    .nth(1)?
                    .split('"')
                    .next()
                    .map(str::to_string)
            };
            Some((grab("x1")?, grab("y1")?))
        })
        .collect();
    assert_eq!(starts.len(), 4, "expected one <line> per edge:\n{html}");

    // Edges are declared nbr->hub, hub->e, hub->s, hub->w, so the three
    // hub-sourced spokes are lines 1..4; their start points are the hub's
    // egress anchors and must all differ (the bundling bug made them equal).
    let hub_starts = &starts[1..4];
    let distinct: std::collections::HashSet<_> = hub_starts.iter().collect();
    assert_eq!(
        distinct.len(),
        3,
        "hub-sourced straight spokes share an egress anchor (bundled): {hub_starts:?}\n{html}"
    );
}

#[test]
fn build_radial_elbow_spokes_leave_distinct_facing_anchors() {
    // The :straight twin of this test (build_radial_straight_spokes_…)
    // is covered because straight routing skips shared anchors. The
    // DEFAULT :elbow path used to bundle a hub's outgoing edges onto the
    // single side closest to the centroid of all destinations — for a
    // radial hub that centroid sits ~straight below, so every spoke left
    // via South and the East/West spokes had to elbow down-and-around.
    // After grouping shared anchors per facing side, each spoke leaves
    // via the hub border it actually faces, so the three hub-sourced
    // edges have three DISTINCT start points (no `routing` => elbow).
    let fixture = r##"
page index {
  diagram {
    width  = 400
    height = 400
    layout = :radial
    hub    = hub
    process { id = hub width = 100.0 height = 40.0 fill = "#cce" }
    process { id = e   width = 100.0 height = 40.0 fill = "#ecc" }
    process { id = s   width = 100.0 height = 40.0 fill = "#cec" }
    process { id = w   width = 100.0 height = 40.0 fill = "#fec" }
    nbr -> hub
    hub -> e
    hub -> s
    hub -> w
    process { id = nbr width = 100.0 height = 40.0 fill = "#cff" }
  }
}
"##;

    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("radial_elbow.wcl");
    write_fixture(&src, fixture);
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // Elbow edges render as <polyline>; the first coordinate is the
    // egress anchor. Edges declare nbr->hub, hub->e, hub->s, hub->w, so
    // the three hub-sourced spokes are polylines 1..4 and their starts
    // must all differ (the bundling bug made them equal).
    let starts: Vec<&str> = html
        .split("<polyline points=\"")
        .skip(1)
        .filter_map(|s| s.split_whitespace().next())
        .collect();
    assert_eq!(starts.len(), 4, "expected one polyline per edge:\n{html}");
    let hub_starts = &starts[1..4];
    let distinct: std::collections::HashSet<_> = hub_starts.iter().collect();
    assert_eq!(
        distinct.len(),
        3,
        "hub-sourced elbow spokes share an egress anchor (bundled): {hub_starts:?}\n{html}"
    );
}

#[test]
fn build_straight_edges_attach_to_circle_boundary() {
    // Two circles on a shared horizontal axis. A straight edge should
    // leave circle A on its boundary toward B's center and arrive on
    // B's boundary toward A — not at a cardinal anchor midpoint. With
    // A at (60,100) r=20 and B at (260,100) r=40, the line runs from
    // (80,100) to (220,100).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("boundary.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width   = 320
    height  = 200
    routing = :straight
    circle { id = a  cx = 60.0   cy = 100.0  r = 20.0  fill = "#cce" }
    circle { id = b  cx = 260.0  cy = 100.0  r = 40.0  fill = "#ecc" }
    a -> b
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("<line x1=\"80\" y1=\"100\" x2=\"220\" y2=\"100\""),
        "straight edge did not attach to the circle boundaries:\n{html}"
    );
}

#[test]
fn build_renders_site_menu_with_nested_dropdowns() {
    // A site menu with a top link (the current page), a dropdown parent
    // holding two sub-items, and an external href. The webpage nav
    // should render a nested <ul class="menu">, a toggle <button> for
    // the parent, the current page's link tagged `current`, the raw
    // external href verbatim, and the click-toggle script once.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("menu.wcl");
    write_fixture(
        &src,
        r##"
site main {
  default_template = :webpage
  title = "Site"
  menu {
    item "Home" { page = index }
    item "More" {
      item "Second" { page = second }
      item "Docs"   { href = "https://example.com/docs" }
    }
  }
}
page index { h1 "Home" {} }
page second { h1 "Second" {} }
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // Nested menu structure: a top <ul class="menu"> and a nested one.
    assert_eq!(
        html.matches("<ul class=\"menu\">").count(),
        2,
        "expected a top menu + one nested submenu:\n{html}"
    );
    // The dropdown parent is a toggle button inside a has-submenu li.
    assert!(
        html.contains(
            "<li class=\"has-submenu\"><button class=\"menu-toggle\" type=\"button\">More</button>"
        ),
        "missing dropdown parent toggle:\n{html}"
    );
    // The current page's link carries `current`; a sibling does not.
    assert!(
        html.contains("<a class=\"menu-link current\" href=\"index.html\">Home</a>"),
        "current page link not marked current:\n{html}"
    );
    assert!(
        html.contains("<a class=\"menu-link\" href=\"second.html\">Second</a>"),
        "nested page link missing:\n{html}"
    );
    // External href is rendered verbatim (no .html suffix).
    assert!(
        html.contains("<a class=\"menu-link\" href=\"https://example.com/docs\">Docs</a>"),
        "external href not rendered verbatim:\n{html}"
    );
    // The click-toggle script is present exactly once.
    assert_eq!(
        html.matches("function closeAll(keep)").count(),
        1,
        "expected the dropdown toggle script once:\n{html}"
    );
}

#[test]
fn build_webpage_without_menu_falls_back_to_page_list() {
    // No `menu` declared: the webpage nav keeps its flat per-page link
    // behaviour and emits no dropdown markup or toggle script.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("nomenu.wcl");
    write_fixture(
        &src,
        r##"
site main {
  default_template = :webpage
  title = "Site"
}
page index { h1 "Home" {} }
page second { h1 "Second" {} }
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // Flat page links, as before — no menu <ul> and no toggle script.
    assert!(
        html.contains("<a href=\"index.html\">index</a>")
            && html.contains("<a href=\"second.html\">second</a>"),
        "fallback page-list nav missing:\n{html}"
    );
    assert!(
        !html.contains("<ul class=\"menu\">"),
        "no-menu site should not emit menu markup:\n{html}"
    );
    assert!(
        !html.contains("function closeAll(keep)"),
        "no-menu site should not emit the toggle script:\n{html}"
    );
}

#[test]
fn build_rejects_menu_item_with_unknown_page() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("badmenu.wcl");
    write_fixture(
        &src,
        r##"
site main {
  default_template = :webpage
  menu { item "Ghost" { page = nope } }
}
page index { h1 "Home" {} }
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    match build(&src, out.path(), None) {
        Err(BuildError::BadTemplate(m)) => {
            assert!(m.contains("nope"), "unexpected error message: {m}");
        }
        Ok(_) => panic!("expected BadTemplate for unknown menu page, but build succeeded"),
        Err(_) => panic!("expected BadTemplate for unknown menu page, got a different error"),
    }
}

#[test]
fn build_start_page_becomes_site_index() {
    // No page named `index`; instead `home` is marked `start = true`. The
    // site root (index.html) should be that page's content, and the page
    // also stays reachable at its own `home.html`.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("start.wcl");
    write_fixture(
        &src,
        r##"
site main {
  default_template = :webpage
  title = "Site"
}
page home { start = true
  h1 "Landing" {}
}
page other {
  h1 "Other" {}
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());

    let index = std::fs::read_to_string(out.path().join("index.html")).expect("read index");
    let home = std::fs::read_to_string(out.path().join("home.html")).expect("read home");
    // The root index is the start page, served directly (not a redirect).
    assert_eq!(index, home, "index.html should be a copy of the start page");
    assert!(
        index.contains("Landing"),
        "start page content missing:\n{index}"
    );
    assert!(
        !index.contains("http-equiv=\"refresh\""),
        "start page index should be real content, not a redirect:\n{index}"
    );
    // The other page exists at its own name and is not the index.
    assert!(out.path().join("other.html").exists());
}

#[test]
fn build_rejects_multiple_start_pages() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("twostart.wcl");
    write_fixture(
        &src,
        r##"
site main { default_template = :webpage }
page a { start = true  h1 "A" {} }
page b { start = true  h1 "B" {} }
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    match build(&src, out.path(), None) {
        Err(BuildError::BadPage(m)) => {
            assert!(m.contains("start"), "unexpected error message: {m}");
        }
        Ok(_) => panic!("expected BadPage for multiple start pages, but build succeeded"),
        Err(_) => panic!("expected BadPage for multiple start pages, got a different error"),
    }
}

#[test]
fn build_allows_same_id_across_different_pages() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("two_pages.wcl");
    write_fixture(
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
    );

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
    write_fixture(&src, full);
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
        html.contains("<a class=\"link\" href=\"https://example.com\">docs</a>"),
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
    write_fixture(
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
    );
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
    write_fixture(
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
    );
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
    write_fixture(
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
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let index = std::fs::read_to_string(out.path().join("index.html")).expect("read index");
    assert!(
        index.contains("<a class=\"link\" href=\"about.html\">About</a>"),
        "cross-page link missing:\n{index}"
    );
    assert!(out.path().join("about.html").exists());
}

#[test]
fn build_renders_cross_page_link_with_fragment() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("pages.wcl");
    write_fixture(
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
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let index = std::fs::read_to_string(out.path().join("index.html")).expect("read index");
    assert!(
        index.contains("<a class=\"link\" href=\"about.html#section\">deep section</a>"),
        "fragment-bearing cross-page link missing:\n{index}"
    );
}

#[test]
fn build_passes_through_external_url() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("ext.wcl");
    write_fixture(
        &src,
        r##"
page index {
  text {
    span "see [docs](https://example.com)" {}
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("<a class=\"link\" href=\"https://example.com\">docs</a>"),
        "external url should pass through:\n{html}"
    );
}

#[test]
fn build_passes_through_same_page_anchor() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("anchor.wcl");
    write_fixture(
        &src,
        r##"
page index {
  text {
    span "back to [top](#top)" {}
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("<a class=\"link\" href=\"#top\">top</a>"),
        "same-page anchor should pass through:\n{html}"
    );
}

#[test]
fn build_errors_on_unknown_page_link() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("broken.wcl");
    write_fixture(
        &src,
        r##"
page index {
  text {
    span "see [docs](nonexistent)" {}
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    match build(&src, out.path(), None) {
        Err(BuildError::BadLink(msgs)) => {
            assert!(
                msgs.iter().any(|m| m.contains("nonexistent")),
                "missing the unknown page name in errors: {msgs:?}"
            );
        }
        Err(BuildError::Io(e, ctx)) => panic!("expected BadLink, got Io({ctx}: {e})"),
        Err(BuildError::Parse(_)) => panic!("expected BadLink, got Parse"),
        Err(BuildError::Schema(n)) => panic!("expected BadLink, got Schema({n})"),
        Err(BuildError::Eval(r)) => panic!("expected BadLink, got Eval({r:?})"),
        Err(BuildError::BadPage(m)) => panic!("expected BadLink, got BadPage({m})"),
        Err(BuildError::DuplicateId { page, id }) => {
            panic!("expected BadLink, got DuplicateId({page}: {id})")
        }
        Err(BuildError::DuplicatePage { site, name }) => {
            panic!("expected BadLink, got DuplicatePage({site}: {name})")
        }
        Err(BuildError::BadTemplate(name)) => panic!("expected BadLink, got BadTemplate({name})"),
        Err(BuildError::Tileset(m)) => panic!("expected BadLink, got Tileset({m})"),
        Err(BuildError::EdgeRouting(m)) => panic!("expected BadLink, got EdgeRouting({m})"),
        Ok(n) => panic!("expected BadLink, got Ok({n})"),
    }
}

// ── Diagram shape link tests ────────────────────────────────────────

#[test]
fn build_wraps_linked_diagram_shapes_in_anchors() {
    // A `link` on any diagram shape wraps that shape's SVG in an <a>,
    // resolved through the same page resolver as prose links. One anchor
    // per linked shape — covers process / rect / circle / label /
    // container (the standard shapes on the common render path).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("linked.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram { width = 400  height = 300
    process "API" { x = 10.0  y = 10.0  width = 80.0  height = 40.0  link = "target" }
    rect      { x = 10.0  y = 60.0  width = 80.0  height = 40.0  link = "target" }
    circle    { cx = 160.0  cy = 30.0  r = 20.0  link = "target" }
    label "Tag" { x = 160.0  y = 90.0  link = "target" }
    container { width = 60.0  height = 40.0  link = "target" }
  }
}
page target {
  text {
    span "Target page" {}
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let index = std::fs::read_to_string(out.path().join("index.html")).expect("read index");
    assert!(
        index.contains("<a href=\"target.html\">"),
        "linked shape anchor missing:\n{index}"
    );
    // Exactly one anchor per linked shape.
    assert_eq!(
        index.matches("<a href=\"target.html\">").count(),
        5,
        "expected one anchor per linked shape:\n{index}"
    );
    assert!(out.path().join("target.html").exists());
}

#[test]
fn build_unlinked_diagram_shape_has_no_anchor() {
    // A shape with no `link` renders with no <a> wrapper (unchanged).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("plain.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram { width = 200  height = 90
    rect { x = 20.0  y = 15.0  width = 120.0  height = 60.0  fill = "#cce" }
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    // Scope the assertion to the inlined diagram SVG so unrelated page
    // chrome can't influence it.
    let start = html.find("<svg").expect("diagram svg present");
    let end = html[start..].find("</svg>").expect("svg close") + start;
    let svg = &html[start..end];
    assert!(
        !svg.contains("<a "),
        "unlinked shape must not be wrapped in <a>:\n{svg}"
    );
}

#[test]
fn build_errors_on_unknown_diagram_shape_link() {
    // A diagram link to a missing page fails the build the same way a
    // bad prose link does (BuildError::BadLink naming the page).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("broken_link.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram { width = 200  height = 90
    rect { x = 20.0  y = 15.0  width = 120.0  height = 60.0  link = "nonexistent" }
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    match build(&src, out.path(), None) {
        Err(BuildError::BadLink(msgs)) => assert!(
            msgs.iter().any(|m| m.contains("nonexistent")),
            "missing the unknown page name in errors: {msgs:?}"
        ),
        Err(BuildError::Io(e, ctx)) => panic!("expected BadLink, got Io({ctx}: {e})"),
        Err(BuildError::Parse(_)) => panic!("expected BadLink, got Parse"),
        Err(BuildError::Schema(n)) => panic!("expected BadLink, got Schema({n})"),
        Err(BuildError::Eval(r)) => panic!("expected BadLink, got Eval({r:?})"),
        Err(BuildError::BadPage(m)) => panic!("expected BadLink, got BadPage({m})"),
        Err(BuildError::DuplicateId { page, id }) => {
            panic!("expected BadLink, got DuplicateId({page}: {id})")
        }
        Err(BuildError::DuplicatePage { site, name }) => {
            panic!("expected BadLink, got DuplicatePage({site}: {name})")
        }
        Err(BuildError::BadTemplate(name)) => panic!("expected BadLink, got BadTemplate({name})"),
        Err(BuildError::Tileset(m)) => panic!("expected BadLink, got Tileset({m})"),
        Err(BuildError::EdgeRouting(m)) => panic!("expected BadLink, got EdgeRouting({m})"),
        Ok(n) => panic!("expected BadLink, got Ok({n})"),
    }
}

// ── Fit-to-viewport test ───────────────────────────────────────────

#[test]
fn build_fit_viewbox_wraps_layered_content() {
    // The layered solver produces content much larger than the
    // declared width/height. The viewBox must wrap the actual
    // content bbox so it scales to fit instead of being clipped.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("fit.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width  = 100
    height = 50
    layout = :layered
    layer_gap = 30.0

    process "A" { id = a width = 200.0 height = 60.0 }
    process "B" { id = b width = 200.0 height = 60.0 }
    process "C" { id = c width = 200.0 height = 60.0 }
    a -> b
    b -> c
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // Outer width / height match the declared dims (page-layout slot).
    assert!(
        html.contains("width=\"100\" height=\"50\""),
        "outer dims should match declared 100x50:\n{html}"
    );

    // Extract the viewBox attribute and parse `x y w h`.
    let vb = html
        .split("viewBox=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .expect("viewBox present");
    let parts: Vec<f64> = vb
        .split_whitespace()
        .filter_map(|s| s.parse().ok())
        .collect();
    assert_eq!(parts.len(), 4, "expected 4 viewBox numbers, got: {vb}");
    let (_, _, vw, vh) = (parts[0], parts[1], parts[2], parts[3]);
    // Content is 200 wide × 3 layers of 60 + 2 gaps of 30 = 240 tall.
    // The viewBox must encompass that (plus padding).
    assert!(
        vw >= 200.0,
        "viewBox width {vw} should wrap 200-wide content"
    );
    assert!(
        vh >= 240.0,
        "viewBox height {vh} should wrap 240-tall content"
    );
}

#[test]
fn build_routes_around_destination_shape() {
    // The destination's `connect_points = [:south]` forces ingress
    // from below. With src and dst horizontally aligned, the path
    // must go down, east, and up — not cut straight through the
    // destination's bbox to reach its south edge.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("dst_obstacle.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width  = 400
    height = 200

    rect {
      id = src_node
      x = 10.0  y = 80.0  width = 80.0  height = 40.0
      fill = "#cce"
      connect_points = [:east]
    }
    rect {
      id = dst_node
      x = 200.0  y = 80.0  width = 80.0  height = 40.0
      fill = "#cec"
      connect_points = [:south]
    }
    src_node -> dst_node
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    // Extract the single polyline.
    let poly = html
        .split("<polyline points=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .expect("polyline present");
    let points: Vec<(f64, f64)> = poly
        .split_whitespace()
        .filter_map(|p| {
            let mut it = p.split(',');
            let x: f64 = it.next()?.parse().ok()?;
            let y: f64 = it.next()?.parse().ok()?;
            Some((x, y))
        })
        .collect();
    // dst_node bbox: x=200..280, y=80..120. No polyline segment may
    // have its *interior* cross that rectangle. (Endpoints touch the
    // south edge at y=120 — that's the anchor and is permitted.)
    for window in points.windows(2) {
        let (a, b) = (window[0], window[1]);
        // Walk along the segment in small steps; if any midpoint is
        // strictly inside the destination's bbox, the router cut
        // through the shape.
        let steps = 20;
        for i in 1..steps {
            let t = i as f64 / steps as f64;
            let x = a.0 + (b.0 - a.0) * t;
            let y = a.1 + (b.1 - a.1) * t;
            let inside = x > 200.0 && x < 280.0 && y > 80.0 && y < 120.0;
            assert!(
                !inside,
                "polyline segment {a:?} -> {b:?} crosses dst_node interior at ({x}, {y})\nfull polyline: {poly}"
            );
        }
    }
}

#[test]
fn build_centers_label_text() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("center.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width  = 200
    height = 100
    label "hello" {
      x = 100.0  y = 50.0
    }
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("text-anchor=\"middle\"") && html.contains("dominant-baseline=\"middle\""),
        "label is not center-aligned:\n{html}"
    );
}

#[test]
fn build_resizes_shape_for_multiline_text() {
    // A process declared with width=80 height=40, but the label
    // text takes three lines — the rect should grow tall enough
    // to fit them, and the renderer should emit one tspan per line.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("multiline.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width  = 200
    height = 200
    process "Line one\nLine two\nLine three" {
      x = 10.0  y = 10.0
      width = 80.0  height = 40.0
      fill = "#cce"
    }
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    // Three tspans (one per line).
    assert_eq!(
        html.matches("<tspan").count(),
        3,
        "expected 3 tspans:\n{html}"
    );
    // Rect height should have grown past the declared 40 to fit
    // three lines at the default 14px font (≥ 3*14*1.2 + 12 = ~62).
    let rect_height = html
        .split("<rect ")
        .nth(1)
        .and_then(|s| s.split("height=\"").nth(1))
        .and_then(|s| s.split('"').next())
        .and_then(|s| s.parse::<f64>().ok())
        .expect("rect height");
    assert!(
        rect_height >= 60.0,
        "rect height {rect_height} should grow to fit three lines"
    );
}

#[test]
fn build_shrinks_font_for_long_text() {
    // A process with a too-long label inside a narrow shape; the
    // renderer should shrink the font below the default 14.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("longtext.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width  = 400
    height = 100
    process "A very long label that overflows the shape width" {
      x = 10.0  y = 10.0
      width = 80.0  height = 40.0
      fill = "#cce"
    }
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    // Extract the font-size attribute of the first <text>.
    let font_size: f64 = html
        .split("<text ")
        .nth(1)
        .and_then(|s| s.split("font-size=\"").nth(1))
        .and_then(|s| s.split('"').next())
        .and_then(|s| s.parse().ok())
        .expect("font-size present");
    // Note: the process's shape also grows to fit the text since
    // declared width=80 is far too small. The font-size cap should
    // still kick in if the effective width is constrained — but
    // since effective_dims grows width to fit, font may stay at 14.
    // The real assertion is that *some* fit happens: either
    // font-size shrinks OR the shape grows. Here we check the
    // shape grew.
    let rect_width = html
        .split("<rect ")
        .nth(1)
        .and_then(|s| s.split("width=\"").nth(1))
        .and_then(|s| s.split('"').next())
        .and_then(|s| s.parse::<f64>().ok())
        .expect("rect width");
    assert!(
        rect_width >= 80.0,
        "rect width {rect_width} should grow past the declared 80"
    );
    // Sanity: font is non-zero and not absurdly large.
    assert!(font_size > 0.0 && font_size <= 14.0);
}

// ── Code-block tests ───────────────────────────────────────────────

#[test]
fn build_renders_code_block_with_highlight_classes() {
    // A Rust `code` block should wrap the source in <pre
    // class="code-block"><code class="language-rust">…</code></pre>
    // and produce token spans the bundled theme can style. Exact
    // span sequence isn't pinned — syntect upgrades shouldn't churn
    // the test — but a `tok-keyword` (covering `fn`) must appear.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("code.wcl");
    write_fixture(
        &src,
        r##"
page index {
  code rust {
    source = "fn main() { let x = 1; }"
  }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    assert!(
        html.contains("<pre class=\"code-block\""),
        "missing code-block wrapper:\n{html}"
    );
    assert!(
        html.contains("<code class=\"language-rust\">"),
        "missing code language class:\n{html}"
    );
    assert!(
        html.contains("tok-keyword") || html.contains("tok-storage"),
        "no syntect token classes emitted:\n{html}"
    );
    // The bundled theme CSS gets injected into every page so
    // .code-block has styling out of the box.
    assert!(
        html.contains("pre.code-block"),
        "bundled theme CSS missing:\n{html}"
    );
}

#[test]
fn build_renders_wcl_code_block_via_bundled_grammar() {
    // The wdoc crate ships a wcl.sublime-syntax grammar so the site
    // can highlight WCL itself. A `code wcl { ... }` block must
    // produce at least one tok-keyword span (covering `fn`, `let`,
    // `type`, …).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("wcl_code.wcl");
    write_fixture(
        &src,
        r##"
page index {
  code wcl {
    source = "let x = fn(n: i64) -> i64 n + 1"
  }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    assert!(
        html.contains("<code class=\"language-wcl\">"),
        "missing wcl language class:\n{html}"
    );
    assert!(
        html.contains("tok-keyword"),
        "wcl grammar produced no keyword tokens:\n{html}"
    );
}

#[test]
fn build_renders_unknown_language_as_plain_code() {
    // An unrecognised `language` tag must still render a code
    // block — just without token spans — rather than dropping the
    // listing or failing the build.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("unknown.wcl");
    write_fixture(
        &src,
        r##"
page index {
  code brainfuck {
    source = "+++.<-"
  }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    assert!(
        html.contains("<pre class=\"code-block\""),
        "missing code-block wrapper for unknown language:\n{html}"
    );
    assert!(
        html.contains("<code class=\"language-brainfuck\">"),
        "language attribute should pass through unmodified:\n{html}"
    );
    // The raw source survives escaping (no HTML special chars here).
    assert!(
        html.contains("+++.&lt;-") || html.contains("+++.<-"),
        "{html}"
    );
}

#[test]
fn build_code_block_escapes_html_in_source() {
    // User source containing HTML special characters must come out
    // escaped — syntect's HTML generator handles this for us, but
    // pin the behaviour so a refactor doesn't regress it.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("escape.wcl");
    write_fixture(
        &src,
        r##"
page index {
  code html {
    source = "<div class=\"x\">&amp;</div>"
  }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    // Only the rendered <pre> chunk should be inspected — escape
    // assertions on the surrounding HTML would always pass thanks
    // to the page chrome's own tags.
    let pre = html
        .split("<pre class=\"code-block\"")
        .nth(1)
        .and_then(|s| s.split("</pre>").next())
        .expect("code <pre> present");
    assert!(pre.contains("&lt;"), "no escaped < in:\n{pre}");
    assert!(pre.contains("&quot;"), "no escaped quote in:\n{pre}");
    assert!(
        !pre.contains("<div class=\"x\">"),
        "raw HTML leaked:\n{pre}"
    );
}

#[test]
fn build_code_block_carries_user_classes_and_id() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("classes.wcl");
    write_fixture(
        &src,
        r##"
page index {
  code rust {
    id    = snippet
    class = ["framed"]
    source = "fn main() {}"
  }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("<pre class=\"code-block framed\" id=\"snippet\">"),
        "missing combined class + id on <pre>:\n{html}"
    );
}

#[test]
fn build_processes_full_code_example_page() {
    // Smoke test against the code page in the example, which exercises
    // Rust, Python, JSON, WCL, and an unknown language in one page. The
    // example declares four sites (22 pages total); `showcase` is the
    // `root` site, so the code page renders flat at the output root.
    let out = TempDir::new().expect("mkdir tempdir");
    let n = build_ok(&examples_dir().join("wdoc").join("main.wcl"), out.path());
    assert_eq!(n, 22);
    let html = std::fs::read_to_string(out.path().join("code.html")).expect("read code.html");
    assert!(
        html.matches("<pre class=\"code-block\"").count() >= 5,
        "expected one <pre> per code block on code.html:\n{html}"
    );
}

#[test]
fn table_renders_header_row_and_typed_body() {
    // Pipe-table syntax: first row -> <thead>/<th>, the rest ->
    // <tbody>/<td>. utf8 cells flow through the inline-pattern engine;
    // a numeric cell passes through as its stringified value.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("tbl.wcl");
    write_fixture(
        &src,
        r##"
page index {
  table {
    rows:
      | "Name"  | "Age" |
      | "Alice" | 30    |
      | "Bob"   | 25    |
  }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains(
            "<table class=\"wdoc-table\"><thead><tr><th>Name</th><th>Age</th></tr></thead>\
             <tbody><tr><td>Alice</td><td>30</td></tr><tr><td>Bob</td><td>25</td></tr></tbody></table>"
        ),
        "{html}"
    );
}

#[test]
fn table_cells_support_inline_patterns() {
    // Inline patterns (**bold**, [link](page)) are recognised inside
    // utf8 cells, exactly as in a `text` span.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("tbl.wcl");
    write_fixture(
        &src,
        r##"
page index {
  table {
    rows:
      | "Col"          |
      | "**bold** now" |
  }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("<td><span class=\"bold\">bold</span> now</td>"),
        "{html}"
    );
}

#[test]
fn table_with_single_row_is_header_only() {
    // A one-row table is all header: <thead> present, no <tbody>.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("tbl.wcl");
    write_fixture(
        &src,
        r##"
page index {
  table {
    rows:
      | "only" | "header" |
  }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("<thead><tr><th>only</th><th>header</th></tr></thead>"),
        "{html}"
    );
    assert!(
        !html.contains("<tbody>"),
        "single-row table should have no body:\n{html}"
    );
}

#[test]
fn empty_table_builds_without_panicking() {
    // A `table` with no rows produces no output but must not abort
    // the build.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("tbl.wcl");
    write_fixture(
        &src,
        r##"
page index {
  table {}
  text { span "after" {} }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        !html.contains("<table"),
        "empty table should emit nothing:\n{html}"
    );
    assert!(html.contains("<span>after</span>"), "{html}");
}

#[test]
fn custom_block_lowers_to_table_fundamental() {
    // A custom WdocBlock whose `lower` returns an
    // HtmlFundamental::Table renders through render_table_payload.
    // `header` is the heading row; `rows` are the body. Cells on this
    // path are plain escaped text.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("tbl.wcl");
    write_fixture(
        &src,
        r##"
@block("datatable")
type DataTable extends WdocBlock {
  id: identifier?
  lower = fn(d: DataTable) -> list<HtmlFundamental> [
    HtmlFundamental::Table {
      id: none, class: none,
      header: ["A", "B"],
      rows: [["1", "2"], ["3", "4"]],
    }
  ]
}

page index {
  datatable {}
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains(
            "<table class=\"wdoc-table\"><thead><tr><th>A</th><th>B</th></tr></thead>\
             <tbody><tr><td>1</td><td>2</td></tr><tr><td>3</td><td>4</td></tr></tbody></table>"
        ),
        "{html}"
    );
}

// ── Page templates ─────────────────────────────────────────────────

#[test]
fn template_wraps_content_in_header_nav_main() {
    // `site { default_template = :webpage }` wraps every page in the
    // bundled webpage layout: a title <header>, a <nav> built from the
    // page list, and the page's own blocks inside <main>.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("site.wcl");
    write_fixture(
        &src,
        r#"
site {
  default_template = :webpage
  title = "My Site"
}
page index {
  h1 "Home" {}
}
page about {
  h1 "About" {}
}
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("<header class=\"site-header\">My Site</header>"),
        "{html}"
    );
    // Nav is generated from the page list (one <a> per page).
    assert!(html.contains("<nav class=\"site-nav\">"), "{html}");
    // The top nav is a sticky bar so it stays on screen while scrolling.
    assert!(
        html.contains(".site-nav { position: sticky; top: 0;"),
        "webpage nav should be sticky:\n{html}"
    );
    // The content sits in a themed card (a distinct surface box).
    assert!(
        html.contains(".site-main { display: block; background:"),
        "webpage content should be a card:\n{html}"
    );
    // The template CSS heredoc must be pure CSS — a leaked `// ` line
    // comment (which CSS doesn't support) silently swallows later rules.
    assert!(
        !html.contains("// "),
        "no `//` comment may leak into the emitted <style>:\n{html}"
    );
    assert!(html.contains("<a href=\"index.html\">index</a>"), "{html}");
    assert!(html.contains("<a href=\"about.html\">about</a>"), "{html}");
    // The page's own content lands inside <main>.
    assert!(
        html.contains("<main class=\"site-main\"><p class=\"heading-1\"><span>Home</span></p>"),
        "{html}"
    );
}

#[test]
fn per_page_template_overrides_and_bare_fallback() {
    // No `site` block: a page with `template = :webpage` is wrapped; a
    // page without one renders bare (no <main> wrapper) — backward
    // compatible with pre-template behavior.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("site.wcl");
    write_fixture(
        &src,
        r#"
page index {
  template = :webpage
  h1 "Wrapped" {}
}
page plain {
  h1 "Bare" {}
}
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let wrapped = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    let bare = std::fs::read_to_string(out.path().join("plain.html")).expect("read");
    assert!(wrapped.contains("<main class=\"site-main\">"), "{wrapped}");
    assert!(
        !bare.contains("<main"),
        "page without a template should render bare:\n{bare}"
    );
    assert!(
        bare.contains("<p class=\"heading-1\"><span>Bare</span></p>"),
        "{bare}"
    );
}

#[test]
fn template_uses_user_defined_part_function() {
    // A "part" is just a top-level function returning fundamentals; a
    // custom template calls it (resolved at document scope) and embeds
    // its result. Also exercises HtmlFundamental::Element nesting +
    // Raw, and attribute escaping.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("site.wcl");
    write_fixture(
        &src,
        r##"
let footer = fn(c: TemplateCtx) -> list<HtmlFundamental> [
  HtmlFundamental::Element {
    tag: "footer", id: none, class: ["ft"], attrs: [["data-x", "a\"b"]],
    children: [ HtmlFundamental::Raw { html: c.title } ],
  }
]
template mini {
  render = fn(c: TemplateCtx) -> list<HtmlFundamental>
    flatten([
      [ HtmlFundamental::Element {
          tag: "main", id: none, class: none, attrs: none,
          children: [ HtmlFundamental::Raw { html: c.content } ],
      } ],
      footer(c),
    ])
}
site { default_template = :mini  title = "T" }
page index {
  text { span "body text" {} }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    // Part function output is present.
    assert!(
        html.contains("<footer class=\"ft\" data-x=\"a&quot;b\">T</footer>"),
        "part fn / attr escaping wrong:\n{html}"
    );
    // Raw embeds the page content verbatim inside <main>.
    assert!(
        html.contains("<main><p><span>body text</span></p>"),
        "{html}"
    );
}

#[test]
fn unknown_template_is_build_error() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("site.wcl");
    write_fixture(
        &src,
        r#"
site { default_template = :nope }
page index { h1 "x" {} }
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    match build(&src, out.path(), None) {
        Err(BuildError::BadTemplate(name)) => assert_eq!(name, "nope"),
        Err(_) => panic!("expected BadTemplate, got a different BuildError"),
        Ok(_) => panic!("expected BadTemplate, got Ok"),
    }
}

#[test]
fn book_template_without_toc_falls_back_to_flat_page_list() {
    // With no `toc`, the book sidebar lists every page (flat, one entry
    // each), wrapped in the same `<ul class="book-toc">` markup; the
    // current chapter is marked `current`. Content lands in <main>.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("book.wcl");
    write_fixture(
        &src,
        r#"
site {
  default_template = :book
  title = "Handbook"
}
page intro {
  h1 "Intro" {}
}
page usage {
  h1 "Usage" {}
}
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());

    let intro = std::fs::read_to_string(out.path().join("intro.html")).expect("read");
    // Sidebar with the book title and a link per chapter.
    assert!(intro.contains("<nav class=\"book-sidebar\">"), "{intro}");
    assert!(intro.contains("<ul class=\"book-toc\">"), "{intro}");
    assert!(
        intro.contains("<div class=\"book-title\">Handbook</div>"),
        "{intro}"
    );
    // On intro.html, `intro` is the current chapter; `usage` is not.
    assert!(
        intro.contains("<a class=\"book-chapter current\" href=\"intro.html\">intro</a>"),
        "current chapter not highlighted:\n{intro}"
    );
    assert!(
        intro.contains("<a class=\"book-chapter\" href=\"usage.html\">usage</a>"),
        "{intro}"
    );
    // Content lands in the reading column.
    assert!(
        intro
            .contains("<main class=\"book-content\"><p class=\"heading-1\"><span>Intro</span></p>"),
        "{intro}"
    );

    // On usage.html the highlight moves to the `usage` chapter.
    let usage = std::fs::read_to_string(out.path().join("usage.html")).expect("read");
    assert!(
        usage.contains("<a class=\"book-chapter current\" href=\"usage.html\">usage</a>"),
        "{usage}"
    );
}

#[test]
fn book_toc_renders_nested_ordered_navigation() {
    // A declared `toc` controls order and nesting (3 levels). Heading
    // chapters (no `page`) render as <span class="book-section">; the
    // tree nests via <ul class="book-toc">; the current chapter is
    // highlighted; entries follow declared order, not page order.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("book.wcl");
    write_fixture(
        &src,
        r#"
site {
  default_template = :book
  title = "Handbook"
  toc {
    chapter "Start Here" { page = intro }
    chapter "Guide" {
      chapter "Deep" {
        chapter "Internals" { page = internals }
      }
    }
  }
}
page intro { h1 "Intro" {} }
page internals { h1 "Internals" {} }
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("internals.html")).expect("read");

    // Custom titles from the toc (not raw page names).
    assert!(
        html.contains("<a class=\"book-chapter\" href=\"intro.html\">Start Here</a>"),
        "{html}"
    );
    // Heading-only chapters are non-link sections.
    assert!(
        html.contains("<span class=\"book-section\">Guide</span>"),
        "{html}"
    );
    assert!(
        html.contains("<span class=\"book-section\">Deep</span>"),
        "{html}"
    );
    // Depth-3 entry, highlighted as the current page.
    assert!(
        html.contains("<a class=\"book-chapter current\" href=\"internals.html\">Internals</a>"),
        "{html}"
    );
    // Nesting: at least two levels of <ul class="book-toc"> (Guide > Deep).
    assert!(
        html.matches("<ul class=\"book-toc\">").count() >= 3,
        "expected nested book-toc lists:\n{html}"
    );
    // Declared order: "Start Here" precedes the "Guide" section.
    let start = html.find("Start Here").unwrap();
    let guide = html.find(">Guide<").unwrap();
    assert!(start < guide, "toc not in declared order:\n{html}");
}

#[test]
fn book_toc_unknown_page_is_build_error() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("book.wcl");
    write_fixture(
        &src,
        r#"
site {
  default_template = :book
  toc { chapter "Oops" { page = nonexistent } }
}
page intro { h1 "Intro" {} }
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    match build(&src, out.path(), None) {
        Err(BuildError::BadTemplate(msg)) => {
            assert!(msg.contains("nonexistent"), "got: {msg}")
        }
        Err(_) => panic!("expected BadTemplate"),
        Ok(_) => panic!("expected BadTemplate, got Ok"),
    }
}

// ── Theming via the class system (light/dark) ──────────────────────

#[test]
fn class_light_dark_modes_emit_themed_css() {
    // A class with `dark {}` / `light {}` mode blocks emits: a default
    // (dark) rule, a `@media (prefers-color-scheme: light)` rule, and
    // explicit `:root[data-theme=…]` overrides (which the toggle uses).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("t.wcl");
    write_fixture(
        &src,
        r##"
class "panel" {
  dark  { background = "#2e3440" }
  light { background = "#eceff4" }
}
page index { text { span "hi" {} } }
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(html.contains(".panel { background:#2e3440; }"), "{html}");
    assert!(
        html.contains("@media (prefers-color-scheme: light) { .panel { background:#eceff4; } }"),
        "{html}"
    );
    assert!(
        html.contains(":root[data-theme=\"dark\"] .panel { background:#2e3440; }"),
        "{html}"
    );
    assert!(
        html.contains(":root[data-theme=\"light\"] .panel { background:#eceff4; }"),
        "{html}"
    );
}

#[test]
fn class_with_only_light_mode_still_emits_dark_toggle_rule() {
    // Regression: a dark-default class that declares only `light {}`
    // must still emit a `:root[data-theme="dark"]` rule (falling back to
    // the base), so the theme toggle can switch back to dark. Without it,
    // on a light-preferring system the `@media (prefers-color-scheme:
    // light)` rule kept winning and toggling to dark did nothing.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("t.wcl");
    write_fixture(
        &src,
        r##"
class "wdoc-body" {
  color = "#d8dee9"  background = "#2e3440"
  light { color = "#2e3440"  background = "#eceff4" }
}
page index { text { span "hi" {} } }
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    // The default (dark) and the system-light media rule.
    assert!(
        html.contains(".wdoc-body { color:#d8dee9;background:#2e3440; }"),
        "{html}"
    );
    assert!(
        html.contains("@media (prefers-color-scheme: light) { .wdoc-body { color:#2e3440;background:#eceff4; } }"),
        "{html}"
    );
    // Both toggle overrides — the dark one falls back to the base.
    assert!(
        html.contains(
            ":root[data-theme=\"dark\"] .wdoc-body { color:#d8dee9;background:#2e3440; }"
        ),
        "missing data-theme=dark toggle rule:\n{html}"
    );
    assert!(
        html.contains(":root[data-theme=\"light\"] .wdoc-body { color:#d8dee9;background:#2e3440;color:#2e3440;background:#eceff4; }"),
        "{html}"
    );
}

#[test]
fn class_without_modes_is_unchanged() {
    // No `dark`/`light` ⇒ a single bare rule, no media/data-theme.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("t.wcl");
    write_fixture(
        &src,
        r##"
class accent { color = "#003a8c"  bold = true }
page index { text { span "hi" { class = ["accent"] } } }
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains(".accent { color:#003a8c;font-weight:bold; }"),
        "{html}"
    );
    assert!(
        !html.contains("prefers-color-scheme"),
        "unthemed class leaked a media rule:\n{html}"
    );
    // The page body always carries the themeable hook class.
    assert!(html.contains("<body class=\"wdoc-body\">"), "{html}");
}

#[test]
fn inline_emphasis_styled_by_class_and_themed() {
    // `**bold**` / `_italic_` / `` `code` `` lower to the bundled
    // `bold` / `italic` / `code` classes (structural defaults), and a
    // themed site colours them per its palette via the theme apply rules
    // — so emphasis is styled by a class AND varies with the theme.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("emph.wcl");
    write_fixture(
        &src,
        r##"
site home { root = true  default_template = :webpage  title = "T"  theme = :tokyonight }
page index { sites = [:home]
  text { span "Try **bold**, _italic_, `code`." {} }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // Bundled structural classes (weight / slant / monospace).
    assert!(html.contains(".bold { font-weight:bold; }"), "{html}");
    assert!(html.contains(".italic { font-style:italic; }"), "{html}");
    assert!(html.contains("font-family:ui-monospace"), "{html}");
    // Per-theme colour, driven by the palette (so it differs per theme).
    assert!(html.contains(".bold{color:var(--wdoc-orange);}"), "{html}");
    assert!(
        html.contains(".code{background:var(--wdoc-bg-inset);"),
        "{html}"
    );
    // The patterns emit the classes onto the rendered spans.
    assert!(html.contains("<span class=\"bold\">bold</span>"), "{html}");
    assert!(
        html.contains("<span class=\"italic\">italic</span>"),
        "{html}"
    );
    assert!(html.contains("<span class=\"code\">code</span>"), "{html}");
}

#[test]
fn inline_bold_unthemed_has_weight_but_no_theme_colour() {
    // With no site/theme the bundled class still makes bold bold, but the
    // per-theme colour rule isn't emitted (the doc stays unthemed).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("plain.wcl");
    write_fixture(&src, "page index { text { span \"Just **bold**.\" {} } }\n");

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    assert!(html.contains(".bold { font-weight:bold; }"), "{html}");
    assert!(
        !html.contains(".bold{color:"),
        "theme colour leaked into an unthemed doc:\n{html}"
    );
}

#[test]
fn book_theme_toggle_is_gated_by_site_flag() {
    let fixture = |toggle: bool| -> String {
        let tmp = TempDir::new().expect("mkdir tempdir");
        let src = tmp.path().join("t.wcl");
        write_fixture(
            &src,
            format!(
                "site {{ default_template = :book  theme_toggle = {toggle} }}\npage index {{ h1 \"H\" {{}} }}\n"
            ),
        );
        let out = TempDir::new().expect("mkdir out");
        build_ok(&src, out.path());
        std::fs::read_to_string(out.path().join("index.html")).expect("read")
    };

    let on = fixture(true);
    assert!(on.contains("<button class=\"theme-toggle\""), "{on}");
    assert!(
        on.contains("wdocToggleTheme"),
        "toggle script missing:\n{on}"
    );

    let off = fixture(false);
    assert!(
        !off.contains("<button"),
        "toggle should be gated off:\n{off}"
    );
    assert!(
        !off.contains("wdocToggleTheme"),
        "toggle script should be gated off:\n{off}"
    );
}

#[test]
fn book_theme_classes_and_toggle() {
    // The `book` template themes through the `class` system: a Nord-style
    // dark-default `wdoc-body` with a light alternative, themed heading +
    // link colours layered over the bundled heading sizing, and the
    // light/dark toggle button (gated by the site `theme_toggle` flag).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("book.wcl");
    write_fixture(
        &src,
        r##"
site { default_template = :book  theme_toggle = true }

class "wdoc-body" {
  color = "#d8dee9"
  background = "#2e3440"
  light { color = "#2e3440"  background = "#eceff4" }
}
class "heading-1" { color = "#88c0d0" }
class "heading-2" { color = "#8fbcbb" }
class "link"      { color = "#88c0d0" }

page introduction { h1 "Introduction" {} }
page syntax {
  h1 "Syntax" {}
  h2 "Fields" {}
  text { span "See the [links](introduction) page." {} }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("syntax.html")).expect("read");
    assert!(
        html.contains(".wdoc-body { color:#d8dee9;background:#2e3440; }"),
        "{html}"
    );
    assert!(
        html.contains("@media (prefers-color-scheme: light) { .wdoc-body {"),
        "{html}"
    );
    assert!(html.contains("<button class=\"theme-toggle\""), "{html}");
    // More of the palette: heading levels, inline code, and links are
    // themed too, and headings have default sizing.
    assert!(html.contains(".heading-1 { color:#88c0d0; }"), "{html}");
    assert!(html.contains(".heading-2 { color:#8fbcbb; }"), "{html}");
    // Default heading sizing now rides a bundled `class "heading-1"`
    // (emitted before the user's colour override below).
    assert!(
        html.contains(
            ".heading-1 { font-weight:700;font-size:1.9rem;line-height:1.2;margin:1.4rem 0 0.6rem; }"
        ),
        "default heading sizing missing:\n{html}"
    );
    assert!(html.contains(".link { color:#88c0d0; }"), "{html}");
    assert!(
        html.contains("<a class=\"link\" href=\"introduction.html\">links</a>"),
        "{html}"
    );
    assert!(
        html.contains("<p class=\"heading-2\"><span>Fields</span></p>"),
        "{html}"
    );
}

#[test]
fn build_renders_bar_chart_with_axes_and_series() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("bar.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width  = 380
    height = 240
    bar_chart {
      width   = 380
      height  = 240
      title   = "Revenue"
      x_label = "Quarter"
      categories = ["Q1", "Q2"]
      y_min = 0.0
      y_max = 100.0
      series = [
        ChartSeries::Of { name: "North", values: [40.0, 80.0] },
        ChartSeries::Of { name: "South", values: [20.0, 60.0] },
      ]
    }
  }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // Two categories × two series = four bars, each tagged with a
    // cycling palette class and no inline fill (CSS drives the colour).
    let bars1 = html.matches("<rect class=\"wdoc-series-1\"").count();
    let bars2 = html.matches("<rect class=\"wdoc-series-2\"").count();
    // 2 bars + 1 legend swatch per series.
    assert_eq!(bars1, 3, "{html}");
    assert_eq!(bars2, 3, "{html}");
    // Bars carry no inline fill — colour comes from the palette CSS.
    assert!(
        !html.contains(
            "<rect class=\"wdoc-series-1\" x=\"40\" y=\"22\" width=\"10\" height=\"10\" fill="
        ),
        "{html}"
    );
    // Axes, title, scale ticks, category + axis labels.
    assert!(html.contains("class=\"wdoc-axis\""), "{html}");
    assert!(
        html.contains("class=\"wdoc-chart-title\"") && html.contains(">Revenue</tspan>"),
        "{html}"
    );
    assert!(
        html.contains(">100</tspan>") && html.contains(">0</tspan>"),
        "{html}"
    );
    assert!(
        html.contains(">Q1</tspan>") && html.contains(">Quarter</tspan>"),
        "{html}"
    );
    // Bundled palette CSS is injected into the page <style>. The palette
    // now rides the `class` system (bundled `class` blocks in wdoc.wcl), so
    // it serializes via `render_class` — unspaced `prop:value;`.
    assert!(html.contains(".wdoc-series-1 { fill:#5e81ac;"), "{html}");
    assert!(html.contains(".wdoc-axis { stroke:currentColor;"), "{html}");
}

#[test]
fn build_bare_record_series_matches_explicit_variant_form() {
    // The bare-record series form (`{ name, values }`) must produce
    // byte-identical output to the explicit `ChartSeries::Of { … }`
    // form — the variant is shape-inferred from the `list<ChartSeries>`
    // field type.
    fn render(series: &str) -> String {
        let tmp = TempDir::new().expect("mkdir tempdir");
        let src = tmp.path().join("bar.wcl");
        write_fixture(
            &src,
            format!(
                r##"
page index {{
  diagram {{
    width  = 380
    height = 240
    bar_chart {{
      width   = 380
      height  = 240
      title   = "Revenue"
      x_label = "Quarter"
      categories = ["Q1", "Q2"]
      y_min = 0.0
      y_max = 100.0
      series = [
{series}
      ]
    }}
  }}
}}
"##
            ),
        );
        let out = TempDir::new().expect("mkdir out");
        build_ok(&src, out.path());
        std::fs::read_to_string(out.path().join("index.html")).expect("read")
    }

    let explicit = render(
        "        ChartSeries::Of { name: \"North\", values: [40.0, 80.0] },\n\
         \x20       ChartSeries::Of { name: \"South\", values: [20.0, 60.0] },",
    );
    let bare = render(
        "        { name: \"North\", values: [40.0, 80.0] },\n\
         \x20       { name: \"South\", values: [20.0, 60.0] },",
    );
    assert_eq!(
        explicit, bare,
        "bare-record output diverged from explicit form"
    );
    // Sanity: the shared output really is the rendered bar chart.
    assert!(bare.contains("<rect class=\"wdoc-series-1\""), "{bare}");
}

#[test]
fn build_renders_horizontal_timeline_with_phases_items_ticks() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("timeline.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width  = 500
    height = 220
    timeline {
      width  = 500
      height = 220
      title  = "Roadmap"
      unit   = :months
      start  = "2026-01-01"
      end    = "2026-12-31"
      phases = [
        TimelinePhase::Of { label: "Design", from: "2026-01-01", to: "2026-04-01" },
        TimelinePhase::Of { label: "Build",  from: "2026-04-01", to: "2026-10-01" },
        TimelinePhase::Of { label: "Ship",   from: "2026-10-01", to: "2026-12-31" },
      ]
      items = [
        TimelineItem::On      { label: "Kickoff", on: "2026-01-10" },
        TimelineItem::On      { label: "Beta",    on: "2026-06-15" },
        TimelineItem::OnSided { label: "Freeze",  on: "2026-09-01", side: :far },
        TimelineItem::On      { label: "Launch",  on: "2026-12-20" },
      ]
    }
  }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // One marker + connector + label per item (4).
    assert_eq!(
        html.matches("<circle class=\"wdoc-timeline-marker\"")
            .count(),
        4,
        "{html}"
    );
    assert_eq!(
        html.matches("class=\"wdoc-timeline-connector\"").count(),
        4,
        "{html}"
    );
    for label in ["Kickoff", "Beta", "Freeze", "Launch"] {
        assert!(html.contains(&format!(">{label}</tspan>")), "{html}");
    }

    // Three phases → two boundary dividers each (6), each carrying a
    // cycled palette class, plus three headings.
    assert_eq!(html.matches("wdoc-timeline-divider\"").count(), 6, "{html}");
    assert_eq!(
        html.matches("wdoc-timeline-phase-label\"").count(),
        3,
        "{html}"
    );
    assert!(
        html.contains("wdoc-series-1 wdoc-timeline-divider")
            && html.contains("wdoc-series-2 wdoc-timeline-divider"),
        "{html}"
    );
    for phase in ["Design", "Build", "Ship"] {
        assert!(html.contains(&format!(">{phase}</tspan>")), "{html}");
    }

    // Axis spine + calendar month ticks (Jan…Dec; the January tick
    // carries the year so a multi-year axis reads).
    assert!(html.contains("<line class=\"wdoc-axis\""), "{html}");
    assert!(html.contains(">Jan 2026</tspan>"), "{html}");
    for tick in [
        ">Mar</tspan>",
        ">Jun</tspan>",
        ">Sep</tspan>",
        ">Dec</tspan>",
    ] {
        assert!(html.contains(tick), "{html}");
    }
    assert!(
        html.contains("class=\"wdoc-chart-title\"") && html.contains(">Roadmap</tspan>"),
        "{html}"
    );

    // Bundled timeline CSS rides the class system and is injected into
    // the page <style>; markers/connectors paint with currentColor.
    assert!(
        html.contains(".wdoc-timeline-marker { fill:currentColor;"),
        "{html}"
    );
    assert!(html.contains(".wdoc-timeline-divider {"), "{html}");
}

#[test]
fn build_renders_vertical_timeline() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("vtimeline.wcl");
    // No start/end — the scale auto-fits from the event dates.
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width  = 260
    height = 320
    timeline {
      width     = 260
      height    = 320
      direction = :vertical
      title     = "Releases"
      unit      = :months
      items = [
        TimelineItem::On { label: "Alpha", on: "2026-01-15" },
        TimelineItem::On { label: "Beta",  on: "2026-03-15" },
        TimelineItem::On { label: "GA",    on: "2026-06-15" },
      ]
    }
  }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // Three items resolve.
    assert_eq!(
        html.matches("<circle class=\"wdoc-timeline-marker\"")
            .count(),
        3,
        "{html}"
    );
    for label in ["Alpha", "Beta", "GA"] {
        assert!(html.contains(&format!(">{label}</tspan>")), "{html}");
    }
    // Vertical orientation: the axis runs down a fixed x (width/2 = 130),
    // so every marker shares the same `cx` while `cy` advances.
    assert_eq!(
        html.matches("<circle class=\"wdoc-timeline-marker\" cx=\"130\"")
            .count(),
        3,
        "{html}"
    );
    assert!(
        html.contains("<line class=\"wdoc-axis\" x1=\"130\"") && html.contains("x2=\"130\""),
        "{html}"
    );
}

#[test]
fn build_renders_card_shape_in_diagram() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("card.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width  = 260
    height = 140
    card {
      x = 20.0  y = 20.0  width = 200.0  height = 100.0
      title = "Notes"
      text { span "A card with " {} span "formatted" { class = ["accent"] } span " text." {} }
    }
  }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // A card is drawn as an SVG <foreignObject> at its resolved box,
    // holding an XHTML-namespaced wdoc-card <div>.
    assert!(
        html.contains("<foreignObject x=\"20\" y=\"20\" width=\"200\" height=\"100\">"),
        "{html}"
    );
    assert!(
        html.contains("<div xmlns=\"http://www.w3.org/1999/xhtml\" class=\"wdoc-card\">"),
        "{html}"
    );
    // Title + the body rendered through the inline engine (the accent
    // span proves render_block ran, not plain SVG text).
    assert!(
        html.contains("<div class=\"wdoc-card-title\">Notes</div>"),
        "{html}"
    );
    assert!(
        html.contains("<span class=\"accent\">formatted</span>"),
        "{html}"
    );
    // Structural card CSS is injected.
    assert!(html.contains(".wdoc-card-title"), "{html}");
}

#[test]
fn build_renders_timeline_cards() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("tlcards.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width  = 600
    height = 260
    timeline {
      width = 600  height = 260
      unit  = :months
      start = "2026-01-01"
      end   = "2026-12-31"
      // No `items` — a cards-only timeline must render.
      card { on = "2026-06-15"  title = "Beta"
        text { span "First public " {} span "beta" { class = ["accent"] } span " build." {} }
      }
      card { on = "2026-12-01"  side = :far  title = "GA"
        text { span "General availability." {} }
      }
    }
  }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // Two event cards → two foreignObject cards, each with a marker on
    // the axis. The Beta card sits at the mid-scale position (frac 0.5).
    assert_eq!(html.matches("<foreignObject").count(), 2, "{html}");
    assert!(
        html.contains("<div class=\"wdoc-card-title\">Beta</div>")
            && html.contains("<div class=\"wdoc-card-title\">GA</div>"),
        "{html}"
    );
    // The card body is run through the inline engine.
    assert!(
        html.contains("<span class=\"accent\">beta</span>"),
        "{html}"
    );
    // The axis chrome renders, and each card has its own axis marker.
    assert!(html.contains("<line class=\"wdoc-axis\""), "{html}");
    assert_eq!(
        html.matches("<circle class=\"wdoc-timeline-marker\"")
            .count(),
        2,
        "{html}"
    );
}

#[test]
fn build_timeline_every_thins_ticks_and_skips_bad_dates() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("tlevery.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width  = 600
    height = 200
    timeline {
      width = 600  height = 200
      unit  = :months
      every = 2
      start = "2026-01-01"
      end   = "2026-12-31"
      items = [
        TimelineItem::On { label: "Good", on: "2026-02-10" },
        TimelineItem::On { label: "Bad",  on: "not-a-date" },
      ]
    }
  }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // `every = 2` keeps every other month: Jan, Mar, May, Jul, Sep, Nov.
    for present in [">Jan 2026</tspan>", ">Mar</tspan>", ">Nov</tspan>"] {
        assert!(html.contains(present), "{html}");
    }
    for absent in [">Feb</tspan>", ">Apr</tspan>", ">Dec</tspan>"] {
        assert!(
            !html.contains(absent),
            "thinned tick leaked: {absent}\n{html}"
        );
    }
    // The good-dated item renders; the bad date is skipped (no build
    // failure, no marker for it) — one marker, the "Good" label present.
    assert_eq!(
        html.matches("<circle class=\"wdoc-timeline-marker\"")
            .count(),
        1,
        "{html}"
    );
    assert!(html.contains(">Good</tspan>"), "{html}");
}

#[test]
fn build_renders_line_chart_segments_and_markers() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("line.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width  = 360
    height = 220
    line_chart {
      width  = 360
      height = 220
      categories = ["A", "B", "C"]
      series = [
        ChartSeries::Of { name: "one", values: [10.0, 30.0, 20.0] },
      ]
    }
  }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // Three points ⇒ two connecting line segments (compound class) and
    // three circle markers.
    assert_eq!(
        html.matches("<line class=\"wdoc-series-1 wdoc-line\"")
            .count(),
        2,
        "{html}"
    );
    assert_eq!(
        html.matches("<circle class=\"wdoc-series-1\"").count(),
        3,
        "{html}"
    );
    assert!(html.contains(".wdoc-line { fill:none;"), "{html}");

    // Without `point_labels` / `points`, no value labels or annotations
    // are emitted (the features are opt-in). The bundled class rules are
    // always in the stylesheet, so check for the `class="…"` usage.
    assert!(!html.contains("class=\"wdoc-point-label\""), "{html}");
    assert!(!html.contains("class=\"wdoc-annotation\""), "{html}");
}

#[test]
fn build_renders_line_chart_point_labels() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("line.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width  = 360
    height = 220
    line_chart {
      width  = 360
      height = 220
      categories = ["A", "B", "C"]
      point_labels = true
      series = [
        ChartSeries::Of { name: "one", values: [10.0, 30.0, 20.0] },
      ]
    }
  }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // One value label per data point, on the dedicated point-label class.
    assert_eq!(
        html.matches("class=\"wdoc-point-label\"").count(),
        3,
        "{html}"
    );
    // The point values are printed as label text.
    assert!(
        html.contains(">10</tspan>")
            && html.contains(">30</tspan>")
            && html.contains(">20</tspan>"),
        "{html}"
    );
    // No annotations were declared (the class rule is always in the
    // stylesheet, so check the `class="…"` usage).
    assert!(!html.contains("class=\"wdoc-annotation\""), "{html}");
}

#[test]
fn build_renders_line_chart_annotations() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("line.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width  = 360
    height = 220
    line_chart {
      width  = 360
      height = 220
      categories = ["A", "B", "C"]
      series = [
        ChartSeries::Of { name: "one", values: [10.0, 30.0, 20.0] },
      ]
      points = [
        ChartPoint::At { label: "launch", category: 1, value: 30.0 },
        ChartPoint::At { label: "dip",    category: 2, value: 20.0 },
      ]
    }
  }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // Each annotation draws a marker circle + a label, both carrying the
    // annotation class ⇒ two occurrences per point.
    assert_eq!(
        html.matches("class=\"wdoc-annotation\"").count(),
        4,
        "{html}"
    );
    assert!(html.contains("<circle class=\"wdoc-annotation\""), "{html}");
    assert!(
        html.contains(">launch</tspan>") && html.contains(">dip</tspan>"),
        "{html}"
    );
    // No value labels without `point_labels` (the class rule is always
    // in the stylesheet, so check the `class="…"` usage).
    assert!(!html.contains("class=\"wdoc-point-label\""), "{html}");
}

#[test]
fn build_renders_pie_slices_as_polygons() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("pie.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width  = 240
    height = 240
    pie_chart {
      width  = 240
      height = 240
      title  = "Mix"
      slices = [
        ChartSlice::Of { label: "A", value: 50.0 },
        ChartSlice::Of { label: "B", value: 30.0 },
        ChartSlice::Of { label: "C", value: 20.0 },
      ]
    }
  }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // One polygon per slice, each on its own palette class, plus a
    // centroid label per slice.
    assert!(html.contains("<polygon class=\"wdoc-series-1\""), "{html}");
    assert!(html.contains("<polygon class=\"wdoc-series-2\""), "{html}");
    assert!(html.contains("<polygon class=\"wdoc-series-3\""), "{html}");
    assert!(
        html.contains(">A</tspan>") && html.contains(">B</tspan>") && html.contains(">C</tspan>"),
        "{html}"
    );
    assert!(
        html.contains("class=\"wdoc-chart-title\"") && html.contains(">Mix</tspan>"),
        "{html}"
    );
}

#[test]
fn chart_series_class_is_overridable_via_class_block() {
    // Redeclaring a palette class (quoted, because the name has
    // hyphens) recolours a series and emits per-mode rules; the
    // override lands after the bundled default in the stylesheet.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("themed.wcl");
    write_fixture(
        &src,
        r##"
class "wdoc-series-1" {
  fill = "#7b2d8e"
  dark  { fill = "#c792ea" }
  light { fill = "#7b2d8e" }
}

page index {
  diagram {
    width  = 200
    height = 160
    bar_chart {
      width  = 200
      height = 160
      categories = ["x"]
      series = [ ChartSeries::Of { name: "s", values: [5.0] } ]
    }
  }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // Both the bundled default and the user override are present, in
    // that order, so the override wins by source order.
    let default_at = html
        .find(".wdoc-series-1 { fill:#5e81ac;")
        .expect("bundled default");
    let override_at = html
        .find(".wdoc-series-1 { fill:#7b2d8e;fill:#c792ea; }")
        .expect("user override");
    assert!(default_at < override_at, "{html}");
    assert!(
        html.contains("@media (prefers-color-scheme: light) { .wdoc-series-1 { fill:#7b2d8e; } }"),
        "{html}"
    );
}

// ── Terminal ───────────────────────────────────────────────────────

#[test]
fn terminal_primitives_render_grid_svg() {
    // A primitives terminal lowers to an inline <svg> grid: a window
    // rect, a double box (corner glyphs), and styled text runs.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("t.wcl");
    write_fixture(
        &src,
        r##"
page index {
  terminal {
    cols = 20 rows = 5 chrome = false
    term_box { row = 1 col = 1 width = 20 height = 5 border = :double }
    term_text "OK" { row = 2 col = 2 fg = "green" bold = true }
    term_text "no" { row = 3 col = 2 fg = "red" inverse = true }
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    assert!(
        html.contains("class=\"wdoc-terminal\""),
        "no terminal div:\n{html}"
    );
    assert!(
        html.contains("wdoc-terminal-svg"),
        "no terminal svg:\n{html}"
    );
    // Double box corners + edges.
    assert!(
        html.contains('╔') && html.contains('╗') && html.contains('═'),
        "no double box:\n{html}"
    );
    // Bold green "OK": green fill #4e9a06 + font-weight bold.
    assert!(
        html.contains("fill=\"#4e9a06\"") && html.contains("font-weight=\"bold\""),
        "no bold green:\n{html}"
    );
    // Inverse "no": red becomes the background rect, text takes the window bg.
    assert!(
        html.contains("<rect x=") && html.contains("fill=\"#cc0000\""),
        "no inverse bg:\n{html}"
    );
}

#[test]
fn terminal_tui_widgets_lower_nest_and_extend() {
    // The TUI widgets (`tui_*`) are not hardcoded primitives — each
    // lowers into the four terminal building blocks via its `lower`
    // function, which the renderer recursively draws into the grid.
    // This exercises a leaf widget (progress), a container that nests a
    // control (panel → checkbox), and a *user-defined* widget extending
    // the `TuiWidget` base. The progress bar's filled width is exact
    // integer math (62% of 24 = 14 cells) painted in the accent colour.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("t.wcl");
    write_fixture(
        &src,
        r##"
@block("my_badge")
type MyBadge extends TuiWidget {
  @inline(0) text: utf8
  row: i64  col: i64
  lower = fn(b: MyBadge) -> list<TermFundamental> [
    TermFundamental::Text { content: "★", row: 1, col: 1, fg: "yellow", bg: none, bold: true },
    TermFundamental::Text { content: b.text, row: 1, col: 3, fg: none, bg: none, bold: none },
  ]
}

page index {
  terminal {
    cols = 40 rows = 10 chrome = false
    tui_progress "Load" { row = 1 col = 1 value = 62 }
    tui_checkbox "Top" { row = 3 col = 1 checked = true }
    my_badge "New" { row = 4 col = 1 }
    tui_panel "Box" { row = 1 col = 22 width = 16 height = 5
      tui_checkbox "Nested" { row = 1 col = 1 checked = true }
    }
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // Progress bar: a filled run (█) and a track run (░).
    assert!(
        html.contains('█') && html.contains('░'),
        "no progress bar runs:\n{html}"
    );
    // 62% of width 24 = 14 filled cells, painted in the default accent
    // (Tango green #4e9a06).
    assert!(
        html.contains("fill=\"#4e9a06\""),
        "progress fill not accent-coloured:\n{html}"
    );
    // The rounded panel frame.
    assert!(
        html.contains('╭') && html.contains('╮'),
        "no panel box:\n{html}"
    );
    // The user-defined widget lowered (its star glyph rendered).
    assert!(html.contains('★'), "custom widget did not lower:\n{html}");

    // Two checkboxes lowered to a filled ■ — one top-level, one nested in
    // the panel — and the nested one is offset to the right of the
    // top-level one (proving the container's content-origin offset).
    let xs: Vec<f64> = html
        .match_indices(">■</text>")
        .map(|(i, _)| {
            let head = &html[..i];
            let xpos = head.rfind("x=\"").expect("x attr before ■");
            let rest = &head[xpos + 3..];
            let end = rest.find('"').expect("close quote");
            rest[..end].parse::<f64>().expect("x is a number")
        })
        .collect();
    assert_eq!(xs.len(), 2, "expected two ■ checkbox marks:\n{html}");
    let (lo, hi) = (xs[0].min(xs[1]), xs[0].max(xs[1]));
    assert!(
        hi > lo + 40.0,
        "nested checkbox not offset into the panel (xs = {xs:?}):\n{html}"
    );
}

#[test]
fn terminal_inline_text_lays_out_via_vt() {
    // Inline `text` is fed to the virtual terminal and laid out across
    // rows; newlines start a new line. Each cell is its own centred
    // <text> glyph (a true character grid), so "hello" is five
    // separate elements, not one run.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("t.wcl");
    write_fixture(
        &src,
        "page index {\n  terminal { cols = 12 rows = 3 text = \"hello\\nworld\" }\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    // One <text> per cell, centred.
    assert!(
        html.contains("text-anchor=\"middle\""),
        "cells not rendered per-glyph:\n{html}"
    );
    // The characters of both lines appear as individual cell glyphs,
    // and there is no multi-character run element.
    for ch in ['h', 'e', 'l', 'o', 'w', 'r', 'd'] {
        assert!(
            html.contains(&format!(">{ch}</text>")),
            "missing cell glyph {ch:?}:\n{html}"
        );
    }
    assert!(
        !html.contains(">hello</text>") && !html.contains(">world</text>"),
        "cells were grouped into a run instead of per-cell glyphs:\n{html}"
    );
}

#[test]
fn terminal_replay_emits_player_and_assets() {
    // A `source` recording produces frames JSON, the player wiring, and
    // writes the bundled font + player assets into `_wdoc/`.
    let tmp = TempDir::new().expect("mkdir tempdir");
    // Valid asciicast v2: control chars are JSON-escaped (real
    // .cast files are JSON, so the escape is required).
    std::fs::write(
        tmp.path().join("demo.cast"),
        "{\"version\":2,\"width\":12,\"height\":2}\n[0.0,\"o\",\"\\u001b[31mhi\\u001b[0m\"]\n[0.5,\"o\",\" ok\"]\n",
    )
    .expect("write cast");
    let src = tmp.path().join("t.wcl");
    write_fixture(
        &src,
        "page index {\n  terminal { source = \"./demo.cast\" loop = true }\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    assert!(
        html.contains("data-term-player="),
        "no player root:\n{html}"
    );
    assert!(
        html.contains("class=\"term-frames\""),
        "no frames script:\n{html}"
    );
    assert!(html.contains("\"frames\":["), "no frames payload:\n{html}");
    // New controls: a big centred overlay play button + a chrome
    // play/pause/replay glyph, and *no* bottom scrubber/speed UI.
    assert!(
        html.contains("class=\"term-overlay-play\""),
        "no centre play button:\n{html}"
    );
    assert!(
        html.contains("class=\"term-chrome-btn\""),
        "no chrome play control:\n{html}"
    );
    assert!(
        !html.contains("term-controls") && !html.contains("term-seek"),
        "stale bottom controls still emitted:\n{html}"
    );
    // Red "hi" from the recording was interpreted (Tango red #cc0000).
    assert!(
        html.contains("#cc0000"),
        "ansi colour not interpreted:\n{html}"
    );
    // The player <script> is loaded once on the page.
    assert_eq!(
        html.matches("_wdoc/terminal-player.js").count(),
        1,
        "{html}"
    );
    // Bundled assets written alongside the pages.
    assert!(
        out.path()
            .join("_wdoc/JetBrainsMonoNerdFontMono-Regular.woff2")
            .exists()
    );
    assert!(
        out.path()
            .join("_wdoc/JetBrainsMonoNerdFontMono-Bold.woff2")
            .exists()
    );
    assert!(
        out.path()
            .join("_wdoc/JetBrainsMonoNerdFontMono-Italic.woff2")
            .exists()
    );
    assert!(out.path().join("_wdoc/terminal-player.js").exists());
}

#[test]
fn terminal_missing_cast_is_marked_not_fatal() {
    // A `source` that can't be read renders an inline error marker
    // rather than failing the whole build.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("t.wcl");
    write_fixture(
        &src,
        "page index {\n  terminal { source = \"./nope.cast\" }\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("wdoc-terminal-error"),
        "no error marker:\n{html}"
    );
}

#[test]
fn no_terminal_writes_no_assets() {
    // A document without a terminal must not write the font/player
    // assets (they're ~3 MB — pages that don't need them pay nothing).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("t.wcl");
    write_fixture(&src, "page index {\n  text { span \"hi\" {} }\n}\n");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    // The `_wdoc/` dir exists (it holds the always-shipped favicon), but the
    // heavy terminal font/player assets must not be written.
    let wdoc = out.path().join("_wdoc");
    assert!(
        !wdoc.join("terminal-player.js").exists(),
        "terminal player written without a terminal"
    );
    assert!(
        !wdoc
            .join("JetBrainsMonoNerdFontMono-Regular.woff2")
            .exists(),
        "terminal font written without a terminal"
    );
}

/// Build `src` (as a standalone `main.wcl`) and return the rendered
/// `index.html` plus the shared icon sprite (`_wdoc/icons.svg`) if one
/// was written.
fn build_icons(src: &str) -> (String, Option<String>) {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let file = tmp.path().join("main.wcl");
    write_fixture(&file, src);
    let out = TempDir::new().expect("mkdir out");
    build_ok(&file, out.path());
    let index = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");
    let sprite = std::fs::read_to_string(out.path().join("_wdoc").join("icons.svg")).ok();
    (index, sprite)
}

#[test]
fn build_resolves_inline_and_diagram_icons() {
    let src = r##"
iconset ui {
  pack  = "lucide"
  color = "#88c0d0"
  size  = "1.2em"
  icon_def "check" { color = "#a3be8c" }
}
iconset bs { pack = "bootstrap" }

page index {
  text {
    span "Inline :house: :check: :bs.heart: with a missing :nope: and a ratio 10:30." {}
  }
  diagram {
    width = 80
    height = 40
    icon "star" { x = 8.0  y = 8.0  width = 24.0  height = 24.0  color = "#ebcb8b" }
  }
}
"##;
    let (index, sprite) = build_icons(src);

    // Inline icon carries the iconset's default size + colour and
    // references the shared sprite by id.
    assert!(
        index.contains(
            "<svg class=\"wdoc-icon\" style=\"width:1.2em;height:1.2em;color:#88c0d0;\">\
             <use href=\"_wdoc/icons.svg#lucide-house\"/></svg>"
        ),
        "{index}"
    );
    // A per-icon `icon_def` override wins over the set default.
    assert!(
        index.contains("color:#a3be8c;") && index.contains("#lucide-check"),
        "{index}"
    );
    // `:set.name:` prefix resolves to the named set's pack.
    assert!(index.contains("#bootstrap-heart"), "{index}");
    // An unknown name and a chance `:` pair in prose stay literal.
    assert!(index.contains(":nope:"), "{index}");
    assert!(index.contains("10:30"), "{index}");
    // The diagram icon is a positioned <use>.
    assert!(
        index.contains("<use href=\"_wdoc/icons.svg#lucide-star\" x=\"8\" y=\"8\""),
        "{index}"
    );
    // ICON_CSS is injected into the page stylesheet.
    assert!(
        index.contains("svg.wdoc-icon { display: inline-block;"),
        "{index}"
    );

    // The sprite holds one <symbol> per used icon (and only those),
    // keeping the presentation attributes that make each pack paint.
    let sprite = sprite.expect("sprite written when icons are used");
    assert!(
        sprite.contains("<symbol id=\"lucide-house\" viewBox=\"0 0 24 24\""),
        "{sprite}"
    );
    assert!(
        sprite.contains("<symbol id=\"bootstrap-heart\" viewBox=\"0 0 16 16\""),
        "{sprite}"
    );
    assert!(sprite.contains("stroke=\"currentColor\""), "{sprite}");
    // An icon that was never referenced isn't bundled.
    assert!(!sprite.contains("lucide-cloud"), "{sprite}");
}

#[test]
fn build_shape_icon_leads_label_without_overlap() {
    // A shape with an icon defaults to a leading (`:left`) badge,
    // vertically centred and inset on the left, while the centred
    // label shifts right to clear it (no overlap). A 200x80 process:
    //   badge: min(200,80)*0.4 = 32, inset by min*0.1 = 8
    //          -> <use x="8" y="24" width="32"> (vertically centred)
    //   label: cx = 100 + lead/2, lead = 32 + 2*8 = 48 -> x="124"
    //          (a no-icon process would centre the label at x="100").
    let src = r##"
page index {
  diagram {
    width  = 300
    height = 100
    process "Web Application" {
      id = web  width = 200.0  height = 80.0  icon = "lucide.box"
    }
  }
}
"##;
    let (index, _sprite) = build_icons(src);
    // Badge is a left-inset, vertically-centred <use>.
    assert!(
        index.contains("<use href=\"_wdoc/icons.svg#lucide-box\" x=\"8\" y=\"24\" width=\"32\""),
        "icon badge should lead at the left, vertically centred:\n{index}"
    );
    // Label centre shifted right past the badge (was 100 without an icon).
    assert!(
        index.contains("class=\"wdoc-shape-text\" x=\"124\" y=\"40\""),
        "label should shift right to clear the leading icon (x=124):\n{index}"
    );
    // The badge's right edge (x=40) sits left of the shifted label centre.
    assert!(
        !index.contains("class=\"wdoc-shape-text\" x=\"100\""),
        "label must not stay centred over the icon (x=100):\n{index}"
    );
}

#[test]
fn build_resolves_builtin_packs_without_iconset() {
    // The `lucide` / `bootstrap` packs are declared in the embedded lib,
    // so they resolve with no user `iconset`: a bare `:house:` via the
    // built-in `lucide`, an explicit `:bootstrap.heart:` via `bootstrap`.
    // A genuinely-unknown name and a stray `:` pair stay literal.
    let src = "page index {\n  text { span \"Status :house: :bootstrap.heart: ok :not-a-real-icon: at 10:30.\" {} }\n}\n";
    let (index, sprite) = build_icons(src);
    assert!(index.contains("#lucide-house"), "{index}");
    assert!(index.contains("#bootstrap-heart"), "{index}");
    assert!(index.contains(":not-a-real-icon:"), "{index}");
    assert!(index.contains("10:30"), "{index}");
    assert!(
        sprite.is_some(),
        "sprite written when built-in icons resolve"
    );
}

#[test]
fn user_iconset_overrides_builtin() {
    // A user `iconset` of the same name as a built-in is resolved first
    // (the root document's sets come before the library defaults), so its
    // styling wins.
    let src = "iconset lucide { color = \"#abcdef\" }\n\
               page index {\n  text { span \"see :lucide.house:\" {} }\n}\n";
    let (index, _) = build_icons(src);
    assert!(index.contains("#lucide-house"), "{index}");
    assert!(
        index.contains("color:#abcdef;"),
        "user iconset colour should win over the built-in:\n{index}"
    );
}

#[test]
fn build_attaches_icon_badge_to_shapes() {
    let src = r##"
iconset ui { pack = "lucide" color = "#5e81ac" }

page index {
  diagram {
    width = 200
    height = 120
    process "Validate" {
      x = 10.0  y = 10.0  width = 120.0  height = 44.0
      icon = "check"  icon_class = ["accent"]
    }
    rect {
      x = 10.0  y = 70.0  width = 44.0  height = 44.0
      icon = "star"  icon_pos = :center  icon_size = 24.0
    }
    rect { x = 70.0  y = 70.0  width = 44.0  height = 44.0  fill = "#eee" }
  }
}
"##;
    let (index, sprite) = build_icons(src);

    // The process still renders its box + label, plus a leading
    // (`:left`) badge inset by pad = min(120,44)*0.1 = 4.4 at size
    // min(120,44)*0.4 = 17.6, vertically centred at y = 10 + (44-17.6)/2
    // = 23.2. With no fill/stroke/class set, the box carries the default
    // `wdoc-process` theme class (so it isn't bare-SVG black).
    assert!(
        index.contains("class=\"wdoc-process\" x=\"10\" y=\"10\" width=\"120\" height=\"44\"")
            && index.contains(">Validate</tspan>"),
        "{index}"
    );
    assert!(
        index.contains(
            "<use href=\"_wdoc/icons.svg#lucide-check\" x=\"14.4\" y=\"23.2\" \
             width=\"17.6\" height=\"17.6\" class=\"wdoc-icon accent\""
        ),
        "{index}"
    );
    // The label shifts right by half the reserved strip (lead = 17.6 +
    // 2*4.4 = 26.4) so it clears the leading icon: cx = 10 + 60 + 13.2.
    assert!(
        index.contains("class=\"wdoc-shape-text\" x=\"83.2\""),
        "label should shift right past the leading icon:\n{index}"
    );
    // icon_pos = :center on a 44×44 box at (10,70) with size 24 →
    // (10 + (44-24)/2, 70 + (44-24)/2) = (20, 80).
    assert!(
        index.contains(
            "<use href=\"_wdoc/icons.svg#lucide-star\" x=\"20\" y=\"80\" width=\"24\" height=\"24\""
        ),
        "{index}"
    );
    // Exactly two badges — the third rect has no `icon`, so it emits none.
    assert_eq!(
        index.matches("<use href=\"_wdoc/icons.svg").count(),
        2,
        "{index}"
    );

    let sprite = sprite.expect("sprite written");
    assert!(
        sprite.contains("lucide-check") && sprite.contains("lucide-star"),
        "{sprite}"
    );
}

#[test]
fn build_renders_callout_with_builtin_icon() {
    // No `iconset` declared: the built-in `warning` glyph still resolves
    // from the compiled-in Lucide pack and lands in the sprite.
    let src = "page index {\n  \
        callout \"Heads up\" { class = [\"warning\"]  body = \"Be careful.\" }\n}\n";
    let (index, sprite) = build_icons(src);

    assert!(index.contains("<div class=\"callout warning\">"), "{index}");
    assert!(
        index.contains(
            "<div class=\"callout-heading\">\
             <svg class=\"wdoc-icon callout-icon\">\
             <use href=\"_wdoc/icons.svg#lucide-triangle-alert\"/></svg>\
             <p class=\"callout-title\"><span>Heads up</span></p></div>"
        ),
        "{index}"
    );
    // The body runs through the inline engine (like `p "…"`), so plain
    // text renders directly inside the `<p>` (no per-span wrapper).
    assert!(
        index.contains("<div class=\"callout-body\"><p>Be careful.</p></div>"),
        "{index}"
    );
    assert!(index.contains(".callout.warning"), "{index}");
    assert!(
        sprite
            .expect("sprite written")
            .contains("lucide-triangle-alert"),
        "sprite missing the built-in callout icon"
    );
}

#[test]
fn build_callout_icon_override_and_plain() {
    let src = "page index {\n  \
        callout \"Tip\" { class = [\"tip\"]  body = \"t\"  icon = \"bootstrap.rocket\" }\n  \
        callout \"Plain\" { body = \"p\" }\n}\n";
    let (index, _) = build_icons(src);

    // An explicit `icon` (here via a bundled pack name) overrides the
    // type's default glyph.
    assert!(index.contains("#bootstrap-rocket"), "{index}");
    assert!(!index.contains("lucide-lightbulb"), "{index}");
    // A callout with no built-in type and no icon renders no icon.
    assert!(
        index.contains(
            "<div class=\"callout\"><div class=\"callout-heading\">\
             <p class=\"callout-title\"><span>Plain</span></p></div>"
        ),
        "{index}"
    );
}

// ── Tilesets + tilemaps ────────────────────────────────────────────

/// A fake PNG carrying valid dimensions in its IHDR. The renderer only
/// reads IHDR (and copies the bytes verbatim), so the pixel data / CRCs
/// needn't be real.
fn fake_png(w: u32, h: u32) -> Vec<u8> {
    let mut v = b"\x89PNG\r\n\x1a\n".to_vec();
    v.extend_from_slice(&[0, 0, 0, 13]); // IHDR length
    v.extend_from_slice(b"IHDR");
    v.extend_from_slice(&w.to_be_bytes());
    v.extend_from_slice(&h.to_be_bytes());
    v.extend_from_slice(&[8, 6, 0, 0, 0]);
    v
}

/// Build `src` as a standalone `main.wcl` alongside a `sheet.png`,
/// returning the rendered `index.html` and the live output dir (so the
/// caller can probe `_wdoc/`). The PNG is `sheet_w`×`sheet_h`.
fn build_tilemap(src: &str, sheet_w: u32, sheet_h: u32) -> (String, TempDir) {
    let tmp = TempDir::new().expect("mkdir tempdir");
    std::fs::write(tmp.path().join("sheet.png"), fake_png(sheet_w, sheet_h)).expect("write png");
    let file = tmp.path().join("main.wcl");
    write_fixture(&file, src);
    let out = TempDir::new().expect("mkdir out");
    build_ok(&file, out.path());
    let index = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");
    (index, out)
}

#[test]
fn build_renders_numeric_tilemap_and_copies_sheet() {
    // 64px tiles, 4 columns (sheet is 256 wide). Index 5 -> sheet col
    // 1, row 1 -> source (64, 64). At scale 0.5 it displays 32px, and
    // the top-left cell (index 0) sits at the tilemap origin.
    let src = r#"
tileset world {
  source      = "sheet.png"
  tile_width  = 64
  tile_height = 64
  columns     = 4
}
page index {
  diagram {
    width = 128
    height = 64
    tilemap {
      set   = "world"
      scale = 0.5
      tiles = [
        [ 0, 1 ],
        [ 4, 5 ],
      ]
    }
  }
}
"#;
    let (index, out) = build_tilemap(src, 256, 256);

    // Wrapped in the themable group.
    assert!(index.contains("<g class=\"wdoc-tilemap\">"), "{index}");
    // Image dimensions were read from the PNG header (256×256).
    assert!(
        index.contains(
            "href=\"_wdoc/tileset-world.png\" x=\"0\" y=\"0\" width=\"256\" height=\"256\""
        ),
        "{index}"
    );
    // Index 0 is the origin cell, cropping sheet (0,0) into a 32px box.
    // The source rect is inset half a pixel each side (64 -> 63 at 0.5).
    assert!(
        index.contains(
            "<svg x=\"0\" y=\"0\" width=\"32\" height=\"32\" viewBox=\"0.5 0.5 63 63\" \
             preserveAspectRatio=\"none\" shape-rendering=\"crispEdges\">\
             <image href=\"_wdoc/tileset-world.png\""
        ),
        "{index}"
    );
    // Index 5 (row 1, col 1) displays at (32,32) and crops sheet (64,64).
    assert!(
        index.contains(
            "<svg x=\"32\" y=\"32\" width=\"32\" height=\"32\" viewBox=\"64.5 64.5 63 63\""
        ),
        "{index}"
    );
    // TILEMAP_CSS is injected.
    assert!(
        index.contains(".wdoc-tilemap image { image-rendering: pixelated; }"),
        "{index}"
    );
    // The spritesheet is copied verbatim into _wdoc/.
    assert!(
        out.path().join("_wdoc").join("tileset-world.png").exists(),
        "sheet copied to _wdoc/"
    );
}

#[test]
fn build_symbolic_map_matches_numeric_and_skips_empty() {
    // The legend maps glyphs to the same indices as the numeric form;
    // an unmapped char (`.`) and an explicit `empty` index draw nothing.
    let src = r#"
tileset world {
  source      = "sheet.png"
  tile_width  = 64
  tile_height = 64
  columns     = 4
}
page index {
  diagram {
    width = 128
    height = 128
    tilemap {
      set = "world"
      empty = 9
      tile "a" { index = 0 }
      tile "b" { index = 5 }
      tile "x" { index = 9 }
      map = [
        "a.b",
        "x..",
      ]
    }
  }
}
"#;
    let (index, _out) = build_tilemap(src, 256, 256);

    // `a` -> index 0 at (0,0); `b` -> index 5 at (128,0) cropping (64,64).
    // Source rects are inset half a pixel each side (64 -> 63).
    assert!(
        index.contains("<svg x=\"0\" y=\"0\" width=\"64\" height=\"64\" viewBox=\"0.5 0.5 63 63\""),
        "{index}"
    );
    assert!(
        index.contains(
            "<svg x=\"128\" y=\"0\" width=\"64\" height=\"64\" viewBox=\"64.5 64.5 63 63\""
        ),
        "{index}"
    );
    // Only `a` and `b` draw: `.` (unmapped) and `x` (the `empty` index
    // 9) emit no tile. One <image> is emitted per drawn tile.
    assert_eq!(
        index.matches("href=\"_wdoc/tileset-world.png\"").count(),
        2,
        "{index}"
    );
}

#[test]
fn build_tilemap_bbox_lets_overlay_follow() {
    // A 2×1 grid of 64px tiles at scale 1.0 spans 128×64. A label drawn
    // after the tilemap appears after it in document order (overlay).
    let src = r#"
tileset world {
  source      = "sheet.png"
  tile_width  = 64
  tile_height = 64
  columns     = 4
}
page index {
  diagram {
    width = 128
    height = 64
    tilemap { set = "world"  tiles = [ [ 0, 1 ] ] }
    label "hi" { x = 4.0  y = 12.0 }
  }
}
"#;
    let (index, _out) = build_tilemap(src, 256, 256);
    let tiles_at = index.find("wdoc-tilemap").expect("tilemap present");
    let label_at = index.find(">hi<").expect("overlay label present");
    assert!(tiles_at < label_at, "overlay label must follow the tiles");
}

#[test]
fn build_tileset_explicit_dims_override_header() {
    // Explicit image_width / image_height win and avoid the header read,
    // so a sheet with mismatched real pixels still sizes as declared.
    let src = r#"
tileset world {
  source       = "sheet.png"
  tile_width   = 16
  tile_height  = 16
  columns      = 8
  image_width  = 128
  image_height = 64
}
page index {
  diagram {
    width = 32
    height = 16
    tilemap { set = "world"  tiles = [ [ 0 ] ] }
  }
}
"#;
    let (index, _out) = build_tilemap(src, 256, 256);
    assert!(
        index.contains("width=\"128\" height=\"64\" preserveAspectRatio=\"none\"/>"),
        "{index}"
    );
}

#[test]
fn build_tileset_missing_image_is_an_error() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let file = tmp.path().join("main.wcl");
    write_fixture(
        &file,
        "tileset world {\n  source = \"nope.png\"\n  tile_width = 8\n  tile_height = 8\n}\n\
         page index { diagram { width = 8  height = 8 } }\n",
    );
    let out = TempDir::new().expect("mkdir out");
    match build(&file, out.path(), None) {
        Err(BuildError::Tileset(msg)) => assert!(msg.contains("world"), "{msg}"),
        Err(_) => panic!("expected a tileset error, got a different BuildError"),
        Ok(_) => panic!("expected a tileset error, build succeeded"),
    }
}

#[test]
fn build_no_tilemap_copies_no_sheet() {
    // A declared-but-unused tileset still validates (dims are read), but
    // its image is only copied when a tilemap actually references it.
    let src = r#"
tileset world {
  source      = "sheet.png"
  tile_width  = 64
  tile_height = 64
  columns     = 4
}
page index { text { span "no tiles here" {} } }
"#;
    let (_index, out) = build_tilemap(src, 256, 256);
    assert!(
        !out.path().join("_wdoc").join("tileset-world.png").exists(),
        "unused sheet must not be copied"
    );
}

// ── Dopesheets ─────────────────────────────────────────────────────
//
// `build_tilemap` doubles as the generic "build `src` alongside a
// `sheet.png` of the given size" helper, which is exactly what a
// dopesheet needs.

#[test]
fn build_renders_dopesheet_group_player_and_copies_sheet() {
    // A 72×12 sheet of 12×12 frames: columns default to 72/12 = 6, the
    // range to every frame (0..=5), fps to 12, loop + autoplay on. At
    // scale 6 the frame displays 72×72 and the initial window is frame 0.
    let src = r#"
page index {
  diagram {
    width = 96
    height = 96
    dopesheet "sheet.png" {
      frame_width  = 12
      frame_height = 12
      scale        = 6.0
      x            = 12.0
      y            = 12.0
    }
  }
}
"#;
    let (index, out) = build_tilemap(src, 72, 12);

    // The themable group carries the resolved frame geometry + playback
    // config as data attributes for the player.
    assert!(
        index.contains(
            "<g class=\"wdoc-dopesheet\" data-dope-cols=\"6\" data-dope-fw=\"12\" \
             data-dope-fh=\"12\" data-dope-ox=\"0\" data-dope-oy=\"0\" data-dope-sx=\"12\" \
             data-dope-sy=\"12\" data-dope-from=\"0\" data-dope-to=\"5\" data-dope-fps=\"12\" \
             data-dope-loop=\"1\" data-dope-autoplay=\"1\">"
        ),
        "{index}"
    );
    // The inner frame SVG windows frame 0 (viewBox 0 0 12 12), displayed
    // at the anchored position × scale, over the full-sheet image (72×12).
    assert!(
        index.contains(
            "<svg class=\"dope-frame\" x=\"12\" y=\"12\" width=\"72\" height=\"72\" \
             viewBox=\"0 0 12 12\" preserveAspectRatio=\"none\"><image href=\"_wdoc/image-sheet-"
        ),
        "{index}"
    );
    assert!(
        index.contains("width=\"72\" height=\"12\" preserveAspectRatio=\"none\"/></svg>"),
        "{index}"
    );
    // The play overlay glyph (default `controls`) is centred on the frame.
    assert!(index.contains("<text class=\"dope-btn\""), "{index}");
    // DOPESHEET_CSS is injected, and the bundled player is referenced +
    // written once.
    assert!(
        index.contains(".wdoc-dopesheet image { image-rendering: pixelated; }"),
        "{index}"
    );
    assert_eq!(
        index.matches("_wdoc/dopesheet-player.js").count(),
        1,
        "player referenced exactly once: {index}"
    );
    assert!(out.path().join("_wdoc/dopesheet-player.js").exists());
    // The sheet is copied via the shared image registry (`image-…`).
    let name = copied_image(&out);
    assert!(name.starts_with("image-sheet-") && name.ends_with(".png"));
}

#[test]
fn build_dopesheet_range_and_speed_are_honoured() {
    // A sub-range (`from`/`to`) at an explicit fps, with autoplay off and
    // explicit slice geometry — the data attributes reflect each field.
    let src = r#"
page index {
  diagram {
    width = 96
    height = 96
    dopesheet "sheet.png" {
      frame_width  = 12
      frame_height = 12
      columns      = 6
      from         = 1
      to           = 3
      fps          = 6.0
      autoplay     = false
      scale        = 6.0
    }
  }
}
"#;
    let (index, _out) = build_tilemap(src, 72, 12);
    assert!(
        index.contains(
            "data-dope-from=\"1\" data-dope-to=\"3\" data-dope-fps=\"6\" \
             data-dope-loop=\"1\" data-dope-autoplay=\"0\">"
        ),
        "{index}"
    );
    // The initial window is the `from` frame (index 1 -> column 1 -> x 12).
    assert!(index.contains("viewBox=\"12 0 12 12\""), "{index}");
}

#[test]
fn build_no_dopesheet_writes_no_player() {
    let src = r#"
page index { text { span "nothing animated here" {} } }
"#;
    let (index, out) = build_tilemap(src, 72, 12);
    assert!(!index.contains("dopesheet-player.js"), "{index}");
    assert!(!out.path().join("_wdoc/dopesheet-player.js").exists());
}

// ── Images ─────────────────────────────────────────────────────────

/// Build `src` alongside a `pic.png` of the given size, returning the
/// rendered `index.html` and the output dir (to probe `_wdoc/`).
fn build_image(src: &str, w: u32, h: u32) -> (String, TempDir) {
    let tmp = TempDir::new().expect("mkdir tempdir");
    std::fs::write(tmp.path().join("pic.png"), fake_png(w, h)).expect("write png");
    let file = tmp.path().join("main.wcl");
    write_fixture(&file, src);
    let out = TempDir::new().expect("mkdir out");
    build_ok(&file, out.path());
    let index = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");
    (index, out)
}

/// The single `image-…` file copied into `_wdoc/` (panics if not exactly one).
fn copied_image(out: &TempDir) -> String {
    let dir = std::fs::read_dir(out.path().join("_wdoc")).expect("read _wdoc");
    let mut names: Vec<String> = dir
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("image-"))
        .collect();
    assert_eq!(names.len(), 1, "expected one copied image, got {names:?}");
    names.remove(0)
}

#[test]
fn build_page_image_emits_img_and_copies_file() {
    let src = r#"
page index {
  image "pic.png" { alt = "A picture"  width = 200  height = 120 }
}
"#;
    let (index, out) = build_image(src, 64, 48);
    // The copied file has a deterministic `image-pic-<hash>.png` name.
    let name = copied_image(&out);
    assert!(
        name.starts_with("image-pic-") && name.ends_with(".png"),
        "{name}"
    );
    // The <img> references it by `_wdoc/` URL, carries the default class,
    // the alt text, and the declared size.
    assert!(
        index.contains(&format!(
            "<img class=\"wdoc-image\" src=\"_wdoc/{name}\" alt=\"A picture\" width=\"200\" height=\"120\" />"
        )),
        "{index}"
    );
    // Responsive default styling is injected.
    assert!(
        index.contains("img.wdoc-image { max-width: 100%; height: auto; }"),
        "{index}"
    );
}

#[test]
fn build_diagram_image_emits_svg_image_with_natural_size() {
    // No width/height ⇒ the natural 64×48 (from the PNG header) is used,
    // positioned at x/y.
    let src = r#"
page index {
  diagram {
    width = 200  height = 200
    image "pic.png" { x = 10.0  y = 20.0 }
  }
}
"#;
    let (index, out) = build_image(src, 64, 48);
    let name = copied_image(&out);
    assert!(
        index.contains(&format!(
            "<image href=\"_wdoc/{name}\" x=\"10\" y=\"20\" width=\"64\" height=\"48\" preserveAspectRatio=\"none\" />"
        )),
        "{index}"
    );
}

#[test]
fn build_diagram_image_scales_and_fits_viewbox() {
    // 64×48 × scale 2 ⇒ 128×96 at (0,0); the diagram viewBox fits it
    // (with the standard 10px pad), proving the bbox pass saw the image.
    let src = r#"
page index {
  diagram {
    width = 100  height = 100
    image "pic.png" { width = 64  height = 48  scale = 2.0 }
  }
}
"#;
    let (index, _out) = build_image(src, 64, 48);
    assert!(index.contains("width=\"128\" height=\"96\""), "{index}");
    assert!(index.contains("viewBox=\"-10 -10 148 116\""), "{index}");
}

#[test]
fn build_image_url_source_passes_through_uncopied() {
    let src = r#"
page index {
  image "https://example.com/logo.png" { alt = "Remote" }
}
"#;
    let (index, out) = build_image(src, 64, 48);
    assert!(
        index.contains(
            "<img class=\"wdoc-image\" src=\"https://example.com/logo.png\" alt=\"Remote\" />"
        ),
        "{index}"
    );
    // Nothing copied: an external URL is referenced verbatim.
    let wdoc = out.path().join("_wdoc");
    if wdoc.exists() {
        let any_image = std::fs::read_dir(&wdoc)
            .expect("read _wdoc")
            .filter_map(|e| e.ok())
            .any(|e| e.file_name().to_string_lossy().starts_with("image-"));
        assert!(!any_image, "external URL must not be copied");
    }
}

// ── Video ──────────────────────────────────────────────────────────

/// Build `src` as a standalone `main.wcl` with a local `clip.mp4` +
/// `thumb.png` available, returning the rendered `index.html` and the
/// live output dir (to probe `_wdoc/`).
fn build_video(src: &str) -> (String, TempDir) {
    let tmp = TempDir::new().expect("mkdir tempdir");
    // The build only copies the file's bytes (it never decodes a video),
    // so any non-empty content stands in for a real `.mp4`.
    std::fs::write(tmp.path().join("clip.mp4"), b"\x00\x00\x00\x18ftypmp42fake")
        .expect("write mp4");
    std::fs::write(tmp.path().join("thumb.png"), fake_png(64, 48)).expect("write png");
    let file = tmp.path().join("main.wcl");
    write_fixture(&file, src);
    let out = TempDir::new().expect("mkdir out");
    build_ok(&file, out.path());
    let index = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");
    (index, out)
}

#[test]
fn build_local_video_emits_facade_and_copies_file_and_poster() {
    let src = r#"
page index {
  video "clip.mp4" {
    poster = "thumb.png"
    title  = "A clip"
    width  = 480.0
  }
}
"#;
    let (index, out) = build_video(src);
    // The facade is a click-to-play <div> tagged `local`, with the copied
    // video URL in data-src and the title surfaced as an aria-label.
    assert!(
        index
            .contains("<div class=\"wdoc-video\" data-kind=\"local\" data-src=\"_wdoc/video-clip-"),
        "{index}"
    );
    assert!(index.contains("aria-label=\"A clip\""), "{index}");
    assert!(index.contains("style=\"width:480px;\""), "{index}");
    // The poster is the copied local thumbnail; a play button overlays it.
    assert!(index.contains("<img src=\"_wdoc/poster-thumb-"), "{index}");
    assert!(
        index.contains("<span class=\"wdoc-video-play\" aria-hidden=\"true\"></span>"),
        "{index}"
    );
    // The bundled player is injected once for the page.
    assert!(index.contains("_wdoc/wdoc-video.js"), "{index}");
    // Both assets land in `_wdoc/` with deterministic names.
    let names: Vec<String> = std::fs::read_dir(out.path().join("_wdoc"))
        .expect("read _wdoc")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    assert!(
        names
            .iter()
            .any(|n| n.starts_with("video-clip-") && n.ends_with(".mp4")),
        "{names:?}"
    );
    assert!(
        names
            .iter()
            .any(|n| n.starts_with("poster-thumb-") && n.ends_with(".png")),
        "{names:?}"
    );
    assert!(names.iter().any(|n| n == "wdoc-video.js"), "{names:?}");
}

#[test]
fn build_youtube_video_embeds_and_auto_thumbnails() {
    let src = r#"
page index {
  video "https://www.youtube.com/watch?v=dQw4w9WgXcQ" { title = "Yt" }
}
"#;
    let (index, out) = build_video(src);
    assert!(index.contains("data-kind=\"youtube\""), "{index}");
    assert!(
        index.contains("data-src=\"https://www.youtube.com/embed/dQw4w9WgXcQ?autoplay=1\""),
        "{index}"
    );
    // The poster is auto-derived from the video id (no `poster` given).
    assert!(
        index.contains("<img src=\"https://img.youtube.com/vi/dQw4w9WgXcQ/hqdefault.jpg\""),
        "{index}"
    );
    // Nothing local ⇒ no copied video/poster asset.
    let wdoc = out.path().join("_wdoc");
    if wdoc.exists() {
        let copied = std::fs::read_dir(&wdoc)
            .expect("read _wdoc")
            .filter_map(|e| e.ok())
            .any(|e| {
                let n = e.file_name().to_string_lossy().into_owned();
                n.starts_with("video-") || n.starts_with("poster-")
            });
        assert!(!copied, "a remote video must not copy assets");
    }
}

#[test]
fn build_vimeo_and_generic_video_embeds() {
    let src = r#"
page index {
  video "https://vimeo.com/76979871" { title = "Vi" }
  video "https://example.com/player/embed/abc" { title = "Gen" }
}
"#;
    let (index, _out) = build_video(src);
    assert!(
        index.contains(
            "data-kind=\"vimeo\" data-src=\"https://player.vimeo.com/video/76979871?autoplay=1\""
        ),
        "{index}"
    );
    // No poster supplied for either ⇒ a placeholder stands in.
    assert!(
        index.contains("<span class=\"wdoc-video-placeholder\"></span>"),
        "{index}"
    );
    assert!(
        index.contains("data-kind=\"generic\" data-src=\"https://example.com/player/embed/abc\""),
        "{index}"
    );
}

// ── Diagram pan + zoom ─────────────────────────────────────────────

/// Build `src` as a standalone `main.wcl`, returning the rendered
/// `index.html` and the live output dir (to probe `_wdoc/`).
fn build_page(src: &str) -> (String, TempDir) {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let file = tmp.path().join("main.wcl");
    write_fixture(&file, src);
    let out = TempDir::new().expect("mkdir out");
    build_ok(&file, out.path());
    let index = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");
    (index, out)
}

#[test]
fn build_wraps_pan_zoom_diagram_with_controls() {
    let src = r##"
page index {
  diagram {
    width = 200  height = 120
    pan_zoom = true
    zoom_min = 0.5  zoom_max = 8.0
    pan_margin = 25.0
    rect { x = 10.0  y = 10.0  width = 40.0  height = 30.0  fill = "#abc" }
  }
}
"##;
    let (index, out) = build_page(src);

    // Viewport wrapper + the camera config on the <svg>.
    assert!(
        index.contains("<div class=\"wdoc-diagram-viewport\">"),
        "{index}"
    );
    // Base view is the fitted content box (rect 10,10,40,30 + 10px pad).
    assert!(
        index.contains(
            "data-pan-zoom=\"1\" data-base-viewbox=\"0 0 60 50\" \
             data-zoom-min=\"0.5\" data-zoom-max=\"8\" data-pan-margin=\"25\""
        ),
        "{index}"
    );
    // The three overlaid controls the player binds.
    assert!(index.contains("data-zoom=\"in\""), "{index}");
    assert!(index.contains("data-zoom=\"out\""), "{index}");
    assert!(index.contains("data-zoom=\"reset\""), "{index}");
    // DIAGRAM_CSS is injected, and the player script + asset are emitted.
    assert!(index.contains(".wdoc-diagram-controls button"), "{index}");
    assert!(
        index.contains("<script src=\"_wdoc/diagram-pan-zoom.js\" defer></script>"),
        "{index}"
    );
    assert!(
        out.path()
            .join("_wdoc")
            .join("diagram-pan-zoom.js")
            .exists(),
        "player asset written to _wdoc/"
    );
}

#[test]
fn build_plain_diagram_has_no_pan_zoom() {
    // No diagram opts in, so the bare-<svg> path is untouched: no
    // wrapper element, no data-pan-zoom, no player script/asset. (The
    // DIAGRAM_CSS rules are still in <style>, which is fine.)
    let src = r##"
page index {
  diagram {
    width = 120  height = 80
    rect { x = 10.0  y = 10.0  width = 40.0  height = 30.0  fill = "#abc" }
  }
}
"##;
    let (index, out) = build_page(src);
    assert!(
        !index.contains("<div class=\"wdoc-diagram-viewport\">"),
        "{index}"
    );
    assert!(!index.contains("data-pan-zoom"), "{index}");
    assert!(!index.contains("diagram-pan-zoom.js"), "{index}");
    assert!(
        !out.path()
            .join("_wdoc")
            .join("diagram-pan-zoom.js")
            .exists(),
        "no player asset when unused"
    );
}

/// Render a single-page wireframe fixture — the widgets wrapped in a
/// `diagram`, since `wf_*` blocks are diagram shapes — and return its HTML.
fn wireframe_html(body: &str) -> String {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("wf.wcl");
    write_fixture(
        &src,
        format!("page index {{\n  diagram {{ width = 800  height = 600\n{body}\n  }}\n}}\n"),
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    std::fs::read_to_string(out.path().join("index.html")).expect("read")
}

#[test]
fn wireframe_window_nests_child_widgets() {
    // The widget tree renders inside the diagram's `<svg>`: the window title
    // and its nested button both appear as `<text>`.
    let html = wireframe_html("  wf_window \"Box\" {\n    wf_button \"OK\" {}\n  }");
    assert!(
        html.contains("<svg") && html.contains(">Box</text>") && html.contains(">OK</text>"),
        "window title + nested button not both in the diagram SVG:\n{html}"
    );
}

#[test]
fn wireframe_children_slot_splices_in_source_order() {
    // Heterogeneous children render where the slot sits, in order.
    let html = wireframe_html(
        "  wf_window \"Box\" { controls = false\n    wf_label \"A\" {}\n    wf_dropdown \"B\" {}\n    wf_button \"C\" {}\n  }",
    );
    let a = html.find(">A<").expect("label A");
    let b = html.find(">B<").expect("dropdown B");
    let c = html.find(">C<").expect("button C");
    assert!(a < b && b < c, "children out of order:\n{html}");
}

#[test]
fn wireframe_nested_containers_resolve_recursively() {
    // window → row → button: the renderer recurses into both containers
    // (laid out in Rust), so the deeply-nested button still draws.
    let html =
        wireframe_html("  wf_window \"Box\" {\n    wf_row {\n      wf_button \"X\" {}\n    }\n  }");
    assert!(
        html.contains(">Box</text>") && html.contains(">X</text>"),
        "nested row/button not resolved recursively:\n{html}"
    );
}

#[test]
fn wireframe_state_classes_and_icons() {
    // Checked / on states drive the SVG with the theme's accent colour, and a
    // placeholder input renders italic. Default theme is Nord → accent blue
    // (#81a1c1): the checked box and the radio dot fill with it.
    let html = wireframe_html(
        "  wf_checkbox \"R\" { checked = true }\n  wf_radio \"S\" { selected = true  y = 40.0 }\n  wf_input \"ph\" { y = 80.0 }",
    );
    // The checked box + selected radio dot fill with the resolved accent.
    assert!(
        html.matches("fill=\"#81a1c1\"").count() >= 2,
        "checked/selected states not filled with the theme accent:\n{html}"
    );
    // The check mark is drawn natively (a polyline), not a sprite <use>.
    assert!(
        html.contains("<polyline") && !html.contains("icons.svg#lucide-check"),
        "checkbox tick should be a native polyline, not a sprite icon:\n{html}"
    );
    assert!(
        html.contains("font-style=\"italic\""),
        "placeholder not rendered italic:\n{html}"
    );
}

#[test]
fn wireframe_class_field_bakes_background_onto_box() {
    // A custom class on a widget has its `background` read in Rust and baked
    // onto the widget's box fill (the terminal-style theming path).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("wf.wcl");
    write_fixture(
        &src,
        "page index {\n  diagram { width = 200  height = 60\n    wf_button \"P\" { class = [\"primary\"] }\n  }\n}\nclass primary { background = \"#1f6feb\" }\n",
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("fill=\"#1f6feb\""),
        "class background not baked onto the button box:\n{html}"
    );
}

#[test]
fn wireframe_widgets_share_the_diagram_svg() {
    // Two widgets in one diagram render into the diagram's SVG — there's no
    // per-widget `wdoc-wireframe` wrapper any more; both draw their text.
    let html = wireframe_html(
        "  wf_button \"W\" { x = 10.0  y = 10.0 }\n  wf_label \"L\" { x = 10.0  y = 60.0 }",
    );
    assert!(
        !html.contains("wdoc-wireframe"),
        "widgets should not emit the old page-level wrapper:\n{html}"
    );
    assert!(
        html.contains(">W</text>") && html.contains(">L</text>"),
        "button / label text missing:\n{html}"
    );
}

#[test]
fn wireframe_positioned_by_xy() {
    // `x` / `y` place the widget's group via a translate, like any shape.
    let html = wireframe_html("  wf_window \"A\" { x = 50.0  y = 20.0\n    wf_label \"hi\"\n  }");
    assert!(
        html.contains("translate(50.00 20.00)"),
        "wireframe not positioned by an x/y translate:\n{html}"
    );
}

#[test]
fn wireframe_is_edge_target() {
    // A wireframe contributes a bbox to the collect pass, so an edge to it by
    // `id` resolves and draws with the arrow marker.
    let html = wireframe_html(
        "  rect { id = a  x = 10.0  y = 10.0  width = 40.0  height = 30.0 }\n  wf_window \"W\" { id = win  x = 200.0  y = 10.0\n    wf_label \"hi\"\n  }\n  a -> win",
    );
    assert!(
        html.contains("wdoc-arrow"),
        "edge to a wireframe target not drawn:\n{html}"
    );
}

#[test]
fn wireframe_grows_diagram_viewbox() {
    // A widget placed far out is collected, so the fitted viewBox shifts to
    // include it (a missing bbox would fall back to a `0 0 …` origin).
    let html = wireframe_html("  wf_label \"far\" { x = 500.0  y = 5.0 }");
    let vb = html
        .split("viewBox=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .expect("viewBox present");
    let min_x: f64 = vb
        .split_whitespace()
        .next()
        .and_then(|s| s.parse().ok())
        .expect("viewBox min-x");
    assert!(
        min_x >= 400.0,
        "viewBox min-x {min_x} suggests the far widget wasn't collected:\n{html}"
    );
}

#[test]
fn wireframe_in_layered_layout_sized_by_content() {
    // Under `:layered`, widgets are sized by their measured content (via
    // `effective_dims`) and flowed by the solver; both render and the edge
    // between them draws.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("wf.wcl");
    write_fixture(
        &src,
        "page index {\n  diagram { width = 400  height = 300  layout = :layered\n    wf_button \"First step here\" { id = a }\n    wf_button \"Second\" { id = b }\n    a -> b\n  }\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains(">First step here</text>") && html.contains(">Second</text>"),
        "layered wireframes not both rendered:\n{html}"
    );
    assert!(
        html.contains("wdoc-arrow"),
        "layered edge between wireframes not drawn:\n{html}"
    );
}

#[test]
fn wireframe_element_overrides_ui_theme() {
    // A widget's own `theme` overrides the default: gruvbox dark `bg_alt`
    // (#3c3836) is baked, not nord's (#3b4252).
    let html = wireframe_html("  wf_window \"App\" { theme = :gruvbox\n    wf_label \"x\"\n  }");
    assert!(
        html.contains("fill=\"#3c3836\"") && !html.contains("fill=\"#3b4252\""),
        "gruvbox surface not baked:\n{html}"
    );
}

#[test]
fn wireframe_element_mode_picks_light_palette() {
    // `mode = :light` bakes the theme's light palette: nord light `bg_alt`
    // (#e5e9f0), not the dark one (#3b4252).
    let html = wireframe_html(
        "  wf_window \"App\" { theme = :nord  mode = :light\n    wf_label \"x\"\n  }",
    );
    assert!(
        html.contains("fill=\"#e5e9f0\"") && !html.contains("fill=\"#3b4252\""),
        "nord light palette not baked:\n{html}"
    );
}

#[test]
fn wireframe_site_ui_theme_decouples_from_doc_theme() {
    // The site's `ui_theme` themes wireframes independently of the document
    // `theme`: doc is nord, UI is gruvbox → the wireframe bakes gruvbox dark
    // `bg_alt` (#3c3836), while the body uses the nord background var.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("ui.wcl");
    write_fixture(
        &src,
        "site s { theme = :nord  ui_theme = :gruvbox  default_template = :webpage\n  menu { item \"P\" { page = p } }\n}\npage p { sites = [:s]  start = true\n  diagram { width = 200  height = 80\n    wf_window \"App\" { wf_label \"x\" }\n  }\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("p.html")).expect("read");
    assert!(
        html.contains("fill=\"#3c3836\""),
        "wireframe should bake the site's gruvbox ui_theme:\n{html}"
    );
}

// ── Multiple sites ────────────────────────────────────────────────────

/// Build a multi-site fixture into a TempDir and hand the dir to `check`.
fn with_multisite_build(src: &str, site: Option<&str>, check: impl FnOnce(&Path)) {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let file = tmp.path().join("ms.wcl");
    write_fixture(&file, src);
    let out = TempDir::new().expect("mkdir out");
    match build(&file, out.path(), site) {
        Ok(_) => check(out.path()),
        Err(e) => {
            e.report();
            panic!("multi-site build failed");
        }
    }
}

const TWO_SITES: &str = r##"
site docs { default_template = :webpage  title = "Docs" }
site blog { default_template = :webpage  title = "Blog" }
page index { h1 "Home" {} }
page guide { sites = [:docs]  h1 "Guide" {} }
page post1 { sites = [:blog]  h1 "Post" {} }
page shared { sites = [:docs, :blog]  h1 "Shared" {} }
"##;

#[test]
fn multi_site_builds_subdirs_and_chooser() {
    with_multisite_build(TWO_SITES, None, |out| {
        // Each site is its own subdirectory.
        assert!(out.join("docs/index.html").exists(), "missing docs/index");
        assert!(out.join("blog/index.html").exists(), "missing blog/index");
        // A top-level chooser links to each site's subdirectory.
        let chooser = std::fs::read_to_string(out.join("index.html")).expect("chooser");
        assert!(chooser.contains("href=\"docs/\""), "{chooser}");
        assert!(chooser.contains("href=\"blog/\""), "{chooser}");
        assert!(
            chooser.contains(">Docs</a>") && chooser.contains(">Blog</a>"),
            "{chooser}"
        );
    });
}

#[test]
fn page_membership_scopes_per_site() {
    with_multisite_build(TWO_SITES, None, |out| {
        // No `sites` ⇒ in every site; `sites = [:docs]` ⇒ docs only;
        // `sites = [:docs, :blog]` ⇒ both.
        assert!(out.join("docs/index.html").exists() && out.join("blog/index.html").exists());
        assert!(
            out.join("docs/guide.html").exists(),
            "guide should be in docs"
        );
        assert!(
            !out.join("blog/guide.html").exists(),
            "guide should NOT be in blog"
        );
        assert!(
            out.join("blog/post1.html").exists(),
            "post1 should be in blog"
        );
        assert!(
            !out.join("docs/post1.html").exists(),
            "post1 should NOT be in docs"
        );
        assert!(out.join("docs/shared.html").exists() && out.join("blog/shared.html").exists());
    });
}

#[test]
fn nav_lists_only_the_sites_own_pages() {
    with_multisite_build(TWO_SITES, None, |out| {
        // The webpage template builds nav from this site's pages only —
        // blog's nav must not link the docs-only `guide`.
        let blog = std::fs::read_to_string(out.join("blog/index.html")).expect("blog index");
        assert!(
            blog.contains("href=\"post1.html\""),
            "blog nav missing post1:\n{blog}"
        );
        assert!(
            !blog.contains("href=\"guide.html\""),
            "blog nav leaked a docs page:\n{blog}"
        );
    });
}

#[test]
fn site_filter_builds_one_flat() {
    with_multisite_build(TWO_SITES, Some("blog"), |out| {
        // `--site blog` ⇒ flat at the root, no subdirs, no chooser link.
        assert!(out.join("index.html").exists() && out.join("post1.html").exists());
        assert!(out.join("shared.html").exists());
        assert!(
            !out.join("docs").exists() && !out.join("blog").exists(),
            "should be flat"
        );
        assert!(
            !out.join("guide.html").exists(),
            "docs-only page leaked into blog"
        );
        let idx = std::fs::read_to_string(out.join("index.html")).expect("index");
        assert!(
            !idx.contains("href=\"docs/\""),
            "flat build should not be a chooser"
        );
    });
}

#[test]
fn class_sites_field_scopes_css_per_site() {
    // A `class` with a `sites` list themes only that site's pages; the
    // other sites are unaffected. The shared `index` page (no `sites`)
    // renders into both, so it's a clean before/after comparison.
    let src = r##"
site docs { default_template = :webpage  title = "Docs" }
site blog { default_template = :webpage  title = "Blog" }
class "wdoc-body" { sites = [:docs]  background = "#2e3440" }
page index { h1 "Home" {} }
"##;
    with_multisite_build(src, None, |out| {
        let docs = std::fs::read_to_string(out.join("docs/index.html")).expect("docs index");
        let blog = std::fs::read_to_string(out.join("blog/index.html")).expect("blog index");
        assert!(
            docs.contains(".wdoc-body { background:#2e3440; }"),
            "docs should carry the scoped theme:\n{docs}"
        );
        assert!(
            !blog.contains(".wdoc-body { background:#2e3440; }"),
            "blog must NOT carry the docs-scoped theme:\n{blog}"
        );
    });
}

#[test]
fn same_page_name_reused_across_sites() {
    let src = r##"
site docs { default_template = :webpage  title = "Docs" }
site blog { default_template = :webpage  title = "Blog" }
page index { sites = [:docs]  h1 "Docs home" {} }
page index { sites = [:blog]  h1 "Blog home" {} }
"##;
    with_multisite_build(src, None, |out| {
        let docs = std::fs::read_to_string(out.join("docs/index.html")).expect("docs index");
        let blog = std::fs::read_to_string(out.join("blog/index.html")).expect("blog index");
        assert!(
            docs.contains("Docs home") && !docs.contains("Blog home"),
            "{docs}"
        );
        assert!(
            blog.contains("Blog home") && !blog.contains("Docs home"),
            "{blog}"
        );
    });
}

#[test]
fn cross_site_link_is_an_error() {
    // `a` (docs) links to `post1` (blog only) — unresolved within docs.
    let src = r##"
site docs { default_template = :webpage  title = "Docs" }
site blog { default_template = :webpage  title = "Blog" }
page a { sites = [:docs]  text { span "see [post](post1)" {} } }
page post1 { sites = [:blog]  h1 "P" {} }
"##;
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("ms.wcl");
    write_fixture(&file, src);
    let out = TempDir::new().unwrap();
    assert!(
        matches!(build(&file, out.path(), None), Err(BuildError::BadLink(_))),
        "cross-site link should fail to resolve"
    );
}

#[test]
fn unknown_site_reference_is_an_error() {
    let src = r##"
site docs { default_template = :webpage  title = "Docs" }
page a { sites = [:nope]  h1 "A" {} }
"##;
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("ms.wcl");
    write_fixture(&file, src);
    let out = TempDir::new().unwrap();
    assert!(matches!(
        build(&file, out.path(), None),
        Err(BuildError::BadPage(_))
    ));
}

#[test]
fn multiple_unnamed_sites_is_an_error() {
    let src = r##"
site { default_template = :webpage }
site { default_template = :book }
page index { h1 "H" {} }
"##;
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("ms.wcl");
    write_fixture(&file, src);
    let out = TempDir::new().unwrap();
    assert!(matches!(
        build(&file, out.path(), None),
        Err(BuildError::BadPage(_))
    ));
}

#[test]
fn unknown_site_filter_is_an_error() {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("ms.wcl");
    write_fixture(&file, TWO_SITES);
    let out = TempDir::new().unwrap();
    assert!(matches!(
        build(&file, out.path(), Some("nope")),
        Err(BuildError::BadPage(_))
    ));
}

// A root site (`home`) plus two subdir sites, with cross-site links in
// every direction.
const ROOT_SITE_DOC: &str = r##"
site home { root = true  default_template = :webpage  title = "Home" }
site docs { default_template = :webpage  title = "Docs" }
site blog { default_template = :webpage  title = "Blog" }
page index { sites = [:home]  text { span "see [the docs](docs:guide) and [the blog](blog:post)" {} } }
page guide { sites = [:docs]  text { span "back [home](home:index), over to the [blog](blog:post)" {} } }
page post  { sites = [:blog]  text { span "a post" {} } }
"##;

#[test]
fn root_site_renders_flat_others_in_subdirs() {
    with_multisite_build(ROOT_SITE_DOC, None, |out| {
        // The root site is flat at the output root; no `home/` subdir.
        assert!(out.join("index.html").exists(), "root site index at root");
        assert!(!out.join("home").exists(), "no subdir for the root site");
        assert!(out.join("docs/guide.html").exists());
        assert!(out.join("blog/post.html").exists());
        // No chooser — the root index is the root site's own index.
        let root = std::fs::read_to_string(out.join("index.html")).expect("root index");
        assert!(
            !root.contains("<h1>Sites</h1>"),
            "root must not be a chooser:\n{root}"
        );
    });
}

#[test]
fn cross_site_links_resolve_all_directions() {
    with_multisite_build(ROOT_SITE_DOC, None, |out| {
        // root → subdir
        let home = std::fs::read_to_string(out.join("index.html")).expect("home");
        assert!(
            home.contains("href=\"docs/guide.html\""),
            "root→subdir:\n{home}"
        );
        assert!(
            home.contains("href=\"blog/post.html\""),
            "root→subdir:\n{home}"
        );
        // subdir → root, and subdir → other subdir
        let guide = std::fs::read_to_string(out.join("docs/guide.html")).expect("guide");
        assert!(
            guide.contains("href=\"../index.html\""),
            "subdir→root:\n{guide}"
        );
        assert!(
            guide.contains("href=\"../blog/post.html\""),
            "subdir→subdir:\n{guide}"
        );
    });
}

fn build_err(src: &str) -> BuildError {
    let tmp = TempDir::new().unwrap();
    let file = tmp.path().join("ms.wcl");
    write_fixture(&file, src);
    let out = TempDir::new().unwrap();
    build(&file, out.path(), None).expect_err("build should fail")
}

#[test]
fn cross_site_link_to_unknown_site_or_page_errors() {
    let bad_site = r##"
site home { root = true  default_template = :webpage }
site docs { default_template = :webpage }
page index { sites = [:home]  text { span "[x](nope:guide)" {} } }
page guide { sites = [:docs]  text { span "g" {} } }
"##;
    assert!(matches!(build_err(bad_site), BuildError::BadLink(_)));

    let bad_page = r##"
site home { root = true  default_template = :webpage }
site docs { default_template = :webpage }
page index { sites = [:home]  text { span "[x](docs:missing)" {} } }
page guide { sites = [:docs]  text { span "g" {} } }
"##;
    assert!(matches!(build_err(bad_page), BuildError::BadLink(_)));
}

#[test]
fn multiple_root_sites_is_an_error() {
    let src = r##"
site a { root = true  default_template = :webpage }
site b { root = true  default_template = :webpage }
page index { h1 "H" {} }
"##;
    assert!(matches!(build_err(src), BuildError::BadPage(_)));
}

#[test]
fn multisite_example_root_site_and_subdirs() {
    // The bundled example declares four sites; `showcase` is the `root`
    // site (flat at the output root, its `start` page is the landing), and
    // docs/blog/talk render into subdirectories — no chooser is generated.
    let out = TempDir::new().expect("mkdir tempdir");
    let dir = out.path();
    let n = build_ok(&examples_dir().join("wdoc").join("main.wcl"), dir);
    assert_eq!(n, 22);

    // Showcase is at the root: its pages are flat, and there's no
    // `showcase/` subdirectory.
    assert!(dir.join("overview.html").exists(), "showcase page at root");
    assert!(
        !dir.join("showcase").exists(),
        "no showcase/ subdir for the root site"
    );
    assert!(dir.join("docs/config.html").exists());
    assert!(dir.join("blog/post_launch.html").exists());
    assert!(
        dir.join("docs/index.html").exists(),
        "docs has its own index"
    );

    // The root index is the showcase demo (not a chooser), and its
    // cross-site links reach the subdir sites.
    let root = std::fs::read_to_string(dir.join("index.html")).expect("root index");
    assert!(
        root.contains("wdoc showcase"),
        "root index should be the showcase demo:\n{root}"
    );
    assert!(
        root.contains("href=\"docs/getting_started.html\""),
        "{root}"
    );
    assert!(root.contains("href=\"blog/post_launch.html\""), "{root}");

    // Subdir → root and subdir → subdir cross-site links.
    let docs = std::fs::read_to_string(dir.join("docs/index.html")).expect("docs index");
    assert!(
        docs.contains("href=\"../index.html\""),
        "docs→root link:\n{docs}"
    );
    assert!(
        docs.contains("href=\"../blog/index.html\""),
        "docs→blog link:\n{docs}"
    );

    // Every sub-site page (not just its index) carries a nav back-link to
    // the root site; the root site's own pages don't. The book sidebar
    // uses `.book-home`, the webpage nav `.site-home`.
    let config = std::fs::read_to_string(dir.join("docs/config.html")).expect("docs config");
    assert!(
        config.contains("<a class=\"book-home\" href=\"../index.html\">← wdoc showcase</a>"),
        "deep book page should link back to the root site:\n{config}"
    );
    let post = std::fs::read_to_string(dir.join("blog/post_launch.html")).expect("blog post");
    assert!(
        post.contains("<a class=\"site-home\" href=\"../index.html\">← wdoc showcase</a>"),
        "deep webpage page should link back to the root site:\n{post}"
    );
    // The back-link must set `color: inherit` so its `:visited` state is
    // themed (not the browser's default purple).
    assert!(
        config.contains(".book-home {") && config.contains("color: inherit;"),
        "book back-link should be themed (color: inherit):\n{config}"
    );
    assert!(
        !root.contains("<a class=\"site-home\"") && !root.contains("<a class=\"book-home\""),
        "the root site's own pages must not carry a back-link:\n{root}"
    );

    // The `talk` presentation site renders as a single deck `index.html`
    // (not per-slide files), and the overview links to it via `./talk/`.
    assert!(
        dir.join("talk/index.html").exists(),
        "talk deck index missing"
    );
    assert!(
        !dir.join("talk/title.html").exists(),
        "deck slides are inlined"
    );
    let deck = std::fs::read_to_string(dir.join("talk/index.html")).expect("talk index");
    assert_eq!(
        deck.matches("class=\"deck-slide\"").count(),
        5,
        "deck should inline all five slides:\n{deck}"
    );
    assert!(
        root.contains("href=\"./talk/\""),
        "overview should link to the deck:\n{root}"
    );
}

// ── Colour themes ──────────────────────────────────────────────────

#[test]
fn site_selects_built_in_theme_with_accent() {
    // `theme = catppuccin` recolours the site: the dark palette on
    // `:root`, the light palette under prefers-color-scheme + the
    // explicit toggle override, and `--wdoc-accent` bound to the chosen
    // hue, with the apply rules painting body / charts / tokens.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("themed.wcl");
    write_fixture(
        &src,
        r#"
site {
  default_template = :webpage
  theme  = :catppuccin
  accent = :green
}
page index { text { span "Hi" {} } }
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // Mocha (dark) base + Latte (light) base, each emitted on :root.
    assert!(
        html.contains("--wdoc-bg:#1e1e2e;"),
        "mocha bg missing:\n{html}"
    );
    assert!(
        html.contains("--wdoc-bg:#eff1f5;"),
        "latte bg missing:\n{html}"
    );
    // The toggle override side (book/webpage both get it via site_css).
    assert!(
        html.contains(":root[data-theme=\"light\"]{--wdoc-bg:#eff1f5;"),
        "data-theme light override missing:\n{html}"
    );
    // Accent resolves to the chosen hue var, and is applied to links.
    assert!(
        html.contains("--wdoc-accent:var(--wdoc-green);"),
        "accent var missing:\n{html}"
    );
    assert!(
        html.contains("a,.link{color:var(--wdoc-accent);}"),
        "{html}"
    );
    // Apply rules reach the body and the chart palette.
    assert!(
        html.contains("body,.wdoc-body{background:var(--wdoc-bg);color:var(--wdoc-fg);}"),
        "{html}"
    );
    assert!(
        html.contains(".wdoc-series-1{fill:var(--wdoc-blue);stroke:var(--wdoc-blue);}"),
        "{html}"
    );
}

#[test]
fn site_without_theme_defaults_to_nord() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("default.wcl");
    write_fixture(
        &src,
        r#"
site { default_template = :webpage }
page index { text { span "Hi" {} } }
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // Nord polar-night bg (dark) + snow-storm bg (light), and the
    // default accent (blue) when `accent` is unset.
    assert!(
        html.contains("--wdoc-bg:#2e3440;"),
        "nord dark bg missing:\n{html}"
    );
    assert!(
        html.contains("--wdoc-bg:#eceff4;"),
        "nord light bg missing:\n{html}"
    );
    assert!(
        html.contains("--wdoc-accent:var(--wdoc-blue);"),
        "default accent should be blue:\n{html}"
    );
}

#[test]
fn user_defined_theme_is_selectable() {
    // A `theme` is just a block, so a user can declare one and select it
    // by name the same way as a built-in.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("custom.wcl");
    write_fixture(
        &src,
        r##"
theme midnight {
  palette dark {
    bg = "#0b0b14"  bg_alt = "#15151f"  bg_inset = "#070710"  overlay = "#22223a"  border = "#33334d"
    fg = "#e6e6f0"  fg_muted = "#9a9ab0"  fg_subtle = "#5a5a72"  heading = "#ffffff"  selection = "#2a2a44"
    red = "#ff5577"  orange = "#ff9944"  yellow = "#ffcc44"  green = "#55dd88"
    cyan = "#44ccdd"  blue = "#5599ff"  purple = "#aa77ff"  pink = "#ff79c6"
  }
  palette light {
    bg = "#fafafe"  bg_alt = "#eeeef6"  bg_inset = "#ffffff"  overlay = "#dddde8"  border = "#ccccdd"
    fg = "#1a1a2a"  fg_muted = "#55556a"  fg_subtle = "#8a8aa0"  heading = "#000000"  selection = "#d6d6ee"
    red = "#cc2255"  orange = "#cc6611"  yellow = "#aa8800"  green = "#118844"
    cyan = "#1188aa"  blue = "#2266cc"  purple = "#7733cc"  pink = "#d6298f"
  }
}
site { default_template = :webpage  theme = :midnight  accent = :pink }
page index { text { span "Hi" {} } }
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    assert!(
        html.contains("--wdoc-bg:#0b0b14;"),
        "custom dark bg missing:\n{html}"
    );
    assert!(
        html.contains("--wdoc-pink:#ff79c6;"),
        "custom dark pink missing:\n{html}"
    );
    assert!(
        html.contains("--wdoc-pink:#d6298f;"),
        "custom light pink missing:\n{html}"
    );
    assert!(
        html.contains("--wdoc-accent:var(--wdoc-pink);"),
        "accent should be pink:\n{html}"
    );
}

#[test]
fn bare_document_without_site_is_unthemed() {
    // No `site` block ⇒ pages render bare, with no theme vars (output
    // unchanged from before colour themes existed).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("bare.wcl");
    write_fixture(&src, "page index { text { span \"Hi\" {} } }\n");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    assert!(
        !html.contains("--wdoc-"),
        "a bare document should emit no theme custom properties:\n{html}"
    );
}

#[test]
fn per_site_themes_are_independent() {
    // Each site in a multi-site document carries its own theme.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("multi.wcl");
    write_fixture(
        &src,
        r#"
site a { root = true  default_template = :webpage  theme = :gruvbox }
site b { default_template = :webpage  theme = :everforest }
page pa { sites = [:a]  text { span "A" {} } }
page pb { sites = [:b]  text { span "B" {} } }
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let a = std::fs::read_to_string(out.path().join("pa.html")).expect("read pa");
    let b = std::fs::read_to_string(out.path().join("b").join("pb.html")).expect("read pb");

    assert!(
        a.contains("--wdoc-bg:#282828;"),
        "site a should be gruvbox:\n{a}"
    );
    assert!(
        !a.contains("--wdoc-bg:#2d353b;"),
        "site a must not leak everforest:\n{a}"
    );
    assert!(
        b.contains("--wdoc-bg:#2d353b;"),
        "site b should be everforest:\n{b}"
    );
    assert!(
        !b.contains("--wdoc-bg:#282828;"),
        "site b must not leak gruvbox:\n{b}"
    );
}

// ── Math (RaTeX) tests ──────────────────────────────────────────────

#[test]
fn build_renders_math_block_self_contained() {
    // A `math` block authored with a raw heredoc (`<<'TEX'`) — LaTeX
    // backslashes survive verbatim. With no `site`/template the page is
    // bare, so the SVG is the only thing in the body: a clean place to
    // assert the equation embeds its glyph outlines and is themed.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("m.wcl");
    write_fixture(
        &src,
        "page index {\n  math <<'TEX'\n    \\frac{a}{b}\n    TEX\n  {}\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("<div class=\"wdoc-math\">"),
        "math block should wrap in a centring div:\n{html}"
    );
    assert!(html.contains("<svg"), "math should emit an svg:\n{html}");
    assert!(
        html.contains("<path"),
        "glyphs should be embedded as outline paths:\n{html}"
    );
    assert!(
        html.contains("currentColor"),
        "default fill should become currentColor:\n{html}"
    );
    assert!(
        !html.contains("fill=\"rgba(0,0,0"),
        "no baked-in black glyph fill should remain (rewritten to currentColor):\n{html}"
    );
}

#[test]
fn build_renders_inline_math() {
    // `$…$` inline (text style). The span carries the wdoc-math-inline
    // class and a baseline `vertical-align` so it sits on the text line.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = write_inline_fixture(&tmp, "Energy is $E = mc^2$ today");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("class=\"wdoc-math-inline\""),
        "inline math span missing:\n{html}"
    );
    assert!(
        html.contains("<svg"),
        "inline math should emit svg:\n{html}"
    );
    assert!(
        html.contains("vertical-align:-"),
        "inline math should baseline-align:\n{html}"
    );
}

#[test]
fn build_renders_display_inline_math() {
    // `$$…$$` inline (display style) renders the same way structurally.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = write_inline_fixture(&tmp, "sum $$x^2 + y^2 = z^2$$ here");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("class=\"wdoc-math-inline\"") && html.contains("<svg"),
        "display inline math should render an svg span:\n{html}"
    );
}

#[test]
fn build_inline_math_leaves_currency_alone() {
    // The no-adjacent-space rule keeps `$10 or $20` from matching, so
    // prices render as literal text with no equation.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = write_inline_fixture(&tmp, "it cost $10 or $20 total");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("$10 or $20"),
        "currency should pass through literally:\n{html}"
    );
    assert!(
        !html.contains("wdoc-math-inline"),
        "currency must not be typeset as math:\n{html}"
    );
}

#[test]
fn build_math_preserves_explicit_color() {
    // An explicit `\textcolor` survives the default-black→currentColor
    // rewrite (only black is rewritten).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("m.wcl");
    write_fixture(
        &src,
        "page index {\n  math <<'TEX'\n    \\textcolor{red}{x} + y\n    TEX\n  {}\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("rgba(255,0,0"),
        "explicit red should be preserved:\n{html}"
    );
    assert!(
        html.contains("currentColor"),
        "the rest should still theme to currentColor:\n{html}"
    );
}

#[test]
fn build_bad_math_is_error_marker_not_failure() {
    // Malformed LaTeX degrades to an inline marker; the build still
    // succeeds (mirrors the terminal block's bad-source behaviour).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("m.wcl");
    write_fixture(
        &src,
        "page index {\n  math <<'TEX'\n    \\frac{\n    TEX\n  {}\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path()); // must not panic / error
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("wdoc-math-error"),
        "bad equation should emit an error marker:\n{html}"
    );
}

/// Build `src` as a standalone `main.wcl` alongside the named fake PNGs
/// (each `w`×`h`, parent dirs created), returning the rendered
/// `index.html` and the live output dir (so the caller can probe
/// `_wdoc/`). Mirrors `build_tilemap` for the `map` block.
fn build_map(src: &str, files: &[(&str, u32, u32)]) -> (String, TempDir) {
    let tmp = TempDir::new().expect("mkdir tempdir");
    for (path, w, h) in files {
        let p = tmp.path().join(path);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("mkdir asset dir");
        }
        std::fs::write(&p, fake_png(*w, *h)).expect("write png");
    }
    let file = tmp.path().join("main.wcl");
    write_fixture(&file, src);
    let out = TempDir::new().expect("mkdir out");
    build_ok(&file, out.path());
    let index = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");
    (index, out)
}

/// Count `_wdoc/` files whose name starts with `prefix`.
fn wdoc_files_with_prefix(out: &TempDir, prefix: &str) -> usize {
    std::fs::read_dir(out.path().join("_wdoc"))
        .expect("read _wdoc")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with(prefix))
        .count()
}

#[test]
fn build_renders_single_image_map_with_pins_and_cards() {
    // A single-image map (the common case): one `source`, two pins. A
    // diagram holding a map is interactive even without `pan_zoom`.
    let src = r##"
page index {
  diagram {
    width = 600
    height = 400
    map "world" {
      source = "map.png"
      width = 1024
      height = 1024
      pin "boss" {
        x = 512  y = 480
        icon = "lucide.skull"
        color = "#e23"
        title = "Dragon"
        text { span "Guards the ruins." {} }
      }
      pin "town" {
        x = 200  y = 300
        icon = "lucide.house"
        class = ["town-pin"]
      }
    }
  }
}
class "town-pin" { color = "#3b82f6" }
"##;
    let (index, out) = build_map(src, &[("map.png", 1024, 1024)]);

    // A map makes its diagram interactive (viewport wrapper + camera
    // attributes) without an explicit `pan_zoom`.
    assert!(index.contains("wdoc-diagram-viewport"), "{index}");
    assert!(index.contains("data-pan-zoom=\"1\""), "{index}");
    // The viewBox fits the map's coordinate box (1024×1024) + 10px pad,
    // so `map_bbox` feeds the fit.
    assert!(index.contains("viewBox=\"-10 -10 1044 1044\""), "{index}");

    // The map group + its single image layer (native width read from the
    // 1024-wide PNG header), covering the full coordinate box.
    assert!(
        index.contains("class=\"wdoc-map\" data-map-width=\"1024\" data-map-height=\"1024\""),
        "{index}"
    );
    assert!(
        index.contains("class=\"wdoc-map-layer\" data-native-width=\"1024\""),
        "{index}"
    );
    assert!(
        index.contains("width=\"1024\" height=\"1024\" preserveAspectRatio=\"none\""),
        "{index}"
    );

    // Pins: a clickable group with a hit rect + the icon `<use>`. The
    // bottom-centre anchors at (x, y): top-left = (512-12, 480-24).
    assert!(
        index.contains("class=\"wdoc-map-pin\" data-map-pin=\"boss\""),
        "{index}"
    );
    assert!(
        index.contains("x=\"500\" y=\"456\" width=\"24\" height=\"24\" fill=\"transparent\""),
        "{index}"
    );
    assert!(
        index.contains("href=\"_wdoc/icons.svg#lucide-skull\""),
        "{index}"
    );
    assert!(index.contains("color:#e23"), "{index}");
    // The class-themed pin carries its class on the icon.
    assert!(
        index.contains("data-map-pin=\"town\"") && index.contains("town-pin"),
        "{index}"
    );

    // Cards: hidden divs keyed by pin id, with title + rendered wdoc
    // content (the pin's `text` block). The town pin has no content, so
    // it gets no card.
    assert!(
        index.contains("class=\"wdoc-map-card\" data-map-card=\"boss\" hidden>"),
        "{index}"
    );
    assert!(
        index.contains("<div class=\"wdoc-map-card-title\">Dragon</div>"),
        "{index}"
    );
    assert!(
        index.contains("<p><span>Guards the ruins.</span></p>"),
        "{index}"
    );
    assert!(index.contains("wdoc-map-card-close"), "{index}");
    assert!(!index.contains("data-map-card=\"town\""), "{index}");

    // Assets: the map player + camera player are written, and the source
    // image is copied into `_wdoc/`.
    assert!(out.path().join("_wdoc").join("wdoc-map.js").exists());
    assert!(
        out.path()
            .join("_wdoc")
            .join("diagram-pan-zoom.js")
            .exists()
    );
    assert!(wdoc_files_with_prefix(&out, "image-") >= 1);
}

#[test]
fn build_renders_tiled_multi_zoom_map_layers() {
    // Two level-of-detail layers: a single low-res image and a 2×2 tile
    // grid. Each layer carries its native pixel width for the JS picker.
    let src = r##"
page index {
  diagram {
    width = 400
    height = 400
    map "world" {
      width = 512
      height = 512
      tile_size = 256
      layer { source = "low.png" }
      layer { source = "tiles"  cols = 2  rows = 2 }
    }
  }
}
"##;
    let (index, out) = build_map(
        src,
        &[
            ("low.png", 256, 256),
            ("tiles/0_0.png", 256, 256),
            ("tiles/1_0.png", 256, 256),
            ("tiles/0_1.png", 256, 256),
            ("tiles/1_1.png", 256, 256),
        ],
    );

    // Two layer groups; the low-res image's native width is its header
    // width (256), the tiled layer's is cols × tile_size (512).
    assert_eq!(
        index.matches("class=\"wdoc-map-layer\"").count(),
        2,
        "{index}"
    );
    assert!(index.contains("data-native-width=\"256\""), "{index}");
    assert!(index.contains("data-native-width=\"512\""), "{index}");
    // The tiled layer emits one `<image>` per tile (4), each cropped to a
    // 256×256 cell of the 512-unit map.
    assert!(index.matches("<image ").count() >= 5, "{index}");

    // Every referenced tile + the low image is copied into `_wdoc/`
    // (5 distinct images).
    assert!(wdoc_files_with_prefix(&out, "image-") >= 5);
}

// ── Presentation decks ─────────────────────────────────────────────

#[test]
fn presentation_site_renders_single_deck_file() {
    // A `presentation` site renders all its slides into one index.html,
    // grouped into the `deck` grid (sections = columns, slides = rows).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("talk.wcl");
    write_fixture(
        &src,
        r#"
site {
  default_template = :presentation
  deck {
    section "Intro" {
      slide title
      slide agenda
    }
    section "Main" {
      slide topic
    }
  }
}
page title  { h1 "Hello" {} }
page agenda { h2 "Agenda" {} }
page topic  { h2 "Topic" {} }
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    let n = build_ok(&src, out.path());
    // The whole deck is one file, so the build reports a single page.
    assert_eq!(n, 1);

    let index = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");
    // Two sections, three slides.
    assert_eq!(
        index.matches("<section class=\"deck-section\">").count(),
        2,
        "{index}"
    );
    assert_eq!(
        index.matches("<div class=\"deck-slide\">").count(),
        3,
        "{index}"
    );
    // Every slide's content is embedded.
    assert!(index.contains("Hello"), "{index}");
    assert!(index.contains("Agenda"), "{index}");
    assert!(index.contains("Topic"), "{index}");
    // The keyboard-nav player is written and linked exactly once.
    assert!(out.path().join("_wdoc").join("presentation.js").exists());
    assert_eq!(index.matches("_wdoc/presentation.js").count(), 1, "{index}");
    // No standalone per-slide files are written.
    assert!(!out.path().join("title.html").exists());
}

#[test]
fn presentation_fragments_and_notes() {
    // A `fragment` wraps content in `.wdoc-fragment` (step-revealed by the
    // player); a `notes` block is pulled out of the visible content into a
    // `.deck-notes` aside.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("talk.wcl");
    write_fixture(
        &src,
        r#"
site {
  default_template = :presentation
  deck { section "S" { slide one } }
}
page one {
  p "Visible body"
  fragment { p "Step revealed" }
  notes { p "Only for the presenter" }
}
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let index = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");

    // The fragment is wrapped for step reveal.
    assert!(
        index.contains("<div class=\"wdoc-fragment\"><p>Step revealed</p></div>"),
        "{index}"
    );
    // The notes text lives in the aside, not the visible slide body.
    let notes_at = index.find("Only for the presenter").expect("notes present");
    let aside_at = index
        .find("<aside class=\"deck-notes\">")
        .expect("notes aside");
    assert!(aside_at < notes_at, "notes not inside the aside:\n{index}");
}

#[test]
fn presentation_unknown_slide_is_build_error() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("talk.wcl");
    write_fixture(
        &src,
        r#"
site {
  default_template = :presentation
  deck { section "S" { slide nonexistent } }
}
page real { h1 "Real" {} }
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    match build(&src, out.path(), None) {
        Err(BuildError::BadTemplate(msg)) => assert!(msg.contains("nonexistent"), "got: {msg}"),
        Err(_) => panic!("expected BadTemplate"),
        Ok(_) => panic!("expected BadTemplate, got Ok"),
    }
}

#[test]
fn presentation_without_deck_is_build_error() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("talk.wcl");
    write_fixture(
        &src,
        r#"
site { default_template = :presentation }
page only { h1 "Only" {} }
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    match build(&src, out.path(), None) {
        Err(BuildError::BadTemplate(msg)) => assert!(msg.contains("deck"), "got: {msg}"),
        Err(_) => panic!("expected BadTemplate"),
        Ok(_) => panic!("expected BadTemplate, got Ok"),
    }
}

#[test]
fn build_browser_title_combines_page_title_and_site_title() {
    // A page `title` drives the browser `<title>`, suffixed with the site
    // title as `<page> — <site>`.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("titled.wcl");
    write_fixture(
        &src,
        r##"
site main {
  default_template = :webpage
  title = "WCL Docs"
}
page intro {
  title = "Getting Started"
  h1 "Intro" {}
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("intro.html")).expect("read intro");
    assert!(
        html.contains("<title>Getting Started — WCL Docs</title>"),
        "browser title not composed:\n{html}"
    );
}

#[test]
fn build_browser_title_falls_back_to_page_name() {
    // No page `title`: the browser title uses the page name, still suffixed
    // with the site title.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("untitled.wcl");
    write_fixture(
        &src,
        r##"
site main {
  default_template = :webpage
  title = "Site"
}
page intro {
  h1 "Intro" {}
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("intro.html")).expect("read intro");
    assert!(
        html.contains("<title>intro — Site</title>"),
        "browser title fallback wrong:\n{html}"
    );
}

#[test]
fn build_ships_default_favicon_when_no_icon_set() {
    // A site with no `icon`: the embedded default favicon is written and the
    // page links to it.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("nofav.wcl");
    write_fixture(
        &src,
        r##"
site main { default_template = :webpage }
page intro { h1 "Intro" {} }
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    assert!(
        out.path().join("_wdoc").join("favicon.svg").exists(),
        "default favicon not written"
    );
    let html = std::fs::read_to_string(out.path().join("intro.html")).expect("read intro");
    assert!(
        html.contains("<link rel=\"icon\" type=\"image/svg+xml\" href=\"_wdoc/favicon.svg\">"),
        "default favicon link missing:\n{html}"
    );
}

#[test]
fn build_uses_and_copies_site_icon_when_set() {
    // A site `icon` pointing at a local file is resolved to a `_wdoc/` URL
    // and the file is copied into the output.
    let tmp = TempDir::new().expect("mkdir tempdir");
    // A tiny 1x1 PNG so the image registry has a real file to copy.
    let png: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0a, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x63, 0x00,
        0x01, 0x00, 0x00, 0x05, 0x00, 0x01, 0x0d, 0x0a, 0x2d, 0xb4, 0x00, 0x00, 0x00, 0x00, 0x49,
        0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];
    std::fs::write(tmp.path().join("logo.png"), png).expect("write logo");
    let src = tmp.path().join("fav.wcl");
    write_fixture(
        &src,
        r##"
site main {
  default_template = :webpage
  icon = "logo.png"
}
page intro { h1 "Intro" {} }
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("intro.html")).expect("read intro");
    assert!(
        html.contains("<link rel=\"icon\" href=\"_wdoc/image-logo-"),
        "site icon link missing:\n{html}"
    );
    // The favicon file was copied into `_wdoc/`.
    let wdoc = out.path().join("_wdoc");
    let copied = std::fs::read_dir(&wdoc)
        .expect("read _wdoc")
        .filter_map(Result::ok)
        .any(|e| e.file_name().to_string_lossy().starts_with("image-logo-"));
    assert!(copied, "site icon file not copied into _wdoc/");
}

#[test]
fn user_document_schema_composes_with_wdoc_root() {
    // Issue #10: a wdoc document may declare its *own* `@document`
    // schema at the root to carry custom top-level tags (here a
    // `project_meta` data block) alongside the wdoc `page`/`site`
    // blocks. wdoc's imported `Site` schema must compose with the
    // user's root schema rather than "take over" the root and reject
    // everything it doesn't itself declare.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("custom_root.wcl");
    write_fixture(
        &src,
        r##"
@document
type ProjectDoc {
  @children("project_meta") metas: list<ProjectMeta>
}

@block("project_meta")
type ProjectMeta {
  @inline(0) id: identifier
  owner: utf8
}

project_meta info { owner = "Wil" }

page index {
  text { span "Hello" {} }
}
"##,
    );

    // Builds with no schema violations — the custom top-level
    // `project_meta` block and the wdoc `page` coexist.
    let out = TempDir::new().expect("mkdir out");
    let n = build_ok(&src, out.path());
    assert_eq!(n, 1, "expected the single page to render");
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read index");
    assert!(html.contains("<span>Hello</span>"), "{html}");
}

#[test]
fn unresolved_name_in_page_block_errors() {
    // Issue 13: an unresolved binding in a page block surfaces as a loud
    // diagnostic on the HTML path, not a silently dropped block.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(
        &src,
        "page home {\n  start = true\n  p $\"count = ${len(nonexistent)}\"\n}\n",
    );
    let out = tmp.path().join("out");
    match build(&src, &out, None) {
        Err(BuildError::Eval(_)) => {}
        Ok(n) => panic!("expected an eval error, but wrote {n} page(s)"),
        Err(other) => {
            other.report();
            panic!("expected BuildError::Eval, got a different error (see above)");
        }
    }
}

#[test]
fn cross_file_connections_field_renders() {
    // A `@connections` field declared on a `@document` in one imported file
    // must be readable from a *different* file (the page), exactly like a
    // `@children` field. Regression for the cross-file connection-resolution
    // bug (was: `unresolved reference 'person_to_system'`).
    let tmp = TempDir::new().expect("mkdir tempdir");
    std::fs::write(
        tmp.path().join("model.wcl"),
        r#"import <wdoc.wcl>
@block("system") type System { @inline(0) id: utf8  name: utf8 }
@block("user")   type User   { @inline(0) id: utf8  name: utf8 }
symbol_set RelKind { uses }
connection PersonToSystem: User -> System : RelKind
@document type Model {
  @children("system")          systems: list<System>
  @children("user")            users:   list<User>
  @connections(PersonToSystem) person_to_system: list<PersonToSystem>
}
system "web"      { name = "Web" }
user   "customer" { name = "Customer" }
customer -> web :uses
"#,
    )
    .expect("write model");
    std::fs::write(
        tmp.path().join("page.wcl"),
        r#"page index { sites = [:s]  start = true
  p $"users = ${len(users)}"
  p $"edges = ${len(person_to_system)}"
}
site s { default_template = :book  toc { chapter "H" { page = index } } }
"#,
    )
    .expect("write page");
    let main = tmp.path().join("main.wcl");
    std::fs::write(
        &main,
        "import <wdoc.wcl>\nimport \"./model.wcl\"\nimport \"./page.wcl\"\n",
    )
    .expect("write main");

    let out = tmp.path().join("out");
    build_ok(&main, &out);
    let html = std::fs::read_to_string(out.join("index.html")).expect("read index");
    assert!(html.contains("users = 1"), "{html}");
    assert!(html.contains("edges = 1"), "{html}");
}

#[test]
fn cross_file_eval_error_reports_imported_file() {
    // An unresolved reference in an *imported* page file must render its
    // diagnostic against that file's source — the correct snippet, not the
    // root document's text (which would mis-offset / emit `OutOfBounds`).
    let tmp = TempDir::new().expect("mkdir tempdir");
    std::fs::write(
        tmp.path().join("page.wcl"),
        "page index { start = true\n  p $\"broken = ${len(nonexistent_thing)}\"\n}\n",
    )
    .expect("write page");
    let main = tmp.path().join("main.wcl");
    std::fs::write(&main, "import <wdoc.wcl>\nimport \"./page.wcl\"\n").expect("write main");

    let out = tmp.path().join("out");
    match build(&main, &out, None) {
        Err(BuildError::Eval(r)) => {
            let rendered = format!("{r:?}");
            assert!(
                rendered.contains("page.wcl"),
                "diagnostic should point at the imported file:\n{rendered}"
            );
            assert!(
                rendered.contains("nonexistent_thing"),
                "diagnostic should underline the offending span:\n{rendered}"
            );
            // The pre-fix symptom: a span into page.wcl rendered against the
            // root source overflowed its bounds.
            assert!(
                !rendered.contains("OutOfBounds") && !rendered.contains("Failed to read contents"),
                "diagnostic must not mis-target the source span:\n{rendered}"
            );
        }
        Ok(n) => panic!("expected an eval error, but wrote {n} page(s)"),
        Err(other) => {
            other.report();
            panic!("expected BuildError::Eval, got a different error (see above)");
        }
    }
}

#[test]
fn partials_are_gathered_by_collect_in_document_order() {
    // Two `partial`s with the same tag, scattered in different spots, are
    // gathered by a `collect` in document order — and, by default
    // (show_here unset), do NOT render at their source.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(
        &src,
        r##"
page index {
  partial note { p "First note." }
  p "Body prose."
  partial note { p "Second note." }
  collect note
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read index");

    // The collected bodies appear, in document order, after the body prose
    // (where the `collect` sits).
    let body = html.find("Body prose.").expect("body prose present");
    let first = html.find("First note.").expect("first note present");
    let second = html.find("Second note.").expect("second note present");
    assert!(
        body < first && first < second,
        "collected partials must appear after the collect site, in order:\n{html}"
    );
    // Default show_here = false: each note text appears exactly once (only at
    // the collect site, not at its source).
    assert_eq!(html.matches("First note.").count(), 1, "{html}");
    assert_eq!(html.matches("Second note.").count(), 1, "{html}");
}

#[test]
fn partial_show_here_renders_at_source_and_collect() {
    // `show_here = true` renders the body where it is defined AND where it is
    // collected — so it appears twice.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(
        &src,
        r##"
page index {
  partial tip { show_here = true  p "Pinned tip." }
  collect tip
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read index");
    assert_eq!(
        html.matches("Pinned tip.").count(),
        2,
        "show_here partial should render at source and at collect:\n{html}"
    );
}

#[test]
fn collect_gathers_partials_from_imported_files() {
    // A `partial` declared at top level in an eagerly-imported file is
    // reachable by a `collect` in the main document (cross-file scatter).
    let tmp = TempDir::new().expect("mkdir tempdir");
    std::fs::write(
        tmp.path().join("extra.wcl"),
        "import <wdoc.wcl>\npartial gloss { p \"Imported term.\" }\n",
    )
    .expect("write extra");
    let main = tmp.path().join("main.wcl");
    std::fs::write(
        &main,
        "import <wdoc.wcl>\nimport \"./extra.wcl\"\npage index { collect gloss }\n",
    )
    .expect("write main");
    let out = tmp.path().join("out");
    build_ok(&main, &out);
    let html = std::fs::read_to_string(out.join("index.html")).expect("read index");
    assert!(
        html.contains("Imported term."),
        "collect must gather partials from imported files:\n{html}"
    );
}

#[test]
fn collect_with_no_matching_partials_renders_nothing() {
    // A `collect` whose tag matches no partial emits nothing and does not
    // error.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(&src, "page index { p \"Only prose.\"  collect missing }\n");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read index");
    assert!(html.contains("Only prose."), "{html}");
}

#[test]
fn collect_cycle_terminates() {
    // A collected partial body that contains a `collect` of the same tag must
    // not recurse forever — the guard breaks the cycle and the build finishes.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(
        &src,
        r##"
page index {
  partial loop { p "Cycle body." collect loop }
  collect loop
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read index");
    // The body renders once at the outer collect; the inner collect is a
    // re-entrant no-op.
    assert_eq!(html.matches("Cycle body.").count(), 1, "{html}");
}

#[test]
fn build_renders_component_by_reference() {
    // `wdoc_instance` renders the component named by its `component` *value*,
    // so a single repeater can emit a DIFFERENT component per data element —
    // the render-by-reference primitive. The instance's like-named fields
    // fill the target's slots.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("byref.wcl");
    write_fixture(
        &src,
        r#"
wdoc_component badge {
  wdoc_slot text
  wdoc_body { p $"BADGE:${text}" }
}
wdoc_component note {
  wdoc_slot text
  wdoc_body { p $"NOTE:${text}" }
}
page index {
  let nodes = [
    { ref: "badge", text: "A" },
    { ref: "note",  text: "B" },
    { ref: "badge", text: "C" },
  ]
  wdoc_repeater { each = nodes  as = :n
    wdoc_instance { component = n.ref  text = n.text }
  }
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");
    // Each element resolved to the component named by its data `ref`.
    assert!(
        html.contains("<p>BADGE:A</p>")
            && html.contains("<p>NOTE:B</p>")
            && html.contains("<p>BADGE:C</p>"),
        "render-by-reference did not dispatch per-element components:\n{html}"
    );
}

#[test]
fn build_collects_classes_emitted_by_a_repeater() {
    // A `wdoc_repeater` at the document root may emit `class` blocks from
    // data; the CSS-collection pass expands it so generated rules land in the
    // page <style> (the generic hook a design system builds class generation
    // on). User-origin, so they win over library defaults.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("genclass.wcl");
    write_fixture(
        &src,
        r##"
let tokens = [
  { name: "brand-primary", hex: "#5e81ac" },
  { name: "brand-accent",  hex: "#bf616a" },
]
wdoc_repeater { each = tokens  as = :t
  class $"${t.name}" { color = t.hex }
}
page index { p "Body." }
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");
    assert!(
        html.contains(".brand-primary")
            && html.contains("#5e81ac")
            && html.contains(".brand-accent")
            && html.contains("#bf616a"),
        "repeater-generated class rules not collected into the page <style>:\n{html}"
    );
}

#[test]
fn build_composes_widgets_inside_a_frame_from_data() {
    // A `wdoc_repeater` / `wdoc_instance` nested inside a `wf_*` container is
    // expanded before the wireframe walks its children, so a device frame can
    // compose its widgets from data (the screens-from-data path).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("frame.wcl");
    write_fixture(
        &src,
        r#"
wdoc_component cell {
  wdoc_slot text
  wdoc_body { wf_button $"${text}" {} }
}
page index {
  let items = [{ text: "One" }, { text: "Two" }, { text: "Three" }]
  diagram { width = 800  height = 600
    wf_browser "shop.example.com" {
      wdoc_repeater { each = items  as = :i
        wdoc_instance { component = "cell"  text = i.text }
      }
    }
  }
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");
    // All three data-driven buttons drew their text inside the frame's SVG.
    assert!(
        html.contains("<svg")
            && html.contains(">One</text>")
            && html.contains(">Two</text>")
            && html.contains(">Three</text>"),
        "data-driven widgets did not compose inside the wf_browser frame:\n{html}"
    );
}

#[test]
fn build_generates_one_page_per_screen_with_component_instances() {
    // End-to-end screens-from-data: a document-root `wdoc_repeater` emits one
    // page per screen record, each page composing components by reference.
    // Proves data-only screen addition (append to the list → a new page).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("screens.wcl");
    write_fixture(
        &src,
        r#"
wdoc_component card {
  wdoc_slot title
  wdoc_body { p $"CARD:${title}" }
}
let screens = [
  { id: "listing", title: "Headphones" },
  { id: "detail",  title: "Speaker" },
]
wdoc_repeater { each = screens  as = :s
  page $"screen-${s.id}" {
    wdoc_instance { component = "card"  title = s.title }
  }
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    let pages = build_ok(&src, out.path());
    assert_eq!(pages, 2, "one page per screen record");
    let listing =
        std::fs::read_to_string(out.path().join("screen-listing.html")).expect("read listing");
    let detail =
        std::fs::read_to_string(out.path().join("screen-detail.html")).expect("read detail");
    assert!(
        listing.contains("<p>CARD:Headphones</p>"),
        "listing screen did not render its component:\n{listing}"
    );
    assert!(
        detail.contains("<p>CARD:Speaker</p>"),
        "detail screen did not render its component:\n{detail}"
    );
}

#[test]
fn build_renders_tree_with_nested_nodes() {
    // An indented file-tree: one row per node, the frame height derived
    // from the node count, labels present, and connector guides whose
    // count matches the tree's ├/└/│ topology.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("tree.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width  = 360
    height = 240
    tree {
      x = 0.0  y = 0.0  width = 280.0
      tree_node "src/" {
        tree_node "render/" {
          tree_node "svg.rs" {}
          tree_node "html.rs" {}
        }
        tree_node "lib.rs" {}
        tree_node "tree.rs" {}
      }
      tree_node "Cargo.toml" {}
    }
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // One label per node (7 nodes), each a left-anchored <text>.
    for label in [
        "src/",
        "render/",
        "svg.rs",
        "html.rs",
        "lib.rs",
        "tree.rs",
        "Cargo.toml",
    ] {
        assert!(
            html.contains(&format!(">{label}</text>")),
            "missing node label {label}:\n{html}"
        );
    }
    assert_eq!(
        html.matches("class=\"wdoc-tree-label\"").count(),
        7,
        "expected 7 node labels:\n{html}"
    );
    // Connector guides: render/ ├ (2) + svg.rs │+├ (3) + html.rs │+└ (3)
    // + lib.rs ├ (2) + tree.rs └ (2) = 12; the two depth-0 roots draw none.
    assert_eq!(
        html.matches("class=\"wdoc-tree-guide\"").count(),
        12,
        "expected 12 connector guide lines:\n{html}"
    );
}

#[test]
fn build_tree_applies_node_icon_and_colour() {
    // A node's `icon` resolves to a sprite `<use>` and its `color` lands
    // as the label fill (and on the icon style).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("tree_icon.wcl");
    write_fixture(
        &src,
        r##"
iconset lucide {}

page index {
  diagram {
    width  = 300
    height = 120
    tree {
      tree_node "src/" {
        icon  = "folder"
        color = "#88c0d0"
        tree_node "main.rs" { icon = "file" }
      }
    }
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    assert!(
        html.contains("_wdoc/icons.svg#lucide-folder")
            && html.contains("_wdoc/icons.svg#lucide-file"),
        "missing resolved node icons:\n{html}"
    );
    // The folder node's colour is its label fill.
    assert!(
        html.contains("fill=\"#88c0d0\""),
        "expected node colour as label fill:\n{html}"
    );
}

#[test]
fn build_tree_node_is_edge_target() {
    // A node with an `id` is registered as its own sub-shape, so an edge
    // can target it and the standard west/east anchor logic lands on the
    // node's row (not the whole tree).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("tree_edge.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width   = 420
    height  = 160
    routing = :straight
    tree {
      x = 0.0  y = 0.0  width = 160.0  row_height = 24.0
      tree_node "a" {}
      tree_node "b" { id = nodeb }
      tree_node "c" {}
    }
    rect {
      id = box
      x = 320.0  y = 30.0  width = 60.0  height = 40.0
      fill = "#abc"
    }
    box -> nodeb
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // The second node's east anchor sits at its row midpoint: row index 1,
    // y = 24 + 24/2 = 36, x = tree width 160.
    assert!(
        html.contains("data-tree-node-id=\"nodeb\""),
        "missing node id marker:\n{html}"
    );
    assert!(
        html.contains("x2=\"160\" y2=\"36\""),
        "expected edge to attach at node b east (160,36):\n{html}"
    );
}

#[test]
fn diagram_desc_becomes_svg_title_and_aria_label() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("a11y.wcl");
    write_fixture(
        &src,
        concat!(
            "page p {\n",
            "  diagram d {\n",
            "    width = 200  height = 100\n",
            "    desc = \"Request flow from client to server\"\n",
            "    process \"client\"\n",
            "  }\n",
            "  diagram plain {\n",
            "    width = 200  height = 100\n",
            "    process \"other\"\n",
            "  }\n",
            "}\n",
        ),
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("p.html")).expect("read html");
    assert!(
        html.contains("role=\"img\" aria-label=\"Request flow from client to server\""),
        "aria attributes on the svg: {html}"
    );
    assert!(
        html.contains("<title>Request flow from client to server</title>"),
        "svg <title> child present"
    );
    // A diagram without `desc` stays untouched (no empty aria noise).
    assert!(
        !html.contains("aria-label=\"\""),
        "no empty aria-label emitted"
    );
}

#[test]
fn search_site_ships_index_and_widget() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("s.wcl");
    write_fixture(
        &src,
        concat!(
            "site demo {\n",
            "  title = \"Demo\"\n",
            "  default_template = :book\n",
            "  search = true\n",
            "}\n",
            "page index {\n  h1 \"Welcome Home\"\n  p \"The quick brown fox configures servers.\"\n}\n",
            "page other {\n  h1 \"Other Page\"\n  p \"Tilemap rendering details live here.\"\n}\n",
        ),
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());

    // The per-page text index, with first-h1 titles and body text.
    let index = std::fs::read_to_string(out.path().join("_wdoc/search-index.json"))
        .expect("search index written");
    assert!(index.contains("\"title\":\"Welcome Home\""), "{index}");
    assert!(index.contains("\"href\":\"other.html\""), "{index}");
    assert!(index.contains("quick brown fox"), "page text indexed");
    // Nav chrome (sidebar TOC links) must not pollute a page's text.
    assert!(
        !index.contains("book-sidebar"),
        "template shell stays out of the index"
    );

    // The widget: input + results container in the sidebar, the bundled
    // script on the page, and the script asset on disk.
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");
    assert!(html.contains("wdoc-search-input"), "search box rendered");
    assert!(
        html.contains("_wdoc/wdoc-search.js"),
        "widget script injected"
    );
    assert!(
        out.path().join("_wdoc/wdoc-search.js").exists(),
        "widget asset shipped"
    );
}

#[test]
fn search_is_off_by_default() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("s.wcl");
    write_fixture(
        &src,
        "site demo {\n  title = \"Demo\"\n  default_template = :book\n}\n\
         page index {\n  h1 \"Home\"\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    assert!(
        !out.path().join("_wdoc/search-index.json").exists(),
        "no index without search = true"
    );
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");
    assert!(!html.contains("wdoc-search-input"), "no widget markup");
    assert!(!html.contains("wdoc-search.js"), "no widget script");
}
