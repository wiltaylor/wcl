//! Lowering for wdoc's typed CSS structure: class-rooted rules (including
//! nesting), base selectors, font faces, media queries, and keyframes.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::Path;

use miette::Report;
use wcl_lang::{Block, Document};

use super::*;

/// One rendered chunk of CSS: the text a page embeds, plus every class name
/// its selectors target. The two travel together because the class lint
/// ([`crate::css_lint`]) needs the names this crate *generated* — reading
/// them back out of the finished stylesheet would mean parsing CSS, and a
/// declaration body is opaque text that may hold a `.` of its own.
///
/// `text` is empty for a rule that emits nothing: an empty `class "name" {}`
/// still declares its name, which is how an author says a hook is deliberate.
#[derive(Default)]
pub(crate) struct RenderedCss {
    /// The rendered CSS.
    pub(crate) text: String,
    /// Class names these rules select.
    pub(crate) classes: BTreeSet<String>,
}

impl RenderedCss {
    /// Pair rendered CSS with the classes it selects.
    fn of(text: String, classes: BTreeSet<String>) -> Self {
        RenderedCss { text, classes }
    }

    /// Fold another chunk in, joining the texts with a newline.
    fn absorb(&mut self, other: RenderedCss) {
        if !other.text.is_empty() {
            push_rule_separator(&mut self.text);
            self.text.push_str(&other.text);
        }
        self.classes.extend(other.classes);
    }
}

/// The class names a selector targets. Quoted text is skipped, so the `.`
/// inside `[data-x=".foo"]` or `content: "…"` is not a class; everything
/// else — descendant, compound, and functional-pseudo-class positions —
/// counts, because a rule that mentions a name depends on that name.
pub(crate) fn selector_classes(selector: &str) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut scan = SelectorScan::default();
    for (index, ch) in selector.char_indices() {
        // A name's own characters are inert to the scan, so reading them
        // twice (once here, once as the next chars) changes nothing.
        if !scan.observe(ch) || ch != '.' {
            continue;
        }
        let tail = &selector[index + 1..];
        let end = tail
            .find(|c: char| !(c.is_ascii_alphanumeric() || c == '-' || c == '_'))
            .unwrap_or(tail.len());
        if end > 0 {
            out.insert(tail[..end].to_string());
        }
    }
    out
}

/// Emit a CSS rule body for a `@block("class")` instance.
/// Returns `None` if the block doesn't have an inline name.
/// Build the CSS declaration string for one styling block (a `class`
/// or one of its `light {}` / `dark {}` mode blocks — they share field
/// names). Empty when no styling fields are set.
pub(crate) fn class_props(block: &Block<'_>) -> String {
    let mut props = String::new();
    // SVG painting — themes diagram shapes and chart series (their
    // `class`es reach the `<rect>` / `<line>` / `<polygon>` elements).
    push_css(&mut props, "fill", field_utf8(block, "fill").as_deref());
    push_css(&mut props, "stroke", field_utf8(block, "stroke").as_deref());
    push_css(
        &mut props,
        "stroke-width",
        field_utf8(block, "stroke_width").as_deref(),
    );
    push_css(
        &mut props,
        "opacity",
        field_utf8(block, "opacity").as_deref(),
    );
    // Callout accent — emits the `--callout-accent` custom property, so a
    // class on a `callout` themes its heading / border / icon. A user
    // class rule is emitted last in the cascade, so it overrides the
    // `.callout { --callout-accent: … }` default for a custom callout type.
    push_css(
        &mut props,
        "--callout-accent",
        field_utf8(block, "accent").as_deref(),
    );
    if let Some(css) = field_utf8(block, "css") {
        props.push_str(&css);
    }
    props
}

