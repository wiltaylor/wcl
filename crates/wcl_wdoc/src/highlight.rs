//! Syntax highlighting for `@block("code")`.
//!
//! Backed by syntect with the pure-Rust `fancy-regex` backend and
//! two-face's curated extra syntaxes (Rust, TOML, TypeScript,
//! Dockerfile, …). Adds the bundled `wcl.sublime-syntax` so the
//! site can highlight WCL itself.
//!
//! Token classes are emitted as `tok-<scope>` so the bundled
//! structured rules in `lib/highlight.wcl` (and user overrides) target stable selectors
//! like `.tok-keyword`, `.tok-string`, `.tok-comment`.

use std::sync::OnceLock;

use syntect::easy::HighlightLines;
use syntect::highlighting::{Theme, ThemeSet};
use syntect::html::{ClassStyle, ClassedHTMLGenerator};
use syntect::parsing::{SyntaxDefinition, SyntaxSet};
use syntect::util::LinesWithEndings;

/// WCL's own syntax definition, bundled because syntect ships none.
const WCL_SYNTAX: &str = include_str!("../assets/wcl.sublime-syntax");

/// Prefix of the token classes syntect mints — one per grammar scope, so
/// the vocabulary is as open-ended as the grammars themselves. Named here
/// because [`crate::css_lint`] exempts the family, and the exemption must
/// not drift from the generator.
pub(crate) const TOKEN_CLASS_PREFIX: &str = "tok-";

/// Prefix of the class naming a code block's language (`language-rust`).
/// Minted from the authored language name, so it is likewise open-ended;
/// see [`TOKEN_CLASS_PREFIX`].
pub(crate) const LANGUAGE_CLASS_PREFIX: &str = "language-";

/// The class naming a code block's language (`language-rust`). `language`
/// arrives already HTML-escaped where a caller escapes it.
pub(crate) fn language_class(language: &str) -> String {
    format!("{LANGUAGE_CLASS_PREFIX}{language}")
}

/// Prefix every emitted token class, so highlighting styles cannot
/// collide with a document's own class names.
const CLASS_STYLE: ClassStyle = ClassStyle::SpacedPrefixed {
    prefix: TOKEN_CLASS_PREFIX,
};

/// The syntax set, built once and reused — loading it is expensive
/// enough to matter on a book-sized document.
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
///
/// When `line_numbers` is set, each source line is wrapped in a
/// `<span class="code-line">` so a CSS counter can draw the gutter (the
/// book code-card). Each wrapped line is highlighted independently, so a
/// construct that spans lines (a block comment) restarts its scope per
/// line — acceptable for the short listings docs carry, and it keeps the
/// per-line HTML self-balanced.
pub(crate) fn highlight_html(source: &str, language: &str, line_numbers: bool) -> String {
    let ss = syntax_set();
    let syn = ss
        .find_syntax_by_token(language)
        .or_else(|| ss.find_syntax_by_name(language))
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    if !line_numbers {
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
        return out.finalize();
    }
    let mut out = String::new();
    for line in LinesWithEndings::from(source) {
        let mut g = ClassedHTMLGenerator::new_with_class_style(syn, ss, CLASS_STYLE);
        if g.parse_html_for_line_which_includes_newline(line).is_err() {
            break;
        }
        // `finalize` closes every span; the line's own trailing newline is
        // plain text at the very end, so trimming it is safe and lets the
        // `display:block` line wrapper own the break.
        out.push_str("<span class=\"code-line\">");
        out.push_str(g.finalize().trim_end_matches('\n'));
        out.push_str("</span>");
    }
    out
}

/// A light syntect theme for the PDF backend (which has no CSS to colour
/// `.tok-*` classes). `InspiredGitHub` reads well on the white page.
fn pdf_theme() -> &'static Theme {
    static THEME: OnceLock<Theme> = OnceLock::new();
    THEME.get_or_init(|| {
        let mut ts = ThemeSet::load_defaults();
        ts.themes
            .remove("InspiredGitHub")
            .or_else(|| ts.themes.values().next().cloned())
            .expect("syntect ships default themes")
    })
}

/// One highlighted source line: a sequence of `(text, rgb)` token runs.
pub(crate) type CodeLine = Vec<(String, (u8, u8, u8))>;

/// Highlight `source` into per-line runs of `(text, rgb)` for the PDF backend.
/// Each inner `Vec` is one source line; trailing newlines are stripped.
pub(crate) fn highlight_spans(source: &str, language: &str) -> Vec<CodeLine> {
    let ss = syntax_set();
    let syn = ss
        .find_syntax_by_token(language)
        .or_else(|| ss.find_syntax_by_name(language))
        .unwrap_or_else(|| ss.find_syntax_plain_text());
    let mut hl = HighlightLines::new(syn, pdf_theme());
    let mut lines = Vec::new();
    for line in LinesWithEndings::from(source) {
        let runs = hl.highlight_line(line, ss).unwrap_or_default();
        let spans: Vec<(String, (u8, u8, u8))> = runs
            .iter()
            .map(|(style, text)| {
                let c = style.foreground;
                (
                    text.trim_end_matches(['\n', '\r']).to_string(),
                    (c.r, c.g, c.b),
                )
            })
            .filter(|(t, _)| !t.is_empty())
            .collect();
        lines.push(spans);
    }
    lines
}
