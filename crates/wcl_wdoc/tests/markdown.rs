//! Integration tests for the Markdown target (`wcl wdoc build --type markdown`).

use std::path::Path;

use tempfile::TempDir;
use wcl_wdoc::{BuildError, markdown};

/// Write a wdoc fixture, prepending the `import <wdoc.wcl>` line a real
/// document needs.
fn write_fixture(path: impl AsRef<Path>, body: &str) {
    let composed = format!("import <wdoc.wcl>\n{body}");
    std::fs::write(path, composed).expect("write wdoc fixture");
}

fn md_ok(file: &Path, out: &Path, site: Option<&str>) -> usize {
    match markdown(file, out, site) {
        Ok(n) => n,
        Err(BuildError::Io(e, ctx)) => panic!("markdown io error: {ctx}: {e}"),
        Err(BuildError::Parse(r)) => panic!("markdown parse error: {r:?}"),
        Err(BuildError::Schema(n)) => panic!("markdown schema error: {n} violations"),
        Err(BuildError::BadPage(m)) => panic!("markdown bad-page error: {m}"),
        Err(BuildError::BadLink(m)) => panic!("markdown bad-link error: {m:?}"),
        Err(err) => panic!("markdown error: {}", err.render_plain()),
    }
}

fn build(body: &str) -> (TempDir, std::path::PathBuf) {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(&src, body);
    let out = tmp.path().join("out");
    let n = md_ok(&src, &out, None);
    assert!(n >= 1, "at least one page written");
    // Keep `tmp` alive (it owns both src and out).
    (tmp, out)
}

