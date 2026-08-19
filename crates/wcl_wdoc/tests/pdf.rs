//! Integration tests for the PDF backend.

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
        Err(PdfError::Eval(r)) => panic!("pdf eval error: {r:?}"),
        Err(PdfError::BadDoc(m)) => panic!("pdf bad-doc error: {m}"),
        Err(PdfError::Render(m)) => panic!("pdf render error: {m}"),
    }
}

/// Read the physical page count out of the (uncompressed) page-tree node.
/// krilla writes `/Count N` in the page tree; an outline node also carries a
/// `/Count`, but it never exceeds the page count, so the max is the page tree.
fn page_count(bytes: &[u8]) -> usize {
    let blob = String::from_utf8_lossy(bytes);
    let mut max = 0usize;
    for (i, _) in blob.match_indices("/Count ") {
        let digits: String = blob[i + "/Count ".len()..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        if let Ok(n) = digits.parse::<usize>() {
            max = max.max(n);
        }
    }
    max
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
fn computed_table_eval_error_fails_the_build() {
    // Same contract as Markdown/HTML: a `rows` expression that fails to
    // evaluate is a hard error, not a silent pipe-table fallback.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("t.wcl");
    write_fixture(
        &src,
        "page p {\n  table {\n    rows = no_such_name\n  }\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    match pdf(&src, out.path(), None, PageSize::A4) {
        Err(PdfError::Eval(r)) => {
            let text = format!("{r:?}");
            assert!(text.contains("no_such_name"), "names the binding: {text}");
        }
        Ok(n) => panic!("expected an eval error, but wrote {n} pdf(s)"),
        Err(_) => panic!("expected PdfError::Eval, got a different error"),
    }
}

#[test]
fn invalid_math_marker_stays_nonfatal() {
    // RaTeX renders invalid LaTeX as an error marker with no `<svg>`;
    // the embed treats "no svg at all" as benign (only a *malformed*
    // `<svg>` is a hard error), so the build must still succeed.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("m.wcl");
    write_fixture(
        &src,
        "page p {\n  math \"\\\\notacommand{\" {}\n  p \"kept\"\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);
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
        "page w {\n  diagram { width = 320  height = 440\n  wf_window \"Sign in\" {\n    wf_input \"you@example.com\"\n    wf_dropdown \"Personal\"\n    wf_row {\n      wf_checkbox \"Remember\" { checked = true }\n      wf_radio \"Annual\"\n    }\n    wf_toggle \"Notifications\" { on = true }\n    wf_grid {\n      columns = 2\n      wf_button \"Cancel\"\n      wf_button \"OK\" { icon = \"lucide.check\" }\n    }\n  }\n  }\n}\n",
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
        "page w {\n  diagram { width = 200  height = 120\n  wf_window \"App\" { theme = :gruvbox  mode = :light\n    wf_button \"OK\"\n    wf_toggle \"On\" { on = true }\n  }\n  }\n}\n",
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
    // A bare widget (no container) is a valid diagram shape and renders on
    // its own, embedded via the diagram's SVG.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("bare.wcl");
    write_fixture(
        &src,
        "page b {\n  diagram { width = 120  height = 40\n  wf_button \"Click me\"\n  }\n}\n",
    );
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
    // file is useless in a distributed PDF). With no poster there is nothing
    // to show, but the content IR's exhaustive video arm names it instead of
    // letting it vanish between the paragraphs either side of it.
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
    // Page text is Flate-compressed, so the italic serif font program — which
    // is embedded only when an italic run is laid out, and nothing else on
    // this page is italic — is the evidence the name was written.
    assert!(
        blob.contains("NotoSerif-Italic"),
        "the video names itself rather than disappearing"
    );
}

#[test]
fn unresolved_name_in_page_block_errors() {
    // Issue 13: an unresolved binding in a page block surfaces as a loud
    // diagnostic on the PDF path, not a silently dropped block.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(
        &src,
        "page home {\n  start = true\n  p $\"count = ${len(nonexistent)}\"\n}\n",
    );
    let out = tmp.path().join("out");
    match pdf(&src, &out, None, PageSize::A4) {
        Err(PdfError::Eval(_)) => {}
        Ok(n) => panic!("expected an eval error, but wrote {n} pdf(s)"),
        Err(other) => {
            other.report();
            panic!("expected PdfError::Eval, got a different error (see above)");
        }
    }
}

#[test]
fn collect_gathers_partials_into_valid_pdf() {
    // Scattered `partial`s gathered by a `collect` render into a valid PDF
    // (text is glyph-encoded, so we assert structural validity, not content).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(
        &src,
        "page p {\n  partial note { p \"First scattered note.\" }\n  p \"Body prose.\"\n  partial note { p \"Second scattered note.\" }\n  collect note\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);
    let bytes = std::fs::read(out.path().join("doc.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"), "output is a PDF");
    assert!(bytes.len() > 1000, "non-trivial pdf produced");
}

#[test]
fn diagram_shapes_and_edges_render_to_pdf() {
    // A flat diagram with manually-placed flowchart shapes connected by an
    // edge embeds as one vector SVG (shapes + the routed edge line).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("flow.wcl");
    write_fixture(
        &src,
        concat!(
            "page f {\n",
            "  diagram {\n",
            "    width = 340  height = 120\n",
            "    process \"Start\" { id = a  x = 10.0   y = 40.0  width = 100.0  height = 40.0 }\n",
            "    process \"End\"   { id = b  x = 220.0  y = 40.0  width = 100.0  height = 40.0 }\n",
            "    a -> b :flow\n",
            "  }\n",
            "}\n",
        ),
    );
    let out = TempDir::new().expect("mkdir out");
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);
    let bytes = std::fs::read(out.path().join("flow.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"));
    assert_eq!(page_count(&bytes), 1);
    assert!(
        bytes.len() > 3000,
        "expected vector diagram content, got {} bytes",
        bytes.len()
    );
}

#[test]
fn layered_flowchart_renders_to_pdf() {
    // Auto-layout (`:layered`) + elbow routing run on the PDF path exactly as
    // on HTML: shapes are ranked from the edge graph, then the whole diagram
    // embeds as a vector SVG.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("layered.wcl");
    write_fixture(
        &src,
        concat!(
            "page l {\n",
            "  diagram {\n",
            "    width = 320  height = 220  layout = :layered  layer_gap = 20.0\n",
            "    process    \"Parse\"  { id = parse   width = 100.0  height = 40.0 }\n",
            "    decision   \"Valid?\" { id = valid   width = 100.0  height = 60.0 }\n",
            "    terminator \"Render\" { id = render  width = 100.0  height = 40.0 }\n",
            "    parse -> valid  :flow\n",
            "    valid -> render :flow\n",
            "  }\n",
            "}\n",
        ),
    );
    let out = TempDir::new().expect("mkdir out");
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);
    let bytes = std::fs::read(out.path().join("layered.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"));
    assert_eq!(page_count(&bytes), 1);
}

