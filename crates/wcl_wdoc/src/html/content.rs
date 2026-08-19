//! The HTML backend's reading of the semantic content IR.
//!
//! One arm per [`Content`] variant, matched **exhaustively**: the union is
//! declared once in `lib/content.wcl`, so a concept added there stops this
//! file compiling rather than quietly rendering nothing. That is the whole
//! mechanism — the three sibling walkers ([`crate::pdf::content`],
//! [`crate::markdown::content`]) obey the same rule, which is what keeps
//! the backends from diverging the way they did while every walker ended
//! in a catch-all arm.
//!
//! Prose fields (`text`, a callout's `heading`, a table cell) run through
//! the shared inline-pattern engine here, exactly as the block-side
//! renderers do — the IR carries prose, not markup.

use std::fmt::Write as _;

use wcl_lang::Document;

use crate::content::{
    CalloutKind, Content, ContentFootnote, ContentTocEntry, ListStyle, chapter_meta_line,
};
use crate::inline::InlinePatterns;
use crate::render::*;
use crate::svg::*;

/// Render one content node to HTML.
pub(crate) fn render_content(doc: &Document, node: &Content, patterns: &InlinePatterns) -> String {
    match node {
        Content::Heading {
            level,
            text,
            id,
            class,
        } => {
            // A real `<h{level}>`, not a `<p>` whose level rides in a class
            // — but the theme's heading rules key on `.heading-N`, so the
            // level is *also* emitted as a style hook. Derived from the
            // number, never parsed back out of it.
            let level = (*level).clamp(1, 6);
            let hook = format!("heading-{level}");
            let mut out = format!("<h{level}{}", classes_attr(&[&hook], class));
            append_attr(&mut out, "id", id.as_deref());
            write!(out, ">{}</h{level}>", patterns.render(doc, text)).expect("write to String");
            out
        }
        Content::Paragraph { text, id, class } => {
            let mut out = format!("<p{}", classes_attr(&[], class));
            append_attr(&mut out, "id", id.as_deref());
            write!(out, ">{}</p>", patterns.render(doc, text)).expect("write to String");
            out
        }
        Content::List {
            items,
            style,
            start,
            id,
            class,
        } => {
            let numbered = matches!(style, Some(ListStyle::Numbered));
            let base: &[&str] = if numbered {
                &["wdoc-list-numbered"]
            } else {
                &[]
            };
            let tag = if numbered { "ol" } else { "ul" };
            let mut out = format!("<{tag}{}", classes_attr(base, class));
            append_attr(&mut out, "id", id.as_deref());
            if let Some(start) = start {
                write!(out, " start=\"{start}\"").expect("write to String");
            }
            out.push('>');
            for item in items {
                write!(out, "<li>{}", patterns.render(doc, &item.text)).expect("write to String");
                for child in item.blocks.iter().flatten() {
                    out.push_str(&render_content(doc, child, patterns));
                }
                out.push_str("</li>");
            }
            write!(out, "</{tag}>").expect("write to String");
            out
        }
        Content::Table {
            rows,
            header,
            caption,
            id,
            class,
        } => {
            let mut out = format!("<table{}", classes_attr(&["wdoc-table"], class));
            append_attr(&mut out, "id", id.as_deref());
            out.push('>');
            if let Some(caption) = caption {
                write!(out, "<caption>{}</caption>", patterns.render(doc, caption))
                    .expect("write to String");
            }
            if let Some(header) = header.as_ref().filter(|h| !h.is_empty()) {
                out.push_str("<thead><tr>");
                for cell in header {
                    write!(out, "<th>{}</th>", patterns.render(doc, cell))
                        .expect("write to String");
                }
                out.push_str("</tr></thead>");
            }
            out.push_str("<tbody>");
            for row in rows {
                out.push_str("<tr>");
                for cell in row {
                    write!(out, "<td>{}</td>", patterns.render(doc, cell))
                        .expect("write to String");
                }
                out.push_str("</tr>");
            }
            out.push_str("</tbody></table>");
            out
        }
        Content::Code {
            source,
            language,
            filename,
            id,
            class,
        } => {
            // The book code-card: window dots + optional filename + the
            // language tag over the highlighted listing. Same markup the
            // `code` block's WCL lowering builds today.
            let language = language.clone().unwrap_or_default();
            let name = filename
                .as_ref()
                .map(|f| format!("<span class=\"code-name\">{}</span>", escape_html(f)))
                .unwrap_or_default();
            let mut out = format!(
                "<figure class=\"code-card\"><div class=\"code-filename\">\
                 <span class=\"code-dots\"><span></span><span></span><span></span></span>\
                 {name}<span class=\"code-lang\">{}</span></div><pre{}",
                escape_html(&language),
                classes_attr(&["code-block"], class),
            );
            append_attr(&mut out, "id", id.as_deref());
            write!(
                out,
                "><code class=\"{}\">{}</code></pre></figure>",
                crate::highlight::language_class(&escape_html(&language)),
                crate::highlight::highlight_html(source, &language, true),
            )
            .expect("write to String");
            out
        }
        Content::Callout {
            kind,
            heading,
            body,
            icon,
            id,
            class,
        } => {
            // The accent rides the `.callout.<kind>` CSS rules, so the kind
            // is emitted as a class alongside any the author supplied; the
            // icon defaults per kind and is overridden by `icon`.
            let mut base: Vec<&str> = vec!["callout"];
            if let Some(kind) = kind {
                base.push(kind.as_wcl());
            }
            let icon_html = icon
                .clone()
                .or_else(|| kind.map(|k| default_icon(k).to_string()))
                .filter(|name| !name.is_empty())
                .and_then(|name| {
                    patterns
                        .icons()
                        .resolve_html_icon(&name, &["callout-icon".to_string()])
                })
                .unwrap_or_default();
            let body: String = body
                .iter()
                .map(|node| render_content(doc, node, patterns))
                .collect();
            let mut out = format!("<div{}", classes_attr(&base, class));
            append_attr(&mut out, "id", id.as_deref());
            write!(
                out,
                "><div class=\"callout-heading\">{icon_html}\
                 <p class=\"callout-title\"><span>{}</span></p></div>\
                 <div class=\"callout-body\">{body}</div></div>",
                patterns.render(doc, heading),
            )
            .expect("write to String");
            out
        }
        Content::Columns { columns, id, class } => {
            let widths = vec!["1fr"; columns.len()].join(" ");
            let mut out = format!("<div{}", classes_attr(&["wdoc-columns"], class));
            append_attr(&mut out, "id", id.as_deref());
            write!(
                out,
                " style=\"display:grid;grid-template-columns:{widths};\">"
            )
            .expect("write to String");
            for column in columns {
                out.push_str("<div class=\"wdoc-column\">");
                for node in column {
                    out.push_str(&render_content(doc, node, patterns));
                }
                out.push_str("</div>");
            }
            out.push_str("</div>");
            out
        }
        Content::Image {
            source,
            alt,
            caption,
            width,
            height,
            id,
            class,
        } => {
            // Registering the source is what copies the asset into the
            // output and rewrites the URL — an IR image is an asset like
            // any other.
            let url = patterns.images().register(source).url;
            let mut img = format!(
                "<img{} src=\"{}\"",
                classes_attr(&["wdoc-image"], class),
                escape_html(&url)
            );
            append_attr(&mut img, "alt", alt.as_deref());
            if let Some(w) = width {
                write!(img, " width=\"{w}\"").expect("write to String");
            }
            if let Some(h) = height {
                write!(img, " height=\"{h}\"").expect("write to String");
            }
            append_attr(&mut img, "id", id.as_deref());
            img.push_str(" />");
            wrap_caption(doc, img, caption.as_deref(), patterns)
        }
        Content::Video {
            source,
            poster,
            title,
            width,
            height,
            caption,
            id,
            class,
        } => {
            let out = crate::blocks::video::render_html(
                crate::blocks::video::VideoPayload {
                    source,
                    poster: poster.as_deref(),
                    title: title.as_deref(),
                    width: *width,
                    height: *height,
                    id: id.as_deref(),
                    class: class.as_deref().unwrap_or(&[]),
                },
                patterns.videos(),
            );
            wrap_caption(doc, out, caption.as_deref(), patterns)
        }
        Content::File {
            path,
            label,
            id,
            class,
        } => {
            // Registering ships the file; without a label it ships silently,
            // exactly as the `file` block does.
            let url = patterns.files().register(path, "").url;
            let Some(label) = label else {
                return String::new();
            };
            let mut out = format!("<a{}", classes_attr(&["wdoc-file"], class));
            append_attr(&mut out, "id", id.as_deref());
            append_attr(&mut out, "href", Some(&url));
            write!(out, ">{}</a>", escape_html(label)).expect("write to String");
            out
        }
        Content::Math {
            latex,
            display,
            id,
            class,
        } => crate::blocks::math::render_math_block(
            latex,
            display.unwrap_or(true),
            class.as_deref().unwrap_or(&[]),
            id.as_deref(),
        ),
        Content::Drawing {
            shapes,
            width,
            height,
            caption,
            desc,
            id,
            class,
        } => {
            let svg = fit_content_drawing(
                doc,
                shapes,
                SvgFrame {
                    width: *width,
                    height: *height,
                    class_attr: &classes_attr(&[], class),
                    id: id.as_deref(),
                    desc: desc.as_deref(),
                },
                patterns,
            );
            wrap_caption(doc, svg, caption.as_deref(), patterns)
        }
        Content::Terminal {
            lines,
            title,
            id,
            class,
        } => {
            let mut out = format!("<div{}", classes_attr(&["wdoc-terminal-static"], class));
            append_attr(&mut out, "id", id.as_deref());
            out.push('>');
            if let Some(title) = title {
                write!(
                    out,
                    "<div class=\"wdoc-terminal-title\">{}</div>",
                    escape_html(title)
                )
                .expect("write to String");
            }
            write!(
                out,
                "<pre><code>{}</code></pre></div>",
                lines
                    .iter()
                    .map(|l| escape_html(l))
                    .collect::<Vec<_>>()
                    .join("\n")
            )
            .expect("write to String");
            out
        }
        Content::Toc {
            entries,
            title,
            id,
            class,
        } => {
            let mut out = format!("<nav{}", classes_attr(&["wdoc-toc"], class));
            append_attr(&mut out, "id", id.as_deref());
            out.push('>');
            if let Some(title) = title {
                write!(out, "<h2>{}</h2>", escape_html(title)).expect("write to String");
            }
            out.push_str(&toc_list(entries, 0, &mut 0, patterns));
            out.push_str("</nav>");
            out
        }
        Content::Footnotes {
            notes,
            title,
            id,
            class,
        } => {
            // The visible number is the `<ol>`'s, not the marker: the
            // marker is the *key* both ends of the link are anchored on
            // (`fn-<marker>` here, `fnref-<marker>` on the reference the
            // `[^id]` rewrite plants — see `super::headings`). A title is
            // a section label rather than a document heading, so it stays
            // out of the page outline.
            let mut out = format!("<section{}", classes_attr(&["wdoc-footnotes"], class));
            append_attr(&mut out, "id", id.as_deref());
            out.push('>');
            if let Some(title) = title {
                write!(
                    out,
                    "<div class=\"wdoc-footnotes-title\">{}</div>",
                    escape_html(title)
                )
                .expect("write to String");
            }
            out.push_str("<ol class=\"wdoc-footnote-list\">");
            for ContentFootnote { marker, text } in notes {
                let marker = escape_html(marker);
                write!(
                    out,
                    "<li class=\"wdoc-footnote-item\" id=\"fn-{marker}\">{}\
                     <a class=\"wdoc-footnote-back\" href=\"#fnref-{marker}\">↩</a></li>",
                    patterns.render(doc, text),
                )
                .expect("write to String");
            }
            out.push_str("</ol></section>");
            out
        }
        Content::ChapterHeader {
            title,
            kicker,
            subtitle,
            reading_time,
            updated,
            version,
            id,
            class,
        } => {
            let mut out = format!("<header{}", classes_attr(&["chapter-header"], class));
            append_attr(&mut out, "id", id.as_deref());
            out.push('>');
            if let Some(kicker) = kicker {
                write!(
                    out,
                    "<p class=\"chapter-kicker\">{}</p>",
                    patterns.render(doc, kicker)
                )
                .expect("write to String");
            }
            // The `heading-1` hook is what sizes a level-1 heading
            // (`lib/css-classes.wcl`) and what the per-page heading pass
            // stamps an anchor id onto, so a chapter title carries it for
            // the same reason a `Content::Heading` does — derived from the
            // level, never read back out of it.
            write!(
                out,
                "<h1 class=\"heading-1\">{}</h1>",
                patterns.render(doc, title)
            )
            .expect("write to String");
            if let Some(subtitle) = subtitle {
                write!(
                    out,
                    "<p class=\"chapter-subtitle\">{}</p>",
                    patterns.render(doc, subtitle)
                )
                .expect("write to String");
            }
            if let Some(meta) = chapter_meta_line(reading_time, updated, version) {
                write!(out, "<p class=\"chapter-meta\">{}</p>", escape_html(&meta))
                    .expect("write to String");
            }
            out.push_str("</header>");
            out
        }
        Content::Fragment { body, id, class } => {
            let mut out = format!("<div{}", classes_attr(&["wdoc-fragment"], class));
            append_attr(&mut out, "id", id.as_deref());
            out.push('>');
            for node in body {
                out.push_str(&render_content(doc, node, patterns));
            }
            out.push_str("</div>");
            out
        }
        Content::SpeakerNotes { body, id } => {
            // Presenter commentary: emitted but hidden, so the deck player
            // can show it and print never does.
            let mut out = String::from("<aside class=\"wdoc-speaker-notes\" hidden");
            append_attr(&mut out, "id", id.as_deref());
            out.push('>');
            for node in body {
                out.push_str(&render_content(doc, node, patterns));
            }
            out.push_str("</aside>");
            out
        }
    }
}