fn read(out: &Path, rel: &str) -> String {
    std::fs::read_to_string(out.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

#[test]
fn writes_one_md_per_page() {
    let (_t, out) = build("page one {\n  h1 \"One\"\n}\npage two {\n  p \"Two.\"\n}\n");
    assert!(out.join("one.md").is_file());
    assert!(out.join("two.md").is_file());
}

#[test]
fn named_slot_fill_children_render_in_place() {
    // A named slot fills an HTML template; Markdown has no template, so
    // the fill's children render where the block sits
    // instead of being dropped.
    let (_t, out) = build(
        "template article {\n  slot content: content\n  slot hero: content?\n  slot footer: content?\n  render = fn(c: TemplateCtx) -> list<Html> slot(c, :content)\n}\nsite { default_template = :article }\npage index {\n  hero {\n    h1 \"Welcome\"\n  }\n  p \"Body.\"\n  footer {\n    p \"Footer note.\"\n  }\n}\n",
    );
    let md = read(&out, "index.md");
    assert!(
        md.contains("# Welcome"),
        "hero slot heading survives:\n{md}"
    );
    assert!(md.contains("Body."), "default content survives:\n{md}");
    assert!(
        md.contains("Footer note."),
        "footer slot content survives:\n{md}"
    );
    let hero = md.find("# Welcome").expect("hero present");
    let body = md.find("Body.").expect("body present");
    let foot = md.find("Footer note.").expect("footer present");
    assert!(
        hero < body && body < foot,
        "slot content renders in document order:\n{md}"
    );
}

#[test]
fn nested_component_content_renders_in_markdown() {
    let (_t, out) = build(
        "wdoc_component inner {\n  wdoc_body {\n    h2 \"Inner frame\"\n    wdoc_content\n  }\n}\n\
         wdoc_component outer {\n  wdoc_body {\n    inner {\n      wdoc_content\n    }\n  }\n}\n\
         page p {\n  outer {\n    p \"Nested **payload**.\"\n  }\n}\n",
    );
    let md = read(&out, "p.md");
    assert!(
        md.contains("## Inner frame"),
        "inner component rendered:\n{md}"
    );
    assert!(
        md.contains("Nested **payload**."),
        "the outer content slot was forwarded through the inner component:\n{md}"
    );
}

#[test]
fn headings_and_prose_render_as_markdown() {
    let (_t, out) = build(
        "page p {\n  h1 \"Title\"\n  h2 \"Sub\"\n  p \"Body with **bold**, _it_ and `code`.\"\n}\n",
    );
    let md = read(&out, "p.md");
    assert!(md.contains("# Title"), "h1 → #: {md}");
    assert!(md.contains("## Sub"), "h2 → ##");
    assert!(md.contains("**bold**"), "bold preserved");
    assert!(md.contains("_it_"), "italic preserved");
    assert!(md.contains("`code`"), "code span preserved");
}

#[test]
fn code_block_keeps_language_and_raw_source() {
    let (_t, out) = build("page p {\n  code rust {\n    source = \"let x = 1;\"\n  }\n}\n");
    let md = read(&out, "p.md");
    assert!(md.contains("```rust"), "fence carries the language: {md}");
    assert!(
        md.contains("let x = 1;"),
        "raw source, not highlighted spans"
    );
}

#[test]
fn block_math_is_a_dollar_fence() {
    let (_t, out) = build("page p {\n  math \"E = mc^2\"\n}\n");
    let md = read(&out, "p.md");
    assert!(
        md.contains("$$\nE = mc^2\n$$"),
        "block math → $$ fence: {md}"
    );
}

#[test]
fn diagram_becomes_a_referenced_svg_file() {
    let (_t, out) = build(
        "page p {\n  diagram {\n    width = 200 height = 100\n    rect a { x = 10 y = 10 width = 80 height = 40 }\n  }\n}\n",
    );
    let md = read(&out, "p.md");
    // The page references an SVG it generated, and that file exists.
    let marker = "](_wdoc/";
    assert!(
        md.contains("![") && md.contains(marker),
        "image ref present: {md}"
    );
    let svg = out.join("_wdoc/p-diagram-1.svg");
    assert!(svg.is_file(), "diagram SVG written");
    let svg_text = std::fs::read_to_string(&svg).expect("read svg");
    assert!(svg_text.contains("<svg"), "is an SVG");
    assert!(svg_text.contains("xmlns="), "carries the SVG namespace");
    assert!(
        !svg_text.contains("data-pan-zoom") && !svg_text.contains("wdoc-diagram-viewport"),
        "static SVG has no interactive chrome"
    );
}

#[test]
fn wireframe_in_diagram_becomes_a_referenced_svg() {
    // A wireframe widget is a diagram shape, so it rides the diagram's
    // static-SVG path: the page references the generated SVG and the widget
    // text is in it.
    let (_t, out) = build(
        "page p {\n  diagram {\n    width = 280 height = 110\n    wf_window \"Settings\" {\n      wf_label \"Body text\"\n    }\n  }\n}\n",
    );
    let md = read(&out, "p.md");
    assert!(
        md.contains("![") && md.contains("](_wdoc/"),
        "image ref present: {md}"
    );
    let svg = out.join("_wdoc/p-diagram-1.svg");
    assert!(svg.is_file(), "diagram SVG written");
    let svg_text = std::fs::read_to_string(&svg).expect("read svg");
    assert!(
        svg_text.contains(">Settings</text>") && svg_text.contains(">Body text</text>"),
        "wireframe widget text not in the diagram SVG: {svg_text}"
    );
}

#[test]
fn frontmatter_block_becomes_yaml_header() {
    let (_t, out) = build(
        "page p {\n  @schemaless frontmatter {\n    title = \"Intro\"\n    tags = [\"overview\", \"api\"]\n    weight = 3\n  }\n  h1 \"Intro\"\n}\n",
    );
    let md = read(&out, "p.md");
    assert!(md.starts_with("---\n"), "leading YAML fence: {md}");
    assert!(md.contains("title: Intro"));
    assert!(md.contains("weight: 3"));
    assert!(
        md.contains("tags:\n  - overview\n  - api"),
        "list serialized: {md}"
    );
    // The fence closes before the first heading.
    let close = md.find("\n---\n").expect("closing fence");
    assert!(md[close..].contains("# Intro"));
}

#[test]
fn frontmatter_string_keys_emit_hyphenated_yaml() {
    // A `@schemaless` frontmatter block may use string-literal keys, so a
    // key that isn't a valid identifier (e.g. `argument-hint`) round-trips
    // verbatim into the YAML header.
    let (_t, out) = build(
        "page p {\n  @schemaless frontmatter {\n    \"argument-hint\" = \"a path\"\n    \"disable-model-invocation\" = false\n  }\n  h1 \"Intro\"\n}\n",
    );
    let md = read(&out, "p.md");
    assert!(md.contains("argument-hint: a path"), "hyphenated key: {md}");
    assert!(
        md.contains("disable-model-invocation: false"),
        "second hyphenated key: {md}"
    );
}

#[test]
fn frontmatter_needs_no_schemaless_marker() {
    // The `Frontmatter` type is itself `@schemaless` (an open, dynamic
    // kind), so a plain `frontmatter { … }` accepts arbitrary keys with
    // no per-instance marker.
    let (_t, out) = build(
        "page p {\n  frontmatter {\n    title = \"Intro\"\n    audience = \"llm\"\n  }\n  h1 \"Intro\"\n}\n",
    );
    let md = read(&out, "p.md");
    assert!(md.starts_with("---\n"), "leading YAML fence: {md}");
    assert!(md.contains("title: Intro"));
    assert!(md.contains("audience: llm"));
}

#[test]
fn internal_links_point_at_md_siblings() {
    let (_t, out) = build(
        "page home {\n  p \"See [the other page](other).\"\n}\npage other {\n  p \"Hi.\"\n}\n",
    );
    let md = read(&out, "home.md");
    assert!(
        md.contains("[the other page](other.md)"),
        "link → .md: {md}"
    );
}

#[test]
fn pipe_table_renders() {
    let (_t, out) = build(
        "page p {\n  table {\n    header = [\"A\", \"B\"]\n    rows = [[\"1\", \"2\"], [\"3\", \"4\"]]\n  }\n}\n",
    );
    let md = read(&out, "p.md");
    assert!(md.contains("| A | B |"), "header row: {md}");
    assert!(md.contains("| --- | --- |"), "separator row");
    assert!(md.contains("| 1 | 2 |"), "body row");
}

#[test]
fn nested_list_indents() {
    let (_t, out) =
        build("page p {\n  list {\n    li \"top\" {\n      li \"nested\"\n    }\n  }\n}\n");
    let md = read(&out, "p.md");
    assert!(md.contains("- top"), "top item: {md}");
    assert!(md.contains("  - nested"), "nested item indented");
}

#[test]
fn local_video_becomes_a_link_to_the_copied_file() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    std::fs::write(tmp.path().join("clip.mp4"), b"not really a video").expect("write clip");
    let src = tmp.path().join("doc.wcl");
    write_fixture(
        &src,
        "page p {\n  p \"before\"\n  video \"clip.mp4\" {\n    title = \"Demo\"\n  }\n  p \"after\"\n}\n",
    );
    let out = tmp.path().join("out");
    md_ok(&src, &out, None);
    let md = read(&out, "p.md");
    assert!(
        md.contains("[Demo](_wdoc/video-clip-"),
        "local video links to the copied asset: {md}"
    );
    let copied = std::fs::read_dir(out.join("_wdoc"))
        .expect("read _wdoc")
        .filter_map(Result::ok)
        .any(|e| e.file_name().to_string_lossy().ends_with(".mp4"));
    assert!(copied, "the video file is copied into _wdoc/");
    assert!(
        md.contains("before") && md.contains("after"),
        "surrounding prose kept"
    );
}

#[test]
fn online_video_leaves_a_link() {
    let (_t, out) = build("page p {\n  video \"https://www.youtube.com/watch?v=aqz-KE-bpKQ\"\n}\n");
    let md = read(&out, "p.md");
    assert!(
        md.contains("](https://www.youtube.com/watch?v=aqz-KE-bpKQ)"),
        "youtube link: {md}"
    );
}

#[test]
fn callout_becomes_github_alert() {
    let (_t, out) = build(
        "page p {\n  callout \"Heads up\" {\n    class = [\"warning\"]\n    body = \"Careful here.\"\n  }\n}\n",
    );
    let md = read(&out, "p.md");
    assert!(md.contains("> [!WARNING]"), "alert keyword: {md}");
    assert!(md.contains("> **Heads up**"), "heading line");
    assert!(md.contains("> Careful here."), "body line");
}

#[test]
fn multi_site_lands_pages_in_subdirectories() {
    let (_t, out) = build(
        "site docs { }\nsite blog { }\n\
         page guide {\n  sites = [:docs]\n  h1 \"Guide\"\n}\n\
         page post {\n  sites = [:blog]\n  h1 \"Post\"\n}\n",
    );
    assert!(out.join("docs/guide.md").is_file(), "docs page under docs/");
    assert!(out.join("blog/post.md").is_file(), "blog page under blog/");
}

#[test]
fn cross_root_children_render_in_a_page() {
    // Issue 13: a `@children` collection under a non-Site `@document` root is
    // readable from a wdoc page expression (`len(concepts)`), rendering the
    // count instead of silently dropping the block.
    let body = "interface Node { id: identifier }\n\
        @block(\"concept\") type Concept extends Node {\n  @inline(0) id: identifier\n  name: utf8\n}\n\
        @document type Topic { @children(\"concept\") concepts: list<Concept> }\n\
        site s { default_template = :book  title = \"R\"  root = true\n  toc { chapter \"C\" { page = home } }\n}\n\
        page home { sites = [:s]  start = true\n  h1 \"Heading\"\n  p $\"count = ${len(concepts)}\"\n}\n\
        concept \"intro\" { name = \"Intro\" }\n\
        concept \"second\" { name = \"Second\" }\n";
    let (_t, out) = build(body);
    let md = read(&out, "home.md");
    assert!(
        md.contains("count = 2"),
        "expected the rendered count, got:\n{md}"
    );
}

#[test]
fn lowerless_block_fails_the_build() {
    // A declared block kind with neither a `lower` nor `@native` would
    // render nothing on every backend. The build refuses the *type*, before
    // a page is rendered — the block says how it is rendered or it doesn't
    // build (the message is asserted in `native.rs`'s own tests).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(
        &src,
        "@block(\"gadget\")\ntype Gadget extends ContentBlock {\n  id: identifier?\n}\n\
         page p {\n  gadget {}\n  p \"kept\"\n}\n",
    );
    let out = tmp.path().join("out");
    match markdown(&src, &out, None) {
        Err(BuildError::Schema(n)) => assert_eq!(n, 1, "one contract violation"),
        Ok(n) => panic!("expected a schema error, but wrote {n} page(s)"),
        Err(_) => panic!("expected BuildError::Schema, got a different error"),
    }
}

