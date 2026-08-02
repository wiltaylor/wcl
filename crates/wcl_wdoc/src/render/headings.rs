//! Per-page heading post-processing.
//!
//! Heading blocks (`h1`..`h6`) lower to a `Content::Heading`, which the HTML
//! reading renders as `<hN class="heading-N">…</hN>` — a real heading tag,
//! with the class kept as the style hook `lib/css-classes.wcl` sizes it
//! through — carrying an id only when the author wrote one. This pass runs
//! over a page's rendered
//! body HTML to (a) synthesise a stable slug `id` on every heading (the
//! anchor target for cross-links and the right rail), (b) prepend the
//! `§ N.M` section-number marker on `h2`/`h3`, and (c) collect the `h2`/`h3`
//! list that drives the book's "on this page" rail. Operating on the final
//! HTML keeps it independent of the render pipeline — no state threading.

use std::collections::HashMap;
use std::fmt::Write as _;
use std::sync::OnceLock;

use regex::Regex;

/// One in-page heading, for the right-rail "on this page" nav.
pub(crate) struct PageHeading {
    pub level: u8,
    pub id: String,
    pub title: String,
    pub number: String,
}

/// Matches a rendered heading: the level digit off the tag, any extra
/// classes after the `heading-N` hook, an optional author id, then the
/// inner markup up to the closing tag. Headings never nest, so the lazy
/// body can't run past its own close. (The `regex` crate has no
/// backreferences, hence `</h[1-6]>` rather than a matched pair.)
fn heading_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r#"(?s)<h([1-6]) class="heading-[1-6]([^"]*)"( id="([^"]*)")?>(.*?)</h[1-6]>"#)
            .expect("valid heading regex")
    })
}

/// Strip tags and unescape the handful of entities the renderer emits, so a
/// heading's plain text is usable as a slug and a rail label.
fn plain_text(html: &str) -> String {
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

/// `Headings & hierarchy` → `headings-hierarchy`.
fn slugify(text: &str) -> String {
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
    let s = slug.trim_end_matches('-').to_string();
    if s.is_empty() {
        "section".to_string()
    } else {
        s
    }
}

/// Rewrite the page body: stamp heading ids + section markers, and return the
/// `h2`/`h3` list for the rail. Idempotent for headings that already carry an
/// id (it's kept). Levels 4-6 get an id but no marker / rail entry.
pub(crate) fn process_page_headings(content: &str) -> (String, Vec<PageHeading>) {
    let re = heading_re();
    let mut used: HashMap<String, u32> = HashMap::new();
    let mut headings = Vec::new();
    let (mut c2, mut c3) = (0u32, 0u32);
    let mut out = String::with_capacity(content.len());
    let mut last = 0;
    for cap in re.captures_iter(content) {
        let m = cap.get(0).expect("whole match");
        out.push_str(&content[last..m.start()]);
        last = m.end();

        let level: u8 = cap[1].parse().unwrap_or(2);
        let extra = &cap[2];
        let inner = &cap[5];
        let text = plain_text(inner);

        let id = match cap.get(4) {
            Some(existing) => existing.as_str().to_string(),
            None => {
                let base = slugify(&text);
                let n = used.entry(base.clone()).or_insert(0);
                *n += 1;
                if *n == 1 { base } else { format!("{base}-{n}") }
            }
        };

        let mut marker = String::new();
        if level == 2 || level == 3 {
            let number = if level == 2 {
                c2 += 1;
                c3 = 0;
                format!("{c2}")
            } else {
                if c2 == 0 {
                    c2 = 1;
                }
                c3 += 1;
                format!("{c2}.{c3}")
            };
            write!(marker, "<span class=\"heading-marker\">§ {number}</span>")
                .expect("write to String");
            headings.push(PageHeading {
                level,
                id: id.clone(),
                title: text,
                number,
            });
        }

        write!(
            out,
            "<h{level} class=\"heading-{level}{extra}\" id=\"{id}\">{marker}{inner}</h{level}>"
        )
        .expect("write to String");
    }
    out.push_str(&content[last..]);
    (out, headings)
}

/// Matches a footnote definition's id (`<li … id="fn-XXX">`).
fn footnote_def_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r#"id="fn-([a-zA-Z0-9_-]+)""#).expect("valid footnote regex"))
}

/// Link `[^id]` references to their `footnote` definitions. References are
/// numbered by definition order; each `[^id]` whose `id` is defined becomes a
/// superscript `<sup class="footnote-ref">` linking to the note. A `[^…]`
/// with no matching definition (e.g. a regex character class in a code
/// sample) is left untouched, so the feature never corrupts unrelated text.
pub(crate) fn process_footnotes(content: &str) -> String {
    let mut order: Vec<String> = Vec::new();
    for cap in footnote_def_re().captures_iter(content) {
        let id = cap[1].to_string();
        if !order.contains(&id) {
            order.push(id);
        }
    }
    if order.is_empty() {
        return content.to_string();
    }
    let mut out = content.to_string();
    for (i, id) in order.iter().enumerate() {
        let n = i + 1;
        let needle = format!("[^{id}]");
        let sup = format!(
            "<sup class=\"footnote-ref\" id=\"fnref-{id}\"><a href=\"#fn-{id}\">{n}</a></sup>"
        );
        out = out.replace(&needle, &sup);
    }
    out
}

/// Build the `list<OnPageHeading>` Value for the template context.
pub(crate) fn on_this_page_value(headings: &[PageHeading]) -> wcl_lang::Value {
    use wcl_lang::Value;
    Value::list(
        headings
            .iter()
            .map(|h| {
                let mut m = std::collections::BTreeMap::new();
                m.insert("level".to_string(), Value::I64(h.level as i64));
                m.insert("id".to_string(), Value::Utf8(h.id.clone()));
                m.insert("title".to_string(), Value::Utf8(h.title.clone()));
                m.insert("number".to_string(), Value::Utf8(h.number.clone()));
                Value::Record {
                    ty: vec!["OnPageHeading".to_string()],
                    fields: std::sync::Arc::new(m),
                }
            })
            .collect(),
    )
}