#[test]
fn diagram_with_multiple_cards_overlays_each() {
    // Two `card` foreignObjects in one diagram: collect_diagram pairs each
    // rendered rect with its source block (in order) and lays both bodies out
    // natively — bold serif titles prove the native overlay path ran.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("cards.wcl");
    write_fixture(
        &src,
        concat!(
            "page p {\n",
            "  diagram d {\n",
            "    width = 480  height = 180\n",
            "    card {\n",
            "      x = 10.0  y = 10.0  width = 220.0  height = 160.0\n",
            "      title = \"First\"\n",
            "      p \"Alpha body text.\"\n",
            "    }\n",
            "    card {\n",
            "      x = 250.0  y = 10.0  width = 220.0  height = 160.0\n",
            "      title = \"Second\"\n",
            "      p \"Beta body with _emphasis_.\"\n",
            "    }\n",
            "  }\n",
            "}\n",
        ),
    );
    let out = TempDir::new().expect("mkdir out");
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);
    let bytes = std::fs::read(out.path().join("cards.pdf")).expect("read pdf");
    let blob = String::from_utf8_lossy(&bytes);
    assert!(
        blob.contains("NotoSerif-Bold"),
        "card titles rendered natively (serif bold embedded)"
    );
    assert!(
        blob.contains("NotoSerif-Italic"),
        "second card's _emphasis_ ran through the inline engine"
    );
}