#[test]
fn computed_table_eval_error_fails_the_build() {
    // A present `rows` expression that fails to evaluate is an authoring
    // error — it must not silently fall back to pipe-table parsing.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(
        &src,
        "page p {\n  table {\n    rows = no_such_name\n  }\n}\n",
    );
    let out = tmp.path().join("out");
    match markdown(&src, &out, None) {
        Err(BuildError::Eval(r)) => {
            let text = format!("{r:?}");
            assert!(text.contains("no_such_name"), "names the binding: {text}");
        }
        Ok(n) => panic!("expected an eval error, but wrote {n} page(s)"),
        Err(_) => panic!("expected BuildError::Eval, got a different error"),
    }
}

#[test]
fn unresolved_name_in_page_block_errors() {
    // Issue 13: a present expression that fails to evaluate (here an
    // unresolved binding) must surface as a loud diagnostic, not silently
    // drop the block with a success exit.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(
        &src,
        "page home {\n  start = true\n  p $\"count = ${len(nonexistent)}\"\n}\n",
    );
    let out = tmp.path().join("out");
    match markdown(&src, &out, None) {
        Err(BuildError::Eval(_)) => {}
        Ok(n) => panic!("expected an eval error, but wrote {n} page(s)"),
        Err(other) => {
            other.report();
            panic!("expected BuildError::Eval, got a different error (see above)");
        }
    }
}

