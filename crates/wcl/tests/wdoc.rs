use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
}

fn wcl() -> Command {
    Command::cargo_bin("wcl").expect("wcl binary built")
}

#[test]
fn wcl_wdoc_build_renders_fundamental_blocks() {
    let out = TempDir::new().expect("mkdir tempdir");
    wcl()
        .arg("wdoc")
        .arg("build")
        .arg(examples_dir().join("wdoc").join("main.wcl"))
        .arg("--out")
        .arg(out.path())
        .assert()
        .success()
        // showcase (15) + docs (3) + blog (3) + talk deck (1) across four sites.
        .stdout(predicate::str::contains("wrote 22 pages"));

    // `showcase` is the `root` site and its `overview` page sets
    // `start = true`, so the root index is that page (not a chooser),
    // with cross-site links into the subdir sites.
    let index = std::fs::read_to_string(out.path().join("index.html")).expect("read index.html");
    assert!(index.contains("<style>"), "{index}");
    assert!(
        index.contains("wdoc showcase"),
        "root index should be the showcase demo:\n{index}"
    );
    assert!(
        index.contains("href=\"docs/getting_started.html\""),
        "missing cross-site link:\n{index}"
    );

    // The richer content (text, SVG, classes) sits on the showcase
    // overview page, flat at the root since showcase is the root site.
    let overview =
        std::fs::read_to_string(out.path().join("overview.html")).expect("read overview.html");
    assert!(overview.contains("<p class=\"accent\">"), "{overview}");
    assert!(overview.contains("<svg"), "{overview}");
    assert!(overview.contains("class=\""), "{overview}");
    assert!(
        overview.contains("<h1 class=\"heading-1\" id="),
        "{overview}"
    );
}

// ---------------------------------------------------------------------------
// Relocatable output, through the CLI.
//
// `crates/wcl_wdoc/tests/build.rs` proves the library emits a build tree that
// works from any directory. This runs the same walk over the same fixture book
// through `wcl wdoc build --out <nested>` — the invocation `just docs-build`
// and the deploy workflow actually use. Keep the two in step.
// ---------------------------------------------------------------------------

/// Every `href=` / `src=` attribute value in one HTML document.
///
/// A deliberately literal scan rather than a parse: the emitted markup is our
/// own, always double-quotes its attributes, and a test that reimplements less
/// is a test that can be trusted to fail for the right reason.
fn html_urls(html: &str) -> Vec<String> {
    let mut urls = Vec::new();
    for attr in ["href=\"", "src=\""] {
        let mut rest = html;
        while let Some(start) = rest.find(attr) {
            rest = &rest[start + attr.len()..];
            let Some(end) = rest.find('"') else { break };
            urls.push(rest[..end].replace("&amp;", "&"));
            rest = &rest[end + 1..];
        }
    }
    urls
}

/// True for a URL that names a file inside the build tree (so it must resolve
/// on disk). Fragments, external schemes and protocol-relative URLs point
/// outside it and carry no on-disk target.
fn is_local_target(url: &str) -> bool {
    !url.is_empty()
        && !url.starts_with('#')
        && !url.starts_with("//")
        && !url.contains("://")
        && !url.starts_with("data:")
        && !url.starts_with("mailto:")
}

/// Assert the build tree under `root` is relocatable: every `href`/`src` in
/// every `.html` under it is relative, and every one naming a local file
/// resolves to a file that exists.
fn assert_relocatable(root: &std::path::Path) {
    let mut checked_pages = 0usize;
    let mut checked_urls = 0usize;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read build tree") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|e| e != "html") {
                continue;
            }
            let html = std::fs::read_to_string(&path).expect("read page");
            let page_dir = path.parent().expect("page has a parent");
            checked_pages += 1;
            for url in html_urls(&html) {
                assert!(
                    !url.starts_with('/'),
                    "{}: root-absolute URL {url:?} — the tree stops working \
                     the moment it is served from a subdirectory",
                    path.display()
                );
                if !is_local_target(&url) {
                    continue;
                }
                let bare = url.split(['#', '?']).next().unwrap_or(&url);
                if bare.is_empty() {
                    continue;
                }
                let target = page_dir.join(bare);
                assert!(
                    target.exists(),
                    "{}: {url:?} resolves to {}, which does not exist",
                    path.display(),
                    target.display()
                );
                checked_urls += 1;
            }
        }
    }
    // A walk that found nothing would pass silently, which is the one way this
    // test could rot into a no-op.
    assert!(
        checked_pages >= 3,
        "expected the fixture's pages, walked {checked_pages}"
    );
    assert!(
        checked_urls >= 8,
        "expected local URLs to check, checked {checked_urls}"
    );
}

#[test]
fn wcl_wdoc_build_into_a_nested_out_dir_is_relocatable() {
    let out = TempDir::new().expect("mkdir tempdir");
    // Nested, and nothing tells the build where it sits — exactly how
    // `docs-build` renders the reference book into `docs/_site/reference`.
    let nested = out.path().join("site").join("reference");
    wcl()
        .arg("wdoc")
        .arg("build")
        .arg(examples_dir().join("wdoc_relocatable").join("main.wcl"))
        .arg("--out")
        .arg(&nested)
        .assert()
        .success()
        .stdout(predicate::str::contains("wrote 3 pages"));

    assert_relocatable(&nested);
}