#[test]
fn card_inside_container_is_collected() {
    // collect_card_blocks descends containers depth-first, so a card nested
    // inside a `container` still pairs with its foreignObject and paints
    // natively.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("nested.wcl");
    write_fixture(
        &src,
        concat!(
            "page p {\n",
            "  diagram d {\n",
            "    width = 360  height = 220\n",
            "    container {\n",
            "      stroke = \"#888\"  padding = 12.0\n",
            "      card {\n",
            "        x = 10.0  y = 10.0  width = 280.0  height = 150.0\n",
            "        title = \"Inside\"\n",
            "        p \"Body within a **container**.\"\n",
            "      }\n",
            "    }\n",
            "  }\n",
            "}\n",
        ),
    );
    let out = TempDir::new().expect("mkdir out");
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);
    let bytes = std::fs::read(out.path().join("nested.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(
        String::from_utf8_lossy(&bytes).contains("NotoSerif-Bold"),
        "nested card body rendered natively"
    );
}

#[test]
fn terminal_with_box_and_styled_text_renders() {
    // The structured terminal form (term_box / term_text with colours, bold,
    // inverse) bakes into a static SVG snapshot and embeds as vector content.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("tui.wcl");
    write_fixture(
        &src,
        concat!(
            "page t {\n",
            "  terminal {\n",
            "    cols = 20  rows = 5  chrome = false\n",
            "    term_box { row = 1 col = 1 width = 20 height = 5 border = :double }\n",
            "    term_text \"OK\" { row = 2 col = 2 fg = \"green\" bold = true }\n",
            "    term_text \"no\" { row = 3 col = 2 fg = \"red\" inverse = true }\n",
            "  }\n",
            "}\n",
        ),
    );
    let out = TempDir::new().expect("mkdir out");
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);
    let bytes = std::fs::read(out.path().join("tui.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(
        bytes.len() > 2000,
        "structured terminal produced vector content, got {} bytes",
        bytes.len()
    );
}

#[test]
fn wireframe_widgets_connect_with_edges() {
    // wf_* widgets extend SvgBlock, so they are edge-addressable diagram
    // shapes: two placed buttons joined by an edge build into one SVG.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("wfedge.wcl");
    write_fixture(
        &src,
        concat!(
            "page w {\n",
            "  diagram { width = 340  height = 120  routing = :straight\n",
            "    wf_button \"OK\"     { id = ok      x = 10.0   y = 40.0 }\n",
            "    wf_button \"Cancel\" { id = cancel  x = 240.0  y = 40.0 }\n",
            "    ok -> cancel :flow\n",
            "  }\n",
            "}\n",
        ),
    );
    let out = TempDir::new().expect("mkdir out");
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);
    let bytes = std::fs::read(out.path().join("wfedge.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"));
    assert_eq!(page_count(&bytes), 1);
}

#[test]
fn long_prose_paginates_to_multiple_pages() {
    // One source `page` with far more prose than a sheet holds: the greedy
    // line-flow paginator breaks it across several physical pages.
    let mut body = String::from("page long {\n");
    for i in 0..12 {
        body.push_str(&format!("  h2 \"Section {i}\"\n"));
        for _ in 0..4 {
            body.push_str(
                "  p \"A paragraph of body text long enough to wrap onto a couple of lines \
                 when shaped at the default A4 content width, giving the paginator real \
                 line-by-line work to do across the section.\"\n",
            );
        }
    }
    body.push_str("}\n");

    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("long.wcl");
    write_fixture(&src, &body);
    let out = TempDir::new().expect("mkdir out");
    // Still ONE pdf — pagination is physical pages within it.
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);
    let bytes = std::fs::read(out.path().join("long.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"));
    let n = page_count(&bytes);
    assert!(n >= 2, "expected the prose to paginate, got {n} page(s)");
}