#[test]
fn partials_are_gathered_by_collect() {
    // Scattered `partial`s are gathered at the `collect` site, in document
    // order, and (default show_here unset) do not render at their source.
    let (_t, out) = build(
        "page p {\n  partial note { p \"First note.\" }\n  p \"Body prose.\"\n  partial note { p \"Second note.\" }\n  collect note\n}\n",
    );
    let md = read(&out, "p.md");
    let body = md.find("Body prose.").expect("body prose present");
    let first = md.find("First note.").expect("first note present");
    let second = md.find("Second note.").expect("second note present");
    assert!(
        body < first && first < second,
        "collected partials must follow the collect site, in order:\n{md}"
    );
    assert_eq!(md.matches("First note.").count(), 1, "{md}");
}

#[test]
fn flowchart_diagram_with_edges_becomes_a_referenced_svg() {
    // Two boxes joined by an edge ride the same static-SVG path as a bare
    // rect diagram: one .svg on disk, referenced from the page as an image.
    let (_t, out) = build(
        "page p {\n  diagram {\n    width = 320 height = 160 layout = :layered\n    process \"Parse\"  { id = parse  width = 100.0 height = 40.0 }\n    process \"Render\" { id = render width = 100.0 height = 40.0 }\n    parse -> render :flow\n  }\n}\n",
    );
    let md = read(&out, "p.md");
    assert!(
        md.contains("](_wdoc/p-diagram-1.svg)"),
        "md references the generated SVG: {md}"
    );
    let svg = read(&out, "_wdoc/p-diagram-1.svg");
    assert!(!svg.trim().is_empty(), "SVG non-empty");
    assert!(svg.trim_start().starts_with("<svg"), "starts with <svg");
    // Box labels render as tspans; the edge is a marker-tipped polyline.
    assert!(
        svg.contains(">Parse</tspan>") && svg.contains(">Render</tspan>"),
        "both box labels drawn: {svg}"
    );
    assert!(
        svg.contains("marker-end=\"url(#wdoc-arrow)\""),
        "edge arrow drawn: {svg}"
    );
}

