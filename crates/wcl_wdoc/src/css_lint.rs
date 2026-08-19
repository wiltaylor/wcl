//! The class lint: after a build has rendered, diff the class names its
//! HTML actually carries against the class names its stylesheet selects —
//! both directions, at warning level.
//!
//! **It reads the output, not the source.** A class name reaches markup
//! three ways: a WCL `class:` field, a raw HTML string inside WCL, and
//! Rust-generated markup. The last two are permanent (template chrome and
//! `@native` blocks), so a source-side check would be blind to most uses.
//! Reading the rendered page sees all three, and sees interpolated names
//! (`format("level-{}", 3)`) already resolved. Symbols could not have
//! carried the check either: a WCL symbol cannot contain a hyphen and
//! every class name is hyphenated.
//!
//! **Two exemptions are structural, and without them the lint is worthless.**
//! Measured against a real build, the naive diff reported 178 names and not
//! one of them was a defect:
//!
//! - **Generator-emitted vocabularies** ([`GENERATED`]). A generator turns
//!   open-ended external data into class names — one per syntax-highlighting
//!   scope, one per code language. No stylesheet can ever declare that
//!   vocabulary, so no waiver list can ever be finished. It was 84% of the
//!   typo direction. Each generator names its family once, beside the code
//!   that mints it; the lint skips both directions for those names.
//! - **Library rules.** wdoc's own stylesheet ships the whole wdoc
//!   vocabulary, and any one document uses a fraction of it. A library rule
//!   that this document never exercises is not dead code — it is another
//!   document's rule. It was the whole dead-code direction. So only rules
//!   **the document itself authors** are judged unused; every rule, library
//!   or authored, still counts as *declaring* its names.
//!
//! What is left is a deliberate hook: a class emitted for a script or for
//! semantics that nothing styles. That is indistinguishable from a typo in
//! the output — nothing in the markup says which was meant — so it is said
//! in the source instead: **an empty `class "name" {}` block declares a name
//! on purpose.** It emits no CSS, and it silences the lint for that name.
//! wdoc's own hooks are declared that way in the stdlib beside the component
//! that emits them, because a hook the renderer stamps into every document
//! would otherwise warn in every document. A new one shows up on the repo's
//! own `just docs-build`, which is a gate part.
//!
//! The lint runs over the **union of all a document's sites**, never one
//! site's build: a document may declare several sites, one page set per
//! site, and a rule scoped to one site would otherwise read as dead in
//! every other. A partial build (`--site`, or the dev server's targeted
//! re-render) therefore does not lint at all.

use std::cell::RefCell;
use std::collections::BTreeSet;

/// The prefix of every generator-emitted class family — today, syntax
/// highlighting's one class per grammar scope and one per code language.
/// Each prefix is the constant the generator itself emits with, so a family
/// cannot drift from its generator. Adding a generator that mints class
/// names from open-ended data means adding its prefix here.
const GENERATED: &[&str] = &[
    crate::blocks::highlight::TOKEN_CLASS_PREFIX,
    crate::blocks::highlight::LANGUAGE_CLASS_PREFIX,
];

/// Whether `name` belongs to a generator's family.
fn is_generated(name: &str) -> bool {
    GENERATED.iter().any(|prefix| name.starts_with(prefix))
}

thread_local! {
    /// Classes a renderer used **structurally** during the current pass:
    /// it read the class's own fields and baked the result into the output
    /// instead of leaving the work to CSS. The wireframe and terminal
    /// renderers do this deliberately — their SVG is embedded in a PDF,
    /// where no stylesheet follows it — so the class is used, no element
    /// carries it, and only the renderer knows. Each consumer says so
    /// itself; a rule that reads its own consumption would have to know
    /// every consumer.
    static STRUCTURAL: RefCell<BTreeSet<String>> = const { RefCell::new(BTreeSet::new()) };
}

/// Record that a renderer resolved `name` and baked its fields into the
/// output. Only call this for a name that actually named a `class` block.
pub(crate) fn record_structural_use(name: &str) {
    STRUCTURAL.with(|slot| {
        let mut slot = slot.borrow_mut();
        if !slot.contains(name) {
            slot.insert(name.to_string());
        }
    });
}

