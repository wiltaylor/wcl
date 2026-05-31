use std::path::{Path, PathBuf};

use tempfile::TempDir;
use wcl_wdoc::{PageSize, PdfError, pdf};

/// Write a wdoc fixture, prepending the `import <wdoc.wcl>` line a real
/// document needs.
fn write_fixture(path: impl AsRef<Path>, body: &str) {
    let composed = format!("import <wdoc.wcl>\n{body}");
    std::fs::write(path, composed).expect("write wdoc fixture");
}

fn pdf_ok(file: &Path, out: &Path, size: PageSize) -> usize {
    match pdf(file, out, None, size) {
        Ok(n) => n,
        Err(PdfError::Io(e, ctx)) => panic!("pdf io error: {ctx}: {e}"),
        Err(PdfError::Parse(r)) => panic!("pdf parse error: {r:?}"),
        Err(PdfError::Schema(n)) => panic!("pdf schema error: {n} violations"),
        Err(PdfError::BadDoc(m)) => panic!("pdf bad-doc error: {m}"),
        Err(PdfError::Render(m)) => panic!("pdf render error: {m}"),
    }
}

#[test]
fn renders_prose_to_a_valid_pdf() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(
        &src,
        "page intro {\n  h1 \"Title\"\n  p \"Some body text that is long enough to occupy a line.\"\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);

    let bytes = std::fs::read(out.path().join("doc.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"), "output is a PDF");
    assert!(bytes.len() > 1000, "non-trivial pdf produced");
    // The serif body font should be embedded (subset).
    let blob = String::from_utf8_lossy(&bytes);
    assert!(blob.contains("NotoSerif"), "body font embedded");
}

#[test]
fn each_page_block_starts_a_new_sheet() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("two.wcl");
    write_fixture(
        &src,
        "page one {\n  p \"First.\"\n}\npage two {\n  p \"Second.\"\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    pdf_ok(&src, out.path(), PageSize::Letter);

    let bytes = std::fs::read(out.path().join("two.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"), "output is a PDF");
    // Two `page` blocks → two sheets; krilla records the count in the (plain)
    // page-tree node.
    let blob = String::from_utf8_lossy(&bytes);
    assert!(
        blob.contains("/Count 2"),
        "expected a 2-page document, page tree: {:?}",
        blob.matches("/Count").count()
    );
}

#[test]
fn inline_styling_strips_markers_and_embeds_faces() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("styled.wcl");
    write_fixture(
        &src,
        "page p {\n  p \"A **bold** and _italic_ and `code` run, plus **bold _and italic_**.\"\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    pdf_ok(&src, out.path(), PageSize::A4);

    let bytes = std::fs::read(out.path().join("styled.pdf")).expect("read pdf");
    let blob = String::from_utf8_lossy(&bytes);
    // The inline engine applied the emphasis classes: bold, italic, the
    // composed bold-italic, and the mono code face are each embedded. (Had the
    // markers been rendered literally, only the regular face would appear.)
    for face in [
        "NotoSerif-Bold",
        "NotoSerif-Italic",
        "NotoSerif-BoldItalic",
        "NotoSansMono",
    ] {
        assert!(blob.contains(face), "expected {face} embedded");
    }
}

#[test]
fn external_links_become_annotations() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("links.wcl");
    write_fixture(
        &src,
        "page home {\n  p \"See [the site](https://example.com/docs) for details.\"\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    pdf_ok(&src, out.path(), PageSize::A4);

    let bytes = std::fs::read(out.path().join("links.pdf")).expect("read pdf");
    let blob = String::from_utf8_lossy(&bytes);
    assert!(blob.contains("/Subtype/Link"), "a link annotation exists");
    assert!(
        blob.contains("https://example.com/docs"),
        "the URI action carries the href"
    );
}

#[test]
fn diagram_card_body_renders_natively() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("card.wcl");
    write_fixture(
        &src,
        "page p {\n  diagram d {\n    width = 300\n    height = 160\n    card {\n      x = 10.0  y = 10.0  width = 280.0  height = 140.0\n      title = \"Notes\"\n      p \"Body with **bold** words.\"\n    }\n  }\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    pdf_ok(&src, out.path(), PageSize::A4);

    let bytes = std::fs::read(out.path().join("card.pdf")).expect("read pdf");
    let blob = String::from_utf8_lossy(&bytes);
    // The card's bold title + `**bold**` body are laid out as real wdoc blocks,
    // so the serif bold face is embedded — the old flat-text card path (SVG
    // <text>) never produced native serif glyphs.
    assert!(
        blob.contains("NotoSerif-Bold"),
        "card body rendered natively (serif bold embedded)"
    );
}