#[test]
fn terminal_becomes_a_referenced_svg_file() {
    let (_t, out) = build(
        "page p {\n  terminal {\n    cols = 30 rows = 3 title = \"demo\"\n    text = \"hello term\"\n  }\n}\n",
    );
    let md = read(&out, "p.md");
    // Alt text comes from the block's `title`.
    assert!(
        md.contains("![demo](_wdoc/p-terminal-1.svg)"),
        "image ref with title alt: {md}"
    );
    let svg = read(&out, "_wdoc/p-terminal-1.svg");
    assert!(!svg.trim().is_empty(), "SVG non-empty");
    assert!(svg.trim_start().starts_with("<svg"), "starts with <svg");
    assert!(svg.contains("xmlns="), "carries the SVG namespace");
    // Grid text is drawn one glyph per <text> cell; assert on the chrome
    // title (a single run) and the cell group instead.
    assert!(svg.contains(">demo</text>"), "chrome title drawn: {svg}");
    assert!(
        svg.contains("class=\"term-cells\""),
        "cell grid present: {svg}"
    );
}

#[test]
fn diagram_and_terminal_share_one_svg_sequence() {
    // Generated SVG filenames are <page>-<kind>-<n>.svg with a single
    // per-page counter, so a diagram then a terminal land at 1 and 2.
    let (_t, out) = build(
        "page p {\n  diagram {\n    width = 120 height = 60\n    rect a { x = 5 y = 5 width = 40 height = 20 }\n  }\n  terminal {\n    cols = 20 rows = 2\n    text = \"ok\"\n  }\n}\n",
    );
    let md = read(&out, "p.md");
    for rel in ["_wdoc/p-diagram-1.svg", "_wdoc/p-terminal-2.svg"] {
        assert!(md.contains(&format!("]({rel})")), "{rel} referenced: {md}");
        let svg = read(&out, rel);
        assert!(
            svg.trim_start().starts_with("<svg"),
            "{rel} starts with <svg"
        );
    }
}

#[test]
fn inline_math_stays_latex_in_prose() {
    // Inline equations are kept textual (`$…$` / `$$…$$`), matching the
    // block-equation policy — never rasterized to SVG on the Markdown path.
    let (_t, out) =
        build("page p {\n  p \"Energy: $E = mc^2$ and display $$\\\\int_0^1 x dx$$ inline.\"\n}\n");
    let md = read(&out, "p.md");
    assert!(md.contains("$E = mc^2$"), "text-style math kept: {md}");
    assert!(
        md.contains("$$\\int_0^1 x dx$$"),
        "display-style math kept: {md}"
    );
    assert!(!out.join("_wdoc").exists(), "no SVG asset written for math");
}

#[test]
fn callout_error_class_maps_to_caution_alert() {
    let (_t, out) = build(
        "page p {\n  callout \"Stop\" {\n    class = [\"error\"]\n    body = \"It broke.\"\n  }\n}\n",
    );
    let md = read(&out, "p.md");
    assert!(md.contains("> [!CAUTION]"), "error → CAUTION: {md}");
    assert!(md.contains("> **Stop**"), "heading line");
    assert!(md.contains("> It broke."), "body line");
}

#[test]
fn only_html_block_is_excluded_from_markdown() {
    // `@only(backends=[:html])` scopes a block out of the :markdown backend,
    // while `@only(backends=[:markdown])` keeps it in.
    let (_t, out) = build(
        "page p {\n  p \"always\"\n  @only(backends=[:html]) p \"website only\"\n  @only(backends=[:markdown]) p \"markdown only\"\n}\n",
    );
    let md = read(&out, "p.md");
    assert!(md.contains("always"), "undecorated block kept: {md}");
    assert!(
        !md.contains("website only"),
        "@only(:html) block excluded: {md}"
    );
    assert!(
        md.contains("markdown only"),
        "@only(:markdown) block kept: {md}"
    );
}

#[test]
fn except_markdown_block_is_excluded() {
    let (_t, out) = build(
        "page p {\n  @except(backends=[:markdown]) p \"not here\"\n  @except(backends=[:pdf]) p \"still here\"\n}\n",
    );
    let md = read(&out, "p.md");
    assert!(
        !md.contains("not here"),
        "@except(:markdown) block excluded: {md}"
    );
    assert!(md.contains("still here"), "@except(:pdf) block kept: {md}");
}

#[test]
fn only_html_diagram_writes_no_svg() {
    // Visibility is checked before dispatch, so a scoped-out diagram leaves
    // no asset behind either.
    let (_t, out) = build(
        "page p {\n  p \"prose\"\n  @only(backends=[:html]) diagram {\n    width = 120 height = 60\n    rect a { x = 5 y = 5 width = 40 height = 20 }\n  }\n}\n",
    );
    let md = read(&out, "p.md");
    assert!(!md.contains("!["), "no image ref: {md}");
    assert!(!out.join("_wdoc").exists(), "no SVG asset dir created");
}

