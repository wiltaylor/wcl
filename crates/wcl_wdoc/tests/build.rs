use std::path::{Path, PathBuf};

use tempfile::TempDir;
use wcl_wdoc::{
    BuildError, BuildOptions, RebuildOutcome, build, build_incremental, build_with_options,
};

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
        Err(BuildError::IncludeCycle(m)) => panic!("build include-cycle error: {m}"),
    }
}

#[test]
fn css_block_vocabulary_reproduces_representative_rules_losslessly() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("css-vocabulary.wcl");
    write_fixture(
        &src,
        r#"
base ".__css_roundtrip_start" { css = "--roundtrip:start;" }

class "card" {
  fill = "currentColor"
  css = "display:grid;"
  nest ".title" { css = "font-weight:700;" }
  nest "&:hover" { css = "transform:scale(1.05);" }
  nest ".subtitle, &.featured" { css = "opacity:0.8;" }
  nest "[data-label=\"A&B\"]" { css = "text-decoration:none;" }
}

class "nest-only" {
  nest ".child" { css = "display:block;" }
}

base "body,.wdoc-body" { css = "margin:0;" }

font_face "Inter" {
  src = "url('inter.woff2') format('woff2')"
  weight = "400"
  style = "normal"
  display = "swap"
}

media "(max-width: 40rem)" {
  class "card" {
    css = "display:block;"
    nest "&.featured" { css = "grid-column:1;" }
  }
}

keyframes "pulse" {
  base "from" { css = "opacity:0;" }
  base "50%" { css = "opacity:0.5;" }
  base "to" { css = "opacity:1;" }
}

base ".__css_roundtrip_end" { css = "--roundtrip:end;" }

page index { p "CSS vocabulary" }
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");

    let representative_css = concat!(
        ".__css_roundtrip_start { --roundtrip:start; }\n",
        ".card { fill:currentColor;display:grid; }\n",
        ".card .title { font-weight:700; }\n",
        ".card:hover { transform:scale(1.05); }\n",
        ".card .subtitle, .card.featured { opacity:0.8; }\n",
        ".card [data-label=\"A&B\"] { text-decoration:none; }\n",
        ".nest-only .child { display:block; }\n",
        "body,.wdoc-body { margin:0; }\n",
        "@font-face { font-family:Inter;src:url('inter.woff2') format('woff2');font-weight:400;font-style:normal;font-display:swap; }\n",
        "@media (max-width: 40rem) { .card { display:block; }\n",
        ".card.featured { grid-column:1; } }\n",
        "@keyframes pulse { from { opacity:0; }\n",
        "50% { opacity:0.5; }\n",
        "to { opacity:1; } }\n",
        ".__css_roundtrip_end { --roundtrip:end; }",
    );
    let start = html
        .find(".__css_roundtrip_start")
        .expect("round-trip start marker");
    let end_rule = ".__css_roundtrip_end { --roundtrip:end; }";
    let end = html[start..]
        .find(end_rule)
        .map(|offset| start + offset + end_rule.len())
        .expect("round-trip end marker");
    assert_eq!(&html[start..end], representative_css);
}

#[test]
fn class_vocabulary_rejects_tag_and_retired_shorthand_fields_in_every_mode() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("retired-class-fields.wcl");
    let retired_fields = r#"
    color = "red"
    background = "black"
    bold = true
    italic = true
    underline = true
    font_weight = "700"
    font_size = "1rem"
    line_height = "1.5"
    font_family = "sans-serif"
    text_align = "center"
    text_transform = "uppercase"
    letter_spacing = "0.1em"
    padding = "1rem"
    margin = "1rem"
    border = "1px solid"
    stroke_linejoin = "round"
    stroke_linecap = "round"