/// Take and clear the structural uses recorded during the current pass.
pub(crate) fn take_structural_uses() -> BTreeSet<String> {
    STRUCTURAL.with(|slot| std::mem::take(&mut *slot.borrow_mut()))
}

/// The class-name evidence one build produced, accumulated as it renders:
/// what the markup carries, what any rule declares, and what the document's
/// own rules style. Recording is `&self` because the page loop reaches it
/// through a shared render context.
#[derive(Default)]
pub(crate) struct ClassScan {
    /// Class names on elements of the rendered pages.
    used: RefCell<BTreeSet<String>>,
    /// Class names any rule selects — library or authored, styled or
    /// deliberately empty. This is the "somebody meant this name" set.
    declared: RefCell<BTreeSet<String>>,
    /// Class names selected by a rule the document itself authors, and that
    /// actually emits CSS. Only these can be reported unused.
    authored: RefCell<BTreeSet<String>>,
}

impl ClassScan {
    /// Record the classes one rendered page carries.
    pub(crate) fn record_markup(&self, html: &str) {
        let mut used = self.used.borrow_mut();
        for name in markup_classes(html) {
            if !used.contains(name) {
                used.insert(name.to_string());
            }
        }
    }

    /// Record class names used otherwise than by appearing in markup —
    /// see [`record_structural_use`].
    pub(crate) fn record_uses(&self, names: impl IntoIterator<Item = String>) {
        self.used.borrow_mut().extend(names);
    }

    /// Record one site's stylesheet vocabulary: every name its rules select,
    /// and the subset the document's own rules style.
    pub(crate) fn record_rules(&self, declared: &BTreeSet<String>, authored: &BTreeSet<String>) {
        self.declared.borrow_mut().extend(declared.iter().cloned());
        self.authored.borrow_mut().extend(authored.iter().cloned());
    }

    /// The warnings this scan justifies, typo direction first, each
    /// direction alphabetical.
    pub(crate) fn findings(&self) -> Vec<String> {
        let used = self.used.borrow();
        let declared = self.declared.borrow();
        let authored = self.authored.borrow();

        let mut out = Vec::new();
        for name in used.difference(&declared) {
            if is_generated(name) {
                continue;
            }
            out.push(format!(
                "class \"{name}\": the rendered pages carry it but no CSS rule selects it \
                 — a misspelled class name, or a hook nothing styles; declare a hook with \
                 an empty `class \"{name}\" {{}}` block to say so on purpose"
            ));
        }
        for name in authored.difference(&used) {
            if is_generated(name) {
                continue;
            }
            out.push(format!(
                "class \"{name}\": this document styles it but no rendered page carries it \
                 — a misspelled selector, or a rule left behind; a class only ever applied \
                 by a script reads this way too"
            ));
        }
        out
    }
}

/// Every class name the `class` attributes of `html` carry.
///
/// A tag-aware scan rather than a search for `class="`: page text and code
/// samples are full of the string, and `<script>` / `<style>` bodies are not
/// markup at all. Both quote forms are accepted, because raw-HTML template
/// chrome is authored by hand.
fn markup_classes(html: &str) -> Vec<&str> {
    let bytes = html.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if html[i..].starts_with("<!--") {
            i = match html[i..].find("-->") {
                Some(end) => i + end + 3,
                None => break,
            };
            continue;
        }
        let name_start = i + 1;
        let name_end = name_start
            + html[name_start..]
                .find(|c: char| c.is_whitespace() || c == '>' || c == '/')
                .unwrap_or(html.len() - name_start);
        let tag = html[name_start..name_end].to_ascii_lowercase();
        let tag_end = match html[i..].find('>') {
            Some(end) => i + end + 1,
            None => break,
        };
        collect_class_attrs(&html[name_end..tag_end], &mut out);
        // A raw-text element's body is script or CSS, never markup.
        if matches!(tag.as_str(), "script" | "style") {
            let close = format!("</{tag}");
            i = match html[tag_end..].to_ascii_lowercase().find(&close) {
                Some(end) => tag_end + end,
                None => break,
            };
            continue;
        }
        i = tag_end;
    }
    out
}

