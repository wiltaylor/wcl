//! Lowering for wdoc's typed CSS structure: class-rooted rules (including
//! nesting), base selectors, font faces, media queries, and keyframes.

use std::fmt::Write as _;

use wcl_lang::Block;

use super::*;

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
pub(crate) fn render_class(block: &Block<'_>) -> Option<String> {
    let name = label_string(block)?;
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
        let css = field_utf8(&nest, "css")?;
        push_rule_separator(&mut out);
        write!(out, "{selector} {{ {css} }}").expect("write to String");
    }
    (!out.is_empty()).then_some(out)
}

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
struct SelectorScan {
    parentheses: u32,
    brackets: u32,
    quote: Option<char>,
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

    fn at_top_level(&self) -> bool {
        self.parentheses == 0 && self.brackets == 0
    }

    fn outside_attribute(&self) -> bool {
        self.brackets == 0
    }
}

/// Emit a selector and opaque declaration body for a `base` block.
pub(crate) fn render_base(block: &Block<'_>) -> Option<String> {
    let selector = label_string(block)?;
    let css = field_utf8(block, "css")?;
    Some(format!("{selector} {{ {css} }}"))
}

/// Emit a typed `@font-face` block.
pub(crate) fn render_font_face(block: &Block<'_>) -> Option<String> {
    let family = label_string(block)?;
    let src = field_utf8(block, "src")?;
    let mut props = String::new();
    push_css(&mut props, "font-family", Some(&family));
    push_css(&mut props, "src", Some(&src));
    push_css(
        &mut props,
        "font-weight",
        field_utf8(block, "weight").as_deref(),
    );
    push_css(
        &mut props,
        "font-style",
        field_utf8(block, "style").as_deref(),
    );
    push_css(
        &mut props,
        "font-display",
        field_utf8(block, "display").as_deref(),
    );
    Some(format!("@font-face {{ {props} }}"))
}

/// Emit a media query containing class-rooted rules.
pub(crate) fn render_media(block: &Block<'_>) -> Option<String> {
    let query = label_string(block)?;
    let rules = block
        .blocks()
        .filter(|child| child.kind() == "class")
        .filter_map(|child| render_class(&child))
        .collect::<Vec<_>>()
        .join("\n");
    Some(format!("@media {query} {{ {rules} }}"))
}

/// Emit a named keyframes rule whose frame selectors are `base` children.
pub(crate) fn render_keyframes(block: &Block<'_>) -> Option<String> {
    let name = label_string(block)?;
    let frames = block
        .blocks()
        .filter(|child| child.kind() == "base")
        .filter_map(|child| render_base(&child))
        .collect::<Vec<_>>()
        .join("\n");
    Some(format!("@keyframes {name} {{ {frames} }}"))
}

pub(crate) fn push_css(out: &mut String, prop: &str, value: Option<&str>) {
    if let Some(v) = value {
        write!(out, "{prop}:{v};").expect("write to String");
    }
}
