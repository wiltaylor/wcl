//! Integration tests for `@only` / `@except` block visibility filters,
//! exercised across all three backends (HTML / Markdown / PDF).

use std::path::Path;

use tempfile::TempDir;
use wcl_wdoc::{
    BuildError, BuildOptions, PageSize, PdfError, build, build_with_options, markdown, pdf,
};

/// A two-site document (a `:webpage` site `main` and a `:book` site `guide`)
/// with one page whose blocks carry every flavour of filter. Each backend
/// gets the same source; only the `backends=` axis differs at render time.
const DOC: &str = r#"import <wdoc.wcl>
site main {
  default_template = :webpage
}
site guide {
  default_template = :book
}
page home {
  p "ALWAYS_HERE"
  @only(sites=[:main]) p "ONLY_MAIN"
  @except(sites=[:main]) p "NOT_MAIN"
  @only(templates=[:book]) p "ONLY_BOOK"
  @except(backends=[:pdf]) p "NOT_PDF"
  @only(backends=[:pdf]) p "ONLY_PDF"
  @only(sites=[:main], templates=[:book]) p "MAIN_AND_BOOK"
}
"#;

fn write(dir: &Path, body: &str) -> std::path::PathBuf {
    let src = dir.join("doc.wcl");
    std::fs::write(&src, body).expect("write fixture");
    src
}