#[test]
fn code_block_splits_across_a_page_break() {
    // A code block taller than the remaining page splits at line boundaries,
    // drawing a fresh background box per page segment.
    let mut code_lines = String::new();
    for i in 0..90 {
        code_lines.push_str(&format!("let value_{i} = {i}; // line {i}\n"));
    }
    let body = format!(
        "page c {{\n  p \"A lead-in paragraph so the code does not start at the page top.\"\n  code rust {{\n    source = <<'CODE'\n{code_lines}CODE\n  }}\n}}\n"
    );

    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("bigcode.wcl");
    write_fixture(&src, &body);
    let out = TempDir::new().expect("mkdir out");
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);
    let bytes = std::fs::read(out.path().join("bigcode.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"));
    let n = page_count(&bytes);
    assert!(n >= 2, "expected the code block to split, got {n} page(s)");
    // The mono code face is embedded on the split path too.
    assert!(String::from_utf8_lossy(&bytes).contains("NotoSansMono"));
}

#[test]
fn table_rows_split_across_a_page_break() {
    // A table with more rows than a sheet holds paginates row-by-row (a row
    // that won't fit starts a new page).
    let mut body = String::from("page t {\n  table {\n    rows:\n      | \"Name\" | \"Score\" |\n");
    for i in 0..55 {
        body.push_str(&format!("      | \"row{i}\" | {i} |\n"));
    }
    body.push_str("  }\n}\n");

    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("bigtable.wcl");
    write_fixture(&src, &body);
    let out = TempDir::new().expect("mkdir out");
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);
    let bytes = std::fs::read(out.path().join("bigtable.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"));
    let n = page_count(&bytes);
    assert!(n >= 2, "expected the table to paginate, got {n} page(s)");
}

#[test]
fn timeline_renders_to_pdf() {
    // The Rust-rendered timeline (calendar math, phases, items) embeds as a
    // vector SVG inside its diagram.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("tl.wcl");
    write_fixture(
        &src,
        concat!(
            "page t {\n",
            "  diagram {\n",
            "    width = 500  height = 220\n",
            "    timeline {\n",
            "      width = 500  height = 220\n",
            "      title = \"Roadmap\"\n",
            "      unit  = :months\n",
            "      start = \"2026-01-01\"\n",
            "      end   = \"2026-12-31\"\n",
            "      phases = [\n",
            "        TimelinePhase::Of { label: \"Design\", from: \"2026-01-01\", to: \"2026-06-01\" },\n",
            "        TimelinePhase::Of { label: \"Build\",  from: \"2026-06-01\", to: \"2026-12-31\" },\n",
            "      ]\n",
            "      items = [\n",
            "        TimelineItem::On { label: \"Kickoff\", on: \"2026-01-10\" },\n",
            "        TimelineItem::On { label: \"Launch\",  on: \"2026-12-20\" },\n",
            "      ]\n",
            "    }\n",
            "  }\n",
            "}\n",
        ),
    );
    let out = TempDir::new().expect("mkdir out");
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);
    let bytes = std::fs::read(out.path().join("tl.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(
        bytes.len() > 3000,
        "timeline produced vector content, got {} bytes",
        bytes.len()
    );
}

#[test]
fn bar_chart_renders_to_pdf() {
    // A pure-WCL-lowered chart (bars, axes, legend) embeds as a vector SVG.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("bar.wcl");
    write_fixture(
        &src,
        concat!(
            "page b {\n",
            "  diagram {\n",
            "    width = 380  height = 240\n",
            "    bar_chart {\n",
            "      width = 380  height = 240\n",
            "      title = \"Revenue\"\n",
            "      x_label = \"Quarter\"\n",
            "      categories = [\"Q1\", \"Q2\"]\n",
            "      y_min = 0.0\n",
            "      y_max = 100.0\n",
            "      series = [\n",
            "        ChartSeries::Of { name: \"North\", values: [40.0, 80.0] },\n",
            "        ChartSeries::Of { name: \"South\", values: [20.0, 60.0] },\n",
            "      ]\n",
            "    }\n",
            "  }\n",
            "}\n",
        ),
    );
    let out = TempDir::new().expect("mkdir out");
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);
    let bytes = std::fs::read(out.path().join("bar.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(
        bytes.len() > 3000,
        "bar chart produced vector content, got {} bytes",
        bytes.len()
    );
}