#[test]
fn embeds_diagrams_and_block_math_as_vectors() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("vis.wcl");
    write_fixture(
        &src,
        "page v {\n  diagram d {\n    rect a { x = 0 y = 0 width = 80 height = 40 }\n    circle b { cx = 160 cy = 20 r = 20 }\n  }\n  math \"E = mc^2\" {}\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    pdf_ok(&src, out.path(), PageSize::A4);

    let bytes = std::fs::read(out.path().join("vis.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"));
    // krilla-svg writes vector paths into a form XObject; the diagram + the
    // RaTeX equation both produce drawing content well beyond a text-only page.
    assert!(
        bytes.len() > 4000,
        "expected substantial vector content, got {} bytes",
        bytes.len()
    );
}

#[test]
fn renders_code_lists_and_tables() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("rich.wcl");
    write_fixture(
        &src,
        "page r {\n  code rust {\n    source = \"fn main() { let x = 1; }\"\n  }\n  list {\n    li \"alpha\"\n    li \"beta\" {\n      li \"nested\"\n    }\n  }\n  table {\n    rows:\n      | \"Name\" | \"Score\" |\n      | \"Ann\"  | 9       |\n      | \"Bob\"  | 7       |\n  }\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    pdf_ok(&src, out.path(), PageSize::A4);

    let bytes = std::fs::read(out.path().join("rich.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"));
    // Code uses the mono face; the bold table header + list markers use sans /
    // serif. All three block kinds drawing means several faces are embedded.
    let blob = String::from_utf8_lossy(&bytes);
    assert!(blob.contains("NotoSansMono"), "code block embedded mono");
    assert!(bytes.len() > 5000, "rich content produced");
}

#[test]
fn renders_callouts() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("call.wcl");
    write_fixture(
        &src,
        "page c {\n  callout \"Heads up\" { class = [\"warning\"] body = \"Mind the **gap**.\" }\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    pdf_ok(&src, out.path(), PageSize::A4);
    let bytes = std::fs::read(out.path().join("call.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"));
    // The bold heading + bold body emphasis embed the serif bold face.
    assert!(String::from_utf8_lossy(&bytes).contains("NotoSerif-Bold"));
}

#[test]
fn inline_math_embeds_as_svg() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("im.wcl");
    write_fixture(
        &src,
        "page m {\n  text {\n    span \"Euler: $e^{i\\\\pi} + 1 = 0$ inline.\"\n  }\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    pdf_ok(&src, out.path(), PageSize::A4);
    let bytes = std::fs::read(out.path().join("im.pdf")).expect("read pdf");
    // Inline math overlays a vector SVG (a Form XObject), so the file is larger
    // than the same prose without it.
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(bytes.len() > 3000, "inline math drew vector content");
}

#[test]
fn one_pdf_per_site_in_toc_order() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("multi.wcl");
    write_fixture(
        &src,
        "site docs {\n  title = \"The Manual\"\n  toc {\n    chapter \"Start\" { page = intro }\n    chapter \"Next\" { page = usage }\n  }\n}\nsite blog {}\npage intro { sites = [:docs]\n  h1 \"Intro\"\n}\npage usage { sites = [:docs]\n  h1 \"Usage\"\n}\npage post { sites = [:blog]\n  h1 \"Post\"\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 2);
    assert!(out.path().join("docs.pdf").exists(), "docs site pdf");
    assert!(out.path().join("blog.pdf").exists(), "blog site pdf");

    // The docs site has an explicit title (a cover page) and a `toc` (a printed
    // Contents page + reader outline), so: cover + contents + two content pages.
    let bytes = std::fs::read(out.path().join("docs.pdf")).expect("read pdf");
    let blob = String::from_utf8_lossy(&bytes);
    assert!(
        blob.contains("/Count 4"),
        "cover + contents + two content pages"
    );
    assert!(
        blob.contains("/Outlines"),
        "the toc became a reader outline"
    );

    // `--site` renders just one.
    let one = TempDir::new().expect("mkdir out");
    match pdf(&src, one.path(), Some("blog"), PageSize::A4) {
        Ok(n) => assert_eq!(n, 1),
        Err(_) => panic!("pdf --site blog failed"),
    }
    assert!(one.path().join("blog.pdf").exists());
    assert!(!one.path().join("docs.pdf").exists());
}