fn read(out: &Path, rel: &str) -> String {
    std::fs::read_to_string(out.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

#[test]
fn html_filters_by_site_template_and_backend() {
    let tmp = TempDir::new().unwrap();
    let src = write(tmp.path(), DOC);
    let out = tmp.path().join("out");
    match build(&src, &out, None) {
        Ok(_) => {}
        Err(BuildError::Schema(n)) => panic!("schema error: {n} violations"),
        Err(_) => panic!("build failed"),
    }

    // `main` is a :webpage site; `guide` is a :book site. Two sites, neither
    // marked `root`, so each renders into its own subdirectory.
    let main = read(&out, "main/home.html");
    let guide = read(&out, "guide/home.html");

    for page in [&main, &guide] {
        assert!(
            page.contains("ALWAYS_HERE"),
            "unfiltered block always shows"
        );
        // `:pdf`-only never shows on the HTML backend; the `@except(:pdf)`
        // block always shows here.
        assert!(page.contains("NOT_PDF"), "except(:pdf) shows on html");
        assert!(!page.contains("ONLY_PDF"), "only(:pdf) hidden on html");
        // `sites=:main AND templates=:book` is unsatisfiable (no site is both)
        // — proves axes are AND'd.
        assert!(!page.contains("MAIN_AND_BOOK"), "AND across axes");
    }

    // Site-name axis.
    assert!(main.contains("ONLY_MAIN"));
    assert!(!main.contains("NOT_MAIN"));
    assert!(!guide.contains("ONLY_MAIN"));
    assert!(guide.contains("NOT_MAIN"));

    // Template-kind axis (main = :webpage, guide = :book).
    assert!(!main.contains("ONLY_BOOK"));
    assert!(guide.contains("ONLY_BOOK"));
}

#[test]
fn all_sites_renders_everything_and_stamps_visibility() {
    // The editor's merged all-views preview: `all_sites` bypasses every
    // `@only` / `@except` filter, and (with `edit_mode` on) stamps each
    // block's visibility on its anchor so the client can indicate/toggle it.
    let tmp = TempDir::new().unwrap();
    let src = write(tmp.path(), DOC);
    let out = tmp.path().join("merged");
    let opts = BuildOptions {
        edit_mode: true,
        all_sites: true,
        ..Default::default()
    };
    match build_with_options(&src, &out, Some("main"), &opts) {
        Ok(_) => {}
        Err(BuildError::Schema(n)) => panic!("schema error: {n} violations"),
        Err(_) => panic!("build failed"),
    }
    let main = read(&out, "home.html");

    // Every block renders, whatever its filters say about site `main`.
    for marker in [
        "ALWAYS_HERE",
        "ONLY_MAIN",
        "NOT_MAIN",
        "ONLY_BOOK",
        "NOT_PDF",
        "ONLY_PDF",
        "MAIN_AND_BOOK",
    ] {
        assert!(main.contains(marker), "{marker} renders under all_sites");
    }

    // `@except(sites=[:main])` stamps its site list on the anchor…
    assert!(
        main.contains("data-wcl-except=\"main\""),
        "except sites stamped: {main}"
    );
    // …while `@only` and the templates/backends axes classify as custom.
    assert!(main.contains("data-wcl-vis=\"custom\""), "custom stamped");
    // The unfiltered block carries neither stamp: after `split('<')` its
    // chunk is exactly `p …attrs…>ALWAYS_HERE`, so the visibility
    // attributes would sit in the same chunk if they leaked.
    let always = main
        .split('<')
        .find(|tag| tag.contains("ALWAYS_HERE"))
        .expect("ALWAYS_HERE tag");
    assert!(
        always.contains("data-wcl-span"),
        "edit-mode anchor present: {always}"
    );
    assert!(
        !always.contains("data-wcl-except") && !always.contains("data-wcl-vis"),
        "plain block unstamped: {always}"
    );
}

#[test]
fn normal_edit_mode_build_emits_no_visibility_stamps() {
    let tmp = TempDir::new().unwrap();
    let src = write(tmp.path(), DOC);
    let out = tmp.path().join("plain");
    let opts = BuildOptions {
        edit_mode: true,
        ..Default::default()
    };
    match build_with_options(&src, &out, Some("main"), &opts) {
        Ok(_) => {}
        Err(BuildError::Schema(n)) => panic!("schema error: {n} violations"),
        Err(_) => panic!("build failed"),
    }
    let main = read(&out, "home.html");
    // Filtering still applies and no visibility stamps leak.
    assert!(!main.contains("NOT_MAIN"));
    assert!(!main.contains("data-wcl-except"));
    assert!(!main.contains("data-wcl-vis"));
}

#[test]
fn markdown_filters_by_site_and_backend() {
    let tmp = TempDir::new().unwrap();
    let src = write(tmp.path(), DOC);
    let out = tmp.path().join("md");
    match markdown(&src, &out, None) {
        Ok(_) => {}
        Err(BuildError::Schema(n)) => panic!("schema error: {n} violations"),
        Err(_) => panic!("markdown failed"),
    }

    let main = read(&out, "main/home.md");
    let guide = read(&out, "guide/home.md");

    // Backend axis: current backend is :markdown.
    assert!(main.contains("NOT_PDF"), "except(:pdf) shows on markdown");
    assert!(!main.contains("ONLY_PDF"), "only(:pdf) hidden on markdown");
    // Site axis still applies on the markdown backend.
    assert!(main.contains("ONLY_MAIN"));
    assert!(!guide.contains("ONLY_MAIN"));
    // Template axis (guide = :book).
    assert!(guide.contains("ONLY_BOOK"));
}

#[test]
fn pdf_drops_blocks_filtered_out_of_the_pdf_backend() {
    // PDF text streams are compressed, so rather than searching for the text we
    // compare byte sizes: a long block hidden from the PDF backend must shrink
    // the output relative to the same block left in.
    let big = "Lorem ipsum dolor sit amet ".repeat(40);
    let shown = format!("import <wdoc.wcl>\npage p {{\n  p \"{big}\"\n}}\n");
    let hidden =
        format!("import <wdoc.wcl>\npage p {{\n  @only(backends=[:html]) p \"{big}\"\n}}\n");

    let render = |body: &str| -> Vec<u8> {
        let tmp = TempDir::new().unwrap();
        let src = write(tmp.path(), body);
        let out = tmp.path().join("pdfout");
        match pdf(&src, &out, None, PageSize::A4) {
            Ok(_) => {}
            Err(PdfError::Schema(n)) => panic!("schema error: {n} violations"),
            Err(_) => panic!("pdf failed"),
        }
        std::fs::read(out.join("doc.pdf")).expect("read pdf")
    };

    let with = render(&shown);
    let without = render(&hidden);
    assert!(with.starts_with(b"%PDF-") && without.starts_with(b"%PDF-"));
    assert!(
        without.len() < with.len(),
        "block filtered out of the pdf backend shrinks the output ({} vs {} bytes)",
        without.len(),
        with.len(),
    );
}