#[test]
fn pie_chart_renders_to_pdf() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("pie.wcl");
    write_fixture(
        &src,
        concat!(
            "page p {\n",
            "  diagram {\n",
            "    width = 240  height = 240\n",
            "    pie_chart {\n",
            "      width = 240  height = 240\n",
            "      title = \"Mix\"\n",
            "      slices = [\n",
            "        ChartSlice::Of { label: \"A\", value: 50.0 },\n",
            "        ChartSlice::Of { label: \"B\", value: 30.0 },\n",
            "        ChartSlice::Of { label: \"C\", value: 20.0 },\n",
            "      ]\n",
            "    }\n",
            "  }\n",
            "}\n",
        ),
    );
    let out = TempDir::new().expect("mkdir out");
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);
    let bytes = std::fs::read(out.path().join("pie.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"));
}

#[test]
fn tree_renders_to_pdf() {
    // The Rust-rendered indented file-tree (rows + connector guides) embeds
    // as a vector SVG inside its diagram.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("tree.wcl");
    write_fixture(
        &src,
        concat!(
            "page t {\n",
            "  diagram { width = 360  height = 220\n",
            "    tree {\n",
            "      tree_node \"src/\" {\n",
            "        tree_node \"render/\" {\n",
            "          tree_node \"svg.rs\" {}\n",
            "          tree_node \"html.rs\" {}\n",
            "        }\n",
            "        tree_node \"lib.rs\" {}\n",
            "      }\n",
            "      tree_node \"Cargo.toml\" {}\n",
            "    }\n",
            "  }\n",
            "}\n",
        ),
    );
    let out = TempDir::new().expect("mkdir out");
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);
    let bytes = std::fs::read(out.path().join("tree.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"));
    assert!(
        bytes.len() > 2000,
        "tree produced vector content, got {} bytes",
        bytes.len()
    );
}

#[test]
fn only_html_block_is_excluded_from_pdf() {
    // `@only(backends=[:html])` content never reaches the PDF. Text streams
    // are compressed, so assert structurally: enough hidden filler to force a
    // second page when visible collapses back to one page when scoped out.
    let filler_visible: String = (0..60)
        .map(|i| format!("  p \"Filler paragraph {i} consuming vertical space.\"\n"))
        .collect();
    let filler_hidden: String = (0..60)
        .map(|i| {
            format!(
                "  @only(backends=[:html]) p \"Filler paragraph {i} consuming vertical space.\"\n"
            )
        })
        .collect();
    let render = |body: &str| -> Vec<u8> {
        let tmp = TempDir::new().expect("mkdir tempdir");
        let src = tmp.path().join("doc.wcl");
        write_fixture(&src, body);
        let out = TempDir::new().expect("mkdir out");
        pdf_ok(&src, out.path(), PageSize::A4);
        std::fs::read(out.path().join("doc.pdf")).expect("read pdf")
    };

    let shown = render(&format!("page p {{\n  p \"Intro.\"\n{filler_visible}}}\n"));
    let hidden = render(&format!("page p {{\n  p \"Intro.\"\n{filler_hidden}}}\n"));
    assert!(
        page_count(&shown) >= 2,
        "control: the visible filler paginates"
    );
    assert_eq!(
        page_count(&hidden),
        1,
        "html-only filler is dropped from the PDF, leaving one page"
    );
}

#[test]
fn except_pdf_block_is_excluded_from_pdf() {
    // The complementary scoping: `@except(backends=[:pdf])` drops the block
    // from the PDF backend (it would still render on HTML).
    let filler: String = (0..60)
        .map(|i| {
            format!(
                "  @except(backends=[:pdf]) p \"Filler paragraph {i} consuming vertical space.\"\n"
            )
        })
        .collect();
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(&src, &format!("page p {{\n  p \"Intro.\"\n{filler}}}\n"));
    let out = TempDir::new().expect("mkdir out");
    pdf_ok(&src, out.path(), PageSize::A4);
    let bytes = std::fs::read(out.path().join("doc.pdf")).expect("read pdf");
    assert_eq!(
        page_count(&bytes),
        1,
        "pdf-excepted filler is dropped, leaving one page"
    );
}