#[test]
fn embeds_a_raster_image() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let asset =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/wdoc/assets/pixel-coin.png");
    std::fs::copy(&asset, tmp.path().join("coin.png")).expect("copy asset png");
    let src = tmp.path().join("img.wcl");
    write_fixture(
        &src,
        "page i {\n  image \"coin.png\" { width = 120.0 }\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    pdf_ok(&src, out.path(), PageSize::A4);
    let bytes = std::fs::read(out.path().join("img.pdf")).expect("read pdf");
    assert!(
        String::from_utf8_lossy(&bytes).contains("/Subtype /Image")
            || String::from_utf8_lossy(&bytes).contains("/Subtype/Image"),
        "an image XObject was embedded"
    );
}

#[test]
fn internal_links_become_destinations() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("xref.wcl");
    write_fixture(
        &src,
        "page intro {\n  p \"Jump to [the next page](usage).\"\n}\npage usage {\n  h1 \"Usage\"\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    pdf_ok(&src, out.path(), PageSize::A4);
    let bytes = std::fs::read(out.path().join("xref.pdf")).expect("read pdf");
    // The internal `[text](usage)` link becomes a GoTo destination annotation.
    assert!(
        String::from_utf8_lossy(&bytes).contains("/Dest"),
        "internal link destination present"
    );
}

#[test]
fn errors_when_no_pages() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("empty.wcl");
    write_fixture(&src, "// nothing here\n");
    let out = TempDir::new().expect("mkdir out");
    assert!(matches!(
        pdf(&src, out.path(), None, PageSize::A4),
        Err(PdfError::BadDoc(_))
    ));
}