#[test]
fn collect_gathers_partials_from_imported_files() {
    // Cross-file scatter: a top-level `partial` in an eagerly-imported file is
    // gathered by a `collect` in the main document.
    let tmp = TempDir::new().expect("mkdir tempdir");
    std::fs::write(
        tmp.path().join("extra.wcl"),
        "import <wdoc.wcl>\npartial gloss { p \"Imported term.\" }\n",
    )
    .expect("write extra");
    let main = tmp.path().join("main.wcl");
    std::fs::write(
        &main,
        "import <wdoc.wcl>\nimport \"./extra.wcl\"\npage p { collect gloss }\n",
    )
    .expect("write main");
    let out = tmp.path().join("out");
    md_ok(&main, &out, None);
    let md = read(&out, "p.md");
    assert!(md.contains("Imported term."), "{md}");
}

#[test]
fn sequence_diagram_writes_standalone_svg() {
    let (_t, out) = build(
        "page seq {\n  sequence_diagram {\n    participant \"a\" { }\n    participant \"b\" { }\n    message \"m1\" { from = \"a\"  to = \"b\"  text = \"hi\" }\n  }\n}\n",
    );
    let md = read(&out, "seq.md");
    assert!(
        md.contains("![sequence diagram](_wdoc/seq-drawing-1.svg)"),
        "image ref:\n{md}"
    );
    let svg = read(&out, "_wdoc/seq-drawing-1.svg");
    assert!(svg.starts_with("<svg"), "standalone svg:\n{svg}");
    assert!(svg.contains("wdoc-lifeline"), "content lowered:\n{svg}");
}

#[test]
fn state_diagram_writes_standalone_svg() {
    let (_t, out) = build(
        "page sc {\n  state_diagram {\n    state \"a\" { initial = true }\n    state \"b\" { final = true }\n    transition \"t1\" { from = \"a\"  to = \"b\"  trigger = \"go\" }\n  }\n}\n",
    );
    let md = read(&out, "sc.md");
    assert!(
        md.contains("![state diagram](_wdoc/sc-drawing-1.svg)"),
        "image ref:\n{md}"
    );
    let svg = read(&out, "_wdoc/sc-drawing-1.svg");
    assert!(svg.contains("wdoc-state"), "content lowered:\n{svg}");
}

#[test]
fn projects_body_fragment_in_markdown() {
    // The `project` block routes through the shared `walk_structural`
    // dispatch, so the Markdown target renders a projected body the same way
    // the HTML target does — content attached to a data record, rendered by
    // reference inside a repeater.
    let (_t, out) = build(
        "@document\n\
         type Infra { @children(\"server\") servers: list<Server> }\n\
         @block(\"server\")\n\
         type Server {\n  @inline(0) name: identifier\n  region: utf8?\n  @child(\"body\") overview: WdocAddressableBody?\n}\n\
         server web01 { region = \"us-east\"\n  body { p $\"Frontend in ${region}.\" }\n}\n\
         server web02 { region = \"eu-west\"\n  body { p $\"Frontend in ${region}.\" }\n}\n\
         page all {\n  wdoc_repeater { each = servers  as = :s\n    project { from = s.overview }\n  }\n}\n",
    );
    let md = read(&out, "all.md");
    assert!(
        md.contains("Frontend in us-east.") && md.contains("Frontend in eu-west."),
        "both bodies projected in markdown:\n{md}"
    );
}

#[test]
fn projects_numeric_labelled_nested_body_in_markdown() {
    // The numeric-label addressing fix lives in wcl_lang resolution (shared by
    // all backends), so a body on a numerically-labelled nested record renders
    // in Markdown the same as HTML.
    let (_t, out) = build(
        "@document\n\
         type Doc { @children(\"tut\") tuts: list<Tut> }\n\
         @block(\"tut\")\n\
         type Tut { @inline(0) id: identifier  @children(\"tstep\") steps: list<TStep> }\n\
         @block(\"tstep\")\n\
         type TStep { @inline(0) n: u32  @child(\"body\") body: WdocAddressableBody? }\n\
         tut t1 {\n  tstep 1 { body { p \"STEP ONE body\" } }\n}\n\
         page pg {\n  wdoc_repeater { each = tuts as = :t\n    wdoc_repeater { each = t.steps as = :st\n      project { from = st.body }\n    }\n  }\n}\n",
    );
    let md = read(&out, "pg.md");
    assert!(
        md.contains("STEP ONE body"),
        "nested numeric-label body in markdown:\n{md}"
    );
}