/// Emit the CSS rule(s) for a `@block("class")`. The class's own
/// fields are shared defaults; optional `dark {}` / `light {}` mode
/// blocks add per-mode overrides. `dark` is the default mode; `light`
/// applies under `prefers-color-scheme: light`; an explicit
/// `:root[data-theme=…]` (set by the theme toggle) overrides both.
pub(crate) fn render_class(block: &Block<'_>) -> Option<RenderedCss> {
    let name = label_string(block)?;
    let mut classes = BTreeSet::from([name.clone()]);
    let base = class_props(block);
    let dark = block.block("dark").map(|b| class_props(&b));
    let light = block.block("light").map(|b| class_props(&b));

    // Default-mode rule: shared fields, with the dark mode merged in
    // (dark is the default) so a later same-specificity declaration
    // wins for overlapping properties.
    let mut default_props = base.clone();
    if let Some(d) = &dark {
        default_props.push_str(d);
    }
    let mut out = String::new();
    if !default_props.is_empty() {
        out = format!(".{name} {{ {default_props} }}");
    }

    if let Some(l) = &light
        && !l.is_empty()
    {
        push_rule_separator(&mut out);
        write!(
            out,
            "@media (prefers-color-scheme: light) {{ .{name} {{ {l} }} }}"
        )
        .expect("write to String");
    }
    // Explicit toggle (`:root[data-theme=…]`) overrides the system
    // preference. Emit BOTH sides whenever the class is themed (declares
    // a `dark` and/or `light` block) — including the side that has no
    // block of its own, which falls back to the base. This is what makes
    // the toggle actually switch: a class that only declares `light {}`
    // still needs a `data-theme="dark"` rule, otherwise on a light-
    // preferring system the `@media (prefers-color-scheme: light)` rule
    // (same specificity as the base) keeps winning and toggling to dark
    // does nothing. The data-theme selector's higher specificity beats
    // the media rule, so the toggle wins on either side. (`dark` is the
    // default, so its rule carries the base alone when no `dark {}` is
    // declared.)
    if dark.is_some() || light.is_some() {
        let d = dark.as_deref().unwrap_or("");
        let l = light.as_deref().unwrap_or("");
        if !base.is_empty() || !d.is_empty() {
            push_rule_separator(&mut out);
            write!(out, ":root[data-theme=\"dark\"] .{name} {{ {base}{d} }}")
                .expect("write to String");
        }
        if !base.is_empty() || !l.is_empty() {
            push_rule_separator(&mut out);
            write!(out, ":root[data-theme=\"light\"] .{name} {{ {base}{l} }}")
                .expect("write to String");
        }
    }
    for nest in block.blocks().filter(|child| child.kind() == "nest") {
        let fragment = label_string(&nest)?;
        let selector = nested_selectors(&format!(".{name}"), &fragment);
        classes.extend(selector_classes(&selector));
        let css = field_utf8(&nest, "css")?;
        push_rule_separator(&mut out);
        write!(out, "{selector} {{ {css} }}").expect("write to String");
    }
    // An empty `class "name" {}` emits nothing and still declares its name:
    // that is how an author says an unstyled hook is deliberate.
    Some(RenderedCss::of(out, classes))
}

/// Append a newline between rules, but not before the first.
fn push_rule_separator(out: &mut String) {
    if !out.is_empty() {
        out.push('\n');
    }
}