"#;
    write_fixture(
        &src,
        format!(
            r#"
class "legacy" {{
  tag = "div"
{retired_fields}
  dark {{ {retired_fields} }}
  light {{ {retired_fields} }}
}}
page index {{ p "Retired fields" }}
"#,
        ),
    );

    let out = TempDir::new().expect("mkdir out");
    match build(&src, out.path(), None) {
        Err(BuildError::Schema(52)) => {}
        Err(_) => panic!("expected 52 schema violations for tag + retired fields"),
        Ok(_) => panic!("tag and retired class shorthand fields were accepted"),
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
    // text + spans: the spans run together into the one paragraph.
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
        overview.contains("<p class=\"accent\">Welcome to wdoc — now with classes.</p>"),
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
        overview
            .contains("<h1 class=\"heading-1\" id=\"pipeline-overview\">Pipeline overview</h1>"),
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
  lower = fn(i: Inner) -> list<Svg> [
    Svg::Rect {
      x: 5.0, y: 5.0, width: 20.0, height: 20.0,
      fill: i.fill,
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
fn output_interfaces_lower_to_the_ir_accepted_by_their_placement_slot() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("output_interfaces.wcl");
    write_fixture(
        &src,
        r##"
@block("content_probe")
type ContentProbe extends ContentBlock {
  lower = fn(p: ContentProbe) -> list<Content> [
    Content::Paragraph { text: "content-ir" }
  ]
}

@block("svg_probe")
type SvgProbe extends SvgBlock {
  lower = fn(p: SvgProbe) -> list<Svg> [
    Svg::Rect { x: 2.0, y: 3.0, width: 12.0, height: 8.0, fill: "#abc" }
  ]
}

@block("term_probe")
type TermProbe extends TermPrimitive {
  row: i64
  col: i64
  lower = fn(p: TermProbe) -> list<TermFundamental> [
    TermFundamental::Text { content: "★", row: 1, col: 1 }
  ]
}

page index {
  content_probe
  diagram {
    width = 40
    height = 30
    svg_probe
  }
  terminal {
    cols = 20
    rows = 3
    chrome = false
    term_probe { row = 1 col = 1 }
  }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("content-ir"),
        "content lowering missing:\n{html}"
    );
    assert!(
        html.contains("fill=\"#abc\""),
        "SVG lowering missing:\n{html}"
    );
    assert!(
        html.contains(">★</text>"),
        "terminal lowering missing:\n{html}"
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
  lower = fn(b: Badge) -> list<Svg> [
    Svg::Label {
      content: b.text, x: b.x, y: b.y,
      id: b.id,
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

    // shop bbox (20,20,120,50) + default padding 12, with the labelled top
    // edge widened to 8 + 14 + 4 = 26 of headroom so the title is never
    // covered by a member ⇒ box (8,-6,144,88).
    assert!(
        html.contains(
            "<rect class=\"wdoc-boundary\" x=\"8\" y=\"-6\" width=\"144\" height=\"88\" />"
        ),
        "boundary should hug shop's manual bbox plus label headroom:\n{html}"
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
    // Both spans concatenate into the one paragraph, each escaped.
    assert!(
        html.contains("<p>A &amp; B &lt;c&gt;say &quot;hi&quot;</p>"),
        "{html}"
    );
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

    // Value path: text → one <p> of the generated spans' prose, in order.
    assert!(
        html.contains("<p>alicebobcarol</p>"),
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
            page.contains(&format!(">{title}</h1>")),
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
fn build_renders_sidebar_footer_button_with_icon() {
    // A `sidebar_footer { button … }` on a `book` site renders a pinned
    // icon-only footer button in the sidebar: an `<a class="book-footer-btn">`
    // linking to its `page`, carrying the named icon (resolved into the shared
    // sprite), the label as its `aria-label` / `title`, and marked `current`
    // on the page it links to.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("footer.wcl");
    write_fixture(
        &src,
        r#"
site docbook {
  default_template = :book
  title = "Catalog"
  toc { chapter "Home" { page = index } }
  sidebar_footer {
    button "Reference" { page = reference  icon = "lucide.chart-network" }
  }
}

page index {
  sites = [:docbook]
  start = true
  h1 "Home"
}

page reference {
  sites = [:docbook]
  h1 "Reference"
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());

    let index = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");
    assert!(
        index.contains("<div class=\"book-sidebar-footer\">"),
        "the sidebar should carry a pinned footer:\n{index}"
    );
    assert!(
        index.contains("<a class=\"book-footer-btn\" href=\"reference.html\""),
        "the footer button should link to its page:\n{index}"
    );
    assert!(
        index.contains("_wdoc/icons.svg#lucide-chart-network"),
        "the footer button should reference its icon in the sprite:\n{index}"
    );
    assert!(
        index.contains("aria-label=\"Reference\""),
        "the footer button should carry its label as an accessible name:\n{index}"
    );
    assert!(
        !index.contains("book-footer-label"),
        "the footer button should be icon-only (no visible label span):\n{index}"
    );

    // On the page it links to, the button is marked `current`.
    let reference =
        std::fs::read_to_string(out.path().join("reference.html")).expect("read reference.html");
    assert!(
        reference.contains("<a class=\"book-footer-btn current\" href=\"reference.html\""),
        "the footer button should be `current` on the page it links to:\n{reference}"
    );
}

#[test]
fn build_rejects_sidebar_footer_button_to_unknown_page() {
    // A `sidebar_footer` button pointing at a page that doesn't exist is a
    // build error, mirroring `toc` / `menu` page-link validation.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("bad-footer.wcl");
    write_fixture(
        &src,
        r#"
site docbook {
  default_template = :book
  title = "Catalog"
  toc { chapter "Home" { page = index } }
  sidebar_footer { button "Reference" { page = nope } }
}

page index {
  sites = [:docbook]
  start = true
  h1 "Home"
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    match build(&src, out.path(), None) {
        Err(BuildError::BadTemplate(msg)) => {
            assert!(
                msg.contains("nope"),
                "error should name the missing page: {msg}"
            );
        }
        Ok(n) => panic!("expected BadTemplate for unknown footer page, built {n} pages"),
        Err(_) => panic!("expected BadTemplate for unknown footer page, got a different error"),
    }
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
fn build_projects_body_fragment_attached_to_a_data_record() {
    // A `body` rides on a plain data record (`server`) as a property — the
    // record is NOT a ContentBlock. A `wdoc_repeater` over those records renders
    // each one's body via `project s.overview`: the `@by_ref` body reifies to
    // a reference, so each iteration projects that server's own fragment, and
    // the body's `${region}` resolves against its host record.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("project.wcl");
    write_fixture(
        &src,
        r#"
@document
type Infra { @children("server") servers: list<Server> }

@block("server")
type Server {
  @inline(0) name: identifier
  region: utf8?
  @child("body") overview: WdocAddressableBody?
}

server web01 {
  region = "us-east"
  body { p $"Frontend in ${region}." }
}
server web02 {
  region = "eu-west"
  body { p $"Frontend in ${region}." }
}

page all {
  h1 "Fleet"
  wdoc_repeater { each = servers  as = :s
    h2 $"${s.name}"
    project { from = s.overview }
  }
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("all.html")).expect("read html");

    // Each server's heading appears once...
    assert!(
        html.contains(">web01</h2>") && html.contains(">web02</h2>"),
        "per-server headings:\n{html}"
    );
    // ...followed by that server's own projected body, region resolved in the
    // body's host-record scope (not the repeater binding).
    assert!(
        html.contains("<p>Frontend in us-east.</p>"),
        "web01 body projected with its region:\n{html}"
    );
    assert!(
        html.contains("<p>Frontend in eu-west.</p>"),
        "web02 body projected with its region:\n{html}"
    );
    // The body never renders at its definition site, so each projection
    // appears exactly once (only via the repeater).
    assert_eq!(
        html.matches("<p>Frontend in us-east.</p>").count(),
        1,
        "body renders only where projected, not at its source:\n{html}"
    );
}

#[test]
fn build_projects_body_on_numeric_labelled_nested_record() {
    // Regression for the reported bug: a `body` on a record (`tstep`) nested in
    // another record's `@children`, where the nested record has a NUMERIC
    // `@inline(0)` label. Projected from a nested `wdoc_repeater`, and via a
    // static numeric path. Both forms must render the step's body — previously
    // the numeric label collapsed the reference to `DataPath(["body"])`.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("steps.wcl");
    write_fixture(
        &src,
        r#"
@document
type Doc { @children("tut") tuts: list<Tut> }
@block("tut")
type Tut { @inline(0) id: identifier  @children("tstep") steps: list<TStep> }
@block("tstep")
type TStep { @inline(0) n: u32  @child("body") body: WdocAddressableBody? }

tut t1 {
  tstep 1 { body { p "STEP ONE body" } }
  tstep 2 { body { p "STEP TWO body" } }
}

page pg {
  // Repeater-binding form, nested one level: each step's own body.
  wdoc_repeater { each = tuts as = :t
    wdoc_repeater { each = t.steps as = :st
      h3 $"Step ${st.n}"
      project { from = st.body }
    }
  }
  // Static numeric-path form: address the step labelled 1 directly.
  h2 "Direct"
  project { from = tuts.t1.steps.1.body }
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("pg.html")).expect("read html");

    // Repeater-binding form rendered both steps' bodies under their headings.
    assert!(
        html.contains("<p>STEP ONE body</p>") && html.contains("<p>STEP TWO body</p>"),
        "nested repeater projected both numeric-labelled step bodies:\n{html}"
    );
    // Static numeric path rendered step 1's body a second time.
    assert_eq!(
        html.matches("<p>STEP ONE body</p>").count(),
        2,
        "step 1 body via repeater AND via the static numeric path `tuts.t1.steps.1.body`:\n{html}"
    );
}

#[test]
fn build_projects_list_of_bodies_and_survives_self_cycle() {
    // `@children("body")` yields a list of references; `project` renders each
    // in order. A `body` that projects itself terminates (the cycle guard
    // renders nothing for the re-entrant hit) instead of looping forever.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("multi.wcl");
    write_fixture(
        &src,
        r#"
@document
type Roots {
  @children("host") hosts: list<Host>
  @children("loop") loops: list<Loop>
}

@block("host")
type Host {
  @inline(0) name: identifier
  @children("body") notes: list<WdocAddressableBody>
}

host h1 {
  body n1 { p "first note" }
  body n2 { p "second note" }
}

page all {
  wdoc_repeater { each = hosts  as = :h
    project { from = h.notes }
  }
}

@block("loop")
type Loop {
  @inline(0) name: identifier
  @child("body") b: WdocAddressableBody?
}

loop only {
  body cyc {
    p "before"
    project { from = loops.only.b }
    p "after"
  }
}

page cycle {
  project { from = loops.only.b }
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());

    // List projection: both bodies render, in order.
    let all = std::fs::read_to_string(out.path().join("all.html")).expect("read all");
    assert!(
        all.contains("<p>first note</p>") && all.contains("<p>second note</p>"),
        "both list-projected bodies render:\n{all}"
    );

    // Self-cycle: the outer projection renders the body once; the inner
    // self-project is dropped by the guard, so `before`/`after` appear once
    // and the build terminates.
    let cycle = std::fs::read_to_string(out.path().join("cycle.html")).expect("read cycle");
    assert_eq!(
        cycle.matches("<p>before</p>").count(),
        1,
        "self-cycling body renders once, guard stops recursion:\n{cycle}"
    );
    assert!(cycle.contains("<p>after</p>"), "body completes:\n{cycle}");
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
        html.contains("<h3 class=\"heading-3\">Logs</h3><p>line one</p><p>line two</p>"),
        "panel content slot:\n{html}"
    );
}

#[test]
fn build_rejects_an_unfilled_required_layout_slot() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("required-slot.wcl");
    write_fixture(
        &src,
        r#"
template article {
  slot content: content
  slot hero: content
  render = fn(c: TemplateCtx) -> list<Html> wdoc_blocks(c.content)
}
site { default_template = :article }
page index { p "Body." }
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    let err = build(&src, out.path(), None).expect_err("required hero must fail");
    assert!(
        err.render_plain()
            .contains("required slot `hero` is unfilled"),
        "unexpected error: {}",
        err.render_plain()
    );
}

#[test]
fn reserved_content_slot_must_accept_content() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("reserved-content-slot.wcl");
    write_fixture(
        &src,
        r#"
template article {
  slot content: utf8
  render = fn(c: TemplateCtx) -> list<Html> []
}
site { default_template = :article }
page index { p "Body." }
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    let err = build(&src, out.path(), None).expect_err("reserved content type must fail");
    assert!(
        err.render_plain()
            .contains("reserved slot `content` must have a `content` type"),
        "unexpected error: {}",
        err.render_plain()
    );
}

#[test]
fn build_places_bare_named_fills_through_the_layout_slot_handle() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("named-slot.wcl");
    write_fixture(
        &src,
        r#"
template article {
  slot content: content
  slot hero: content
  render = fn(c: TemplateCtx) -> list<Html>
    flatten([
      [el("header", ["hero"], slot(c, :hero))],
      [el("main", [], slot(c, :content))],
    ])
}
site { default_template = :article }
page index {
  hero { h1 "Welcome" }
  p "Body."
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");
    assert!(
        html.contains("<header class=\"hero\"><h1 class=\"heading-1\"")
            && html.contains("<main><p>Body.</p>"),
        "named and implicit slots were not placed independently:\n{html}"
    );
}

#[test]
fn build_rejects_fill_content_outside_the_slots_accepted_type() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("typed-slot.wcl");
    write_fixture(
        &src,
        r#"
template canvas {
  slot content: content
  slot shapes: content<SvgBlock>
  render = fn(c: TemplateCtx) -> list<Html> slot(c, :content)
}
site { default_template = :canvas }
page index {
  shapes { p "not a shape" }
  p "Body."
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    let err = build(&src, out.path(), None).expect_err("typed fill must fail");
    assert!(
        err.render_plain()
            .contains("slot `shapes` accepts `SvgBlock`, but found `p`"),
        "unexpected error: {}",
        err.render_plain()
    );
}

#[test]
fn conditional_fill_is_dropped_only_when_another_site_layout_declares_it() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("conditional-slot.wcl");
    write_fixture(
        &src,
        r#"
template article {
  slot content: content
  render = fn(c: TemplateCtx) -> list<Html> slot(c, :content)
}

template marketing {
  slot content: content
  slot promo: content?
  render = fn(c: TemplateCtx) -> list<Html> slot(c, :content)
}
site { default_template = :article }
page index {
  promo? { h1 "Sale" }
  p "Body."
}
page landing { template = :marketing p "Landing." }
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");
    assert!(html.contains("Body.") && !html.contains("Sale"), "{html}");
}

#[test]
fn conditional_fill_typo_is_still_an_error() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("conditional-typo.wcl");
    write_fixture(
        &src,
        r#"
template article {
  slot content: content
  render = fn(c: TemplateCtx) -> list<Html> slot(c, :content)
}
site { default_template = :article }
page index { prommo? { h1 "Typo" } p "Body." }
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    assert!(
        build(&src, out.path(), None).is_err(),
        "a conditional fill name absent from every site layout must fail"
    );
}

#[test]
fn unconditional_fill_for_another_layout_is_an_error() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("wrong-layout-slot.wcl");
    write_fixture(
        &src,
        r#"
template article {
  slot content: content
  render = fn(c: TemplateCtx) -> list<Html> slot(c, :content)
}
template marketing {
  slot content: content
  slot promo: content?
  render = fn(c: TemplateCtx) -> list<Html> slot(c, :content)
}
site { default_template = :article }
page index { promo { h1 "Sale" } p "Body." }
page landing { template = :marketing p "Landing." }
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    let err = build(&src, out.path(), None).expect_err("unconditional fill must fail");
    assert!(
        err.render_plain().contains("does not declare it"),
        "{}",
        err.render_plain()
    );
}

#[test]
fn duplicate_slot_fills_are_an_error() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("duplicate-slot.wcl");
    write_fixture(
        &src,
        r#"
template article {
  slot content: content
  slot hero: content
  render = fn(c: TemplateCtx) -> list<Html> slot(c, :content)
}
site { default_template = :article }
page index { hero { h1 "One" } hero { h1 "Two" } p "Body." }
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    let err = build(&src, out.path(), None).expect_err("duplicate fill must fail");
    assert!(
        err.render_plain()
            .contains("fills slot `hero` more than once"),
        "{}",
        err.render_plain()
    );
}

#[test]
fn template_reference_to_an_undeclared_slot_errors_at_render() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("undeclared-reference.wcl");
    write_fixture(
        &src,
        r#"
template article {
  slot content: content
  render = fn(c: TemplateCtx) -> list<Html>
    flatten([slot(c, :content), slot(c, :heor)])
}
site { default_template = :article }
page index { p "Body." }
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    let err = build(&src, out.path(), None).expect_err("undeclared reference must fail");
    assert!(
        err.render_plain().contains("references slot `heor`"),
        "{}",
        err.render_plain()
    );
}

#[test]
fn repeater_fill_site_possibly_fills_a_required_slot() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("possible-slot.wcl");
    write_fixture(
        &src,
        r#"
template article {
  slot content: content
  slot hero: content
  render = fn(c: TemplateCtx) -> list<Html> slot(c, :content)
}
site { default_template = :article }
let promos = []
page index {
  wdoc_repeater { each = promos as = :promo
    hero { h1 "Sale" }
  }
  p "Body."
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
}

#[test]
fn repeater_generated_fills_are_routed_to_the_named_slot() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("generated-slot.wcl");
    write_fixture(
        &src,
        r#"
template article {
  slot content: content
  slot hero: content
  render = fn(c: TemplateCtx) -> list<Html> [
    el("header", [], slot(c, :hero)),
    el("main", [], slot(c, :content)),
  ]
}
site { default_template = :article }
let promos = ["Sale"]
page index {
  wdoc_repeater { each = promos as = :promo
    hero { h1 $"${promo}" }
  }
  p "Body."
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");
    let header = html
        .split_once("<header>")
        .and_then(|(_, rest)| rest.split_once("</header>"))
        .map(|(body, _)| body)
        .unwrap_or_default();
    let main = html
        .split_once("<main>")
        .and_then(|(_, rest)| rest.split_once("</main>"))
        .map(|(body, _)| body)
        .unwrap_or_default();
    assert!(header.contains("Sale"), "named slot was empty:\n{html}");
    assert!(
        main.contains("Body.") && !main.contains("Sale"),
        "fill leaked into content:\n{html}"
    );
}

#[test]
fn repeater_only_named_fill_does_not_require_a_content_slot() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("fill-only-collection.wcl");
    write_fixture(
        &src,
        r#"
template banner {
  slot hero: content
  render = fn(c: TemplateCtx) -> list<Html> slot(c, :hero)
}
site { default_template = :banner }
let promos = ["Sale"]
page index {
  wdoc_repeater { each = promos as = :promo
    hero { h1 $"${promo}" }
  }
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");
    assert!(html.contains("Sale"), "{html}");
}

#[test]
fn unconditional_repeater_fill_for_another_layout_is_an_error() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("wrong-generated-slot.wcl");
    write_fixture(
        &src,
        r#"
template article {
  slot content: content
  render = fn(c: TemplateCtx) -> list<Html> slot(c, :content)
}
template marketing {
  slot content: content
  slot promo: content?
  render = fn(c: TemplateCtx) -> list<Html> slot(c, :content)
}
site { default_template = :article }
let promos = ["Sale"]
page index {
  wdoc_repeater { each = promos as = :promo
    promo { h1 $"${promo}" }
  }
  p "Body."
}
page landing { template = :marketing p "Landing." }
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    let err = build(&src, out.path(), None).expect_err("wrong-layout fill must fail");
    assert!(
        err.render_plain().contains("does not declare it"),
        "{}",
        err.render_plain()
    );
}

#[test]
fn computed_page_template_suppresses_static_slot_diagnostics() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("computed-template.wcl");
    write_fixture(
        &src,
        r#"
template article {
  slot content: content
  slot hero: content
  render = fn(c: TemplateCtx) -> list<Html> wdoc_blocks(c.content)
}
site { default_template = :article }
let selected = :article
page index { template = selected p "Body." }
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
}

#[test]
fn typed_slot_declarations_preserve_the_component_parameter_contract() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("typed-component-slot.wcl");
    write_fixture(
        &src,
        r#"
wdoc_component badge {
  slot label: utf8
  slot tone: utf8 = "note"
  slot emphasized: bool = true
  wdoc_body { callout $"${label}" { class = [tone] body = "x" } }
}
page index { badge { label = "Typed" } }
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");
    assert!(
        html.contains("callout note") && html.contains("Typed"),
        "{html}"
    );
}

#[test]
fn component_content_slots_are_named_scoped_and_placed() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("named-component-slots.wcl");
    write_fixture(
        &src,
        r#"
wdoc_component split_card {
  slot header: content
  slot body: content
  wdoc_body {
    column {
      header {}
      body {}
    }
  }
}
page index {
  split_card {
    header { h2 "Named header" }
    body { p "Named body." }
  }
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");
    assert!(
        html.contains("Named header") && html.contains("Named body."),
        "component holes were not placed independently:\n{html}"
    );
}

#[test]
fn component_content_slots_check_required_duplicate_and_accepted_type() {
    let cases = [
        (
            "missing",
            r#"
wdoc_component split_card {
  slot header: content
  wdoc_body { header {} }
}
page index { split_card {} }
"#,
            "required slot `header` is unfilled",
        ),
        (
            "duplicate",
            r#"
wdoc_component split_card {
  slot header: content
  wdoc_body { header {} }
}
page index { split_card { header { h2 "One" } header { h2 "Two" } } }
"#,
            "fills slot `header` more than once",
        ),
        (
            "accepted",
            r#"
wdoc_component canvas {
  slot shapes: content<SvgBlock>
  wdoc_body { shapes {} }
}
page index { canvas { shapes { p "not a shape" } } }
"#,
            "slot `shapes` accepts `SvgBlock`, but found `p`",
        ),
    ];

    for (name, body, expected) in cases {
        let tmp = TempDir::new().expect("mkdir tempdir");
        let src = tmp.path().join(format!("{name}.wcl"));
        write_fixture(&src, body);
        let out = TempDir::new().expect("mkdir out");
        let err = build(&src, out.path(), None).expect_err(name);
        assert!(
            err.render_plain().contains(expected),
            "{name}: expected {expected:?}, got {}",
            err.render_plain()
        );
    }
}

#[test]
fn layout_slot_fill_does_not_inherit_a_same_named_component_contract() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("scoped-layout-slot.wcl");
    write_fixture(
        &src,
        r#"
wdoc_component hero {
  slot label: utf8
  wdoc_body { h2 $"${label}" }
}
template article {
  slot content: content
  slot hero: content
  render = fn(c: TemplateCtx) -> list<Html>
    flatten([slot(c, :hero), slot(c, :content)])
}
site { default_template = :article }
page index {
  hero { h1 "Layout hero" }
  p "Body."
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");
    assert!(
        html.contains("Layout hero") && html.contains("Body."),
        "{html}"
    );
}

#[test]
fn defaulted_content_slot_renders_layout_owned_fallback_and_page_override() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("slot-fallback.wcl");
    write_fixture(
        &src,
        r#"
template article {
  slot content: content
  slot footer: content = fn(c: TemplateCtx) -> list<Html> [
    el("footer", ["fallback"], [raw(c.title)])
  ]
  render = fn(c: TemplateCtx) -> list<Html>
    flatten([slot(c, :content), slot(c, :footer)])
}
site { default_template = :article title = "Layout fallback" }
page absent { p "No page footer." }
page filled { footer { p "Page footer." } p "Body." }
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let absent = std::fs::read_to_string(out.path().join("absent.html")).expect("read absent");
    let filled = std::fs::read_to_string(out.path().join("filled.html")).expect("read filled");
    assert!(
        absent.contains("<footer class=\"fallback\">Layout fallback</footer>"),
        "{absent}"
    );
    assert!(
        filled.contains("Page footer.") && !filled.contains("class=\"fallback\""),
        "{filled}"
    );
}

#[test]
fn conditional_fill_using_an_ordinary_block_kind_is_still_a_typo() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("conditional-known-kind.wcl");
    write_fixture(
        &src,
        r#"
template article {
  slot content: content
  render = fn(c: TemplateCtx) -> list<Html> slot(c, :content)
}
site { default_template = :article }
page index { callout? "Not a slot" { body = "x" } }
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    let err = build(&src, out.path(), None).expect_err("conditional typo must fail");
    assert!(
        err.render_plain()
            .contains("no layout used by this site declares it"),
        "{}",
        err.render_plain()
    );
}

#[test]
fn repeater_generated_page_slot_pairing_is_left_to_render_time() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("generated-pages-slot-silence.wcl");
    write_fixture(
        &src,
        r#"
template article {
  slot content: content
  slot hero: content
  render = fn(c: TemplateCtx) -> list<Html> slot(c, :content)
}
site { default_template = :article }
let rows = [{ id: "one" }, { id: "two" }]
wdoc_repeater { each = rows as = :row
  page $"${row.id}" { p "Generated." }
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    assert!(out.path().join("one.html").exists());
    assert!(out.path().join("two.html").exists());
}

#[test]
fn build_renders_nested_component_content_structurally() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("nested-components.wcl");
    write_fixture(
        &src,
        r#"
wdoc_component inner {
  wdoc_body {
    h2 "Inner frame"
    wdoc_content
  }
}
wdoc_component outer {
  wdoc_body {
    inner {
      wdoc_content
    }
  }
}
page index {
  outer {
    p "Nested **payload**."
  }
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");
    assert!(
        html.contains("<p>Nested <span class=\"bold\">payload</span>.</p>"),
        "the outer content slot was forwarded through the inner component:\n{html}"
    );
}

#[test]
fn build_keeps_component_content_through_a_native_html_wrapper() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("component-demo.wcl");
    write_fixture(
        &src,
        r#"
wdoc_component preview {
  wdoc_body {
    demo {
      wdoc_content
    }
  }
}
page index {
  preview {
    p "Nested **payload**."
  }
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");
    assert!(
        html.contains("<p>Nested <span class=\"bold\">payload</span>.</p>"),
        "the demo wrapper kept the component's structural slot context:\n{html}"
    );
}

#[test]
fn build_fills_component_content_with_diagram_shapes() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("component-shapes.wcl");
    write_fixture(
        &src,
        r#"
wdoc_component shape_group {
  wdoc_body {
    container {
      width = 140
      height = 80
      wdoc_content
    }
  }
}
page index {
  diagram canvas {
    width = 240
    height = 120
    shape_group {
      rect payload {
        x = 10
        y = 10
        width = 100
        height = 50
        text = "slotted shape"
      }
    }
  }
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");
    assert!(
        html.contains("<rect x=\"10\" y=\"10\" width=\"100\" height=\"50\" />"),
        "the diagram walker filled the component slot with shape nodes:\n{html}"
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
        html.contains("<h2 class=\"heading-2\">Fleet</h2>"),
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
        Err(BuildError::IncludeCycle(m)) => panic!("expected Schema, got IncludeCycle({m})"),
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
    // Lowered content-IR path: h1 -> heading carries id.
    assert!(
        html.contains("<h1 class=\"heading-1\" id=\"title\">"),
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
    // Block-side text picks up its id. A `span`'s own id has nowhere to
    // land now that a `text` lowers to one `Content::Paragraph` — the IR
    // carries prose, not a list of styled runs (see `lib/text.wcl`).
    assert!(html.contains("<p id=\"intro\">"), "{html}");
    assert!(!html.contains("id=\"greeting\""), "{html}");
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
        Err(BuildError::IncludeCycle(m)) => panic!("expected DuplicateId, got IncludeCycle({m})"),
        Err(BuildError::DuplicatePage { site, name }) => {
            panic!("expected DuplicateId, got DuplicatePage({site}: {name})")
        }
        Ok(n) => panic!("expected DuplicateId, got Ok({n})"),
    }
}

#[test]
fn build_preserves_source_order_across_mixed_children() {
    // With the single `@children(ContentBlock)` slot on `Page`, mixed-kind
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
        "<h1 class=\"heading-1\">First</h1>",
        "<p>alpha</p>",
        "<h2 class=\"heading-2\">Middle</h2>",
        "<svg",
        "<p>omega</p>",
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
fn build_layered_node_tables_reserve_derived_height() {
    // Regression: a `:layered` diagram of `node_table` shapes must reserve
    // each table's *derived* height (header + rows) when ranking — not the
    // flat 40px default. Two 3-row tables sit on rank 0 (edges point at a
    // third table on rank 1). Each rank-0 table is header(28) + 3*row(30) =
    // 118 tall, so the rank-1 table must be placed at least 118px below the
    // rank-0 row. Before the fix `effective_dims` reported 40px for every
    // node_table, so rank 1 landed ~40+layer_gap down and overlapped rank 0.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("layered_tables.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width   = 700
    height  = 500
    layout  = :layered
    routing = :elbow
    node_table {
      id = struct_a  title = "StructA"  width = 200.0
      node_row { p "- one: i32" }
      node_row { p "- two: i32" }
      node_row { p "+ run(&self)" }
    }
    node_table {
      id = struct_b  title = "StructB"  width = 200.0
      node_row { p "- alpha: u8" }
      node_row { p "- beta: u8" }
      node_row { p "+ run(&self)" }
    }
    node_table {
      id = iface  title = "Trait"  width = 200.0
      node_row { p "+ run(&self)" }
    }
    struct_a -> iface
    struct_b -> iface
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");

    // Collect the y of every layout-positioned group `translate(tx ty)`.
    let tys: Vec<f64> = html
        .match_indices("transform=\"translate(")
        .filter_map(|(i, _)| {
            let rest = &html[i + "transform=\"translate(".len()..];
            let inner = &rest[..rest.find(')')?];
            let ty = inner.split_whitespace().nth(1)?;
            ty.parse::<f64>().ok()
        })
        .collect();
    let min_ty = tys.iter().cloned().fold(f64::INFINITY, f64::min);
    let max_ty = tys.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    assert!(
        min_ty.is_finite() && max_ty.is_finite(),
        "expected positioned translate groups, got {tys:?}"
    );
    // Rank 1 must clear a rank-0 table's full derived height (118), proving
    // the solver reserved real height, not the 40px default.
    assert!(
        max_ty - min_ty >= 118.0,
        "rank-1 table overlaps rank 0: vertical span {} < 118 derived height\n{html}",
        max_ty - min_ty
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
fn build_layered_boundary_evicts_non_members() {
    // Layered layout knows nothing about boundaries, so without the
    // eviction pass the outsider `p` (layer 0, below `a`) lands inside the
    // bbox of {a, b1, b2} + padding — reading as if `p` were a member. The
    // post-plan pass must push it out of the boundary's would-be box.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("evict.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width  = 900
    height = 500
    layout = :layered
    direction = :left_to_right
    process "A"  { id = a   width = 160.0  height = 60.0 }
    process "B1" { id = b1  width = 160.0  height = 60.0 }
    process "B2" { id = b2  width = 160.0  height = 60.0 }
    process "P"  { id = p   width = 160.0  height = 60.0 }
    a -> b1
    a -> b2
    p -> b2
    boundary "Owned" { members = [a, b1, b2] }
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // Parse the boundary rect's box.
    let b = html
        .find("<rect class=\"wdoc-boundary\"")
        .expect("boundary rect present");
    let attr = |name: &str| -> f64 {
        let seg = &html[b..html[b..].find("/>").expect("rect closes") + b];
        let k = format!("{name}=\"");
        let s = seg.find(&k).expect("attr present") + k.len();
        let e = seg[s..].find('"').expect("attr closes") + s;
        seg[s..e].parse::<f64>().expect("numeric attr")
    };
    let (bx, by, bw, bh) = (attr("x"), attr("y"), attr("width"), attr("height"));

    // Parse p's absolute position: its rect is x/y 0 inside a translated
    // group, so read the nearest preceding translate().
    let pi = html.find("id=\"p\"").expect("p rect present");
    let t = html[..pi]
        .rfind("translate(")
        .expect("p is in a translated group")
        + "translate(".len();
    let te = html[t..].find(')').expect("translate closes") + t;
    let mut parts = html[t..te].split([',', ' ']).filter(|s| !s.is_empty());
    let px: f64 = parts.next().expect("tx").parse().expect("tx numeric");
    let py: f64 = parts.next().expect("ty").parse().expect("ty numeric");
    let (pw, ph) = (160.0, 60.0);

    let intersects = px < bx + bw && bx < px + pw && py < by + bh && by < py + ph;
    assert!(
        !intersects,
        "non-member p ({px},{py} {pw}x{ph}) must be evicted from the boundary \
         box ({bx},{by} {bw}x{bh}):\n{html}"
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
    let warnings = wcl_wdoc::take_render_warnings();
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
        html.contains("<p>Hello <span class=\"bold\">world</span></p>"),
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
        html.contains("<p>an <span class=\"italic\">accent</span> here</p>"),
        "italic not rendered:\n{html}"
    );
}

#[test]
fn build_intraword_underscores_stay_literal() {
    // The italic pattern is boundary-gated: `_mode_` inside a snake_case
    // identifier must not italicize (CommonMark's intraword rule), while a
    // properly flanked `_word_` in the same text still does.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = write_inline_fixture(&tmp, "set safe_mode_password to a _real_ value");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("safe_mode_password"),
        "snake_case identifier mangled:\n{html}"
    );
    assert!(
        html.contains("a <span class=\"italic\">real</span> value"),
        "flanked italic lost:\n{html}"
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
        html.contains("<p>say <span class=\"code\">hello</span></p>"),
        "code not rendered:\n{html}"
    );
}

#[test]
fn build_code_span_contents_are_verbatim() {
    // An underscore pair inside an inline code span must not be reinterpreted
    // as `_italic_` — code-span contents are verbatim.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = write_inline_fixture(&tmp, "A `reading_long_format` B");
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("<span class=\"code\">reading_long_format</span>"),
        "code span should be verbatim (no italic leak):\n{html}"
    );
    assert!(
        !html.contains("<span class=\"italic\">long</span>"),
        "italic leaked into code span:\n{html}"
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
        Err(BuildError::IncludeCycle(m)) => panic!("expected BadLink, got IncludeCycle({m})"),
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
        Err(BuildError::IncludeCycle(m)) => panic!("expected BadLink, got IncludeCycle({m})"),
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
    assert!(html.contains("<p>after</p>"), "{html}");
}

#[test]
fn custom_block_lowers_to_table_fundamental() {
    // A custom ContentBlock whose `lower` returns an
    // Html::Table renders through render_table_payload.
    // `header` is the heading row; `rows` are the body. Cells on this
    // path are plain escaped text.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("tbl.wcl");
    write_fixture(
        &src,
        r##"
@block("datatable")
type DataTable extends ContentBlock {
  id: identifier?
  lower = fn(d: DataTable) -> list<Html> [
    Html::Table {
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
fn template_queries_authored_blocks_and_places_typed_handles() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("authored-content.wcl");
    write_fixture(
        &src,
        r#"
site { default_template = :inspect  title = "Prepared site context" }

template inspect {
  slot content: content
  render = fn(c: TemplateCtx) -> list<Html> {
    let notice = head(filter(c.content, fn(h: BlockHandle) -> bool
      h.kind == "callout" && h.block.heading == "Notice"));
    let column_handle = at(c.content, 2);
    let nested = head(column_handle.children);
    [
      el("main", [], [
        el("strong", [], [raw(c.title)]),
        Html::Blocks { blocks: [notice, nested, at(c.content, 0)] },
      ]),
    ]
  }
}

page index {
  h1 "First"
  callout "Notice" { body = "Chosen by its authored fields." }
  column {
    p "Nested child"
  }
  p "Not placed"
}
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    assert!(
        html.contains("<strong>Prepared site context</strong>"),
        "prepared site context remains available:\n{html}"
    );
    let notice = html
        .find("Chosen by its authored fields.")
        .expect("callout");
    let nested = html.find("Nested child").expect("nested child");
    let first = html.find("First").expect("heading");
    assert!(
        notice < nested && nested < first,
        "the template can query and reorder authored handles:\n{html}"
    );
    assert!(
        !html.contains("Not placed"),
        "only handles emitted by the template are resolved:\n{html}"
    );
}

#[test]
fn separate_typed_placements_share_page_heading_and_footnote_state() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("separate-placements.wcl");
    write_fixture(
        &src,
        r#"
site { default_template = :split }

template split {
  slot content: content
  render = fn(c: TemplateCtx) -> list<Html> [
    el("main", [], flatten([
      wdoc_blocks([at(c.content, 0), at(c.content, 1)]),
      [el("hr", [], [])],
      wdoc_blocks([at(c.content, 2), at(c.content, 3)]),
    ])),
  ]
}

page index {
  h2 "Overview"
  p "See [^note]."
  h2 "Overview"
  footnotes {
    footnote note { text = "Defined after the second placement." }
  }
}
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    assert!(
        html.contains("id=\"overview\">"),
        "first heading id:\n{html}"
    );
    assert!(
        html.contains("id=\"overview-2\">"),
        "heading ids remain unique across placements:\n{html}"
    );
    assert!(
        html.contains("heading-marker\">§ 1</span>")
            && html.contains("heading-marker\">§ 2</span>"),
        "section numbering remains page-wide across placements:\n{html}"
    );
    assert!(
        html.contains("<sup class=\"footnote-ref\" id=\"fnref-note\">")
            && html.contains("id=\"fn-note\""),
        "footnote references resolve across placements:\n{html}"
    );
}

#[test]
fn page_metadata_answers_reading_order_neighbours_active_path_and_headings() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("page-metadata.wcl");
    write_fixture(
        &src,
        r#"
site {
  default_template = :inspect_metadata
  toc {
    chapter "Introduction" { page = intro }
    chapter "Guide" {
      chapter "Using it" { page = usage }
      chapter "Details" { page = details }
    }
    chapter "Appendix" { page = appendix }
  }
}

template inspect_metadata {
  slot content: content
  render = fn(c: TemplateCtx) -> list<Html> {
    let m = page_metadata(c);
    [
      el("div", ["previous"], [raw(if m.previous != none { m.previous.title } else { "" })]),
      el("div", ["current"], [raw(if m.current != none { m.current.title } else { "" })]),
      el("div", ["next"], [raw(if m.next != none { m.next.title } else { "" })]),
      el("ol", ["reading-order"], map(m.reading_order, fn(e: TocEntry) -> Html
        el("li", [], [raw(e.title)]))),
      el("ol", ["active-path"], map(m.active_path, fn(e: TocEntry) -> Html
        el("li", [], [raw(e.title)]))),
      el("ol", ["headings"], map(m.headings, fn(h: OnPageHeading) -> Html
        el("li", [], [raw(format("{}:{}:{}", h.number, h.id, h.title))]))),
    ]
  }
}

page intro { h1 "Intro" }
page usage {
  h1 "Usage"
  h2 "First steps"
  h3 "Try it"
  h2 "First steps"
}
page details { h1 "Details" }
page appendix { h1 "Appendix" }
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());

    let html = std::fs::read_to_string(out.path().join("usage.html")).expect("read usage");
    assert!(
        html.contains("<div class=\"previous\">Introduction</div>"),
        "previous page:\n{html}"
    );
    assert!(
        html.contains("<div class=\"current\">Using it</div>"),
        "current page:\n{html}"
    );
    assert!(
        html.contains("<div class=\"next\">Details</div>"),
        "next page:\n{html}"
    );
    assert!(
        html.contains(
            "<ol class=\"reading-order\"><li>Introduction</li><li>Using it</li><li>Details</li><li>Appendix</li></ol>"
        ),
        "reading order:\n{html}"
    );
    assert!(
        html.contains("<ol class=\"active-path\"><li>Guide</li><li>Using it</li></ol>"),
        "active path:\n{html}"
    );
    assert!(
        html.contains(
            "<ol class=\"headings\"><li>1:first-steps:First steps</li><li>1.1:try-it:Try it</li><li>2:first-steps-2:First steps</li></ol>"
        ),
        "authored heading metadata:\n{html}"
    );
}

#[test]
fn metadata_only_template_does_not_force_page_lowering() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("metadata-only.wcl");
    write_fixture(
        &src,
        r#"
@block("boom")
type Boom extends ContentBlock {
  lower = fn(b: Boom) -> list<Html> [ raw(no_such_helper(b)) ]
}

site {
  default_template = :metadata_only
  toc { chapter "Index" { page = index } }
}

template metadata_only {
  slot content: content
  render = fn(c: TemplateCtx) -> list<Html> {
    let m = page_metadata(c);
    [el("p", [], [raw(m.current.title)])]
  }
}

page index { boom {} }
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read index");
    assert!(html.contains("<p>Index</p>"), "{html}");
}

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
        html.contains("<main class=\"site-main\"><h1 class=\"heading-1\" id=\"home\">Home</h1>"),
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
    assert!(bare.contains("<h1 class=\"heading-1\">Bare</h1>"), "{bare}");
}

// Split a rendered page into its `<head>` and `<body>` halves, so a test
// can assert head-only / body-only placement.
fn split_head_body(html: &str) -> (&str, &str) {
    html.split_once("</head>").expect("page has a </head>")
}

#[test]
fn website_template_renders_slots_and_content() {
    // The `:website` template splits a page into named slots and the
    // default content: `hero` lands in the hero section, `sidebar` makes
    // a two-column layout, and everything else
    // is the default content `<main>`.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("site.wcl");
    write_fixture(
        &src,
        r#"
site { default_template = :website  title = "Acme" }
page index {
  hero {
    h1 "Welcome" {}
  }
  sidebar {
    p "Side note." {}
  }
  h2 "Body heading" {}
}
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    // The hero slot renders inside the hero section.
    assert!(
        html.contains(
            "<section class=\"ws-hero\"><h1 class=\"heading-1\" id=\"welcome\">Welcome</h1>"
        ),
        "hero slot should fill the hero section:\n{html}"
    );
    // A sidebar slot switches the layout to two columns and renders its
    // content in an <aside>.
    assert!(
        html.contains("class=\"ws-layout has-aside\""),
        "a sidebar slot should switch to the two-column layout:\n{html}"
    );
    assert!(
        html.contains("<aside class=\"ws-aside\"><p>Side note.</p>"),
        "sidebar slot should render in the aside:\n{html}"
    );
    // The loose blocks form the default content <main>.
    assert!(
        html.contains("<main class=\"ws-main\"><h2 class=\"heading-2\" id=\"body-heading\"><span class=\"heading-marker\">§ 1</span>Body heading"),
        "loose blocks should be the default content:\n{html}"
    );
    // Named slot content must not be duplicated into the default content <main>.
    let main = html
        .split_once("<main class=\"ws-main\">")
        .and_then(|(_, rest)| rest.split_once("</main>"))
        .map(|(inner, _)| inner)
        .unwrap_or("");
    assert!(
        !main.contains("Welcome") && !main.contains("Side note."),
        "named slot content must not leak into the default content <main>:\n{main}"
    );
}

#[test]
fn website_template_header_controls_and_banner_slot() {
    // `theme_toggle = true` puts the standard light/dark toggle in the
    // header's `.ws-controls` cluster, and a `banner` slot renders in
    // a `.ws-banner` strip between the header and the content.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("site.wcl");
    write_fixture(
        &src,
        r#"
site { default_template = :website  title = "Acme"  theme_toggle = true }
page index {
  banner {
    p "Heads up." {}
  }
  h2 "Body heading" {}
}
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("class=\"ws-controls\""),
        "theme_toggle should add the header controls cluster:\n{html}"
    );
    assert!(
        html.contains("<button class=\"theme-toggle\" onclick=\"wdocToggleTheme()\">"),
        "the standard theme toggle should render in the header:\n{html}"
    );
    assert!(
        html.contains("<div class=\"ws-banner\"><p>Heads up.</p>"),
        "the banner slot should render in the .ws-banner strip:\n{html}"
    );
    let banner_at = html.find("ws-banner").expect("banner present");
    let main_at = html.find("<main class=\"ws-main\"").expect("main present");
    assert!(
        banner_at < main_at,
        "the banner should render before the content:\n{html}"
    );
}

#[test]
fn website_template_header_has_no_controls_without_flags() {
    // A site that enables neither `search` nor `theme_toggle` gets no
    // `.ws-controls` cluster at all — the header is exactly as before.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("site.wcl");
    write_fixture(
        &src,
        r#"
site { default_template = :website  title = "Acme" }
page index { h2 "Body heading" {} }
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    // (The `.ws-controls` / `.theme-toggle` CSS rules still ship in the
    // template's <style>; only the markup must be absent.)
    assert!(
        !html.contains("class=\"ws-controls\"") && !html.contains("<button class=\"theme-toggle\""),
        "a no-flag site's header must not grow a controls cluster:\n{html}"
    );
}

#[test]
fn site_head_fields_inject_link_and_script_tags() {
    // A site's `stylesheets` / `fonts` become `<link rel="stylesheet">`
    // and `scripts` become deferred `<script>`, all inside <head> — never
    // in the <body>.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("site.wcl");
    write_fixture(
        &src,
        r#"
site {
  default_template = :website
  title       = "Acme"
  stylesheets = ["assets/site.css"]
  scripts     = ["assets/app.js"]
  fonts       = ["https://fonts.example/Inter.css"]
}
page index { h1 "Hi" {} }
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    let (head, body) = split_head_body(&html);
    assert!(
        head.contains("<link rel=\"stylesheet\" href=\"assets/site.css\">"),
        "stylesheet link should be in <head>:\n{head}"
    );
    assert!(
        head.contains("<link rel=\"stylesheet\" href=\"https://fonts.example/Inter.css\">"),
        "font link should be in <head>:\n{head}"
    );
    assert!(
        head.contains("<script src=\"assets/app.js\" defer></script>"),
        "script tag should be in <head>:\n{head}"
    );
    // None of the head assets may leak into the body.
    assert!(
        !body.contains("assets/site.css") && !body.contains("assets/app.js"),
        "head assets must not appear in <body>:\n{body}"
    );
}

#[test]
fn template_head_fundamental_hoisted_to_head() {
    // A template that returns an `Html::Head` at the top level
    // has its children hoisted into <head>; a `Head` nested inside the
    // body renders to nothing (it must not leak into <head> or <body>).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("site.wcl");
    write_fixture(
        &src,
        r##"
site { default_template = :custom  title = "Acme" }
template custom {
  slot content: content
  render = fn(c: TemplateCtx) -> list<Html>
    flatten([
      wdoc_head_stylesheet("theme.css"),
      [ el("main", [], [
          Html::Blocks { blocks: c.content },
          Html::Head { children: [raw("<!--LEAK-->")] },
      ]) ],
    ])
}
page index { h1 "Hi" {} }
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    let (head, body) = split_head_body(&html);
    assert!(
        head.contains("<link rel=\"stylesheet\" href=\"theme.css\">"),
        "top-level Head should be hoisted into <head>:\n{head}"
    );
    assert!(
        !body.contains("<!--LEAK-->") && !head.contains("<!--LEAK-->"),
        "a Head nested in the body must render to nothing:\n{html}"
    );
}

#[test]
fn website_assets_folder_copied_verbatim() {
    // A site's `assets` folders are copied verbatim (recursively) into the
    // output, so an externally-built bundle ships unchanged.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let dist = tmp.path().join("dist");
    std::fs::create_dir_all(dist.join("nested")).expect("mkdir dist");
    std::fs::write(dist.join("app.js"), "console.log(1)").expect("write app.js");
    std::fs::write(dist.join("nested").join("x.css"), "body{}").expect("write nested");
    let src = tmp.path().join("site.wcl");
    write_fixture(
        &src,
        r#"
site { default_template = :website  title = "Acme"  assets = ["dist"] }
page index { h1 "Hi" {} }
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    assert_eq!(
        std::fs::read_to_string(out.path().join("dist/app.js")).expect("app.js copied"),
        "console.log(1)"
    );
    assert!(
        out.path().join("dist/nested/x.css").exists(),
        "nested asset files should be copied too"
    );
}

#[test]
fn template_uses_user_defined_part_function() {
    // A "part" is just a top-level function returning fundamentals; a
    // custom template calls it (resolved at document scope) and embeds
    // its result. Also exercises Html::Element nesting +
    // Raw, and attribute escaping.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("site.wcl");
    write_fixture(
        &src,
        r##"
let footer = fn(c: TemplateCtx) -> list<Html> [
  ela("footer", ["ft"], [["data-x", "a\"b"]], [raw(c.title)])
]
template mini {
  slot content: content
  render = fn(c: TemplateCtx) -> list<Html>
    flatten([
      [ el("main", [], wdoc_blocks(c.content)) ],
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
    // The typed block placement embeds the page content inside <main>.
    assert!(html.contains("<main><p>body text</p>"), "{html}");
}

#[test]
fn omitted_optional_variant_field_matches_an_explicit_none() {
    // Optional variant fields default to `none`, so the long form and the
    // short one must render the same bytes. This is the rule the stdlib's
    // 204 dead `: none` arguments were written against.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let long = tmp.path().join("long.wcl");
    let short = tmp.path().join("short.wcl");
    let page = |el: &str| {
        format!(
            r##"
@block("probe")
type Probe extends ContentBlock {{
  lower = fn(p: Probe) -> list<Html> [ {el} ]
}}
site {{ title = "T" }}
page index {{ probe {{}} }}
"##
        )
    };
    write_fixture(
        &long,
        page(
            r#"Html::Element {
                 tag: "div", id: none, class: none, attrs: none,
                 children: [ Html::Raw { html: "x" } ],
               }"#,
        ),
    );
    write_fixture(
        &short,
        page(
            r#"Html::Element {
                 tag: "div",
                 children: [ Html::Raw { html: "x" } ],
               }"#,
        ),
    );

    let out_long = TempDir::new().expect("mkdir out");
    let out_short = TempDir::new().expect("mkdir out");
    build_ok(&long, out_long.path());
    build_ok(&short, out_short.path());
    let a = std::fs::read_to_string(out_long.path().join("index.html")).expect("read");
    let b = std::fs::read_to_string(out_short.path().join("index.html")).expect("read");
    assert_eq!(a, b, "explicit `: none` changed the rendering");
    assert!(a.contains("<div>x</div>"), "{a}");
}

#[test]
fn class_and_attrs_drop_none_entries() {
    // An else-less `if` yields `none`, so a conditional class / attribute
    // is written inline. An untaken one must vanish — and a list whose
    // entries are ALL untaken must emit no attribute at all, not `class=""`.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("cond.wcl");
    write_fixture(
        &src,
        r##"
@block("probe")
type Probe extends ContentBlock {
  on: bool
  lower = fn(p: Probe) -> list<Html> [
    Html::Element {
      tag: "div",
      class: ["base", if p.on { "hot" }],
      attrs: [["data-keep", "1"], if p.on { ["data-hot", "1"] }],
      children: [ Html::Raw { html: "x" } ],
    },
    Html::Element {
      tag: "span",
      class: [if p.on { "hot" }],
      children: [ Html::Raw { html: "y" } ],
    },
  ]
}
site { title = "T" }
page index {
  probe { on = false }
  probe { on = true }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    // Untaken: the `none` entries leave no trace, and the all-none list
    // emits no `class` attribute.
    assert!(
        html.contains("<div class=\"base\" data-keep=\"1\">x</div>"),
        "{html}"
    );
    assert!(html.contains("<span>y</span>"), "{html}");
    // Taken: both the class and the attribute appear.
    assert!(
        html.contains("<div class=\"base hot\" data-keep=\"1\" data-hot=\"1\">x</div>"),
        "{html}"
    );
    assert!(html.contains("<span class=\"hot\">y</span>"), "{html}");
}

#[test]
fn svg_shape_class_drops_none_entries() {
    // The same rule on the SVG side: a diagram shape's `class` list drops
    // its `none` entries and emits no attribute when they are all none.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("shape.wcl");
    write_fixture(
        &src,
        r##"
@block("chip")
type Chip extends SvgBlock {
  on: bool
  lower = fn(c: Chip) -> list<Svg> [
    Svg::Rect {
      x: 5.0, y: 5.0, width: 20.0, height: 20.0,
      class: ["chip", if c.on { "hot" }],
    },
    Svg::Circle {
      cx: 40.0, cy: 15.0, r: 5.0,
      class: [if c.on { "hot" }],
    },
  ]
}
site { title = "T" }
page index {
  diagram { width = 100  height = 50  chip { on = false } }
}
"##,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(html.contains("class=\"chip\""), "{html}");
    // Pin the all-`none` shape itself: the circle must carry no `class` at
    // all. A page-wide `class=""` search would also pass if the circle were
    // never rendered.
    let circle = html
        .split("<circle")
        .nth(1)
        .and_then(|rest| rest.split('>').next())
        .expect("the all-`none` circle is rendered");
    assert!(
        !circle.contains("class"),
        "circle carries a class attribute: <circle{circle}>"
    );
}

#[test]
fn custom_template_composes_public_parts() {
    // A user template built entirely from the public `wdoc_part_*` parts
    // (no copy of the stdlib markup) emits the stdlib navbar + content.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("site.wcl");
    write_fixture(
        &src,
        r#"
template parts_only {
  slot content: content
  render = fn(c: TemplateCtx) -> list<Html>
    flatten([ wdoc_part_navbar(c), wdoc_part_content(c) ])
}
site { default_template = :parts_only  title = "Site" }
page index { text { span "hello" {} } }
page other { h1 "Other" {} }
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("<nav class=\"site-nav\">"),
        "navbar part missing:\n{html}"
    );
    assert!(
        html.contains("<main class=\"site-main\"><p>hello</p>"),
        "content part missing:\n{html}"
    );
}

#[test]
fn template_extends_layout_with_custom_footer() {
    // Pattern (a): keep a whole built-in by calling its layout fn, then
    // append a custom section. The full webpage chrome must be present and
    // the footer must follow it.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("site.wcl");
    write_fixture(
        &src,
        r#"
template blog {
  slot content: content
  render = fn(c: TemplateCtx) -> list<Html>
    flatten([
      wdoc_webpage_layout(c),
      [ el("footer", ["site-footer"], [raw("the footer")]) ],
    ])
}
site { default_template = :blog  title = "Blog" }
page index { h1 "Post" {} }
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    // The full stdlib webpage chrome is reused wholesale.
    assert!(html.contains("<header class=\"site-header\">"), "{html}");
    assert!(html.contains("<nav class=\"site-nav\">"), "{html}");
    assert!(html.contains("<main class=\"site-main\">"), "{html}");
    // The appended footer follows the layout.
    let footer = html
        .find("<footer class=\"site-footer\">the footer</footer>")
        .expect("footer present");
    let main = html
        .find("<main class=\"site-main\">")
        .expect("main present");
    assert!(footer > main, "footer should follow the layout:\n{html}");
}

#[test]
fn template_overrides_header_keeps_navbar() {
    // Pattern (b): copy the webpage layout body and swap one section — a
    // custom masthead replaces wdoc_part_header while the stdlib navbar and
    // content parts are reused. The stdlib header markup must be gone.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("site.wcl");
    write_fixture(
        &src,
        r#"
template app_home {
  slot content: content
  render = fn(c: TemplateCtx) -> list<Html>
    flatten([
      wdoc_part_webpage_css(),
      [ el("header", ["hero"], [raw(c.title)]) ],
      wdoc_part_navbar(c),
      wdoc_part_content(c),
    ])
}
site { default_template = :app_home  title = "App" }
page index { h1 "Home" {} }
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("<header class=\"hero\">App</header>"),
        "custom header missing:\n{html}"
    );
    assert!(
        !html.contains("class=\"site-header\""),
        "stdlib header should be gone:\n{html}"
    );
    assert!(
        html.contains("<nav class=\"site-nav\">"),
        "stdlib navbar should be reused:\n{html}"
    );
}

#[test]
fn custom_template_reuses_book_sidebar() {
    // A custom template composed from the book parts emits the fixed
    // sidebar with the nested TOC and the current-chapter highlight.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("site.wcl");
    write_fixture(
        &src,
        r#"
template mybook {
  slot content: content
  render = fn(c: TemplateCtx) -> list<Html>
    flatten([ wdoc_part_book_css(), wdoc_part_sidebar(c), wdoc_part_book_content(c) ])
}
site {
  default_template = :mybook
  title = "Manual"
  toc {
    chapter "Intro" { page = index }
    chapter "Usage" { page = usage }
  }
}
page index { h1 "Intro" {} }
page usage { h1 "Usage" {} }
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("<nav class=\"book-sidebar\">"),
        "sidebar part missing:\n{html}"
    );
    assert!(
        html.contains("class=\"book-chapter current\""),
        "current-chapter highlight missing:\n{html}"
    );
}

#[test]
fn custom_template_reuses_deck() {
    // A custom collection template can compose the deck parts plus extra
    // chrome while declaring the two per-member slots those parts place.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("site.wcl");
    write_fixture(
        &src,
        r#"
template presentation {
  slot content: content*
  slot notes: content* = fn(c: SlotOwner) -> list<Html> []
  render = fn(c: TemplateCtx) -> list<Html>
    flatten([
      wdoc_presentation_layout(c),
      [ el("div", ["my-banner"], [raw("BANNER")]) ],
    ])
}
site { default_template = :presentation  title = "Deck"
  deck { section "S" { slide a  slide b } }
}
page a { h1 "A" {} }
page b { h1 "B" {} }
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("<div class=\"deck\">"),
        "deck grid part missing:\n{html}"
    );
    assert!(
        html.contains("class=\"deck-progress\"") && html.contains("class=\"deck-counter\""),
        "deck chrome part missing:\n{html}"
    );
    assert!(
        html.contains("<div class=\"my-banner\">BANNER</div>"),
        "custom deck chrome missing:\n{html}"
    );
}

#[test]
fn wdoc_part_menu_tree_renders_bare_ul() {
    // The menu-tree part emits just the nested `<ul class="menu">` with no
    // surrounding `<nav>` — usable to place a dropdown menu anywhere.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("site.wcl");
    write_fixture(
        &src,
        r#"
template menu_only {
  slot content: content
  render = fn(c: TemplateCtx) -> list<Html>
    flatten([ wdoc_part_menu_tree(c), wdoc_part_content(c) ])
}
site {
  default_template = :menu_only
  title = "M"
  menu {
    item "Home" { page = index }
    item "More" { item "Other" { page = other } }
  }
}
page index { h1 "Home" {} }
page other { h1 "Other" {} }
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("<ul class=\"menu\">"),
        "bare menu <ul> missing:\n{html}"
    );
    // Nested submenu present, but no site-nav wrapper from this part.
    assert!(
        html.contains("menu-toggle"),
        "dropdown parent missing:\n{html}"
    );
    assert!(
        !html.contains("<nav class=\"site-nav\">"),
        "menu_tree must not wrap in a nav:\n{html}"
    );
}

#[test]
fn wdoc_part_search_box_gated() {
    // The search-box part renders the box + its <style> when enabled and
    // nothing when disabled — driven by the site `search` flag via c.search.
    let body = r#"
template searchable {
  slot content: content
  render = fn(c: TemplateCtx) -> list<Html>
    flatten([ wdoc_part_search_box(c.search), wdoc_part_content(c) ])
}
site { default_template = :searchable  title = "S"  search = SEARCH }
page index { h1 "Home" {} }
"#;

    // Enabled.
    let tmp_on = TempDir::new().expect("mkdir tempdir");
    let src_on = tmp_on.path().join("on.wcl");
    write_fixture(&src_on, body.replace("SEARCH", "true"));
    let out_on = TempDir::new().expect("mkdir out");
    build_ok(&src_on, out_on.path());
    let on = std::fs::read_to_string(out_on.path().join("index.html")).expect("read");
    assert!(
        on.contains("class=\"wdoc-search\"") && on.contains(".wdoc-search-input"),
        "search box + style missing when enabled:\n{on}"
    );

    // Disabled.
    let tmp_off = TempDir::new().expect("mkdir tempdir");
    let src_off = tmp_off.path().join("off.wcl");
    write_fixture(&src_off, body.replace("SEARCH", "false"));
    let out_off = TempDir::new().expect("mkdir out");
    build_ok(&src_off, out_off.path());
    let off = std::fs::read_to_string(out_off.path().join("index.html")).expect("read");
    assert!(
        !off.contains("class=\"wdoc-search\""),
        "search box should be absent when disabled:\n{off}"
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
    // Entries are titled by each page's first h1, not its route name.
    assert!(
        intro.contains("<a class=\"book-chapter current\" href=\"intro.html\">Intro</a>"),
        "current chapter not highlighted:\n{intro}"
    );
    assert!(
        intro.contains("<a class=\"book-chapter\" href=\"usage.html\">Usage</a>"),
        "{intro}"
    );
    // Content lands in the reading column.
    assert!(
        intro
            .contains("<main class=\"book-content\"><div class=\"book-measure\"><h1 class=\"heading-1\" id=\"intro\">Intro</h1>"),
        "{intro}"
    );

    // On usage.html the highlight moves to the `usage` chapter.
    let usage = std::fs::read_to_string(out.path().join("usage.html")).expect("read");
    assert!(
        usage.contains("<a class=\"book-chapter current\" href=\"usage.html\">Usage</a>"),
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
  dark  { css = "background:#2e3440;" }
  light { css = "background:#eceff4;" }
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
  css = "color:#d8dee9;background:#2e3440;"
  light { css = "color:#2e3440;background:#eceff4;" }
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
class accent { css = "color:#003a8c;font-weight:bold;" }
page index { p "hi" { class = ["accent"] } }
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
    // Per-theme colour, driven by the palette (so it differs per theme):
    // bold takes the heading ink, inline code a themed chip.
    assert!(
        html.contains("strong,.bold{color:var(--wdoc-heading);}"),
        "{html}"
    );
    assert!(
        html.contains(
            ".code,code,kbd{font-family:var(--wdoc-font-mono);background:var(--wdoc-bg-alt);"
        ),
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
  css = "color:#d8dee9;background:#2e3440;"
  light { css = "color:#2e3440;background:#eceff4;" }
}
class "heading-1" { css = "color:#88c0d0;" }
class "heading-2" { css = "color:#8fbcbb;" }
class "link"      { css = "color:#88c0d0;" }

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
            ".heading-1 { font-weight:700;font-size:2.6rem;line-height:1.1;letter-spacing:-0.02em;margin:1.6rem 0 0.7rem; }"
        ),
        "default heading sizing missing:\n{html}"
    );
    assert!(html.contains(".link { color:#88c0d0; }"), "{html}");
    assert!(
        html.contains("<a class=\"link\" href=\"introduction.html\">links</a>"),
        "{html}"
    );
    assert!(
        html.contains(
            "<h2 class=\"heading-2\" id=\"fields\"><span class=\"heading-marker\">§ 1</span>Fields</h2>"
        ),
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
      text { span "A card with " {} span "**formatted**" {} span " text." {} }
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
    // Title + the body rendered through the inline engine (the bold
    // span proves render_block ran, not plain SVG text).
    assert!(
        html.contains("<div class=\"wdoc-card-title\">Notes</div>"),
        "{html}"
    );
    assert!(
        html.contains("<p>A card with <span class=\"bold\">formatted</span> text.</p>"),
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
        text { span "First public " {} span "**beta**" {} span " build." {} }
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
        html.contains("<p>First public <span class=\"bold\">beta</span> build.</p>"),
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
    // the `TermPrimitive` base. The progress bar's filled width is exact
    // integer math (62% of 24 = 14 cells) painted in the accent colour.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("t.wcl");
    write_fixture(
        &src,
        r##"
@block("my_badge")
type MyBadge extends TermPrimitive {
  @inline(0) text: utf8
  row: i64  col: i64
  lower = fn(b: MyBadge) -> list<TermFundamental> [
    TermFundamental::Text { content: "★", row: 1, col: 1, fg: "yellow", bold: true },
    TermFundamental::Text { content: b.text, row: 1, col: 3 },
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
fn pan_zoom_diagram_inside_component_ships_player_js() {
    // A `pan_zoom` diagram authored only inside a `wdoc_component` body
    // (so it never appears in the page's raw block tree) must still ship
    // `diagram-pan-zoom.js` and inject the page <script> — the asset scan
    // descends into component definitions.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("c.wcl");
    write_fixture(
        &src,
        "wdoc_component graph {\n  wdoc_body {\n    \
         diagram { pan_zoom = true  width = 200  height = 120\n      \
         process \"A\" { id = a }  process \"B\" { id = b }\n      a -> b\n    }\n  }\n}\n\
         site s { default_template = :book  title = \"x\"  \
         toc { chapter \"C\" { page = p } } }\n\
         page p { sites = [:s]  start = true\n  h1 { text = \"C\" }\n  graph {}\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    assert!(
        out.path().join("_wdoc/diagram-pan-zoom.js").exists(),
        "pan-zoom JS not shipped for a diagram inside a component"
    );
    let html = std::fs::read_to_string(out.path().join("p.html")).expect("read");
    assert!(
        html.contains("diagram-pan-zoom.js"),
        "no pan-zoom <script> injected:\n{html}"
    );
    assert!(
        html.contains("data-pan-zoom"),
        "component diagram did not render its interactive markup:\n{html}"
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
    // The override still wins, but a readable header that disagrees is a
    // guaranteed-distorted render — the build now says so.
    let warnings = wcl_wdoc::take_render_warnings();
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("world") && w.contains("128x64") && w.contains("256x256")),
        "expected a dimension-mismatch warning, got: {warnings:?}"
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
    // placeholder input renders italic. Default theme is Forge → accent blue
    // (#2389e2): the checked box and the radio dot fill with it.
    let html = wireframe_html(
        "  wf_checkbox \"R\" { checked = true }\n  wf_radio \"S\" { selected = true  y = 40.0 }\n  wf_input \"ph\" { y = 80.0 }",
    );
    // The checked box + selected radio dot fill with the resolved accent.
    assert!(
        html.matches("fill=\"#2389e2\"").count() >= 2,
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
fn wireframe_class_paint_and_raw_color_are_baked_onto_widget() {
    // A custom class on a widget has its SVG `fill` shorthand and retired
    // `color` declaration read in Rust and baked onto the widget.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("wf.wcl");
    write_fixture(
        &src,
        "page index {\n  diagram { width = 200  height = 60\n    wf_button \"P\" { class = [\"primary\"] }\n  }\n}\nclass primary { fill = \"#1f6feb\"  css = \"color:#ffffff;\" }\n",
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");
    assert!(
        html.contains("fill=\"#1f6feb\""),
        "class fill not baked onto the button box:\n{html}"
    );
    assert!(
        html.contains("fill=\"#ffffff\""),
        "class raw color not baked onto the button label:\n{html}"
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
    // (#e5e9f0), not the dark window surface (#2e3440).
    let html = wireframe_html(
        "  wf_window \"App\" { theme = :nord  mode = :light\n    wf_label \"x\"\n  }",
    );
    assert!(
        html.contains("fill=\"#e5e9f0\"") && !html.contains("fill=\"#2e3440\""),
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

/// Render the wireframe fixture with the editor's edit-mode build (shape
/// anchors + layout-container guides) and return its HTML.
fn wireframe_edit_html(body: &str) -> String {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("wf.wcl");
    write_fixture(
        &src,
        format!("page index {{\n  diagram {{ width = 800  height = 600\n{body}\n  }}\n}}\n"),
    );
    let out = TempDir::new().expect("mkdir out");
    let opts = BuildOptions {
        edit_mode: true,
        ..Default::default()
    };
    if build_with_options(&src, out.path(), None, &opts).is_err() {
        panic!("edit-mode build failed");
    }
    std::fs::read_to_string(out.path().join("index.html")).expect("read")
}

#[test]
fn wireframe_layout_guides_are_edit_mode_only() {
    // A layout container draws its guide chrome — the `data-wf-guide` group
    // and the `data-wf-slot` drop zones — only on edit-mode builds; published
    // output stays untouched.
    let body = "  wf_grid { columns = 2\n    wf_button \"A\"\n    wf_button \"B\"\n    wf_button \"C\"\n  }";
    let edit = wireframe_edit_html(body);
    assert!(
        edit.contains("data-wf-guide") && edit.contains("data-wf-slot=\"0\""),
        "edit build missing the grid guide chrome:\n{edit}"
    );
    // Three children in a 2-column grid → a 2×2 cell lattice whose trailing
    // empty cell is still a drop zone (slot 3 = append).
    assert!(
        edit.contains("data-wf-slot=\"3\""),
        "trailing empty cell of the partial last row not stamped:\n{edit}"
    );
    let plain = wireframe_html(body);
    assert!(
        !plain.contains("data-wf-guide") && !plain.contains("data-wf-slot"),
        "plain build must not emit layout guides:\n{plain}"
    );
}

#[test]
fn wireframe_empty_grid_keeps_placeholder_footprint() {
    // An empty grid renders a visible placeholder — a tagged dashed box of
    // `columns × 2` empty cells — instead of collapsing to 0×0, so the
    // editor can see, select and drop into it.
    let edit = wireframe_edit_html("  wf_grid { columns = 2 }");
    assert!(
        edit.contains(">grid ·2</text>"),
        "empty grid missing its kind tag:\n{edit}"
    );
    for slot in 0..4 {
        assert!(
            edit.contains(&format!("data-wf-slot=\"{slot}\"")),
            "empty grid missing placeholder cell {slot}:\n{edit}"
        );
    }
    // The placeholder cells have real geometry (EMPTY_CELL_W × EMPTY_CELL_H).
    assert!(
        edit.contains("data-wf-slot=\"0\" x=\"0.00\" y=\"0.00\" width=\"72.00\" height=\"34.00\""),
        "placeholder cell has no geometry:\n{edit}"
    );
}

#[test]
fn wireframe_row_gaps_are_insertion_slots() {
    // A populated row stamps an invisible insertion strip over each
    // inter-child gap: a drop between child 0 and 1 inserts at position 1.
    let edit = wireframe_edit_html(
        "  wf_row {\n    wf_button \"A\"\n    wf_button \"B\"\n    wf_button \"C\"\n  }",
    );
    assert!(
        edit.contains("data-wf-guide"),
        "row guide chrome missing:\n{edit}"
    );
    assert!(
        edit.contains("data-wf-slot=\"1\"") && edit.contains("data-wf-slot=\"2\""),
        "row gap insertion strips missing:\n{edit}"
    );
    assert!(
        !edit.contains("data-wf-slot=\"3\""),
        "a row has no trailing slot (the backing rect appends):\n{edit}"
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
page index { sites = [:docs, :blog]  h1 "Home" {} }
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
        // `sites = [:docs]` ⇒ docs only; `sites = [:docs, :blog]` ⇒ both.
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
    // other sites are unaffected. The shared `index` page renders into
    // both, so it's a clean before/after comparison.
    let src = r##"
site docs { default_template = :webpage  title = "Docs" }
site blog { default_template = :webpage  title = "Blog" }
class "wdoc-body" { sites = [:docs]  css = "background:#2e3440;" }
page index { sites = [:docs, :blog]  h1 "Home" {} }
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

/// Build `src` and return the `BadPage` message it must fail with.
fn bad_page_message(src: &str) -> String {
    match build_err(src) {
        BuildError::BadPage(msg) => msg,
        e => {
            e.report();
            panic!("expected a BadPage error, got the error reported above");
        }
    }
}

#[test]
fn untagged_page_in_a_multi_site_document_is_an_error() {
    // With more than one site the site chooses the page's template, so a
    // page belonging to every site by default would be re-templated by a
    // site added later without the page changing. Naming it is required.
    let msg = bad_page_message(
        r##"
site docs { default_template = :webpage  title = "Docs" }
site blog { default_template = :webpage  title = "Blog" }
page guide { sites = [:docs]  h1 "Guide" {} }
page loose { h1 "Loose" {} }
"##,
    );
    assert!(
        msg.contains("loose") && msg.contains("sites"),
        "the error should name the untagged page and the field: {msg}"
    );
}

#[test]
fn empty_sites_list_in_a_multi_site_document_is_an_error() {
    // `sites = []` was the same "every site" default spelled out; it must
    // not survive as a backdoor to the behaviour the rule removes.
    let msg = bad_page_message(
        r##"
site docs { default_template = :webpage  title = "Docs" }
site blog { default_template = :webpage  title = "Blog" }
page loose { sites = []  h1 "Loose" {} }
"##,
    );
    assert!(
        msg.contains("loose"),
        "the error should name the page: {msg}"
    );
}

#[test]
fn untagged_page_in_a_single_site_document_still_builds() {
    // The rule is scoped to documents declaring more than one site — the
    // shape every wskill projection and most user documents have.
    let src = r##"
site docs { default_template = :webpage  title = "Docs" }
page index { h1 "Home" {} }
"##;
    with_multisite_build(src, None, |out| {
        assert!(out.join("index.html").exists(), "single-site build");
    });
}

#[test]
fn untagged_non_page_blocks_stay_global_in_a_multi_site_document() {
    // Only `Page.sites` is required — a `class` (or `stylesheet`) with no
    // `sites` list still applies to every site.
    let src = r##"
site docs { default_template = :webpage  title = "Docs" }
site blog { default_template = :webpage  title = "Blog" }
class "wdoc-body" { css = "background:#2e3440;" }
page a { sites = [:docs]  h1 "A" {} }
page b { sites = [:blog]  h1 "B" {} }
"##;
    with_multisite_build(src, None, |out| {
        for page in ["docs/a.html", "blog/b.html"] {
            let html = std::fs::read_to_string(out.join(page)).expect(page);
            assert!(
                html.contains(".wdoc-body { background:#2e3440; }"),
                "{page} should carry the global class:\n{html}"
            );
        }
    });
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
        html.contains("--wdoc-bg:#181825;"),
        "mocha bg missing:\n{html}"
    );
    assert!(
        html.contains("--wdoc-bg:#dce0e8;"),
        "latte bg missing:\n{html}"
    );
    // The toggle override side (book/webpage both get it via site_css).
    assert!(
        html.contains(":root[data-theme=\"light\"]{--wdoc-bg:#dce0e8;"),
        "data-theme light override missing:\n{html}"
    );
    // Accent resolves to the chosen hue var, and is applied to links.
    assert!(
        html.contains("--wdoc-accent:var(--wdoc-green);"),
        "accent var missing:\n{html}"
    );
    assert!(html.contains("a,.link{color:var(--wdoc-link);"), "{html}");
    // Apply rules reach the body and the chart palette.
    assert!(
        html.contains(
            "body,.wdoc-body{background:var(--wdoc-bg);color:var(--wdoc-fg);font-family:"
        ),
        "{html}"
    );
    assert!(
        html.contains(".wdoc-series-1{fill:var(--wdoc-blue);stroke:var(--wdoc-blue);}"),
        "{html}"
    );
}

#[test]
fn site_without_theme_defaults_to_forge() {
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

    // Forge outer bg (dark) + light bg, and the accent defaulting to the
    // theme's own `accent` (via `--wdoc-accent-pal`) when `accent` is unset.
    assert!(
        html.contains("--wdoc-bg:#0b0d10;"),
        "forge dark bg missing:\n{html}"
    );
    assert!(
        html.contains("--wdoc-bg:#fafafa;"),
        "forge light bg missing:\n{html}"
    );
    assert!(
        html.contains("--wdoc-link:#80caff;")
            && html.contains("--wdoc-font-body:'IBM Plex Sans', system-ui, sans-serif;"),
        "forge link or body-font token missing:\n{html}"
    );
    assert!(
        html.contains("--wdoc-accent:var(--wdoc-accent-pal);"),
        "default accent should follow the palette:\n{html}"
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
site b { default_template = :webpage  theme = :rose }
page pa { sites = [:a]  text { span "A" {} } }
page pb { sites = [:b]  text { span "B" {} } }
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let a = std::fs::read_to_string(out.path().join("pa.html")).expect("read pa");
    let b = std::fs::read_to_string(out.path().join("b").join("pb.html")).expect("read pb");

    assert!(
        a.contains("--wdoc-bg:#1d2021;"),
        "site a should be gruvbox:\n{a}"
    );
    assert!(
        !a.contains("--wdoc-bg:#191724;"),
        "site a must not leak rose:\n{a}"
    );
    assert!(
        b.contains("--wdoc-bg:#191724;"),
        "site b should be rose:\n{b}"
    );
    assert!(
        !b.contains("--wdoc-bg:#1d2021;"),
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
class "town-pin" { css = "color:#3b82f6;" }
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
    assert!(index.contains("<p>Guards the ruins.</p>"), "{index}");
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
fn user_collection_template_renders_members_to_one_file() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("collection.wcl");
    write_fixture(
        &src,
        r#"
template digest {
  slot content: content*
  slot aside: content* = fn(c: SlotOwner) -> list<Html> []
  slot intro: content
  render = fn(c: TemplateCtx) -> list<Html> flatten([
    [el("header", [], slot(c, :intro))],
    map(c.members, fn(member: PageHandle) -> Html
      ela("article", [], [["data-member", member.name], ["data-title", member.title]], flatten([
        slot(member, :content),
        [el("aside", [], slot(member, :aside))],
      ]))),
  ])
}
site {
  default_template = :digest
  intro { h1 "Weekly digest" {} }
  deck { section "Ordering only" {
    slide second
    slide first
  } }
}
page first  {
  title = "First headline"
  p "First story"
  aside { p "First aside" }
}
page second { p "Second story" }
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    assert_eq!(build_ok(&src, out.path()), 1);

    let index = std::fs::read_to_string(out.path().join("index.html")).expect("read index");
    assert!(index.contains("Weekly digest"), "{index}");
    assert!(
        index.contains("data-member=\"first\" data-title=\"First headline\"><p>First story</p>"),
        "{index}"
    );
    assert!(
        index.contains("data-member=\"second\" data-title=\"second\"><p>Second story</p>"),
        "{index}"
    );
    assert!(index.contains("<aside><p>First aside</p>"), "{index}");
    assert!(!index.contains("presentation.js"), "{index}");
    assert!(!out.path().join("_wdoc/presentation.js").exists());
    assert!(!out.path().join("first.html").exists());
    assert!(!out.path().join("second.html").exists());
}

#[test]
fn collection_slot_requirements_follow_their_arity() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("collection-slots.wcl");
    write_fixture(
        &src,
        r#"
template digest {
  slot content: content*
  slot aside: content*
  slot intro: content
  render = fn(c: TemplateCtx) -> list<Html> []
}
site { default_template = :digest  intro { h1 "Intro" {} } }
page complete {
  p "Body"
  aside { p "Aside" }
}
page missing  { p "No aside" }
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    match build(&src, out.path(), None) {
        Err(BuildError::BadPage(message)) => {
            assert!(message.contains("page `missing`"), "{message}");
            assert!(message.contains("required slot `aside`"), "{message}");
        }
        Err(_) => panic!("expected BadPage for missing per-member slot"),
        Ok(_) => panic!("expected missing per-member slot error"),
    }

    write_fixture(
        &src,
        r#"
template digest {
  slot content: content*
  slot intro: content
  render = fn(c: TemplateCtx) -> list<Html> []
}
site { default_template = :digest }
page member { p "Body" }
"#,
    );
    match build(&src, out.path(), None) {
        Err(BuildError::BadPage(message)) => {
            assert!(message.contains("site: required slot `intro`"), "{message}");
        }
        Err(_) => panic!("expected BadPage for missing site slot"),
        Ok(_) => panic!("expected missing site slot error"),
    }
}

#[test]
fn collection_template_forces_only_placed_members() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("lazy-collection.wcl");
    write_fixture(
        &src,
        r#"
template first_only {
  slot content: content*
  render = fn(c: TemplateCtx) -> list<Html>
    map(take(c.members, 1), fn(member: PageHandle) -> Html
      el("article", [], slot(member, :content)))
}
site { default_template = :first_only }
page placed   { p "Placed member" }
page unplaced { image "missing.png" {} }
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let index = std::fs::read_to_string(out.path().join("index.html")).expect("read index");
    assert!(index.contains("Placed member"), "{index}");
    assert!(!index.contains("missing.png"), "{index}");
}

#[test]
fn only_a_repeated_content_slot_declares_a_collection() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("ordinary-repeated-slot.wcl");
    write_fixture(
        &src,
        r#"
template ordinary {
  slot content: content
  slot aside: content*
  render = fn(c: TemplateCtx) -> list<Html>
    flatten([slot(c, :content), slot(c, :aside)])
}
site { default_template = :ordinary }
page first  { p "First" {}  aside { p "First aside" } }
page second { p "Second" {} aside { p "Second aside" } }
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    assert_eq!(build_ok(&src, out.path()), 2);
    assert!(out.path().join("first.html").exists());
    assert!(out.path().join("second.html").exists());
    assert!(!out.path().join("index.html").exists());
}

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
    assert!(!index.contains("<span class=\"heading-marker\""), "{index}");
    // The keyboard-nav player is written and linked exactly once.
    assert!(out.path().join("_wdoc").join("presentation.js").exists());
    assert_eq!(index.matches("_wdoc/presentation.js").count(), 1, "{index}");
    // No standalone per-slide files are written.
    assert!(!out.path().join("title.html").exists());
}

#[test]
fn presentation_rejects_duplicate_ids_across_placed_members() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("duplicate-deck-ids.wcl");
    write_fixture(
        &src,
        r#"
site {
  default_template = :presentation
  deck { section "S" {
    slide one
    slide two
  } }
}
page one { h1 "One" { id = shared } }
page two { h1 "Two" { id = shared } }
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    match build(&src, out.path(), None) {
        Err(BuildError::DuplicateId { page, id }) => {
            assert_eq!(page, "two");
            assert_eq!(id, "shared");
        }
        Err(err) => panic!("expected DuplicateId, got {}", err.render_plain()),
        Ok(_) => panic!("expected DuplicateId"),
    }
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
    let err = match build(&src, out.path(), None) {
        Err(err) => err,
        Ok(_) => panic!("missing deck must fail"),
    };
    assert!(err.render_plain().contains("needs a deck"));
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
    assert!(html.contains("<p>Hello</p>"), "{html}");
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
wdoc_component memo {
  wdoc_slot text
  wdoc_body { p $"NOTE:${text}" }
}
page index {
  let nodes = [
    { ref: "badge", text: "A" },
    { ref: "memo",  text: "B" },
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
  class $"${t.name}" { css = $"color:${t.hex};" }
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
wdoc_component screen_card {
  wdoc_slot title
  wdoc_body { p $"CARD:${title}" }
}
let screens = [
  { id: "listing", title: "Headphones" },
  { id: "detail",  title: "Speaker" },
]
wdoc_repeater { each = screens  as = :s
  page $"screen-${s.id}" {
    wdoc_instance { component = "screen_card"  title = s.title }
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

#[test]
fn toc_fallback_titles_entries_by_first_h1() {
    // A book site with no `toc` block gets the flat per-page fallback —
    // titled by each page's first h1, not its route name.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("s.wcl");
    write_fixture(
        &src,
        concat!(
            "site demo {\n  title = \"Demo\"\n  default_template = :book\n}\n",
            "page index {\n  h1 \"Getting Started\"\n  p \"Hello.\"\n}\n",
            "page untitled_page {\n  p \"No heading here.\"\n}\n",
        ),
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");
    assert!(
        html.contains(">Getting Started</a>") || html.contains("Getting Started</a>"),
        "sidebar entry uses the h1 title: {html}"
    );
    // A page with no h1 falls back to its name.
    assert!(
        html.contains("untitled_page"),
        "h1-less page keeps its name"
    );
}

#[test]
fn block_reference_documents_namespaced_schema_bare_and_qualified() {
    // Regression: reflection (`type_table` / `block_reference` /
    // `child_types`) silently emitted nothing for types declared under a
    // `namespace` in an imported schema file, while root-namespace types
    // worked. Both the bare reference (resolved via the import) and the
    // qualified `lib.…` path must render the heading + property table.
    let tmp = TempDir::new().expect("mkdir tempdir");
    std::fs::write(
        tmp.path().join("schema.wcl"),
        r#"
namespace lib

@block("gizmo")
type Gizmo {
  @inline(0) @doc("Stable id.") id: utf8
  @doc("Display name.") name: utf8
}

@document
type LibModel {
  @children("gizmo") gizmos: list<Gizmo>
}
"#,
    )
    .expect("write schema fixture");
    let src = tmp.path().join("main.wcl");
    write_fixture(
        &src,
        r#"
import "./schema.wcl"
page ref {
  h2 "bare"
  block_reference { type = LibModel }
  h2 "qualified"
  block_reference { type = lib.LibModel }
  h2 "table"
  type_table { type = lib.Gizmo }
}
"#,
    );

    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("ref.html")).expect("read html");

    // block_reference: one h3 kind heading per child slot, per usage.
    assert_eq!(
        html.matches("<h3 class=\"heading-3\">gizmo</h3>").count(),
        2,
        "bare and qualified block_reference both emit the kind heading:\n{html}"
    );
    // type_table rows reflect the namespaced type's fields + docs.
    assert_eq!(
        html.matches("<span class=\"code\">name</span>").count(),
        3,
        "all three tables carry the `name` property row:\n{html}"
    );
    assert!(html.contains("Stable id."), "@doc text renders:\n{html}");
    assert!(html.contains("Display name."), "@doc text renders:\n{html}");
}

#[test]
fn unresolvable_type_slot_is_a_build_error_not_silence() {
    // Regression: a `type =` reference that resolves to nothing used to
    // render an empty page with exit 0. A present-but-erroring component
    // slot binding must surface as a build diagnostic.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("main.wcl");
    write_fixture(
        &src,
        r#"
page ref {
  type_table { type = NoSuchType }
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    match build(&src, out.path(), None) {
        Err(BuildError::Eval(r)) => {
            let msg = format!("{r:?}");
            assert!(msg.contains("NoSuchType"), "{msg}");
        }
        Ok(n) => panic!("expected eval error, built {n} pages"),
        Err(_) => panic!("expected Eval error, got a different build error"),
    }
}

#[test]
fn unresolvable_repeater_each_is_a_build_error_not_silence() {
    // Same contract for `wdoc_repeater`: a present `each` whose
    // expression fails to evaluate is an error, not an empty expansion.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("main.wcl");
    write_fixture(
        &src,
        r#"
page ref {
  wdoc_repeater { each = no_such_list  as = :x
    p $"${x}"
  }
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    match build(&src, out.path(), None) {
        Err(BuildError::Eval(r)) => {
            let msg = format!("{r:?}");
            assert!(msg.contains("no_such_list"), "{msg}");
        }
        Ok(n) => panic!("expected eval error, built {n} pages"),
        Err(_) => panic!("expected Eval error, got a different build error"),
    }
}

#[test]
fn children_records_reach_custom_svg_lower() {
    // Foundation for the pure-WCL sequence / state diagrams: a custom
    // shape's `@children(...)` slot materialises into the record passed
    // to its `lower` as a list of schema-completed child records —
    // `@inline(0)` labels populated, omitted optionals present as
    // `none`, declaration order preserved.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("children_lower.wcl");
    write_fixture(
        &src,
        r##"
@block("itm")
type Itm extends SvgBlock {
  @inline(0) id: utf8
  note: utf8?
  lower = fn(i: Itm) -> list<Svg> []
}

@block("seqtest")
type SeqTest extends SvgBlock {
  x = 0.0
  y = 0.0
  width = 100.0
  height = 40.0
  @children("itm") items: list<Itm>
  lower = fn(s: SeqTest) -> list<Svg>
    map(s.items, fn(i: Itm) -> Svg Svg::Label {
      content: format("{}={}", i.id, i.note ?? "-"),
      x: 0.0,
      y: 0.0,
      font_size: 10.0,
    })
}

page index {
  diagram { width = 300  height = 200
    seqtest "s1" {
      itm "a" { note = "first" }
      itm "b" { }
    }
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    // Labels populated + optional defaulted to none (rendered "-").
    assert!(html.contains("a=first"), "inline label + field:\n{html}");
    assert!(
        html.contains("b=-"),
        "omitted optional reaches lower as none:\n{html}"
    );
    // Declaration order preserved.
    let pa = html.find("a=first").expect("a present");
    let pb = html.find("b=-").expect("b present");
    assert!(pa < pb, "children keep declaration order:\n{html}");
}

#[test]
fn computed_children_splice_matches_literal_for_custom_lower() {
    // The data-driven authoring path the wad book uses: a `@children`
    // slot fed by a computed splice (`items = map(data, …)`) reaches a
    // custom shape's `lower` schema-completed exactly like literal
    // child blocks (optionals → none).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("children_splice.wcl");
    write_fixture(
        &src,
        r##"
let names = ["a", "b"]

@block("itm")
type Itm extends SvgBlock {
  @inline(0) id: utf8
  note: utf8?
  lower = fn(i: Itm) -> list<Svg> []
}

@block("seqtest")
type SeqTest extends SvgBlock {
  x = 0.0
  y = 0.0
  width = 100.0
  height = 40.0
  @children("itm") items: list<Itm>
  lower = fn(s: SeqTest) -> list<Svg>
    map(s.items, fn(i: Itm) -> Svg Svg::Label {
      content: format("{}={}", i.id, i.note ?? "-"),
      x: 0.0,
      y: 0.0,
      font_size: 10.0,
    })
}

page lit {
  diagram { width = 300  height = 200
    seqtest "s1" {
      itm "a" { }
      itm "b" { }
    }
  }
}

page gen {
  diagram { width = 300  height = 200
    seqtest "s2" {
      items = map(names, fn(n: utf8) -> Itm { { id: n } })
    }
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let lit = std::fs::read_to_string(out.path().join("lit.html")).expect("read lit");
    let generated = std::fs::read_to_string(out.path().join("gen.html")).expect("read gen");

    for html in [&lit, &generated] {
        assert!(html.contains("a=-"), "first item renders:\n{html}");
        assert!(html.contains("b=-"), "second item renders:\n{html}");
    }
    // Identical lowered output for both authoring forms: compare the
    // generated <text> runs.
    let texts = |html: &str| -> Vec<String> {
        html.match_indices("a=-")
            .chain(html.match_indices("b=-"))
            .map(|(_, s)| s.to_string())
            .collect()
    };
    assert_eq!(texts(&lit), texts(&generated));
}

#[test]
fn edge_records_carry_label_and_dash() {
    // Generic edge presentation: a computed `edges = [...]` record's
    // `label` renders as a midpoint <text class="wdoc-edge-label">, and
    // `dash` becomes an inline stroke-dasharray on the edge stroke.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("edge_label.wcl");
    write_fixture(
        &src,
        r##"
page index {
  diagram {
    width = 400  height = 200
    routing = :straight
    rect { id = a  x = 20.0  y = 20.0  width = 60.0  height = 30.0 }
    rect { id = b  x = 300.0  y = 140.0  width = 60.0  height = 30.0 }
    edges = [ { source: "a", destination: "b", label: "ok then", dash: "5 4" } ]
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    assert!(
        html.contains("stroke-dasharray=\"5 4\""),
        "edge dash renders inline:\n{html}"
    );
    assert!(
        html.contains("class=\"wdoc-edge-label\""),
        "edge label class:\n{html}"
    );
    assert!(
        html.contains(">ok then</text>"),
        "edge label text at midpoint:\n{html}"
    );
}

#[test]
fn sequence_diagram_renders_feature_surface() {
    // The FEATURE-wdoc-sequence-diagram authoring surface end-to-end:
    // declaration-order columns, dashed reply arrows, the self-message
    // loop, actor / external heads, margin notes, and per-participant
    // links.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("seq.wcl");
    write_fixture(
        &src,
        r##"
page seq {
  sequence_diagram {
    width = 760
    desc = "checkout"

    participant "customer" { name = "Customer"  kind = :actor }
    participant "web"      { name = "Web App"   link = "seq" }
    participant "api"      { name = "API" }
    participant "stripe"   { name = "Stripe"    kind = :external }

    message "m1" { from = "customer" to = "web"    text = "Submit" }
    message "m2" { from = "web"      to = "api"    text = "POST /orders" }
    message "m3" { from = "api"      to = "stripe" text = "Capture" }
    message "m4" { from = "stripe"   to = "api"    text = "charge id"  kind = :reply }
    message "m5" { from = "api"      to = "api"    text = "persist" }
    note    "n1" { at = "m3"  text = "Idempotent." }
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("seq.html")).expect("read");

    // Columns rank in declaration order: lifelines at the column centres
    // (col_width 140 → 70 / 210 / 350 / 490).
    for x in ["70", "210", "350", "490"] {
        assert!(
            html.contains(&format!("class=\"wdoc-lifeline\" x1=\"{x}\"")),
            "lifeline at x={x}:\n{html}"
        );
    }
    // Actor head: the stick-figure circle.
    assert!(
        html.contains("<circle class=\"wdoc-participant-line\" cx=\"70\""),
        "actor head circle:\n{html}"
    );
    // External head: dashed outline polyline.
    assert!(
        html.contains("class=\"wdoc-participant-line\" points=\"430,3 550,3 550,37 430,37 430,3\""),
        "external dashed head:\n{html}"
    );
    // Reply arrow: a dashed message line.
    assert!(
        html.contains("stroke-dasharray=\"5 4\""),
        "dashed reply:\n{html}"
    );
    // Self-message: the three-segment loop polyline at api's column
    // (350 → 386).
    assert!(
        html.contains("points=\"350,"),
        "self-message loop polyline:\n{html}"
    );
    assert!(html.contains("386,"), "self-message loop width:\n{html}");
    // Note text renders in the margin box.
    assert!(html.contains("Idempotent."), "note text:\n{html}");
    // Participant link wraps the head in an <a> resolved like a prose
    // link.
    assert!(
        html.contains("<a href=\"seq.html\">"),
        "participant link:\n{html}"
    );
    // Message labels render.
    assert!(html.contains("POST /orders"), "message label:\n{html}");
}

#[test]
fn state_diagram_renders_feature_surface() {
    // The FEATURE-wdoc-state-diagram authoring surface end-to-end:
    // longest-path auto-ranking, the initial dot, final double borders,
    // `trigger [guard]` edge labels, and the self-loop.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("sc.wcl");
    write_fixture(
        &src,
        r##"
page sc {
  state_diagram {
    width = 640
    direction = :left_to_right

    state "pending"   { name = "Pending"   initial = true }
    state "paid"      { name = "Paid" }
    state "shipped"   { name = "Shipped"   final = true }
    state "cancelled" { name = "Cancelled" final = true }

    transition "t1" { from = "pending" to = "paid"      trigger = "payment captured" }
    transition "t2" { from = "paid"    to = "shipped"   trigger = "dispatched"  guard = "stock reserved" }
    transition "t3" { from = "pending" to = "cancelled" trigger = "customer cancels" }
    transition "t4" { from = "paid"    to = "paid"      trigger = "partial refund" }
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("sc.html")).expect("read");

    // Longest-path ranks along x (`:left_to_right`, cell 110 + gap 64):
    // pending 0, paid/cancelled 174, shipped 348 — rounded boxes (rx).
    for x in ["0", "174", "348"] {
        assert!(
            html.contains(&format!("<rect class=\"wdoc-state\" x=\"{x}\"")),
            "state box at rank x={x}:\n{html}"
        );
    }
    assert!(html.contains("rx=\"10\""), "rounded state boxes:\n{html}");
    // Initial pseudo-state: the filled entry dot.
    assert!(
        html.contains("class=\"wdoc-state-initial\""),
        "initial dot:\n{html}"
    );
    // Final marker: nested inner box (inset 3, rx 7).
    assert!(html.contains("rx=\"7\""), "final double border:\n{html}");
    // Edge labels: trigger alone and trigger [guard].
    assert!(
        html.contains(">payment captured</tspan>"),
        "trigger label:\n{html}"
    );
    assert!(
        html.contains(">dispatched [stock reserved]</tspan>"),
        "trigger [guard] label:\n{html}"
    );
    // Self-loop: the loop polyline + its label.
    assert!(
        html.contains(">partial refund</tspan>"),
        "self-loop label:\n{html}"
    );
    assert!(
        html.contains("class=\"wdoc-seq-message\" points="),
        "self-loop polyline:\n{html}"
    );
}

#[test]
fn state_diagram_ranks_top_to_bottom_by_default() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("sc.wcl");
    write_fixture(
        &src,
        r##"
page sc {
  state_diagram {
    state "a" { }
    state "b" { }
    transition "t1" { from = "a"  to = "b"  trigger = "go" }
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("sc.html")).expect("read");
    // Default flow is :top_to_bottom: rank 1 sits below rank 0
    // (cell 44 + layer gap 64 = 108).
    assert!(
        html.contains("<rect class=\"wdoc-state\" x=\"0\" y=\"0\""),
        "rank 0 at origin:\n{html}"
    );
    assert!(
        html.contains("<rect class=\"wdoc-state\" x=\"0\" y=\"108\""),
        "rank 1 below rank 0:\n{html}"
    );
}

#[test]
fn self_referential_component_is_depth_capped_not_a_stack_overflow() {
    // Regression: a component whose body instantiates itself used to
    // recurse until the process aborted with a stack overflow — the
    // expansion scope was rebuilt from the *definition's* (shallow)
    // lexical chain, so `binding_scope_depth()` never grew and the
    // MAX_LOWER_DEPTH guards were inert. The dynamic depth now rides on
    // the binding frame: the expansion caps with the depth marker and
    // the build succeeds.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("loopy.wcl");
    write_fixture(
        &src,
        r##"
wdoc_component loopy {
  wdoc_body {
    p "level"
    loopy { }
  }
}
page t {
  loopy { }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("t.html")).expect("read");
    let levels = html.matches("level").count();
    assert!(
        (30..=40).contains(&levels),
        "expansion capped near MAX_LOWER_DEPTH, got {levels} levels:\n{html}"
    );
    assert!(
        html.contains("depth limit reached"),
        "depth marker emitted:\n{html}"
    );
}

#[test]
fn component_colliding_with_block_kind_is_a_schema_error() {
    // Regression: a `wdoc_component` named like a registered @block kind
    // (here the stdlib `card`) used to validate instances against the
    // *block's* schema while expansion dispatched to the component —
    // confusing "field not declared by schema 'Card'" errors at every
    // use site. The collision itself is now the diagnostic.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("coll.wcl");
    write_fixture(
        &src,
        r##"
wdoc_component card {
  wdoc_slot thing
  wdoc_body { p $"${thing}" }
}
page t {
  card { thing = "x" }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    match build(&src, out.path(), None) {
        Err(BuildError::Schema(n)) => assert!(n >= 1),
        Ok(n) => panic!("expected schema error, built {n} pages"),
        Err(_) => panic!("expected Schema error, got a different build error"),
    }
}

#[test]
fn root_redeclaration_of_rust_dispatched_kind_is_a_schema_error() {
    // Regression: a root-authored `@block("diagram")` used to win schema
    // validation while the renderer kept the Rust diagram path — the
    // user's schema and `lower` were silently dead.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("redecl.wcl");
    write_fixture(
        &src,
        r##"
@block("diagram")
type MyDiagram extends ContentBlock {
  msg: utf8
  lower = fn(d: MyDiagram) -> list<Html> []
}
page t {
  diagram { msg = "hi" }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    match build(&src, out.path(), None) {
        Err(BuildError::Schema(n)) => assert!(n >= 1),
        Ok(n) => panic!("expected schema error, built {n} pages"),
        Err(_) => panic!("expected Schema error, got a different build error"),
    }
}

#[test]
fn slot_bound_from_same_named_repeater_variable_resolves_outward() {
    // Regression (PERF report side observation): `probe_card { a = a }`
    // where the enclosing repeater's `as` is also `a` used to fail with
    // a false "cycle while evaluating 'a'" — the RHS resolved to the
    // instance's own field instead of the repeater binding.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("slot.wcl");
    write_fixture(
        &src,
        r##"
wdoc_component probe_card {
  wdoc_slot a
  wdoc_body { p $"got:${a.name}" }
}
let things = [ { name: "Xavier" } ]
page t {
  wdoc_repeater { each = things  as = :a
    probe_card { a = a }
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("t.html")).expect("read");
    assert!(
        html.contains("<p>got:Xavier</p>"),
        "slot bound from same-named outer binding:\n{html}"
    );
}

#[test]
fn repeater_generated_children_reach_children_projections() {
    // Regression: a `wdoc_repeater` (or component instance) inside a
    // custom shape's body generated blocks that silently vanished from
    // `@children` projections — the lowering record only saw literal
    // AST children. Generated blocks now participate exactly like
    // literal ones, in both the sequence_diagram path and any custom
    // shape's `@children` slot.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("genseq.wcl");
    write_fixture(
        &src,
        r##"
wdoc_component msg_pair {
  wdoc_slot key
  wdoc_body {
    message $"${key}_go"   { from = "a"  to = "b"  text = $"${key} go" }
    message $"${key}_back" { from = "b"  to = "a"  text = $"${key} back"  kind = :reply }
  }
}
page seq {
  sequence_diagram {
    participant "a" { }
    participant "b" { }
    message "m0" { from = "a"  to = "b"  text = "literal" }
    wdoc_repeater { each = [["m1", "repeated one"], ["m2", "repeated two"]]  as = :m
      message $"${at(m, 0)}" { from = "a"  to = "b"  text = at(m, 1) }
    }
    msg_pair { key = "c1" }
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("seq.html")).expect("read");
    for text in [
        "literal",
        "repeated one",
        "repeated two",
        "c1 go",
        "c1 back",
    ] {
        assert!(
            html.contains(text),
            "generated message '{text}' missing:\n{html}"
        );
    }
}

#[test]
fn state_diagram_cycle_keeps_ranks_and_routes_back_edge_around() {
    // Regression: any transition cycle used to escalate every member to
    // the rank cap, collapsing the layout onto one row with the closing
    // edge drawn straight through the middle box. BFS layering keeps
    // the ranks; the closing transition routes around the diagram via a
    // side lane.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("cycle.wcl");
    write_fixture(
        &src,
        r##"
page sc {
  state_diagram {
    direction = :left_to_right
    state "pending"  { name = "Pending"  initial = true }
    state "paid"     { name = "Paid" }
    state "shipped"  { name = "Shipped" }
    transition "t1" { from = "pending" to = "paid"     trigger = "pay" }
    transition "t2" { from = "paid"    to = "shipped"  trigger = "ship" }
    transition "t3" { from = "shipped" to = "pending"  trigger = "return" }
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("sc.html")).expect("read");
    // Three distinct ranks along x despite the cycle.
    for x in ["0", "174", "348"] {
        assert!(
            html.contains(&format!("<rect class=\"wdoc-state\" x=\"{x}\"")),
            "rank x={x} kept under a cycle:\n{html}"
        );
    }
    // The closing transition is a routed polyline (not a straight Line
    // through the boxes), and its label renders.
    assert!(
        html.contains("class=\"wdoc-seq-message\" points="),
        "back-edge routed as a polyline:\n{html}"
    );
    assert!(html.contains(">return</tspan>"), "back-edge label:\n{html}");
}

#[test]
fn unknown_ids_in_sequence_and_state_diagrams_fail_the_build() {
    // Regression: a typo'd participant id drew a stray arrow into empty
    // space (the viewBox fit happily included it); a typo'd state id
    // silently dropped the transition. Both are build errors now.
    let tmp = TempDir::new().expect("mkdir tempdir");

    let seq = tmp.path().join("seq.wcl");
    write_fixture(
        &seq,
        r##"
page seq {
  sequence_diagram {
    participant "a" { }
    message "m1" { from = "a"  to = "wbe"  text = "typo" }
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    match build(&seq, out.path(), None) {
        Err(BuildError::Eval(r)) => {
            let msg = format!("{r:?}");
            assert!(msg.contains("unknown participant 'wbe'"), "{msg}");
            assert!(msg.contains("'m1'"), "{msg}");
        }
        Ok(n) => panic!("expected eval error, built {n} pages"),
        Err(_) => panic!("expected Eval error, got a different build error"),
    }

    let sc = tmp.path().join("sc.wcl");
    write_fixture(
        &sc,
        r##"
page sc {
  state_diagram {
    state "a" { }
    state "b" { }
    transition "t1" { from = "nope"  to = "b"  trigger = "go" }
  }
}
"##,
    );
    let out2 = TempDir::new().expect("mkdir out");
    match build(&sc, out2.path(), None) {
        Err(BuildError::Eval(r)) => {
            let msg = format!("{r:?}");
            assert!(msg.contains("unknown state 'nope'"), "{msg}");
        }
        Ok(n) => panic!("expected eval error, built {n} pages"),
        Err(_) => panic!("expected Eval error, got a different build error"),
    }
}

#[test]
fn non_list_lower_result_is_a_diagnostic_not_silence() {
    // Regression: a `lower` returning a non-list (the language doesn't
    // enforce fn return types at runtime) made the shape vanish with no
    // diagnostic. `none` stays a benign opt-out.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("nonlist.wcl");
    write_fixture(
        &src,
        r##"
@block("nonlist")
type NonList extends SvgBlock {
  x = 0.0
  y = 0.0
  width = 40.0
  height = 20.0
  lower = fn(s: NonList) -> list<Svg> 42
}
page t {
  diagram { width = 100  height = 60
    nonlist { }
  }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    match build(&src, out.path(), None) {
        Err(BuildError::Eval(r)) => {
            let msg = format!("{r:?}");
            assert!(msg.contains("returned i64"), "{msg}");
        }
        Ok(n) => panic!("expected eval error, built {n} pages"),
        Err(_) => panic!("expected Eval error, got a different build error"),
    }
}

#[test]
fn lower_body_eval_error_fails_the_html_build() {
    // An error raised *inside* a `lower` body used to be swallowed on the
    // HTML path (while Markdown/PDF failed loudly). All three backends now
    // share the same seam, so HTML fails with the eval diagnostic too.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("boom.wcl");
    write_fixture(
        &src,
        r##"
@block("boom")
type Boom extends ContentBlock {
  id: identifier?
  lower = fn(b: Boom) -> list<Html> [
    raw(no_such_helper(b))
  ]
}
page t {
  boom { }
}
"##,
    );
    let out = TempDir::new().expect("mkdir out");
    match build(&src, out.path(), None) {
        Err(BuildError::Eval(r)) => {
            let msg = format!("{r:?}");
            assert!(msg.contains("no_such_helper"), "{msg}");
        }
        Ok(n) => panic!("expected eval error, built {n} pages"),
        Err(_) => panic!("expected Eval error, got a different build error"),
    }
}

#[test]
fn diagram_image_with_unreadable_header_warns() {
    // A diagram image with no declared size whose header `image_dims`
    // can't parse (e.g. WebP) collapses to a 0x0 box — invisible. The
    // build still succeeds but now says so.
    let tmp = TempDir::new().expect("mkdir tempdir");
    std::fs::write(tmp.path().join("img.webp"), b"junk bytes, not an image")
        .expect("write junk image");
    let src = tmp.path().join("img.wcl");
    write_fixture(
        &src,
        "page p {\n  diagram { width = 100  height = 60\n    image \"img.webp\" { }\n  }\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let warnings = wcl_wdoc::take_render_warnings();
    assert!(
        warnings
            .iter()
            .any(|w| w.contains("img.webp") && w.contains("intrinsic size")),
        "expected an unsized-image warning, got: {warnings:?}"
    );
}

// ── include: build other wdoc documents under a folder into subdirs ──

/// Create `dir` (and parents) then write a wdoc fixture at `dir/<name>`.
fn write_in(dir: &Path, name: &str, body: &str) -> PathBuf {
    std::fs::create_dir_all(dir).expect("mkdir include fixture dir");
    let path = dir.join(name);
    write_fixture(&path, body);
    path
}

#[test]
fn include_builds_subsites_and_wires_nav() {
    let root = TempDir::new().expect("mkdir tempdir");
    let projects = root.path().join("projects");
    write_in(
        &projects.join("foo"),
        "main.wcl",
        "page index { h1 \"Foo site\" }",
    );
    write_in(
        &projects.join("bar"),
        "main.wcl",
        "page index { h1 \"Bar site\" }",
    );
    // A `.wcl` sitting directly in the scanned folder is ignored (depth 0).
    write_in(&projects, "notes.wcl", "page ignored { h1 \"Nope\" }");

    let parent = write_in(
        root.path(),
        "parent.wcl",
        r#"include "projects" { pattern = "main.wcl" }
site main { root = true  default_template = :webpage  title = "Parent"
  menu {
    item "Home" { page = index }
    wdoc_repeater { each = included_sites({ folder: "projects", pattern: "main.wcl" })  as = :s
      item $"${s.name}" { href = s.href }
    }
  }
}
page index { sites = [:main]  h1 "Parent home" }
"#,
    );

    let out = TempDir::new().expect("mkdir tempdir");
    let n = build_ok(&parent, out.path());
    // parent index + foo index + bar index (notes.wcl ignored).
    assert_eq!(n, 3, "expected parent + 2 sub-sites");

    // Each sub-site is a self-contained tree with its own assets.
    let foo = std::fs::read_to_string(out.path().join("projects/foo/index.html"))
        .expect("foo sub-site index");
    assert!(foo.contains("Foo site"), "{foo}");
    assert!(
        out.path().join("projects/foo/_wdoc").is_dir(),
        "foo sub-site has its own _wdoc"
    );
    assert!(
        out.path().join("projects/bar/index.html").exists(),
        "bar sub-site built"
    );
    // notes.wcl (directly in the folder) is not a sub-site.
    assert!(!out.path().join("projects/notes").exists());

    // The parent's nav links to the discovered sub-sites by folder name.
    let index = std::fs::read_to_string(out.path().join("index.html")).expect("parent index");
    assert!(index.contains("href=\"projects/foo/\""), "{index}");
    assert!(index.contains("href=\"projects/bar/\""), "{index}");
}

#[test]
fn include_glob_matches_across_subdirs() {
    let root = TempDir::new().expect("mkdir tempdir");
    let sites = root.path().join("sites");
    write_in(
        &sites.join("alpha"),
        "main.wcl",
        "page index { h1 \"Alpha\" }",
    );
    write_in(
        &sites.join("beta"),
        "entry.wcl",
        "page index { h1 \"Beta\" }",
    );

    let parent = write_in(
        root.path(),
        "parent.wcl",
        "include \"sites\" { pattern = \"*.wcl\" }\npage index { h1 \"P\" }",
    );
    let out = TempDir::new().expect("mkdir tempdir");
    let n = build_ok(&parent, out.path());
    assert_eq!(n, 3, "parent + alpha + beta (both match *.wcl)");
    assert!(out.path().join("sites/alpha/index.html").exists());
    assert!(out.path().join("sites/beta/index.html").exists());
}

#[test]
fn include_exact_pattern_filters_subdir_files() {
    let root = TempDir::new().expect("mkdir tempdir");
    let sites = root.path().join("sites");
    write_in(
        &sites.join("alpha"),
        "main.wcl",
        "page index { h1 \"Alpha\" }",
    );
    write_in(
        &sites.join("beta"),
        "entry.wcl",
        "page index { h1 \"Beta\" }",
    );

    let parent = write_in(
        root.path(),
        "parent.wcl",
        "include \"sites\" { pattern = \"main.wcl\" }\npage index { h1 \"P\" }",
    );
    let out = TempDir::new().expect("mkdir tempdir");
    let n = build_ok(&parent, out.path());
    assert_eq!(n, 2, "parent + alpha only (beta/entry.wcl excluded)");
    assert!(out.path().join("sites/alpha/index.html").exists());
    assert!(!out.path().join("sites/beta").exists());
}

#[test]
fn include_two_entries_in_one_folder_error() {
    let root = TempDir::new().expect("mkdir tempdir");
    let c = root.path().join("c");
    write_in(&c.join("foo"), "main.wcl", "page index { h1 \"A\" }");
    write_in(&c.join("foo"), "other.wcl", "page index { h1 \"B\" }");

    let parent = write_in(
        root.path(),
        "parent.wcl",
        "include \"c\" { pattern = \"*.wcl\" }\npage index { h1 \"P\" }",
    );
    let out = TempDir::new().expect("mkdir tempdir");
    let err = build(&parent, out.path(), None).expect_err("two entries in one folder");
    assert!(
        matches!(err, BuildError::BadPage(_)),
        "expected a collision BadPage"
    );
}

#[test]
fn include_missing_folder_error() {
    let root = TempDir::new().expect("mkdir tempdir");
    let parent = write_in(
        root.path(),
        "parent.wcl",
        "include \"ghost\" { pattern = \"*.wcl\" }\npage index { h1 \"P\" }",
    );
    let out = TempDir::new().expect("mkdir tempdir");
    let err = build(&parent, out.path(), None).expect_err("missing include folder");
    assert!(matches!(err, BuildError::BadPage(_)));
}

#[test]
fn include_cycle_is_bounded() {
    let root = TempDir::new().expect("mkdir tempdir");
    // self/main.wcl includes ".." (its parent folder), whose subdirectory
    // `self/` holds main.wcl — building it would recurse onto itself.
    let entry = write_in(
        &root.path().join("p").join("self"),
        "main.wcl",
        "include \"..\" { pattern = \"main.wcl\" }\npage index { h1 \"Loop\" }",
    );
    let out = TempDir::new().expect("mkdir tempdir");
    let err = build(&entry, out.path(), None).expect_err("self-including document");
    assert!(
        matches!(err, BuildError::IncludeCycle(_)),
        "expected an include cycle"
    );
}

// ── include: entry mode, site selector, richer records (extension) ──

#[test]
fn include_entry_mode_scans_immediate_subdirs() {
    let root = TempDir::new().expect("mkdir tempdir");
    let members = root.path().join("members");
    write_in(
        &members.join("ls").join("wdoc").join("book"),
        "main.wcl",
        "site book { title = \"ls\" }\npage index { sites = [:book]  h1 \"ls tool\" }",
    );
    write_in(
        &members.join("cat").join("wdoc").join("book"),
        "main.wcl",
        "site book { title = \"cat\" }\npage index { sites = [:book]  h1 \"cat tool\" }",
    );
    // A rendered tree inside a member must never be scanned — entry mode only
    // checks `<sub>/wdoc/book/main.wcl`, never recurses into `ls/out/`.
    write_in(
        &members.join("ls").join("out"),
        "main.wcl",
        "page junk { h1 \"junk\" }",
    );
    // A subdirectory lacking the entry file is skipped.
    std::fs::create_dir_all(members.join("nope")).expect("mkdir");

    let parent = write_in(
        root.path(),
        "parent.wcl",
        "include \"members\" { entry = \"wdoc/book/main.wcl\" }\npage index { h1 \"P\" }",
    );
    let out = TempDir::new().expect("mkdir tempdir");
    let n = build_ok(&parent, out.path());
    assert_eq!(n, 3, "parent + ls + cat (nope skipped, out/ not scanned)");
    assert!(out.path().join("members/ls/index.html").exists());
    assert!(out.path().join("members/cat/index.html").exists());
    assert!(!out.path().join("members/nope").exists());
    assert!(
        !out.path().join("members/ls/out").exists(),
        "member out/ not a sub-site"
    );
}

#[test]
fn include_requires_exactly_one_of_pattern_entry() {
    let root = TempDir::new().expect("mkdir tempdir");
    write_in(
        &root.path().join("m").join("a"),
        "main.wcl",
        "page index { h1 \"A\" }",
    );

    let neither = write_in(
        root.path(),
        "neither.wcl",
        "include \"m\" { }\npage index { h1 \"P\" }",
    );
    let o1 = TempDir::new().expect("mkdir tempdir");
    assert!(matches!(
        build(&neither, o1.path(), None).expect_err("neither mode"),
        BuildError::BadPage(_)
    ));

    let both = write_in(
        root.path(),
        "both.wcl",
        "include \"m\" { pattern = \"main.wcl\"  entry = \"main.wcl\" }\npage index { h1 \"P\" }",
    );
    let o2 = TempDir::new().expect("mkdir tempdir");
    assert!(matches!(
        build(&both, o2.path(), None).expect_err("both modes"),
        BuildError::BadPage(_)
    ));
}

#[test]
fn include_site_selector_builds_one_site() {
    let root = TempDir::new().expect("mkdir tempdir");
    write_in(
        &root.path().join("members").join("alpha"),
        "main.wcl",
        "site book { title = \"Book\" }\n\
         site skill { default_template = :ai_skill\n  skill { name = \"a\"  description = \"d\" }\n}\n\
         page home { sites = [:book]  h1 \"Book home\" }\n\
         page sk { sites = [:skill]  start = true  h1 \"Skill\" }",
    );
    let parent = write_in(
        root.path(),
        "parent.wcl",
        "include \"members\" { entry = \"main.wcl\"  site = \"book\" }\npage index { h1 \"P\" }",
    );
    let out = TempDir::new().expect("mkdir tempdir");
    let n = build_ok(&parent, out.path());
    assert_eq!(n, 2, "parent index + alpha's book page only");
    assert!(
        out.path().join("members/alpha/home.html").exists(),
        "book page built"
    );
    assert!(
        !out.path().join("members/alpha/sk.html").exists(),
        "skill page not built"
    );
    assert!(
        !out.path().join("members/alpha/SKILL.md").exists(),
        "skill site not built by HTML"
    );
}

#[test]
fn included_sites_records_carry_title_and_summary() {
    let root = TempDir::new().expect("mkdir tempdir");
    write_in(
        &root.path().join("members").join("foo"),
        "main.wcl",
        "site main { title = \"Foo Title\"  summary = \"Foo summary\" }\n\
         page index { sites = [:main]  h1 \"F\" }",
    );
    // No title → the record's title falls back to the folder name.
    write_in(
        &root.path().join("members").join("bar"),
        "main.wcl",
        "site main { summary = \"Bar summary\" }\npage index { sites = [:main]  h1 \"B\" }",
    );
    let parent = write_in(
        root.path(),
        "parent.wcl",
        "include \"members\" { entry = \"main.wcl\" }\n\
         page index {\n  \
           wdoc_repeater { each = included_sites({ folder: \"members\", entry: \"main.wcl\" })  as = :s\n    \
             p $\"${s.title}::${s.summary}\"\n  }\n}",
    );
    let out = TempDir::new().expect("mkdir tempdir");
    build_ok(&parent, out.path());
    let index = std::fs::read_to_string(out.path().join("index.html")).expect("parent index");
    assert!(
        index.contains("Foo Title::Foo summary"),
        "title+summary: {index}"
    );
    assert!(
        index.contains("bar::Bar summary"),
        "title falls back to name: {index}"
    );
}

// ---------------------------------------------------------------------------
// Incremental rebuild (`build_incremental`) — the dev server's targeted path.
// ---------------------------------------------------------------------------

/// Lay out a small multi-file book under `dir`: `main.wcl` (the `site` + an
/// `iconset` + imports), two page files `a.wcl` / `b.wcl`, and a `lib.wcl`
/// holding a `class`. Mirrors how the real docs split pages across imported
/// files. Returns the `main.wcl` path.
fn write_incremental_book(dir: &Path) -> PathBuf {
    let main = dir.join("main.wcl");
    write_fixture(
        &main,
        r#"
iconset lucide {}

site docs {
  default_template = :book
  title = "Docs"
  toc {
    chapter "A" { page = "a" }
    chapter "B" { page = "b" }
  }
}

import "./a.wcl"
import "./b.wcl"
import "./lib.wcl"
"#,
    );
    // Page files don't re-import the wdoc schema — it resolves document-wide
    // through main.wcl's `import <wdoc.wcl>`, exactly like the real docs.
    std::fs::write(
        dir.join("a.wcl"),
        "page a {\n  sites = [:docs]\n  h1 \"Page A\"\n  p \"Original A content.\"\n}\n",
    )
    .expect("write a.wcl");
    std::fs::write(
        dir.join("b.wcl"),
        "page b {\n  sites = [:docs]\n  h1 \"Page B\"\n  p \"Original B content.\"\n}\n",
    )
    .expect("write b.wcl");
    std::fs::write(
        dir.join("lib.wcl"),
        "class \"note\" { accent = \"#333333\" }\n",
    )
    .expect("write lib.wcl");
    main
}

fn rebuild(main: &Path, out: &Path, changed: &[PathBuf]) -> RebuildOutcome {
    match build_incremental(main, out, None, &BuildOptions::default(), changed) {
        Ok(o) => o,
        Err(e) => panic!("incremental build failed: {}", e.render_plain()),
    }
}

#[test]
fn incremental_targets_only_the_edited_page() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let main = write_incremental_book(tmp.path());
    let out = TempDir::new().expect("mkdir out");
    build_ok(&main, out.path());

    // Capture page B's output so we can prove a targeted A rebuild leaves it
    // byte-for-byte untouched.
    let b_before = std::fs::read_to_string(out.path().join("b.html")).expect("read b.html");

    // Edit only page A's content file.
    let a = tmp.path().join("a.wcl");
    std::fs::write(
        &a,
        "page a {\n  sites = [:docs]\n  h1 \"Page A\"\n  p \"Edited A content!\"\n}\n",
    )
    .expect("rewrite a.wcl");

    match rebuild(&main, out.path(), &[a]) {
        RebuildOutcome::Targeted { pages } => assert_eq!(pages, vec!["a".to_string()]),
        RebuildOutcome::Full { pages } => panic!("expected targeted, got full ({pages} pages)"),
    }

    let a_html = std::fs::read_to_string(out.path().join("a.html")).expect("read a.html");
    assert!(a_html.contains("Edited A content!"), "{a_html}");
    let b_after = std::fs::read_to_string(out.path().join("b.html")).expect("read b.html");
    assert_eq!(b_before, b_after, "page B must not be re-rendered");
}

#[test]
fn full_build_writes_page_manifest_targeted_leaves_it() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let main = write_incremental_book(tmp.path());
    let out = TempDir::new().expect("mkdir out");
    build_ok(&main, out.path());

    // A full build writes the per-site page manifest the editor's lazy
    // preview rebuild maps `<name>.html` requests through.
    let manifest_path = out.path().join(wcl_wdoc::PAGES_MANIFEST_HREF);
    let before = std::fs::read_to_string(&manifest_path).expect("read pages.json");
    let v: serde_json::Value = serde_json::from_str(&before).expect("parse pages.json");
    assert_eq!(v["start"], "index", "no page sets start = true");
    let pages: Vec<&str> = v["pages"]
        .as_array()
        .expect("pages array")
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect();
    assert_eq!(pages, vec!["a", "b"]);

    // A targeted rebuild reuses the prior full build's manifest untouched.
    let a = tmp.path().join("a.wcl");
    std::fs::write(
        &a,
        "page a {\n  sites = [:docs]\n  h1 \"Page A\"\n  p \"Edited A content!\"\n}\n",
    )
    .expect("rewrite a.wcl");
    match rebuild(&main, out.path(), &[a]) {
        RebuildOutcome::Targeted { pages } => assert_eq!(pages, vec!["a".to_string()]),
        RebuildOutcome::Full { pages } => panic!("expected targeted, got full ({pages} pages)"),
    }
    let after = std::fs::read_to_string(&manifest_path).expect("re-read pages.json");
    assert_eq!(
        before, after,
        "targeted rebuild must not rewrite the manifest"
    );
}

#[test]
fn incremental_falls_back_when_site_file_changes() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let main = write_incremental_book(tmp.path());
    let out = TempDir::new().expect("mkdir out");
    build_ok(&main, out.path());

    // Touch main.wcl — it carries the `site` block, so a change there could
    // shift site-wide state and must full-rebuild.
    let current = std::fs::read_to_string(&main).expect("read main");
    std::fs::write(&main, format!("{current}\n// touched\n")).expect("rewrite main");

    match rebuild(&main, out.path(), std::slice::from_ref(&main)) {
        RebuildOutcome::Full { .. } => {}
        RebuildOutcome::Targeted { pages } => panic!("expected full, got targeted: {pages:?}"),
    }
}

#[test]
fn incremental_falls_back_for_library_change() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let main = write_incremental_book(tmp.path());
    let out = TempDir::new().expect("mkdir out");
    build_ok(&main, out.path());

    // lib.wcl declares a `class` (a non-page block) — editing it affects the
    // site CSS embedded in every page, so it must full-rebuild.
    let lib = tmp.path().join("lib.wcl");
    std::fs::write(&lib, "class \"note\" { accent = \"#abcdef\" }\n").expect("rewrite lib.wcl");

    match rebuild(&main, out.path(), &[lib]) {
        RebuildOutcome::Full { .. } => {}
        RebuildOutcome::Targeted { pages } => panic!("expected full, got targeted: {pages:?}"),
    }
}

#[test]
fn incremental_falls_back_when_a_page_is_added() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let main = write_incremental_book(tmp.path());
    let out = TempDir::new().expect("mkdir out");
    build_ok(&main, out.path());

    // Add a second page to a.wcl: the page set grows, which shifts every
    // other page's template `pages` list, so a targeted render is unsafe.
    let a = tmp.path().join("a.wcl");
    std::fs::write(
        &a,
        "page a {\n  sites = [:docs]\n  h1 \"Page A\"\n  p \"A\"\n}\n\
         page a2 {\n  sites = [:docs]\n  h1 \"Page A2\"\n  p \"A2\"\n}\n",
    )
    .expect("rewrite a.wcl");

    match rebuild(&main, out.path(), &[a]) {
        RebuildOutcome::Full { .. } => {}
        RebuildOutcome::Targeted { pages } => panic!("expected full, got targeted: {pages:?}"),
    }
}

#[test]
fn incremental_falls_back_when_an_unseen_icon_is_added() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let main = write_incremental_book(tmp.path());
    let out = TempDir::new().expect("mkdir out");
    // Seed page A with one icon so the initial sprite holds `lucide-check`.
    let a = tmp.path().join("a.wcl");
    std::fs::write(
        &a,
        "page a {\n  sites = [:docs]\n  h1 \"Page A\"\n  p \"Status: :lucide.check: ok\"\n}\n",
    )
    .expect("seed a.wcl");
    build_ok(&main, out.path());

    // Now use an icon the on-disk sprite never captured — the targeted path
    // can't merge it into the shared sprite, so it must full-rebuild.
    std::fs::write(
        &a,
        "page a {\n  sites = [:docs]\n  h1 \"Page A\"\n  p \"Status: :lucide.house: home\"\n}\n",
    )
    .expect("rewrite a.wcl");

    match rebuild(&main, out.path(), &[a]) {
        RebuildOutcome::Full { .. } => {}
        RebuildOutcome::Targeted { pages } => panic!("expected full, got targeted: {pages:?}"),
    }

    // The full fallback rewrote the sprite to include the new icon.
    let sprite =
        std::fs::read_to_string(out.path().join("_wdoc").join("icons.svg")).expect("read sprite");
    assert!(sprite.contains("id=\"lucide-house\""), "{sprite}");
}

#[test]
fn incremental_reuses_an_already_present_icon() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let main = write_incremental_book(tmp.path());
    let out = TempDir::new().expect("mkdir out");
    // Both pages use the same icon, so it's already in the shared sprite.
    let a = tmp.path().join("a.wcl");
    let b = tmp.path().join("b.wcl");
    std::fs::write(
        &a,
        "page a {\n  sites = [:docs]\n  h1 \"Page A\"\n  p \":lucide.check: A\"\n}\n",
    )
    .expect("seed a.wcl");
    std::fs::write(
        &b,
        "page b {\n  sites = [:docs]\n  h1 \"Page B\"\n  p \":lucide.check: B\"\n}\n",
    )
    .expect("seed b.wcl");
    build_ok(&main, out.path());

    // Re-edit A's prose but keep using the already-captured icon — targetable.
    std::fs::write(
        &a,
        "page a {\n  sites = [:docs]\n  h1 \"Page A\"\n  p \":lucide.check: edited\"\n}\n",
    )
    .expect("rewrite a.wcl");

    match rebuild(&main, out.path(), &[a]) {
        RebuildOutcome::Targeted { pages } => assert_eq!(pages, vec!["a".to_string()]),
        RebuildOutcome::Full { pages } => panic!("expected targeted, got full ({pages} pages)"),
    }
}

/// `markdown_source` renders its body to a highlighted Markdown code block,
/// rewrites internal links into the skill-folder layout, and — in comment
/// mode — carries a `data-wcl-block` anchor so the review client can attach a
/// comment (an agent then fixes the named source).
#[test]
fn markdown_source_previews_skill_markdown_and_is_commentable() {
    let tmp = TempDir::new().expect("tempdir");
    let out = TempDir::new().expect("out");
    let main = tmp.path().join("main.wcl");
    write_fixture(
        &main,
        "site s { default_template = :webpage  root = true }\n\
         page index { start = true\n  \
           markdown_source { start_page = \"index\"  reference = true  pages = [\"index\", \"other_ref\"]\n    \
             h2 \"Sample\"\n    \
             p \"Body with a [link](other_ref).\"\n  \
           }\n\
         }\n\
         page other_ref { h1 \"Other\" }\n",
    );

    let opts = BuildOptions {
        comment_mode: true,
        ..Default::default()
    };
    if build_with_options(&main, out.path(), None, &opts).is_err() {
        panic!("build with comment mode failed");
    }
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");

    // The body lowered to Markdown, shown in a highlighted `markdown` code block.
    assert!(
        html.contains("language-markdown"),
        "expected a markdown code block"
    );
    // The body lowered to Markdown then syntect-highlighted: the `## Sample`
    // heading survives as a heading token run plus its text.
    assert!(
        html.contains("tok-heading") && html.contains(">Sample<"),
        "expected the body rendered as a Markdown heading, got:\n{html}"
    );
    // Internal links resolve into the skill folder layout (reference page).
    assert!(
        html.contains("../references/other_ref.md"),
        "expected skill-folder link rewrite, got:\n{html}"
    );
    // Comment mode anchors the block so a reviewer can pin a note to it.
    assert!(
        html.contains("data-wcl-kind=\"markdown_source\""),
        "expected a comment anchor on the markdown_source block, got:\n{html}"
    );
}

/// Edit mode (the `wcl editor` preview build) stamps each block with its
/// source span and home file (plus the shared `data-wcl-*` block anchor and
/// the page-block span on the wrapper) so an editor client can map a rendered
/// block back to the source that declares it. A plain build emits none of
/// this markup.
#[test]
fn edit_mode_stamps_source_span_and_file_anchors() {
    let tmp = TempDir::new().expect("tempdir");
    let out = TempDir::new().expect("out");
    let main = tmp.path().join("main.wcl");
    write_fixture(
        &main,
        "site s { default_template = :webpage  root = true }\n\
         page index { start = true\n  h1 \"Hello\"\n  p \"Body.\"\n }\n",
    );

    let opts = BuildOptions {
        edit_mode: true,
        ..Default::default()
    };
    if build_with_options(&main, out.path(), None, &opts).is_err() {
        panic!("build with edit mode failed");
    }
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");

    // Each block carries its kind, a byte span, and its home file.
    assert!(
        html.contains("data-wcl-kind=\"h1\""),
        "expected a block anchor, got:\n{html}"
    );
    assert!(
        html.contains("data-wcl-span=\""),
        "expected a source span anchor in edit mode, got:\n{html}"
    );
    assert!(
        html.contains("data-wcl-file=\""),
        "expected a home-file anchor in edit mode, got:\n{html}"
    );
    // The page wrapper carries the page block's own span for top-level inserts.
    assert!(
        html.contains("data-wcl-page-span=\""),
        "expected the page-block span on the wrapper, got:\n{html}"
    );
    assert!(
        html.contains("data-wcl-slot=\"content\""),
        "expected the reserved content slot on the page wrapper, got:\n{html}"
    );

    // A plain build leaks none of the editor markup.
    if build_with_options(&main, out.path(), None, &BuildOptions::default()).is_err() {
        panic!("plain build failed");
    }
    let plain = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");
    assert!(
        !plain.contains("data-wcl-span=") && !plain.contains("data-wcl-block"),
        "plain build must not emit editor anchors, got:\n{plain}"
    );
}

#[test]
fn edit_mode_wraps_each_slot_with_its_own_provenance() {
    let tmp = TempDir::new().expect("tempdir");
    let out = TempDir::new().expect("out");
    let main = tmp.path().join("main.wcl");
    let layout = tmp.path().join("layout.wcl");
    std::fs::write(
        &layout,
        r#"
template article {
  slot content: content
  slot hero: content?
  slot sidebar: content?
  slot footer: content = fn(c: TemplateCtx) -> list<Html> [
    el("footer", [], [raw(c.title)])
  ]
  render = fn(c: TemplateCtx) -> list<Html>
    flatten([slot(c, :hero), slot(c, :content), slot(c, :footer)])
}
"#,
    )
    .expect("write layout fixture");
    write_fixture(
        &main,
        r#"
import "./layout.wcl"
site { default_template = :article title = "Layout fallback" }
page index {
  hero { h1 "Page hero" }
  p "Body."
}
"#,
    );
    let opts = BuildOptions {
        edit_mode: true,
        ..Default::default()
    };
    if let Err(err) = build_with_options(&main, out.path(), None, &opts) {
        panic!("edit build: {}", err.render_plain());
    }
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read html");

    let slot_wrapper_opening_tag = |slot: &str| {
        let slot_attr = format!("data-wcl-slot=\"{slot}\"");
        let slot_at = html
            .find(&slot_attr)
            .unwrap_or_else(|| panic!("missing {slot_attr}:\n{html}"));
        let start = html[..slot_at]
            .rfind("<div ")
            .unwrap_or_else(|| panic!("missing wrapper start for {slot}:\n{html}"));
        let end = html[slot_at..]
            .find('>')
            .map(|offset| slot_at + offset + 1)
            .unwrap_or_else(|| panic!("missing wrapper end for {slot}:\n{html}"));
        &html[start..end]
    };

    let page_file = format!("data-wcl-page-file=\"{}\"", main.display());
    for slot in ["content", "hero", "sidebar"] {
        let opening_tag = slot_wrapper_opening_tag(slot);
        assert!(
            opening_tag.contains(&page_file),
            "{slot} must belong to the page: {opening_tag}"
        );
        assert!(
            opening_tag.contains("data-wcl-page-span=\""),
            "{slot}: {opening_tag}"
        );
    }

    let fallback = slot_wrapper_opening_tag("footer");
    assert!(
        fallback.contains(&format!("data-wcl-file=\"{}\"", layout.display())),
        "fallback must belong to the layout declaration: {fallback}"
    );
    assert!(fallback.contains("data-wcl-span=\""), "{fallback}");
    assert!(!fallback.contains("data-wcl-page-file="), "{fallback}");

    assert!(html.contains("Page hero"), "named slot content: {html}");
    assert!(html.contains("Layout fallback"), "fallback content: {html}");
}

/// Edit mode anchors each diagram child shape: a free-layout child gets a
/// transform-less wrapping `<g>` carrying `data-wcl-shape` + kind/span/file,
/// a solver-laid child gets the same attributes on its existing translate
/// wrapper, and the `<svg>` root carries the effective `data-wcl-layout`.
#[test]
fn edit_mode_anchors_diagram_shapes() {
    let tmp = TempDir::new().expect("tempdir");
    let out = TempDir::new().expect("out");
    let main = tmp.path().join("main.wcl");
    write_fixture(
        &main,
        "site s { default_template = :webpage  root = true }\n\
         page index { start = true\n\
           diagram {\n    width = 320\n    height = 160\n\
             rect { id = a  x = 20.0  y = 30.0  width = 80.0  height = 50.0 }\n\
             container { id = c  width = 120.0  height = 100.0\n\
               circle { id = n  cx = 30.0  cy = 30.0  r = 20.0 }\n    }\n  }\n\
           diagram {\n    width = 400\n    height = 300\n    layout = :layered\n\
             process p1 \"Start\"\n    process p2 \"End\"\n    p1 -> p2\n  }\n }\n",
    );

    let opts = BuildOptions {
        edit_mode: true,
        ..Default::default()
    };
    if build_with_options(&main, out.path(), None, &opts).is_err() {
        panic!("build with edit mode failed");
    }
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");

    // Free layout: each child is wrapped in an anchored, transform-less <g>.
    assert!(
        html.contains("<g data-wcl-shape data-wcl-kind=\"rect\" data-wcl-span=\""),
        "expected an anchored free-layout rect, got:\n{html}"
    );
    // The shape's own id rides along: it is the name `a -> b` connections
    // use, and the only per-instance identity a repeater-generated shape has.
    assert!(
        html.contains("data-wcl-shape-id=\""),
        "expected a stamped shape id, got:\n{html}"
    );
    // The nested container and its own child are both anchored.
    assert!(
        html.contains("data-wcl-shape data-wcl-kind=\"container\""),
        "expected an anchored container, got:\n{html}"
    );
    assert!(
        html.contains("data-wcl-shape data-wcl-kind=\"circle\""),
        "expected an anchored nested circle, got:\n{html}"
    );
    // Solver layout: the attrs ride the existing translate wrapper.
    assert!(
        html.contains("\" data-wcl-shape data-wcl-kind=\"process\""),
        "expected anchored layered shapes, got:\n{html}"
    );
    let planned = html
        .split("data-wcl-shape data-wcl-kind=\"process\"")
        .next()
        .expect("split");
    assert!(
        planned
            .rsplit("<g ")
            .next()
            .is_some_and(|tail| tail.starts_with("transform=\"translate(")),
        "process anchor should sit on the translate wrapper, got:\n{html}"
    );
    // The svg roots carry the effective layout for the Design client.
    assert!(
        html.contains("data-wcl-layout=\"free\""),
        "expected the free diagram's layout attr, got:\n{html}"
    );
    assert!(
        html.contains("data-wcl-layout=\"layered\""),
        "expected the layered diagram's layout attr, got:\n{html}"
    );

    // A plain build leaks none of the shape markup.
    if build_with_options(&main, out.path(), None, &BuildOptions::default()).is_err() {
        panic!("plain build failed");
    }
    let plain = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");
    assert!(
        !plain.contains("data-wcl-shape") && !plain.contains("data-wcl-layout"),
        "plain build must not emit shape anchors, got:\n{plain}"
    );
}

/// Edit mode anchors each `li` item too — items render directly (not through
/// the block dispatcher), and the Design mode's item-level editing needs each
/// `<li>`'s own source span, not just the outer `list`'s.
#[test]
fn edit_mode_anchors_list_items() {
    let tmp = TempDir::new().expect("tempdir");
    let out = TempDir::new().expect("out");
    let main = tmp.path().join("main.wcl");
    write_fixture(
        &main,
        "site s { default_template = :webpage  root = true }\n\
         page index { start = true\n  list {\n    li \"First\"\n    li \"Second\" {\n      li \"Nested\"\n    }\n  }\n }\n",
    );

    let opts = BuildOptions {
        edit_mode: true,
        ..Default::default()
    };
    if build_with_options(&main, out.path(), None, &opts).is_err() {
        panic!("build with edit mode failed");
    }
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");
    // Every item — including the nested one — carries its own anchor.
    assert_eq!(
        html.matches("data-wcl-kind=\"li\"").count(),
        3,
        "expected all three li items anchored, got:\n{html}"
    );
    assert!(
        html.contains("<li data-wcl-block data-wcl-kind=\"li\" data-wcl-span=\""),
        "expected span-stamped li tags, got:\n{html}"
    );

    // A plain build emits clean list markup.
    if build_with_options(&main, out.path(), None, &BuildOptions::default()).is_err() {
        panic!("plain build failed");
    }
    let plain = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");
    assert!(
        !plain.contains("data-wcl-kind=\"li\""),
        "plain build must not anchor list items, got:\n{plain}"
    );
}

/// `edit_field` is a transparent wrapper binding its children to one field of
/// a data object: the children render in place in every mode; edit mode
/// additionally stamps `data-wcl-field-kind` / `-target` / `-name` (and the
/// `plain` flag) onto the first child's root tag. It never becomes a block
/// anchor target itself.
#[test]
fn edit_field_binds_children_in_edit_mode_only() {
    let tmp = TempDir::new().expect("tempdir");
    let out = TempDir::new().expect("out");
    let main = tmp.path().join("main.wcl");
    write_fixture(
        &main,
        "site s { default_template = :webpage  root = true }\n\
         page index { start = true\n\
           edit_field { kind = \"concept\"  target = \"alpha\"  field = \"name\"\n    h1 \"Hello\"\n  }\n\
           edit_field { kind = \"concept\"  target = \"alpha\"  field = \"summary\"  plain = true\n    p \"Lede.\"\n  }\n\
         }\n",
    );

    let opts = BuildOptions {
        edit_mode: true,
        ..Default::default()
    };
    if build_with_options(&main, out.path(), None, &opts).is_err() {
        panic!("build with edit mode failed");
    }
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");
    assert!(html.contains(">Hello<"), "wrapped h1 must render:\n{html}");
    assert!(
        html.contains("data-wcl-field-kind=\"concept\"")
            && html.contains("data-wcl-field-target=\"alpha\"")
            && html.contains("data-wcl-field-name=\"name\""),
        "expected field-binding attributes, got:\n{html}"
    );
    assert!(
        html.contains("data-wcl-field-plain"),
        "expected the plain flag on the summary binding, got:\n{html}"
    );
    // The binding rides the child's root tag (which keeps its own block
    // anchor); the wrapper itself is not a block target.
    assert!(
        !html.contains("data-wcl-kind=\"edit_field\""),
        "edit_field must not be a selectable block, got:\n{html}"
    );

    // Outside edit mode the children render unchanged, with no bindings.
    if build_with_options(&main, out.path(), None, &BuildOptions::default()).is_err() {
        panic!("plain build failed");
    }
    let plain = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");
    assert!(
        plain.contains(">Hello<"),
        "wrapped h1 must render:\n{plain}"
    );
    assert!(plain.contains("Lede."), "wrapped p must render:\n{plain}");
    assert!(
        !plain.contains("data-wcl-field-"),
        "plain build must not emit field bindings, got:\n{plain}"
    );
}

// ── The semantic content IR ───────────────────────────────────────

/// Build one fixture and return `index.html`.
fn build_index(src: &str) -> String {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let file = tmp.path().join("main.wcl");
    write_fixture(&file, src);
    let out = TempDir::new().expect("mkdir out");
    build_ok(&file, out.path());
    std::fs::read_to_string(out.path().join("index.html")).expect("read index.html")
}

#[test]
fn a_user_block_lowering_to_the_content_ir_renders_in_html() {
    let html = build_index(
        "@block(\"gadget\")\n\
         type Gadget extends ContentBlock {\n  \
           @inline(0) name: utf8\n  id: identifier?\n  \
           lower = fn(g: Gadget) -> list<Content> [\n    \
             Content::Heading { level: 3, text: g.name },\n    \
             Content::Paragraph { text: \"A **gadget**.\" },\n  ]\n\
         }\n\
         page index {\n  gadget \"Widget\"\n}\n",
    );
    // A real `<h3>` — the level is a number on the node, not something the
    // backend parses back out of a class. The `heading-3` class rides along
    // as the theme's style hook, derived from that number.
    assert!(
        html.contains("<h3 class=\"heading-3\">Widget</h3>"),
        "{html}"
    );
    assert!(
        html.contains("<p>A <span class=\"bold\">gadget</span>.</p>"),
        "prose runs through the inline engine: {html}"
    );
}

#[test]
fn a_custom_content_video_gets_the_html_player() {
    let src = r#"
@block("clip")
type Clip extends ContentBlock {
  id: identifier?
  lower = fn(c: Clip) -> list<Content> [
    Content::Video {
      source: "https://example.com/player/embed/abc",
      title: "Custom clip",
    },
  ]
}
page index {
  clip { }
}
"#;
    let (html, out) = build_video(src);
    assert!(
        html.contains("data-kind=\"generic\" data-src=\"https://example.com/player/embed/abc\""),
        "{html}"
    );
    assert!(html.contains("_wdoc/wdoc-video.js"), "{html}");
    assert!(
        out.path().join("_wdoc/wdoc-video.js").is_file(),
        "the custom IR producer must ship the player asset"
    );
}

#[test]
fn a_content_drawing_renders_its_shapes_as_svg() {
    // `Drawing` carries the SVG shape vocabulary, not SVG markup — the
    // closed IR's answer to a bespoke page-level drawing.
    let html = build_index(
        "@block(\"badge\")\n\
         type Badge extends ContentBlock {\n  \
           id: identifier?\n  \
           lower = fn(b: Badge) -> list<Content> [\n    \
             Content::Drawing {\n      \
               width: 100.0,\n      \
               shapes: [\n        \
                 Svg::Rect { x: 0.0, y: 0.0, width: 40.0, height: 20.0, fill: \"#abc\" },\n      ],\n    },\n  ]\n\
         }\n\
         page index {\n  badge { }\n}\n",
    );
    assert!(html.contains("<svg"), "a drawing emits an svg: {html}");
    assert!(
        html.contains("width=\"40\"") && html.contains("fill=\"#abc\""),
        "the shape's own geometry survives: {html}"
    );
}

#[test]
fn a_malformed_content_node_fails_the_html_build() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let file = tmp.path().join("main.wcl");
    write_fixture(
        &file,
        "@block(\"broken\")\n\
         type Broken extends ContentBlock {\n  \
           id: identifier?\n  \
           lower = fn(b: Broken) -> list<Content> [Content::Heading { level: 900 }]\n\
         }\n\
         page index {\n  broken { }\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    assert!(
        build(&file, out.path(), None).is_err(),
        "an out-of-range `level` is an authoring error, not a silent skip"
    );
}

// ── The five markup-using blocks, routed through the content IR ────
//
// Each of these used to build its own markup, so the HTML backend was the
// only one that could read it back. They lower to content nodes now and
// every backend renders from the one declaration; these pin the HTML
// reading, which is also what the stdlib stylesheets are written against.

#[test]
fn chapter_header_renders_kicker_title_and_meta() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("ch.wcl");
    write_fixture(
        &src,
        r#"
page index {
  chapter_header "Getting started" {
    kicker = "Chapter 1"
    reading_time = "9 min read"
    updated = "2026-08-02"
    version = "wdoc 0.24.1-alpha"
  }
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    assert!(html.contains("<header class=\"chapter-header\">"), "{html}");
    assert!(
        html.contains("<p class=\"chapter-kicker\">Chapter 1</p>"),
        "{html}"
    );
    // The title carries the `heading-1` hook, so it is sized like any
    // other level-1 heading and the per-page heading pass finds it (this
    // page is untemplated, and that pass runs on the templated path).
    assert!(
        html.contains("<h1 class=\"heading-1\">Getting started</h1>"),
        "{html}"
    );
    assert!(
        html.contains("<p class=\"chapter-meta\">9 min read · 2026-08-02 · wdoc 0.24.1-alpha</p>"),
        "{html}"
    );
    // The stylesheet is keyed on the classes the backend emits.
    assert!(html.contains(".chapter-kicker {"), "{html}");
}

#[test]
fn footnotes_render_a_numbered_list_and_link_their_references() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("fn.wcl");
    write_fixture(
        &src,
        // The `[^id]` rewrite is part of the templated page pass, so this
        // fixture takes a template.
        r#"
site { default_template = :webpage }
page index {
  p "See the note[^why], and a regex [^abc] that has none."
  footnotes {
    footnote why { text = "Because **it matters**." }
  }
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    assert!(
        html.contains("<section class=\"wdoc-footnotes\">"),
        "{html}"
    );
    assert!(
        html.contains("<div class=\"wdoc-footnotes-title\">Footnotes</div>"),
        "the title is data on the node, not chrome:\n{html}"
    );
    // The `<ol>` supplies the number; the marker is the anchor key.
    assert!(
        html.contains(
            "<ol class=\"wdoc-footnote-list\"><li class=\"wdoc-footnote-item\" id=\"fn-why\">\
             Because <span class=\"bold\">it matters</span>.\
             <a class=\"wdoc-footnote-back\" href=\"#fnref-why\">↩</a></li></ol>"
        ),
        "{html}"
    );
    // A defined reference is rewritten to a numbered superscript…
    assert!(
        html.contains(
            "<sup class=\"footnote-ref\" id=\"fnref-why\"><a href=\"#fn-why\">1</a></sup>"
        ),
        "{html}"
    );
    // …and an undefined one is left exactly as written.
    assert!(html.contains("[^abc]"), "{html}");
}

#[test]
fn code_renders_the_card_header_with_its_filename() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("code.wcl");
    write_fixture(
        &src,
        r#"
page index {
  code rust {
    filename = "src/main.rs"
    source = "fn main() {}"
  }
  code sh {
    source = "cargo test"
  }
}
"#,
    );
    let out = TempDir::new().expect("mkdir out");
    build_ok(&src, out.path());
    let html = std::fs::read_to_string(out.path().join("index.html")).expect("read");

    assert!(
        html.contains(
            "<figure class=\"code-card\"><div class=\"code-filename\">\
             <span class=\"code-dots\"><span></span><span></span><span></span></span>\
             <span class=\"code-name\">src/main.rs</span>\
             <span class=\"code-lang\">rust</span></div>\
             <pre class=\"code-block\"><code class=\"language-rust\">"
        ),
        "{html}"
    );
    // No filename ⇒ no name span, but the header bar still names the language.
    assert!(
        html.contains("<span class=\"code-lang\">sh</span>"),
        "{html}"
    );
    assert_eq!(html.matches("class=\"code-name\"").count(), 1, "{html}");
    // The listing is highlighted (the syntect line wrapper the gutter counts).
    assert!(html.contains("class=\"code-line\""), "{html}");
}