#[test]
fn a_block_the_target_does_not_cover_fails_the_build() {
    // `markdown_source` previews a page's generated Markdown by tapping the
    // emitter from inside the HTML build: `@native(backends = [:html])`. On
    // any other target the build refuses rather than rendering nothing — the
    // failure names the kind, the target, and the waiver.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(
        &src,
        "page p {\n  markdown_source {\n    p \"previewed\"\n  }\n}\n",
    );
    let out = tmp.path().join("out");
    match markdown(&src, &out, None) {
        Err(BuildError::Eval(r)) => {
            let text = format!("{r:?}");
            assert!(
                text.contains("markdown_source")
                    && text.contains(":markdown")
                    && text.contains("@except"),
                "names the kind, the target and the waiver: {text}"
            );
        }
        Ok(n) => panic!("expected an uncovered-target error, but wrote {n} page(s)"),
        Err(e) => panic!("expected BuildError::Eval, got {e}", e = e.render_plain()),
    }
}

#[test]
fn an_uncovered_block_is_waived_per_instance_by_the_backends_axis() {
    // The counterpart: capability says `markdown_source` can't render here,
    // author intent says it shouldn't, and the build proceeds — with the rest
    // of the page intact.
    let (_t, out) = build(
        "page p {\n  @except(backends = [:markdown])\n  markdown_source {\n    p \"previewed\"\n  }\n  p \"kept\"\n}\n",
    );
    let md = read(&out, "p.md");
    assert!(md.contains("kept"), "the rest of the page renders:\n{md}");
    assert!(!md.contains("previewed"), "the waived block is gone:\n{md}");
}

#[test]
fn column_children_stack_in_place() {
    // Markdown has no side-by-side layout, so a `column` degrades to its
    // children in source order — the layout is lost, the content is not.
    let (_t, out) = build(
        "page p {\n  column { widths = [50.0, 50.0]\n    p \"left side\"\n    p \"right side\"\n  }\n}\n",
    );
    let md = read(&out, "p.md");
    let left = md.find("left side").expect("left child rendered");
    let right = md.find("right side").expect("right child rendered");
    assert!(left < right, "children keep source order:\n{md}");
}

// ── The semantic content IR ───────────────────────────────────────

#[test]
fn a_user_block_lowering_to_the_content_ir_renders_in_markdown() {
    // The extension mechanism used to be HTML-only: `lower_recurse` lived in
    // the HTML renderer alone, so a custom block whose lowering wasn't one of
    // Markdown's hand-listed kinds rendered in the book and nowhere else.
    // Lowering to the content IR is read by every backend from one
    // declaration.
    let (_t, out) = build(
        "@block(\"gadget\")\n\
         type Gadget extends ContentBlock {\n  \
           @inline(0) name: utf8\n  id: identifier?\n  \
           lower = fn(g: Gadget) -> list<Content> [\n    \
             Content::Heading { level: 3, text: g.name },\n    \
             Content::Paragraph { text: \"A **gadget**.\" },\n  ]\n\
         }\n\
         page p {\n  gadget \"Widget\"\n}\n",
    );
    let md = read(&out, "p.md");
    assert!(md.contains("### Widget"), "heading level from the IR: {md}");
    assert!(md.contains("A **gadget**."), "paragraph prose: {md}");
}

#[test]
fn a_lowering_returning_another_custom_variant_recurses_in_markdown() {
    // `outer` lowers to a custom `Inner` variant, which lowers again to
    // content. Before the shared seam this chain resolved only in HTML.
    let (_t, out) = build(
        "union Step { Inner { text: utf8 } }\n\
         @block(\"inner\")\n\
         type Inner extends ContentBlock {\n  \
           text: utf8\n  id: identifier?\n  \
           lower = fn(i: Inner) -> list<Content> [Content::Paragraph { text: i.text }]\n\
         }\n\
         @block(\"outer\")\n\
         type Outer extends ContentBlock {\n  \
           @inline(0) text: utf8\n  id: identifier?\n  \
           lower = fn(o: Outer) -> list<Step> [Step::Inner { text: o.text }]\n\
         }\n\
         page p {\n  outer \"through the chain\"\n}\n",
    );
    let md = read(&out, "p.md");
    assert!(md.contains("through the chain"), "chain resolved: {md}");
}

#[test]
fn a_callout_body_is_content_not_a_string() {
    // The body is a `list<Content>`, so richer body content quotes into the
    // alert block instead of being flattened into one paragraph string.
    let (_t, out) = build(
        "@block(\"boxed\")\n\
         type Boxed extends ContentBlock {\n  \
           id: identifier?\n  \
           lower = fn(b: Boxed) -> list<Content> [\n    \
             Content::Callout {\n      \
               kind: :tip, heading: \"Try it\",\n      \
               body: [\n        \
                 Content::Paragraph { text: \"Run this:\" },\n        \
                 Content::Code { source: \"cargo test\", language: \"sh\" },\n      ],\n    },\n  ]\n\
         }\n\
         page p {\n  boxed { }\n}\n",
    );
    let md = read(&out, "p.md");
    assert!(md.contains("> [!TIP]"), "kind → alert keyword: {md}");
    assert!(md.contains("> **Try it**"), "heading: {md}");
    assert!(
        md.contains("> ```sh"),
        "nested code fence stays quoted: {md}"
    );
    assert!(md.contains("> cargo test"), "code body quoted: {md}");
}

