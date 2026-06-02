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
