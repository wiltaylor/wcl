//! The Markdown backend's reading of the semantic content IR.
//!
//! One arm per [`Content`] variant, matched **exhaustively** — the sibling
//! of [`crate::render::content_html`] and [`crate::pdf::content`]. The
//! skill target is this backend too (a skill folder is Markdown pages plus
//! a manifest), so these arms are what a skill renders as well: the
//! "four backends" of the content IR are three walkers, one of them shared.
//!
//! Where the HTML reading emits markup and the PDF one emits flow nodes,
//! this one emits complete Markdown blocks — the caller joins them with
//! blank lines.

use crate::build::BuildError;
use crate::content::{
    CalloutKind, Content, ContentFootnote, ContentTocEntry, ListStyle, chapter_meta_line,
};

use super::emit::{Emitter, escape_cell, escape_link_text, fence, image_ref, render_pipe_table};

impl Emitter<'_> {
    /// Emit one content node as zero or more complete Markdown blocks.
    pub(super) fn content(
        &mut self,
        node: &Content,
        out: &mut Vec<String>,
    ) -> Result<(), BuildError> {
        match node {
            Content::Heading { level, text, .. } => out.push(format!(
                "{} {}",
                "#".repeat((*level).clamp(1, 6) as usize),
                self.inline(text)
            )),
            Content::Paragraph { text, .. } => self.push_para(text, out),
            Content::List { items, style, .. } => {
                let mut lines = Vec::new();
                self.content_list(
                    items,
                    matches!(style, Some(ListStyle::Numbered)),
                    0,
                    &mut lines,
                );
                out.push(lines.join("\n"));
            }
            Content::Table {
                rows,
                header,
                caption,
                ..
            } => {
                let cell = |em: &Self, c: &String| escape_cell(&em.inline(c));
                let header: Vec<String> = header.iter().flatten().map(|c| cell(self, c)).collect();
                let rows: Vec<Vec<String>> = rows
                    .iter()
                    .map(|row| row.iter().map(|c| cell(self, c)).collect())
                    .collect();
                out.push(render_pipe_table(&header, &rows));
                if let Some(caption) = caption {
                    self.push_para(caption, out);
                }
            }
            Content::Code {
                source,
                language,
                filename,
                ..
            } => {
                // No code-card chrome in Markdown; a filename still names
                // the listing, as a line above it.
                if let Some(filename) = filename {
                    out.push(format!("`{filename}`"));
                }
                out.push(fence(language.as_deref().unwrap_or_default(), source));
            }
            Content::Callout {
                kind,
                heading,
                body,
                ..
            } => {
                // A GitHub-style alert blockquote: the keyword line, the
                // heading, then the body — which is content, so it renders
                // through these same arms and gets quoted line by line.
                let mut inner: Vec<String> = Vec::new();
                let heading = self.inline(heading);
                if !heading.trim().is_empty() {
                    inner.push(format!("**{heading}**"));
                }
                let mut body_blocks = Vec::new();
                for node in body {
                    self.content(node, &mut body_blocks)?;
                }
                inner.extend(body_blocks);
                let mut lines = vec![format!("> [!{}]", alert(*kind))];
                for line in inner.join("\n\n").split('\n') {
                    lines.push(if line.is_empty() {
                        ">".to_string()
                    } else {
                        format!("> {line}")
                    });
                }
                out.push(lines.join("\n"));
            }
            // Markdown has no columns: they stack in order.
            Content::Columns { columns, .. } => {
                for column in columns {
                    for node in column {
                        self.content(node, out)?;
                    }
                }
            }
            Content::Image {
                source,
                alt,
                caption,
                ..
            } => {
                let entry = self.patterns.images().register(source);
                out.push(image_ref(
                    alt.as_deref().unwrap_or_default(),
                    &self.asset_href(&entry.url),
                ));
                if let Some(caption) = caption {
                    self.push_para(caption, out);
                }
            }
            Content::Video {
                source,
                title,
                caption,
                ..
            } => {
                // Static Markdown can't play a video: an online one becomes
                // a link, a local one is copied out and linked.
                let url = match crate::video::online_url(source) {
                    Some(url) => url,
                    None => self.asset_href(&self.patterns.videos().register(source, "video")),
                };
                let label = title
                    .clone()
                    .or_else(|| caption.clone())
                    .unwrap_or_else(|| url.clone());
                out.push(format!("[{}]({url})", escape_link_text(&label)));
            }
            Content::File { path, label, .. } => {
                // Registering ships the file; without a label it ships
                // silently, exactly as the `file` block does.
                let entry = self.patterns.files().register(path, "");
                if let Some(label) = label {
                    out.push(format!(
                        "[{}]({})",
                        escape_link_text(label),
                        self.asset_href(&entry.url)
                    ));
                }
            }
            // The Markdown target keeps math textual rather than
            // rasterizing it.
            Content::Math { latex, .. } => out.push(format!("$$\n{}\n$$", latex.trim())),
            Content::Drawing {
                shapes,
                width,
                height,
                caption,
                desc,
                ..
            } => {
                let svg = crate::render::fit_content_drawing(
                    self.doc,
                    shapes,
                    crate::render::SvgFrame {
                        width: *width,
                        height: *height,
                        class_attr: "",
                        id: None,
                        desc: desc.as_deref(),
                    },
                    self.patterns,
                );
                let rel = self.write_svg("drawing", &svg)?;
                out.push(image_ref(
                    desc.as_deref().or(caption.as_deref()).unwrap_or("drawing"),
                    &rel,
                ));
                if let Some(caption) = caption {
                    self.push_para(caption, out);
                }
            }
            Content::Terminal { lines, title, .. } => {
                if let Some(title) = title {
                    out.push(format!("`{title}`"));
                }
                out.push(fence("", &lines.join("\n")));
            }
            Content::Toc { entries, title, .. } => {
                if let Some(title) = title {
                    out.push(format!("## {}", self.inline(title)));
                }
                let lines: Vec<String> = entries
                    .iter()
                    .map(
                        |ContentTocEntry {
                             depth,
                             title,
                             target,
                             number,
                         }| {
                            let label = match number {
                                Some(n) => format!("{n} {title}"),
                                None => title.clone(),
                            };
                            let text = match target {
                                Some(target) => format!(
                                    "[{}]({})",
                                    escape_link_text(&label),
                                    self.patterns.resolve_href(target)
                                ),
                                None => escape_link_text(&label),
                            };
                            format!("{}- {text}", "  ".repeat(*depth as usize))
                        },
                    )
                    .collect();
                out.push(lines.join("\n"));
            }
            Content::Footnotes { notes, title, .. } => {
                if let Some(title) = title {
                    out.push(format!("## {}", self.inline(title)));
                }
                for ContentFootnote { marker, text } in notes {
                    out.push(format!("[^{marker}]: {}", self.inline(text)));
                }
            }
            Content::ChapterHeader {
                title,
                kicker,
                subtitle,
                reading_time,
                updated,
                version,
                ..
            } => {
                if let Some(kicker) = kicker {
                    out.push(format!("_{}_", self.inline(kicker)));
                }
                out.push(format!("# {}", self.inline(title)));
                if let Some(subtitle) = subtitle {
                    self.push_para(subtitle, out);
                }
                if let Some(meta) = chapter_meta_line(reading_time, updated, version) {
                    out.push(format!("_{meta}_"));
                }
            }
            // A step-reveal group has no steps in static output: its body
            // renders in place, exactly as the `fragment` block does.
            Content::Fragment { body, .. } => {
                for node in body {
                    self.content(node, out)?;
                }
            }
            // Presenter-only commentary, declared as omitted from printed
            // output — and the `notes` block is already dropped here.
            Content::SpeakerNotes { .. } => {}
        }
        Ok(())
    }

    /// Flatten list items into indented Markdown lines. Content nested
    /// under an item that isn't a sub-list renders at the item's indent.
    fn content_list(
        &mut self,
        items: &[crate::content::ContentListItem],
        ordered: bool,
        depth: usize,
        lines: &mut Vec<String>,
    ) {
        for (i, item) in items.iter().enumerate() {
            let indent = "  ".repeat(depth);
            let marker = if ordered {
                format!("{}.", i + 1)
            } else {
                "-".to_string()
            };
            lines.push(format!("{indent}{marker} {}", self.inline(&item.text)));
            for nested in item.blocks.iter().flatten() {
                if let Content::List { items, style, .. } = nested {
                    self.content_list(
                        items,
                        matches!(style, Some(ListStyle::Numbered)),
                        depth + 1,
                        lines,
                    );
                    continue;
                }
                let mut blocks = Vec::new();
                // A nested block's own I/O (an SVG write) can fail; a list
                // item is not the place to abort the page, so it degrades
                // to no nested content rather than failing the build.
                if self.content(nested, &mut blocks).is_err() {
                    continue;
                }
                let sub = "  ".repeat(depth + 1);
                for block in blocks {
                    for line in block.split('\n') {
                        lines.push(format!("{sub}{line}"));
                    }
                }
            }
        }
    }
}

/// Map a callout kind to a GitHub alert keyword. GitHub has five, so
/// `success` shares `TIP` — the closest positive admonition it offers.
fn alert(kind: Option<CalloutKind>) -> &'static str {
    match kind {
        Some(CalloutKind::Note) | Some(CalloutKind::Info) | None => "NOTE",
        Some(CalloutKind::Tip) | Some(CalloutKind::Success) => "TIP",
        Some(CalloutKind::Warning) => "WARNING",
        Some(CalloutKind::Error) => "CAUTION",
    }
}