/// The built-in glyph for a callout kind, used when the node carries no
/// explicit `icon`. Lucide names, resolved from the compiled-in pack, so a
/// callout renders its icon with no `iconset` declared.
fn default_icon(kind: CalloutKind) -> &'static str {
    match kind {
        CalloutKind::Note | CalloutKind::Info => "lucide.info",
        CalloutKind::Tip => "lucide.lightbulb",
        CalloutKind::Warning => "lucide.triangle-alert",
        CalloutKind::Error => "lucide.circle-x",
        CalloutKind::Success => "lucide.circle-check",
    }
}

/// ` class="base… author…"`, or nothing when both are empty. Repeats are
/// dropped: a callout's kind is a class the backend emits *and* the one
/// the author usually wrote (`class = ["warning"]`), and `class="callout
/// warning warning"` is nobody's intent.
fn classes_attr(base: &[&str], class: &Option<Vec<String>>) -> String {
    let mut names: Vec<String> = Vec::new();
    for name in base
        .iter()
        .map(|s| (*s).to_string())
        .chain(class.iter().flatten().cloned())
    {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    classes_attr_from_names(&names)
}

/// Wrap `html` in a `<figure>` with a caption, or return it unchanged.
fn wrap_caption(
    doc: &Document,
    html: String,
    caption: Option<&str>,
    patterns: &InlinePatterns,
) -> String {
    match caption {
        Some(caption) => format!(
            "<figure class=\"wdoc-figure\">{html}<figcaption>{}</figcaption></figure>",
            patterns.render(doc, caption)
        ),
        None => html,
    }
}

/// Render the TOC entries at `depth` as one `<ul>`, recursing into the
/// deeper runs that follow each entry. `i` is the caller's cursor into
/// `entries`, so the flat depth-ordered list nests without a second pass.
fn toc_list(
    entries: &[ContentTocEntry],
    depth: u8,
    i: &mut usize,
    patterns: &InlinePatterns,
) -> String {
    let mut out = String::from("<ul>");
    while let Some(entry) = entries.get(*i) {
        if entry.depth != depth {
            break;
        }
        *i += 1;
        out.push_str("<li>");
        let number = entry
            .number
            .as_ref()
            .map(|n| format!("<span class=\"wdoc-toc-number\">{}</span>", escape_html(n)))
            .unwrap_or_default();
        let title = escape_html(&entry.title);
        match &entry.target {
            Some(target) => write!(
                out,
                "{number}<a href=\"{}\">{title}</a>",
                escape_html(&patterns.resolve_href(target))
            ),
            None => write!(out, "{number}{title}"),
        }
        .expect("write to String");
        // Any deeper run that follows belongs to this item.
        if entries.get(*i).is_some_and(|next| next.depth > depth) {
            let deeper = entries[*i].depth;
            out.push_str(&toc_list(entries, deeper, i, patterns));
        }
        out.push_str("</li>");
    }
    out.push_str("</ul>");
    out
}
