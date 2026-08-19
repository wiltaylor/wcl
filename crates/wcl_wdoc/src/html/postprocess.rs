//! Page-wide HTML finishing that must span separate typed placements.
//!
//! Heading navigation metadata is derived from authored handles by the
//! `page_metadata` builtin. This pass only stamps the matching anchor ids into
//! emitted HTML. Nothing visible is injected into a heading: the section
//! numbering that `HeadingSequence::number` computes exists for the metadata
//! side alone, where it selects which levels reach a template's outline and
//! fills `OnPageHeading.number` for a template that chooses to render it.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::OnceLock;

use regex::Regex;

#[derive(Default)]
/// Running heading state for one page: the slugs already taken, and
/// the section counters that produce heading numbers.
pub(crate) struct HeadingSequence {
    /// Slug → how many times it has been claimed, so a repeat gets a
    /// suffix rather than a duplicate id.
    used: HashMap<String, u32>,
    /// Current level-2 section number.
    h2: u32,
    /// Current level-3 section number.
    h3: u32,
}

impl HeadingSequence {
    /// A unique element id for a heading: the author's when they gave
    /// one, else a slug of the title, suffixed if already taken.
    pub(crate) fn id(&mut self, title: &str, existing: Option<&str>) -> String {
        if let Some(existing) = existing {
            return existing.to_string();
        }
        let base = heading_slug(title);
        let count = self.used.entry(base.clone()).or_insert(0);
        *count += 1;
        if *count == 1 {
            base
        } else {
            format!("{base}-{count}")
        }
    }

    /// Advance the counters and return the section number for a heading
    /// at this level.
    pub(crate) fn number(&mut self, level: u8) -> Option<String> {
        match level {
            2 => {
                self.h2 += 1;
                self.h3 = 0;
                Some(self.h2.to_string())
            }
            3 => {
                if self.h2 == 0 {
                    self.h2 = 1;
                }
                self.h3 += 1;
                Some(format!("{}.{}", self.h2, self.h3))
            }
            _ => None,
        }
    }
}

/// Matches an emitted heading tag.
fn heading_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?s)<h([1-6]) class="heading-[1-6]([^"]*)"( id="([^"]*)")?>(.*?)</h[1-6]>"#)
            .expect("valid heading regex")
    })
}

/// Strip markup, leaving the text — the search index body and the
/// fallback page title.
pub(crate) fn plain_text(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for ch in html.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .trim()
        .to_string()
}

/// Slugify heading text into an id-safe form.
pub(crate) fn heading_slug(text: &str) -> String {
    let mut slug = String::with_capacity(text.len());
    let mut prev_dash = false;
    for ch in text.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash && !slug.is_empty() {
            slug.push('-');
            prev_dash = true;
        }
    }
    let slug = slug.trim_end_matches('-');
    if slug.is_empty() {
        "section".to_string()
    } else {
        slug.to_string()
    }
}

/// Rewrite a page's headings to carry unique ids and section numbers.
pub(crate) fn process_page_headings(content: &str, state: &mut HeadingSequence) -> String {
    let mut out = String::with_capacity(content.len());
    let mut last = 0;
    for cap in heading_re().captures_iter(content) {
        let whole = cap.get(0).expect("whole match");
        out.push_str(&content[last..whole.start()]);
        last = whole.end();

        let level: u8 = cap[1].parse().unwrap_or(2);
        let extra = &cap[2];
        let inner = &cap[5];
        let title = plain_text(inner);
        let id = state.id(&title, cap.get(4).map(|existing| existing.as_str()));

        write!(
            out,
            "<h{level} class=\"heading-{level}{extra}\" id=\"{id}\">{inner}</h{level}>"
        )
        .expect("write to String");
    }
    out.push_str(&content[last..]);
    out
}

/// Matches an emitted footnote definition.
fn footnote_def_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"id="fn-([a-zA-Z0-9_-]+)""#).expect("valid footnote regex"))
}

/// Collect footnote definitions and re-emit them together at the end
/// of the page, numbered in reference order.
pub(crate) fn process_footnotes(content: &str) -> String {
    let mut order = Vec::new();
    for cap in footnote_def_re().captures_iter(content) {
        let id = cap[1].to_string();
        if !order.contains(&id) {
            order.push(id);
        }
    }
    let mut out = content.to_string();
    for (i, id) in order.iter().enumerate() {
        let needle = format!("[^{id}]");
        let replacement = format!(
            "<sup class=\"footnote-ref\" id=\"fnref-{id}\"><a href=\"#fn-{id}\">{}</a></sup>",
            i + 1
        );
        out = out.replace(&needle, &replacement);
    }
    out
}