/// Collect the values of every `class` attribute in one tag's attribute
/// text, split into names.
fn collect_class_attrs<'a>(attrs: &'a str, out: &mut Vec<&'a str>) {
    let mut rest = attrs;
    while let Some(at) = rest.find("class") {
        let after = &rest[at + "class".len()..];
        // `class` must be a whole attribute name: preceded by a separator
        // and followed by `=` (allowing space either side).
        let preceded_ok = rest[..at]
            .chars()
            .next_back()
            .is_none_or(|c| c.is_whitespace() || c == '"' || c == '\'');
        let value = after.trim_start();
        if !preceded_ok || !value.starts_with('=') {
            rest = after;
            continue;
        }
        let value = value[1..].trim_start();
        let Some(quote) = value.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            rest = after;
            continue;
        };
        let body = &value[1..];
        let Some(end) = body.find(quote) else {
            return;
        };
        out.extend(body[..end].split_ascii_whitespace());
        rest = &body[end + 1..];
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(names: &[&str]) -> BTreeSet<String> {
        names.iter().map(|s| s.to_string()).collect()
    }

    fn scan(used: &str, declared: &[&str], authored: &[&str]) -> ClassScan {
        let scan = ClassScan::default();
        scan.record_markup(used);
        scan.record_rules(&set(declared), &set(authored));
        scan
    }

    #[test]
    fn markup_scan_reads_both_quote_forms_and_splits_names() {
        let names = markup_classes("<div class=\"a b\"><span class='c'>x</span></div>");
        assert_eq!(names, vec!["a", "b", "c"]);
    }

    #[test]
    fn markup_scan_ignores_text_script_and_style_bodies() {
        let html = "<p>write class=\"prose\" in a sample</p>\
                    <script>el.innerHTML = '<i class=\"js\">';</script>\
                    <style>.real { content: 'class=\"css\"'; }</style>\
                    <b class=\"kept\">y</b>";
        assert_eq!(markup_classes(html), vec!["kept"]);
    }

    #[test]
    fn markup_scan_ignores_a_lookalike_attribute_name() {
        let html = "<div data-class=\"nope\" superclass=\"nope\" class=\"yes\">x</div>";
        assert_eq!(markup_classes(html), vec!["yes"]);
    }

    #[test]
    fn markup_scan_skips_comments() {
        assert_eq!(
            markup_classes("<!-- <div class=\"ghost\"> --><div class=\"real\">"),
            vec!["real"]
        );
    }

    #[test]
    fn a_used_class_no_rule_selects_is_reported() {
        let findings = scan("<p class=\"lp-titel\">x</p>", &["lp-title"], &["lp-title"]).findings();
        assert_eq!(findings.len(), 2, "{findings:?}");
        assert!(findings[0].contains("\"lp-titel\""));
        assert!(findings[0].contains("no CSS rule selects it"));
        assert!(findings[1].contains("\"lp-title\""));
        assert!(findings[1].contains("no rendered page carries it"));
    }

    #[test]
    fn an_empty_class_declaration_silences_a_hook() {
        // Declared but not authored-with-CSS: the empty `class "hook" {}` form.
        let findings = scan("<p class=\"hook\">x</p>", &["hook"], &[]).findings();
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn generator_vocabularies_are_exempt_in_both_directions() {
        let findings = scan(
            "<code class=\"language-rust\"><span class=\"tok-keyword\">fn</span></code>",
            &["tok-string"],
            &["tok-string"],
        )
        .findings();
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn an_unused_library_rule_is_not_dead_code() {
        // Declared by a library rule, styled by no rule of this document's.
        let findings = scan("<p class=\"used\">x</p>", &["used", "ws-hero"], &["used"]).findings();
        assert!(findings.is_empty(), "{findings:?}");
    }

    #[test]
    fn a_name_used_on_one_site_covers_a_rule_scoped_to_another() {
        // The union across sites: page markup from site A, rule from site B.
        let scan = ClassScan::default();
        scan.record_rules(
            &set(["deck-title"].as_slice()),
            &set(["deck-title"].as_slice()),
        );
        scan.record_markup("<h1 class=\"deck-title\">x</h1>");
        assert!(scan.findings().is_empty(), "{:?}", scan.findings());
    }
}
