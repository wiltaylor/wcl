//! Syntax highlighting for `@block("code")`.
//!
//! Backed by syntect with the pure-Rust `fancy-regex` backend and
//! two-face's curated extra syntaxes (Rust, TOML, TypeScript,
//! Dockerfile, …). Adds the bundled `wcl.sublime-syntax` so the
//! site can highlight WCL itself.
//!
//! Token classes are emitted as `tok-<scope>` so the bundled
//! `code-theme.css` (and user overrides) target stable selectors
//! like `.tok-keyword`, `.tok-string`, `.tok-comment`.

use std::sync::OnceLock;

use syntect::html::{ClassStyle, ClassedHTMLGenerator};
use syntect::parsing::{SyntaxDefinition, SyntaxSet};
use syntect::util::LinesWithEndings;

const WCL_SYNTAX: &str = include_str!("../assets/wcl.sublime-syntax");
const THEME_CSS: &str = include_str!("../assets/code-theme.css");

const CLASS_STYLE: ClassStyle = ClassStyle::SpacedPrefixed { prefix: "tok-" };

fn syntax_set() -> &'static SyntaxSet {
    static SET: OnceLock<SyntaxSet> = OnceLock::new();
    SET.get_or_init(|| {
        let mut builder = two_face::syntax::extra_newlines().into_builder();
        // The bundled WCL grammar. If it ever fails to load we'd
        // rather degrade to "no WCL highlighting" than panic the
        // whole build, so swallow the error and continue.
        if let Ok(def) = SyntaxDefinition::load_from_str(WCL_SYNTAX, true, Some("source.wcl")) {
            builder.add(def);
        }
        builder.build()
    })
}

/// Highlight `source` for `language` and return the inner HTML to
/// drop inside `<pre><code>…</code></pre>` — a stream of nested
/// `<span class="tok-…">` elements. Unknown languages fall back to
/// plain-text rendering (no token classes; text is still escaped).
pub(crate) fn highlight_html(source: &str, language: &str) -> String {
    let ss = syntax_set();
    let syn = ss
        .find_syntax_by_token(language)
        .or_else(|| ss.find_syntax_by_name(language))
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let mut out = ClassedHTMLGenerator::new_with_class_style(syn, ss, CLASS_STYLE);
    for line in LinesWithEndings::from(source) {
        // The only error this returns is regex failure inside a
        // grammar; treat it as "stop here" rather than panicking.
        if out
            .parse_html_for_line_which_includes_newline(line)
            .is_err()
        {
            break;
        }
    }
    out.finalize()
}

/// Bundled CSS for `.code-block` containers and `.tok-*` token
/// classes. Injected into every rendered page's `<style>` block
/// alongside the per-document class rules.
pub(crate) fn theme_css() -> &'static str {
    THEME_CSS
}
