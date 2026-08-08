//! Relocatable output: a build tree works from whatever directory it lands
//! in, because every URL inside it is relative.
//!
//! The deploy bets on this. `just docs-build` renders the landing page into
//! `docs/_site` and then the reference book into `docs/_site/reference`, with
//! no copy or rewrite step between them, so the book is served from a
//! directory the build was never told about.
//!
//! Two crates check it over the same fixture book (`examples/wdoc_relocatable`)
//! — `wcl_wdoc/tests/build.rs` through the library, `wcl/tests/wdoc.rs` through
//! `wcl wdoc build --out`. They share this module rather than a copy each: a
//! second copy is a second thing to keep right. `wcl`'s test reaches it with a
//! `#[path]` attribute, which is why this lives in a subdirectory — cargo
//! builds every top-level file under `tests/` as its own test binary, but not
//! the files below one.
//!
//! **Scope.** `href` and `src` only. The stylesheet each page inlines also
//! emits `url(…)` font references. Those resolve now — #276 stopped the
//! renderer emitting an `@font-face` rule for a bundled family the site does
//! not ship — so widening this walk to `url(…)` has become possible; it is
//! tracked as #279 rather than done here. Until then the `@font-face` URLs
//! are covered by their own tests in `wcl_wdoc/tests/build.rs`, which check
//! that every `_wdoc/…` a page's CSS names was written.

use std::path::Path;

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

/// What a URL means for relocatability.
enum UrlKind {
    /// Anchored at the server root — the one thing that breaks the property,
    /// because the tree's own root is wherever it was dropped.
    RootAbsolute,
    /// Points outside the tree (a fragment, another host, an inline payload),
    /// so it has no on-disk target and cannot be root-anchored either.
    External,
    /// Names a file inside the tree, relative to the page that references it.
    Local,
}

fn classify(url: &str) -> UrlKind {
    // Protocol-relative first: `//host/path` starts with `/` but names another
    // host, so it is external rather than root-absolute.
    if url.starts_with("//") || url.starts_with('#') || url.contains("://") {
        UrlKind::External
    } else if url.starts_with('/') {
        UrlKind::RootAbsolute
    } else if url.is_empty() || url.starts_with("data:") || url.starts_with("mailto:") {
        UrlKind::External
    } else {
        UrlKind::Local
    }
}

/// Every `.html` under `root`, as `(path, contents)`. The one tree walk the
/// checks over a build share, so a second one can't drift from it.
pub fn html_pages(root: &Path) -> Vec<(std::path::PathBuf, String)> {
    let mut pages = Vec::new();
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
            pages.push((path, html));
        }
    }
    pages
}

/// Assert the build tree under `root` is relocatable: no `href`/`src` in any
/// `.html` under it is root-absolute, and every one naming a local file
/// resolves to a file that exists.
///
/// `min_pages` / `min_urls` are the floor the caller expects to have walked. A
/// walk that found nothing would pass silently, which is the one way this
/// could rot into a no-op.
pub fn assert_relocatable(root: &Path, min_pages: usize, min_urls: usize) {
    let mut walked_pages = 0usize;
    let mut checked_urls = 0usize;
    for (path, html) in html_pages(root) {
        let page_dir = path.parent().expect("page has a parent");
        walked_pages += 1;
        for url in html_urls(&html) {
            match classify(&url) {
                UrlKind::RootAbsolute => panic!(
                    "{}: root-absolute URL {url:?} — the tree stops working \
                     the moment it is served from a subdirectory",
                    path.display()
                ),
                UrlKind::External => {}
                UrlKind::Local => {
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
    }
    assert!(
        walked_pages >= min_pages,
        "expected at least {min_pages} pages under {}, walked {walked_pages}",
        root.display()
    );
    assert!(
        checked_urls >= min_urls,
        "expected at least {min_urls} local URLs to resolve, checked {checked_urls}"
    );
}