#[test]
fn collect_cycle_terminates_in_pdf() {
    // A collected partial body containing a `collect` of the same tag must not
    // recurse forever — the guard breaks the cycle and the PDF builds.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(
        &src,
        "page p {\n  partial loop { p \"Cycle body.\"  collect loop }\n  collect loop\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);
    let bytes = std::fs::read(out.path().join("doc.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"), "output is a PDF");
}

// ── generator expansion (repeaters / components) ──────────────────

#[test]
fn repeater_stamps_body_into_the_pdf() {
    // A page-level `wdoc_repeater` expands via the shared helpers (as on
    // the HTML / Markdown paths). The bold inline in the stamped body only
    // embeds the bold subset if the repeater actually rendered.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("rep.wcl");
    write_fixture(
        &src,
        concat!(
            "page p {\n",
            "  let items = [\"alpha\", \"beta\", \"gamma\"]\n",
            "  h1 \"Inventory\"\n",
            "  wdoc_repeater { each = items  as = :it\n",
            "    p $\"Entry **${it}** present.\"\n",
            "  }\n",
            "}\n",
        ),
    );
    let out = TempDir::new().expect("mkdir out");
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);
    let bytes = std::fs::read(out.path().join("rep.pdf")).expect("read pdf");
    let blob = String::from_utf8_lossy(&bytes);
    assert!(
        blob.contains("NotoSerif-Bold"),
        "repeater body rendered (bold subset embedded)"
    );
}

#[test]
fn component_instance_expands_into_the_pdf() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("comp.wcl");
    write_fixture(
        &src,
        concat!(
            "wdoc_component note_box {\n",
            "  wdoc_slot title\n",
            "  wdoc_body {\n",
            "    h2 $\"${title}\"\n",
            "    p \"Component **body** text.\"\n",
            "  }\n",
            "}\n",
            "page p {\n",
            "  h1 \"Top\"\n",
            "  note_box { title = \"From slot\" }\n",
            "}\n",
        ),
    );
    let out = TempDir::new().expect("mkdir out");
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);
    let bytes = std::fs::read(out.path().join("comp.pdf")).expect("read pdf");
    let blob = String::from_utf8_lossy(&bytes);
    assert!(
        blob.contains("NotoSerif-Bold"),
        "component body rendered (bold subset embedded)"
    );
}

#[test]
fn component_content_slot_renders_instance_children() {
    // The outer component forwards its content slot through the inner one.
    // The payload's bold run only embeds the bold face if that structural
    // slot context survives both component expansions.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("slot.wcl");
    write_fixture(
        &src,
        concat!(
            "wdoc_component inner {\n",
            "  wdoc_body {\n",
            "    p \"Inner frame.\"\n",
            "    wdoc_content\n",
            "  }\n",
            "}\n",
            "wdoc_component outer {\n",
            "  wdoc_body {\n",
            "    inner {\n",
            "      wdoc_content\n",
            "    }\n",
            "  }\n",
            "}\n",
            "page p {\n",
            "  outer {\n",
            "    p \"Inner **payload** here.\"\n",
            "  }\n",
            "}\n",
        ),
    );
    let out = TempDir::new().expect("mkdir out");
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);
    let bytes = std::fs::read(out.path().join("slot.pdf")).expect("read pdf");
    let blob = String::from_utf8_lossy(&bytes);
    assert!(
        blob.contains("NotoSerif-Bold"),
        "wdoc_content spliced the instance children (bold subset embedded)"
    );
}