#[test]
fn a_malformed_content_node_fails_the_build() {
    // A `Content` value missing a required field is an authoring error, not
    // a node that quietly renders nothing.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(
        &src,
        "@block(\"broken\")\n\
         type Broken extends ContentBlock {\n  \
           id: identifier?\n  \
           lower = fn(b: Broken) -> list<Content> [Content::Heading { level: 900 }]\n\
         }\n\
         page p {\n  broken { }\n}\n",
    );
    let out = tmp.path().join("out");
    assert!(
        markdown(&src, &out, None).is_err(),
        "a malformed content node must fail the build"
    );
}

// ── The five markup-using blocks, routed through the content IR ────
//
// Each of these carried information that existed only in HTML while the
// block built its own markup: this backend descended the lowered
// `<header>` / `<section>` / `<figure>` and emitted whatever prose fell
// out of it. They lower to content nodes now, so the fields reach here.

#[test]
fn chapter_header_metadata_survives_in_markdown() {
    let (_t, out) = build(
        "page one {\n  chapter_header \"Getting started\" {\n    \
           kicker = \"Chapter 1\"\n    reading_time = \"9 min read\"\n    \
           updated = \"2026-08-02\"\n    version = \"wdoc 0.24.1-alpha\"\n  }\n}\n",
    );
    let md = read(&out, "one.md");
    assert!(md.contains("# Getting started"), "title:\n{md}");
    assert!(md.contains("_Chapter 1_"), "kicker:\n{md}");
    assert!(
        md.contains("_9 min read · 2026-08-02 · wdoc 0.24.1-alpha_"),
        "the meta line, joined by the one shared separator:\n{md}"
    );
}

#[test]
fn footnotes_emit_gfm_definitions_under_their_title() {
    let (_t, out) = build(
        "page one {\n  p \"See the note[^why].\"\n  footnotes {\n    \
           footnote why { text = \"Because **it matters**.\" }\n    \
           footnote later { text = \"A second note.\" }\n  }\n}\n",
    );
    let md = read(&out, "one.md");
    // The section title is data now, not HTML chrome.
    assert!(md.contains("## Footnotes"), "title:\n{md}");
    // The marker is the definition's id — a GFM footnote label, so a
    // reader that resolves references has one to resolve. (The reference
    // in prose stays escaped: the inline engine can't tell `[^why]` from
    // a regex character class without the page's definitions, which is
    // the same reason the HTML pass only rewrites defined ones.)
    assert!(md.contains("See the note\\[^why\\]."), "reference:\n{md}");
    assert!(
        md.contains("[^why]: Because **it matters**."),
        "definition, inline patterns applied:\n{md}"
    );
    assert!(md.contains("[^later]: A second note."), "second:\n{md}");
}

#[test]
fn code_filename_survives_in_markdown() {
    let (_t, out) = build(
        "page one {\n  code rust {\n    filename = \"src/main.rs\"\n    \
           source = \"fn main() {}\"\n  }\n}\n",
    );
    let md = read(&out, "one.md");
    assert!(
        md.contains("`src/main.rs`"),
        "filename names the listing:\n{md}"
    );
    assert!(md.contains("```rust\nfn main() {}\n```"), "fence:\n{md}");
}

#[test]
fn heading_levels_come_from_the_node_not_a_class() {
    let (_t, out) =
        build("page one {\n  h1 \"One\"\n  h2 \"Two\"\n  h3 \"Three\"\n  h6 \"Six\"\n}\n");
    let md = read(&out, "one.md");
    for expect in ["# One", "## Two", "### Three", "###### Six"] {
        assert!(md.contains(expect), "{expect} missing:\n{md}");
    }
}

#[test]
fn a_text_block_runs_its_spans_together_into_one_paragraph() {
    let (_t, out) = build(
        "page one {\n  text {\n    span \"Plain, then \"\n    \
           span \"**bold**\"\n    span \", then plain.\"\n  }\n}\n",
    );
    let md = read(&out, "one.md");
    assert!(
        md.contains("Plain, then **bold**, then plain."),
        "one paragraph:\n{md}"
    );
}