#[test]
fn renders_wireframe_window_as_svg() {
    // A window with a mix of controls + nested layout containers renders to
    // one embedded SVG; the dropdown's chevron records an icon, which gets
    // spliced into the PDF (so the build must succeed and produce vector
    // content well past a text-only page).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("wf.wcl");
    write_fixture(
        &src,
        "page w {\n  wf_window \"Sign in\" {\n    wf_input \"you@example.com\"\n    wf_dropdown \"Personal\"\n    wf_row {\n      wf_checkbox \"Remember\" { checked = true }\n      wf_radio \"Annual\"\n    }\n    wf_toggle \"Notifications\" { on = true }\n    wf_grid {\n      columns = 2\n      wf_button \"Cancel\"\n      wf_button \"OK\" { icon = \"lucide.check\" }\n    }\n  }\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);
    let bytes = std::fs::read(out.path().join("wf.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(
        bytes.len() > 4000,
        "expected substantial wireframe vector content, got {} bytes",
        bytes.len()
    );
}

#[test]
fn renders_wireframe_with_ui_theme_override() {
    // A per-element `theme`/`mode` override resolves + bakes in the PDF path
    // too (the wireframe SVG embeds with concrete theme colours).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("uiwf.wcl");
    write_fixture(
        &src,
        "page w {\n  wf_window \"App\" { theme = :gruvbox  mode = :light\n    wf_button \"OK\"\n    wf_toggle \"On\" { on = true }\n  }\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);
    let bytes = std::fs::read(out.path().join("uiwf.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(
        bytes.len() > 2000,
        "themed wireframe produced vector content"
    );
}

#[test]
fn renders_bare_wireframe_widget() {
    // A top-level bare widget (no container) is a valid page child and
    // renders on its own.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("bare.wcl");
    write_fixture(&src, "page b {\n  wf_button \"Click me\"\n}\n");
    let out = TempDir::new().expect("mkdir out");
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);
    let bytes = std::fs::read(out.path().join("bare.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"));
}

#[test]
fn terminal_pdf_is_self_contained() {
    // Regression guard for the baked terminal snapshot: a terminal embeds
    // its own window background + colours, so it builds to a valid PDF with
    // no reliance on a `<div>` / injected CSS.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("term.wcl");
    write_fixture(
        &src,
        "page t {\n  terminal {\n    title = \"shell\"\n    text = \"$ echo hello\\nhello\"\n  }\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);
    let bytes = std::fs::read(out.path().join("term.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(bytes.len() > 2000, "terminal produced vector content");
}

#[test]
fn book_toc_adds_outline_and_contents_page() {
    // A `book` site with a `toc` gets a reader-sidebar outline (krilla
    // `/Outlines`) and a printed "Contents" page, both built from the toc.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("book.wcl");
    write_fixture(
        &src,
        concat!(
            "site handbook {\n",
            "  title = \"Handbook\"\n",
            "  default_template = :book\n",
            "  toc {\n",
            "    chapter \"Introduction\" { page = intro }\n",
            "    chapter \"Guide\" {\n",
            "      chapter \"Getting Started\" { page = start }\n",
            "    }\n",
            "  }\n",
            "}\n",
            "page intro {\n  sites = [:handbook]\n  h1 \"Introduction\"\n  p \"Welcome.\"\n}\n",
            "page start {\n  sites = [:handbook]\n  h1 \"Getting Started\"\n  p \"Begin.\"\n}\n",
        ),
    );
    let out = TempDir::new().expect("mkdir out");
    pdf_ok(&src, out.path(), PageSize::A4);

    let bytes = std::fs::read(out.path().join("handbook.pdf")).expect("read pdf");
    let blob = String::from_utf8_lossy(&bytes);
    // The reader outline is present and names the chapters (titles are written
    // uncompressed in the outline item dictionaries).
    assert!(blob.contains("/Outlines"), "pdf carries an outline");
    assert!(
        blob.contains("Introduction"),
        "outline names the first chapter"
    );
    assert!(
        blob.contains("Getting Started"),
        "outline names the nested chapter"
    );
    // Cover + Contents + two content pages.
    assert!(
        blob.contains("/Count 4"),
        "cover + contents + two content sheets"
    );
}

#[test]
fn no_toc_means_no_outline() {
    // A site without a `toc` is unchanged: no outline, no Contents page.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("plain.wcl");
    write_fixture(
        &src,
        concat!(
            "site plain {\n  default_template = :book\n}\n",
            "page only {\n  sites = [:plain]\n  h1 \"Only\"\n  p \"Body.\"\n}\n",
        ),
    );
    let out = TempDir::new().expect("mkdir out");
    pdf_ok(&src, out.path(), PageSize::A4);

    let bytes = std::fs::read(out.path().join("plain.pdf")).expect("read pdf");
    let blob = String::from_utf8_lossy(&bytes);
    assert!(!blob.contains("/Outlines"), "no toc ⇒ no outline");
    assert!(blob.contains("/Count 1"), "just the single content sheet");
}

#[test]
fn online_video_renders_a_link_in_pdf() {
    // An online video can't play in a static PDF, so it collects as a link
    // to the video (here YouTube → its watch URL).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("vid.wcl");
    write_fixture(
        &src,
        "page watch {\n  video \"https://youtu.be/dQw4w9WgXcQ\" { title = \"Talk recording\" }\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    pdf_ok(&src, out.path(), PageSize::A4);

    let bytes = std::fs::read(out.path().join("vid.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"), "output is a PDF");
    let blob = String::from_utf8_lossy(&bytes);
    // A link annotation with a URI action pointing at the canonical watch URL.
    assert!(blob.contains("/Link"), "a link annotation is emitted:\n");
    assert!(
        blob.contains("https://www.youtube.com/watch?v=dQw4w9WgXcQ"),
        "the watch URL is the link target"
    );
}

#[test]
fn local_video_has_no_link_in_pdf() {
    // A local video gets only its poster — never a link (a path to a local
    // file is useless in a distributed PDF). With no poster it adds nothing,
    // so the surrounding prose is all that remains.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("local.wcl");
    write_fixture(
        &src,
        "page show {\n  p \"Before.\"\n  video \"media/clip.mp4\" { title = \"Local\" }\n  p \"After.\"\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    pdf_ok(&src, out.path(), PageSize::A4);

    let bytes = std::fs::read(out.path().join("local.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"), "output is a PDF");
    let blob = String::from_utf8_lossy(&bytes);
    assert!(
        !blob.contains("clip.mp4"),
        "a local video path is never linked"
    );
}