#[test]
fn sequence_diagram_embeds_in_pdf() {
    // Smoke: a sequence_diagram page renders to a PDF without panicking
    // (the lowered SVG goes through the shared vector embedder).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("seq.wcl");
    write_fixture(
        &src,
        "site s { title = \"S\" }\npage seq {\n  sites = [:s]\n  h1 \"Seq\"\n  sequence_diagram {\n    participant \"a\" { }\n    participant \"b\" { }\n    message \"m1\" { from = \"a\"  to = \"b\"  text = \"hi\" }\n  }\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    let n = pdf_ok(&src, out.path(), PageSize::A4);
    assert_eq!(n, 1);
}

#[test]
fn state_diagram_embeds_in_pdf() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("sc.wcl");
    write_fixture(
        &src,
        "site s { title = \"S\" }\npage sc {\n  sites = [:s]\n  h1 \"Sc\"\n  state_diagram {\n    state \"a\" { initial = true }\n    state \"b\" { final = true }\n    transition \"t1\" { from = \"a\"  to = \"b\"  trigger = \"go\" }\n  }\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    let n = pdf_ok(&src, out.path(), PageSize::A4);
    assert_eq!(n, 1);
}

#[test]
fn a_file_block_refuses_to_build_a_pdf() {
    // A PDF is one self-contained document: there is no output folder beside
    // it to copy a shipped file into, so `file` declares no `:pdf` coverage
    // and the build says so rather than dropping the block. This is the
    // question the mechanism exists to force an answer to.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(
        &src,
        "site s { title = \"S\" }\npage p {\n  sites = [:s]\n  h1 \"P\"\n  file \"setup.sh\" { dir = \"scripts\"  as = \"run setup\" }\n}\n",
    );
    std::fs::write(tmp.path().join("setup.sh"), "#!/bin/sh\n").expect("write asset");
    let out = TempDir::new().expect("mkdir out");
    match pdf(&src, out.path(), None, PageSize::A4) {
        Err(PdfError::Eval(r)) => {
            let text = format!("{r:?}");
            assert!(
                text.contains("file") && text.contains(":pdf") && text.contains("@except"),
                "names the kind, the target and the waiver: {text}"
            );
        }
        Ok(n) => panic!("expected an uncovered-target error, but wrote {n} page(s)"),
        Err(_) => panic!("expected PdfError::Eval"),
    }
}

#[test]
fn a_waived_file_block_lets_the_pdf_build() {
    // The stated intent: this document does not want the file in its PDF.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(
        &src,
        "site s { title = \"S\" }\npage p {\n  sites = [:s]\n  h1 \"P\"\n  @except(backends = [:pdf])\n  file \"setup.sh\" { dir = \"scripts\"  as = \"run setup\" }\n}\n",
    );
    std::fs::write(tmp.path().join("setup.sh"), "#!/bin/sh\n").expect("write asset");
    let out = TempDir::new().expect("mkdir out");
    let n = pdf_ok(&src, out.path(), PageSize::A4);
    assert_eq!(n, 1);
}

#[test]
fn column_children_stack_in_a_pdf() {
    // A PDF page flow has no side-by-side layout, so a `column` renders its
    // children in source order instead of dropping them. The text is font-
    // subset encoded in the output, so the signal is that the wrapped
    // content reaches the page at all: the same body inside a `column`
    // produces a PDF the size of the bare one, not the size of an empty page.
    fn build(body: &str) -> usize {
        let tmp = TempDir::new().expect("mkdir tempdir");
        let src = tmp.path().join("doc.wcl");
        write_fixture(&src, body);
        let out = TempDir::new().expect("mkdir out");
        assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);
        std::fs::read(out.path().join("doc.pdf"))
            .expect("read pdf")
            .len()
    }
    let empty = build("page p {\n  h1 \"P\"\n}\n");
    let bare = build(
        "page p {\n  h1 \"P\"\n  p \"left side of the layout\"\n  p \"right side of the layout\"\n}\n",
    );
    let wrapped = build(
        "page p {\n  h1 \"P\"\n  column { widths = [50.0, 50.0]\n    p \"left side of the layout\"\n    p \"right side of the layout\"\n  }\n}\n",
    );
    assert!(bare > empty, "the fixture's body has to show up at all");
    assert!(
        wrapped >= bare,
        "column children must reach the PDF (empty {empty}, bare {bare}, wrapped {wrapped})"
    );
}