/// Apply `parent` independently to each top-level selector-list branch.
/// Commas inside functional pseudo-classes, attribute selectors, or quoted
/// values stay inside their branch.
fn nested_selectors(parent: &str, fragment: &str) -> String {
    let mut branches = Vec::new();
    let mut start = 0;
    let mut scan = SelectorScan::default();

    for (index, ch) in fragment.char_indices() {
        if scan.observe(ch) && ch == ',' && scan.at_top_level() {
            branches.push(fragment[start..index].trim());
            start = index + ch.len_utf8();
        }
    }
    branches.push(fragment[start..].trim());

    branches
        .into_iter()
        .map(|branch| expand_selector_branch(parent, branch))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Replace active parent references while leaving ampersands in quoted,
/// escaped, or attribute-selector text alone.
fn expand_selector_branch(parent: &str, branch: &str) -> String {
    let mut out = String::with_capacity(branch.len() + parent.len());
    let mut scan = SelectorScan::default();
    let mut replaced_parent = false;

    for ch in branch.chars() {
        if scan.observe(ch) && ch == '&' && scan.outside_attribute() {
            out.push_str(parent);
            replaced_parent = true;
        } else {
            out.push(ch);
        }
    }

    if replaced_parent {
        out
    } else if branch.is_empty() {
        parent.to_string()
    } else {
        format!("{parent} {branch}")
    }
}

#[derive(Default)]
/// Nesting state for a left-to-right scan of a selector, so a comma
/// inside brackets or quotes is not mistaken for a selector break.
struct SelectorScan {
    /// Open parenthesis depth.
    parentheses: u32,
    /// Open square-bracket depth.
    brackets: u32,
    /// The quote character currently open, if any.
    quote: Option<char>,
    /// Whether the previous character was a backslash.
    escaped: bool,
}

impl SelectorScan {
    /// Advance the shared selector lexical state. Returns whether `ch` is
    /// active selector syntax rather than quoted or escaped text.
    fn observe(&mut self, ch: char) -> bool {
        if self.escaped {
            self.escaped = false;
            return false;
        }
        if ch == '\\' {
            self.escaped = true;
            return false;
        }
        if let Some(delimiter) = self.quote {
            if ch == delimiter {
                self.quote = None;
            }
            return false;
        }
        match ch {
            '\'' | '"' => {
                self.quote = Some(ch);
                false
            }
            '(' => {
                self.parentheses += 1;
                true
            }
            ')' => {
                self.parentheses = self.parentheses.saturating_sub(1);
                true
            }
            '[' => {
                self.brackets += 1;
                true
            }
            ']' => {
                self.brackets = self.brackets.saturating_sub(1);
                true
            }
            _ => true,
        }
    }

    /// Whether the scan is outside every bracket and quote.
    fn at_top_level(&self) -> bool {
        self.parentheses == 0 && self.brackets == 0
    }

    /// Whether the scan is outside an attribute selector.
    fn outside_attribute(&self) -> bool {
        self.brackets == 0
    }
}

/// Emit a selector and opaque declaration body for a `base` block.
pub(crate) fn render_base(block: &Block<'_>) -> Option<RenderedCss> {
    let selector = label_string(block)?;
    let css = field_utf8(block, "css")?;
    Some(RenderedCss::of(
        format!("{selector} {{ {css} }}"),
        selector_classes(&selector),
    ))
}

/// Emit a typed `@font-face` block.
pub(crate) fn render_font_face(block: &Block<'_>) -> Option<String> {
    let family = label_string(block)?;
    let src = field_utf8(block, "src")?;
    let mut props = format!("font-family: {family};");
    push_spaced_css(
        &mut props,
        "font-weight",
        field_utf8(block, "weight").as_deref(),
    );
    push_spaced_css(
        &mut props,
        "font-style",
        field_utf8(block, "style").as_deref(),
    );
    push_spaced_css(
        &mut props,
        "font-display",
        field_utf8(block, "display").as_deref(),
    );
    push_spaced_css(&mut props, "src", Some(&src));
    Some(format!("@font-face {{ {props} }}"))
}

/// Emit a media query containing class-rooted rules.
pub(crate) fn render_media(block: &Block<'_>) -> Option<RenderedCss> {
    let query = label_string(block)?;
    let mut inner = RenderedCss::default();
    for child in block.blocks() {
        if let Some(rule) = render_css_block(&child) {
            inner.absorb(rule);
        }
    }
    Some(RenderedCss::of(
        format!("@media {query} {{ {} }}", inner.text),
        inner.classes,
    ))
}

/// Emit a named keyframes rule whose frame selectors are `base` children.
/// Frame selectors are `from` / `to` / percentages, so they carry no class
/// names — but they render through the same path, so any that did would be
/// collected.
pub(crate) fn render_keyframes(block: &Block<'_>) -> Option<RenderedCss> {
    let name = label_string(block)?;
    let mut inner = RenderedCss::default();
    for child in block.blocks().filter(|child| child.kind() == "base") {
        if let Some(frame) = render_base(&child) {
            inner.absorb(frame);
        }
    }
    Some(RenderedCss::of(
        format!("@keyframes {name} {{ {} }}", inner.text),
        inner.classes,
    ))
}

/// Render one structured CSS block. Containers call this after they have
/// already decided placement and site visibility.
pub(crate) fn render_css_block(block: &Block<'_>) -> Option<RenderedCss> {
    match block.kind() {
        "class" => render_class(block),
        "base" => render_base(block),
        "font_face" => render_font_face(block).map(|text| RenderedCss::of(text, BTreeSet::new())),
        "media" => render_media(block),
        "keyframes" => render_keyframes(block),
        _ => None,
    }
}

/// Render every named style once for reuse across a site's page loop.
pub(crate) fn render_styles(doc: &Document) -> BTreeMap<String, RenderedCss> {
    doc.blocks()
        .filter(|block| block.kind() == "style")
        .filter_map(|style| Some((label_string(&style)?, render_style(&style))))
        .collect()
}

/// Render one named `style` bundle. The build reads the same function for
/// the bundle's class vocabulary, so the two cannot walk it differently.
pub(crate) fn render_style(style: &Block<'_>) -> RenderedCss {
    let mut out = RenderedCss::default();
    for block in style.blocks() {
        if let Some(rule) = render_css_block(&block) {
            out.absorb(rule);
        }
    }
    out
}

/// Reject a `//` line comment inside a declaration body.
///
/// `//` is not CSS. A browser parses the rest of the declaration as garbage
/// and drops it, which is the failure the old CSS heredocs were shouted
/// about: one stray `//` silently swallowed everything after it. Per-rule
/// declaration strings shrank the blast radius to a single rule; rejecting
/// the comment closes it. `//` inside a quoted string or an unquoted
/// `url(…)` is a URL, not a comment, and passes.
pub(crate) fn comment_errors(doc: &Document) -> Vec<Report> {
    let mut out = Vec::new();
    for (origin, block) in doc.blocks_with_source() {
        check_comments(&block, origin, &mut out);
    }
    out
}

/// Walk one top-level block's CSS subtree. Only the container kinds are
/// descended into, so this never wanders off into page content. It reads
/// the blocks a document declares: a `css` a generator computes is a
/// string built at eval time, not a declaration body someone typed.
fn check_comments(block: &Block<'_>, origin: Option<&Path>, out: &mut Vec<Report>) {
    if matches!(block.kind(), "class" | "base" | "nest" | "dark" | "light")
        && let Some(css) = field_utf8(block, "css")
        && has_line_comment(&css)
    {
        let what = label_string(block).unwrap_or_else(|| block.kind().to_string());
        let file = origin.map_or_else(
            || "this document".to_string(),
            |path| path.display().to_string(),
        );
        out.push(miette::miette!(
            code = "wdoc::css_line_comment",
            "{} \"{what}\" in {file}: the `css` declaration contains a `//` line comment \
             — CSS has no line comments, so a browser discards the rest of the rule; use \
             `/* … */` or delete it",
            block.kind(),
        ));
    }
    if matches!(
        block.kind(),
        "style" | "media" | "keyframes" | "class" | "font_face"
    ) {
        for child in block.blocks() {
            check_comments(&child, origin, out);
        }
    }
}

/// Whether `css` holds a `//` outside a quoted string and outside an
/// unquoted `url(…)`.
fn has_line_comment(css: &str) -> bool {
    let bytes = css.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' | b'\'' => {
                i = skip_quoted(bytes, i);
                continue;
            }
            b'/' if bytes.get(i + 1) == Some(&b'/') => return true,
            _ => {}
        }
        // Byte-wise, because a declaration body is arbitrary UTF-8 and a
        // slice by index would land mid-character.
        if bytes[i..]
            .get(..4)
            .is_some_and(|head| head.eq_ignore_ascii_case(b"url("))
        {
            i = skip_url(bytes, i + 4);
            continue;
        }
        i += 1;
    }
    false
}

