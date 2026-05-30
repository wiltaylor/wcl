use std::path::Path;

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