#[test]
fn a_file_block_inside_a_card_still_refuses_to_build_a_pdf() {
    // A `card` body renders as HTML inside the diagram's `<foreignObject>`
    // whichever target embeds it, so the renderer running there is the HTML
    // one. `file`'s non-coverage is about the *output*, though — a PDF ships
    // no folder to copy into — so the check has to fail here too, whichever
    // of the two paths a card body takes, or the PDF gets a link to an asset
    // that was never written.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(
        &src,
        "site s { title = \"S\" }\npage p {\n  sites = [:s]\n  h1 \"P\"\n  diagram {\n    width = 400  height = 120\n    card { x = 10.0  y = 10.0  width = 380.0  height = 100.0\n      file \"setup.sh\" { dir = \"scripts\"  as = \"run setup\" }\n    }\n  }\n}\n",
    );
    std::fs::write(tmp.path().join("setup.sh"), "#!/bin/sh\n").expect("write asset");
    let out = TempDir::new().expect("mkdir out");
    match pdf(&src, out.path(), None, PageSize::A4) {
        Err(PdfError::Eval(r)) => {
            let text = format!("{r:?}");
            assert!(
                text.contains("file") && text.contains(":pdf"),
                "names the kind and the output target: {text}"
            );
        }
        Ok(n) => panic!("expected an uncovered-target error, but wrote {n} page(s)"),
        Err(_) => panic!("expected PdfError::Eval"),
    }
}

#[test]
fn a_user_block_lowering_to_the_content_ir_renders_in_pdf() {
    // The PDF backend reads the content IR through the same exhaustive
    // match as the others, so a custom block reaches it without the
    // backend knowing the block's kind.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("ir.wcl");
    write_fixture(
        &src,
        "@block(\"gadget\")\n\
         type Gadget extends ContentBlock {\n  \
           @inline(0) name: utf8\n  id: identifier?\n  \
           lower = fn(g: Gadget) -> list<Content> [\n    \
             Content::Callout { kind: :warning, heading: g.name,\n      \
               body: [Content::Paragraph { text: \"Mind the **gap**.\" }] },\n  ]\n\
         }\n\
         page c {\n  gadget \"Heads up\"\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    pdf_ok(&src, out.path(), PageSize::A4);
    let bytes = std::fs::read(out.path().join("ir.pdf")).expect("read pdf");
    assert!(bytes.starts_with(b"%PDF-"));
    // The bold heading + bold body emphasis embed the serif bold face.
    assert!(String::from_utf8_lossy(&bytes).contains("NotoSerif-Bold"));
}

#[test]
fn a_lowering_returning_another_custom_variant_recurses_in_pdf() {
    // `outer` lowers to a custom `Inner` variant, which lowers again to
    // content. Before the shared seam this chain resolved only in HTML,
    // because the recursion lived in the HTML renderer alone.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("chain.wcl");
    write_fixture(
        &src,
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
         page c {\n  outer \"through the chain\"\n}\n",
    );
    let out = TempDir::new().expect("mkdir out");
    pdf_ok(&src, out.path(), PageSize::A4);
    let bytes = std::fs::read(out.path().join("chain.pdf")).expect("read pdf");
    // The chain resolved to prose: the body serif face is embedded, which an
    // empty page would not do.
    assert!(
        String::from_utf8_lossy(&bytes).contains("NotoSerif"),
        "empty page"
    );
}

#[test]
fn the_markup_using_blocks_reach_the_pdf_through_the_content_ir() {
    // `chapter_header`, `code` and `footnotes` used to build their own
    // markup, so this backend descended a `<header>` / `<figure>` /
    // `<section>` and lost the kicker, the meta line, the filename and the
    // section title entirely. They lower to content nodes now: the meta
    // line and the filename become flow paragraphs, the listing becomes
    // coloured runs in the mono face, and the title becomes a heading.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("ir.wcl");
    write_fixture(
        &src,
        "page one {\n  \
           chapter_header \"Getting started\" {\n    \
             kicker = \"Chapter 1\"\n    reading_time = \"9 min read\"\n  }\n  \
           code rust {\n    filename = \"src/main.rs\"\n    \
             source = \"fn main() {}\"\n  }\n  \
           footnotes {\n    footnote why { text = \"Because it matters.\" }\n  }\n\
         }\n",
    );
    let out = TempDir::new().expect("mkdir out");
    assert_eq!(pdf_ok(&src, out.path(), PageSize::A4), 1);
    let bytes = std::fs::read(out.path().join("ir.pdf")).expect("read pdf");
    let blob = String::from_utf8_lossy(&bytes);
    // The chapter title and the footnotes title are headings (bold serif),
    // and the listing + filename caption draw in the mono face.
    assert!(blob.contains("NotoSans-Bold"), "heading face embedded");
    assert!(blob.contains("NotoSansMono"), "code face embedded");
}
