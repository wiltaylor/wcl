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
//! **Scope.** `href` and `src` attributes, plus the `url(…)` values in the
//! stylesheet each page inlines — mostly `@font-face` `src:` descriptors. #276
//! stopped the renderer emitting an `@font-face` rule for a bundled family the
//! site does not ship, so those resolve now, and #279 widened the walk to
//! them. A font URL is a URL. It breaks the tree the same way.
//!
//! The CSS half reads `<style>` bodies and nothing else. A page may also
//! *show* CSS, in a code listing. That is prose about a URL, not one the
//! browser will fetch, and resolving it would fail a page that is fine. The
//! scoping is the whole defence — escaped quotes are not, because a listing
//! moved inside a `<style>` body would be read like any other rule.

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

/// Every `url(…)` target in the page's inlined stylesheets.
///
/// `<style>` bodies only, so a code listing that shows CSS stays out of it.
/// Public because `wcl_wdoc/tests/build.rs` reads the same values to check the
/// conditionally-written `_wdoc/…` faces, and one scanner cannot drift from
/// itself.
pub fn style_urls(html: &str) -> Vec<String> {
    let mut urls = Vec::new();
    for style in html.split("<style").skip(1) {
        let Some(body) = style.split_once('>').map(|(_, rest)| rest) else {
            continue;
        };
        let mut rest = body.split("</style>").next().unwrap_or(body);
        while let Some(start) = rest.find("url(") {
            rest = &rest[start + "url(".len()..];
            let Some(end) = rest.find(')') else { break };
            urls.push(rest[..end].trim().trim_matches(['\'', '"']).to_string());
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
/// `.html` under it, and no `url(…)` in any stylesheet it inlines, is
/// root-absolute, and every one naming a local file resolves to a file that
/// exists.
///
/// `min_pages`, `min_attr_urls` and `min_style_urls` are the floor the caller
/// expects to have walked. A walk that found nothing would pass silently,
/// which is the one way this could rot into a no-op.
///
/// The two URL floors are separate on purpose. One combined count lets either
/// half rot to zero and still clear the bar, because the other half carries
/// it — the fixture book alone puts thirteen font URLs on every page.
pub fn assert_relocatable(
    root: &Path,
    min_pages: usize,
    min_attr_urls: usize,
    min_style_urls: usize,
) {
    let mut walked_pages = 0usize;
    let mut checked_attr_urls = 0usize;
    let mut checked_style_urls = 0usize;
    for (path, html) in html_pages(root) {
        let page_dir = path.parent().expect("page has a parent");
        walked_pages += 1;
        // One verdict, applied to both vocabularies; the counts stay apart.
        let resolves = |url: &String| match classify(url) {
            UrlKind::RootAbsolute => panic!(
                "{}: root-absolute URL {url:?} — the tree stops working \
                 the moment it is served from a subdirectory",
                path.display()
            ),
            UrlKind::External => false,
            UrlKind::Local => {
                let bare = url.split(['#', '?']).next().unwrap_or(url);
                if bare.is_empty() {
                    return false;
                }
                let target = page_dir.join(bare);
                assert!(
                    target.exists(),
                    "{}: {url:?} resolves to {}, which does not exist",
                    path.display(),
                    target.display()
                );
                true
            }
        };
        checked_attr_urls += html_urls(&html).iter().filter(|u| resolves(u)).count();
        checked_style_urls += style_urls(&html).iter().filter(|u| resolves(u)).count();
    }
    assert!(
        walked_pages >= min_pages,
        "expected at least {min_pages} pages under {}, walked {walked_pages}",
        root.display()
    );
    assert!(
        checked_attr_urls >= min_attr_urls,
        "expected at least {min_attr_urls} local attribute URLs to resolve, \
         checked {checked_attr_urls}"
    );
    assert!(
        checked_style_urls >= min_style_urls,
        "expected at least {min_style_urls} local stylesheet URLs to resolve, \
         checked {checked_style_urls}"
    );
}
