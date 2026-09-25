#![no_main]

use std::path::PathBuf;
use std::sync::OnceLock;

use libfuzzer_sys::fuzz_target;

/// One scratch directory for the whole run: the input is written to
/// `doc.wcl` inside it and the site is built into `out/`, both overwritten
/// on every iteration. It lives for the process and is removed on a clean
/// exit.
fn scratch() -> &'static PathBuf {
    static DIR: OnceLock<tempfile::TempDir> = OnceLock::new();
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        DIR.get_or_init(|| tempfile::tempdir().expect("fuzz scratch dir"))
            .path()
            .to_path_buf()
    })
}

// Build the input as a wdoc site: parse, evaluate, lower, lay out, and
// render HTML. The stdlib import is prepended so the bytes fuzz the page
// vocabulary rather than spending themselves on the import line. Any
// `BuildError` is an expected outcome; only a panic, a hang or an abort is
// a finding.
fuzz_target!(|data: &[u8]| {
    let Ok(body) = std::str::from_utf8(data) else {
        return;
    };
    let dir = scratch();
    let src = dir.join("doc.wcl");
    if std::fs::write(&src, format!("import <wdoc.wcl>\n{body}")).is_err() {
        return;
    }
    let _ = wcl_wdoc::build(&src, &dir.join("out"), None);
    let _ = wcl_wdoc::take_render_warnings();
});
