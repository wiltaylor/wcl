//! Integration tests for the Markdown target (`wcl wdoc markdown`).

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
        Err(_) => panic!("markdown error"),
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
fn skill_site_is_skipped_and_rejected_when_named() {
    // A `:ai_skill` site alongside a plain markdown site: the markdown target
    // skips the skill site, and naming it with `--site` is an error.
    let body = "site web { default_template = :webpage }\n\
         site sk { default_template = :ai_skill\n  \
         skill { name = \"sk\"  description = \"D.\" }\n}\n\
         page home { sites = [:sk]  start = true\n  h1 \"Skill\"\n}\n\
         page doc { sites = [:web]\n  h1 \"Doc\"\n}\n";
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(&src, body);

    // Build all → only the non-skill page `doc` (no `sites`) is written; the
    // skill site's `home` page is not.
    let out = tmp.path().join("out");
    md_ok(&src, &out, None);
    assert!(out.join("doc.md").is_file(), "non-skill page written");
    assert!(!out.join("home.md").exists(), "skill page skipped");

    // Naming the skill site explicitly is rejected with guidance.
    let out2 = tmp.path().join("out2");
    match markdown(&src, &out2, Some("sk")) {
        Err(BuildError::BadPage(m)) => {
            assert!(m.contains("wcl wdoc skill"), "points at skill: {m}")
        }
        Err(_) => panic!("expected BadPage"),
        Ok(_) => panic!("naming a skill site must error"),
    }
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
fn frontmatter_without_schemaless_is_an_actionable_error() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(
        &src,
        "page p {\n  frontmatter {\n    title = \"Intro\"\n  }\n  h1 \"Intro\"\n}\n",
    );
    let out = tmp.path().join("out");
    match markdown(&src, &out, None) {
        Err(BuildError::BadPage(msg)) => {
            assert!(
                msg.contains("@schemaless"),
                "guidance mentions the fix: {msg}"
            );
        }
        Ok(n) => panic!("expected a BadPage error, but it succeeded with {n} pages"),
        Err(_) => panic!("expected a BadPage error, got a different BuildError"),
    }
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
fn local_video_is_skipped() {
    let (_t, out) = build("page p {\n  p \"before\"\n  video \"clip.mp4\"\n  p \"after\"\n}\n");
    let md = read(&out, "p.md");
    assert!(!md.contains("clip.mp4"), "local video dropped: {md}");
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
        @document type Wskill { @children(\"concept\") concepts: list<Concept> }\n\
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