/// Index just past the string starting at `open`, honouring backslash
/// escapes. An unterminated string ends the scan.
fn skip_quoted(bytes: &[u8], open: usize) -> usize {
    let quote = bytes[open];
    let mut i = open + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b if b == quote => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// Index just past the `url(…)` whose contents start at `start`.
fn skip_url(bytes: &[u8], start: usize) -> usize {
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'"' | b'\'' => i = skip_quoted(bytes, i),
            b')' => return i + 1,
            _ => i += 1,
        }
    }
    bytes.len()
}

/// Append `prop: value;` when the value is present; a no-op otherwise.
pub(crate) fn push_css(out: &mut String, prop: &str, value: Option<&str>) {
    if let Some(v) = value {
        write!(out, "{prop}:{v};").expect("write to String");
    }
}

/// Like [`push_css`], with a leading space for inline style
/// attributes.
fn push_spaced_css(out: &mut String, prop: &str, value: Option<&str>) {
    if let Some(value) = value {
        write!(out, " {prop}: {value};").expect("write to String");
    }
}

#[cfg(test)]
mod tests {
    use super::{has_line_comment, selector_classes};

    fn classes(selector: &str) -> Vec<String> {
        selector_classes(selector).into_iter().collect()
    }

    #[test]
    fn selector_classes_reads_every_position_a_name_appears_in() {
        assert_eq!(classes(".card .title"), ["card", "title"]);
        assert_eq!(classes(".card:is(.wide, .tall)"), ["card", "tall", "wide"]);
        assert_eq!(classes("a.link::before"), ["link"]);
    }

    #[test]
    fn selector_classes_skips_quoted_text_and_bare_dots() {
        // The `.` inside a quoted attribute value is not a class, and a `.`
        // that starts no name (a decimal, a stray) yields nothing.
        assert_eq!(classes("[data-x=\".ghost\"].real"), ["real"]);
        assert!(classes("li::marker").is_empty());
    }

    #[test]
    fn a_line_comment_is_found_only_outside_strings_and_urls() {
        assert!(has_line_comment("color: red; // muted"));
        assert!(has_line_comment("// the whole body"));
        // Both URL forms carry a scheme separator, not a comment.
        assert!(!has_line_comment(
            "background: url(https://a.example/b.png);"
        ));
        assert!(!has_line_comment(
            "background: url('https://a.example/b.png');"
        ));
        assert!(!has_line_comment("content: \"https://a.example\";"));
        // An escaped quote keeps the string open past the `//` inside it.
        assert!(!has_line_comment(r#"content: "a\"//b";"#));
        // An unterminated string ends the scan rather than looping.
        assert!(!has_line_comment("content: \"unclosed // "));
        assert!(!has_line_comment("border-radius: 4px;"));
    }
}
