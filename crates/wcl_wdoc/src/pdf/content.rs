//! The PDF backend's reading of the semantic content IR.
//!
//! One arm per [`Content`] variant, matched **exhaustively** — the sibling
//! of [`crate::html::content`] and [`crate::markdown::content`].
//! Where the HTML reading emits markup, this one emits [`BlockNode`]s: the
//! paint-agnostic flow model [`layout`](crate::pdf::layout) paginates.
//!
//! `pdf::ir::BlockNode` is the IR's own ancestor — the one backend that
//! could not fake semantics out of markup built a per-concept block model
//! by hand. Most arms below are therefore a rename rather than a
//! translation.

use std::path::Path;

use wcl_lang::Document;

use crate::content::{
    CalloutKind, Content, ContentFootnote, ContentTocEntry, ListStyle, chapter_meta_line,
};
use crate::inline::InlinePatterns;

use super::collect::{bold_run, image_node};
use super::ir::{BlockNode, CodeSpan, InlineRun, ListLine, TextStyle, TocLine};

/// Collect one content node into PDF flow nodes.
pub(crate) fn collect_content(
    doc: &Document,
    node: &Content,
    patterns: &InlinePatterns,
    base_dir: Option<&Path>,
    out: &mut Vec<BlockNode>,
) {
    match node {
        Content::Heading { level, text, .. } => out.push(BlockNode::Heading {
            level: (*level).clamp(1, 6),
            // A heading renders as one styled run — inline emphasis inside
            // a heading is rare and would fight the heading style.
            runs: vec![InlineRun::Text {
                text: text.clone(),
                style: TextStyle::heading(),
            }],
        }),
        Content::Paragraph { text, .. } => out.push(BlockNode::Paragraph {
            runs: patterns.render_runs(doc, text),
        }),
        Content::List { items, style, .. } => {
            let mut lines = Vec::new();
            let mut beneath = Vec::new();
            collect_list(
                doc,
                items,
                matches!(style, Some(ListStyle::Numbered)),
                0,
                "",
                patterns,
                base_dir,
                &mut lines,
                &mut beneath,
            );
            out.push(BlockNode::List { lines });
            out.append(&mut beneath);
        }
        Content::Table {
            rows,
            header,
            caption,
            ..
        } => {
            out.push(BlockNode::Table {
                header: header
                    .iter()
                    .flatten()
                    .map(|c| {
                        patterns
                            .render_runs(doc, c)
                            .into_iter()
                            .map(bold_run)
                            .collect()
                    })
                    .collect(),
                rows: rows
                    .iter()
                    .map(|row| row.iter().map(|c| patterns.render_runs(doc, c)).collect())
                    .collect(),
            });
            if let Some(caption) = caption {
                out.push(BlockNode::Paragraph {
                    runs: patterns.render_runs(doc, caption),
                });
            }
        }
        Content::Code {
            source,
            language,
            filename,
            ..
        } => {
            // The code-card's header bar is HTML chrome; a filename still
            // belongs in the printed listing, so it becomes a caption line.
            if let Some(filename) = filename {
                out.push(BlockNode::Paragraph {
                    runs: vec![InlineRun::Text {
                        text: filename.clone(),
                        style: TextStyle::code(),
                    }],
                });
            }
            out.push(BlockNode::Code {
                lines: crate::blocks::highlight::highlight_spans(
                    // Filled by the include pass, inline or from a file.
                    source.as_deref().unwrap_or_default(),
                    language.as_deref().unwrap_or_default(),
                )
                .into_iter()
                .map(|spans| {
                    spans
                        .into_iter()
                        .map(|(text, color)| CodeSpan { text, color })
                        .collect()
                })
                .collect(),
            });
        }
        Content::Callout {
            kind,
            heading,
            body,
            ..
        } => {
            // The callout box paints one shaped heading over one shaped
            // body, so prose flattens into it; anything richer (a nested
            // list, a code sample) is placed *beneath* the box rather than
            // dropped.
            let mut runs = Vec::new();
            let mut beneath = Vec::new();
            for node in body {
                match node {
                    Content::Paragraph { text, .. } => {
                        // The box shapes one wrapped paragraph, so several
                        // run together with a space — cosmic-text softens a
                        // `\n` to one anyway, so pretending otherwise would
                        // just be a lie in the source.
                        if !runs.is_empty() {
                            runs.push(InlineRun::Text {
                                text: " ".to_string(),
                                style: TextStyle::body(),
                            });
                        }
                        runs.extend(patterns.render_runs(doc, text));
                    }
                    other => collect_content(doc, other, patterns, base_dir, &mut beneath),
                }
            }
            out.push(BlockNode::Callout {
                accent: accent(*kind),
                heading: patterns
                    .render_runs(doc, heading)
                    .into_iter()
                    .map(bold_run)
                    .collect(),
                body: runs,
            });
            out.append(&mut beneath);
        }
        // A printed page is a single column: the columns stack in order.
        Content::Columns { columns, .. } => {
            for column in columns {
                for node in column {
                    collect_content(doc, node, patterns, base_dir, out);
                }
            }
        }
        Content::Image {
            source,
            caption,
            width,
            height,
            ..
        } => {
            if let Some(node) = image_node(
                source,
                base_dir,
                width.map(|w| w as f32),
                height.map(|h| h as f32),
            ) {
                out.push(node);
            }
            if let Some(caption) = caption {
                out.push(BlockNode::Paragraph {
                    runs: patterns.render_runs(doc, caption),
                });
            }
        }
        Content::Video {
            source,
            poster,
            title,
            width,
            height,
            caption,
            ..
        } => {
            let before = out.len();
            // Static print can't play a video: show the poster and, for an
            // online video only, link to it (a local path is useless in a
            // distributed PDF).
            if let Some(node) = poster.as_ref().and_then(|p| {
                image_node(
                    p,
                    base_dir,
                    width.map(|w| w as f32),
                    height.map(|h| h as f32),
                )
            }) {
                out.push(node);
            }
            if let Some(url) = crate::blocks::video::online_url(source) {
                let label = title
                    .clone()
                    .or_else(|| caption.clone())
                    .unwrap_or_else(|| url.clone());
                out.push(BlockNode::Paragraph {
                    runs: vec![InlineRun::Link {
                        runs: vec![InlineRun::Text {
                            text: label,
                            style: TextStyle::body(),
                        }],
                        href: url,
                    }],
                });
            } else if out.len() == before {
                // A local video path is useless in a distributed PDF. With
                // no poster, name the content rather than silently dropping
                // a covered page concept.
                let label = title
                    .as_ref()
                    .or(caption.as_ref())
                    .map(|t| format!("Video: {t}"))
                    .unwrap_or_else(|| "Video".to_string());
                out.push(BlockNode::Paragraph {
                    runs: vec![InlineRun::Text {
                        text: label,
                        style: TextStyle {
                            italic: true,
                            ..TextStyle::body()
                        },
                    }],
                });
            } else if let Some(caption) = caption {
                out.push(BlockNode::Paragraph {
                    runs: patterns.render_runs(doc, caption),
                });
            }
        }
        // The PDF ships no asset folder, so a `File` prints as its label
        // linked to the path — inert for a local file, live for a URL.
        Content::File { path, label, .. } => {
            let Some(label) = label else {
                return;
            };
            out.push(BlockNode::Paragraph {
                runs: vec![InlineRun::Link {
                    runs: vec![InlineRun::Text {
                        text: label.clone(),
                        style: TextStyle::body(),
                    }],
                    href: path.clone(),
                }],
            });
        }
        Content::Math {
            latex,
            display,
            class,
            ..
        } => out.push(BlockNode::Svg {
            svg: crate::blocks::math::render_math_block(
                latex,
                display.unwrap_or(true),
                class.as_deref().unwrap_or(&[]),
                None,
            ),
        }),
        Content::Drawing {
            shapes,
            width,
            height,
            caption,
            desc,
            ..
        } => {
            out.push(BlockNode::Svg {
                svg: crate::svg::fit_content_drawing(
                    doc,
                    shapes,
                    crate::svg::SvgFrame {
                        width: *width,
                        height: *height,
                        class_attr: "",
                        id: None,
                        desc: desc.as_deref(),
                    },
                    patterns,
                ),
            });
            if let Some(caption) = caption {
                out.push(BlockNode::Paragraph {
                    runs: patterns.render_runs(doc, caption),
                });
            }
        }
        Content::Terminal { lines, title, .. } => {
            if let Some(title) = title {
                out.push(BlockNode::Paragraph {
                    runs: vec![InlineRun::Text {
                        text: title.clone(),
                        style: TextStyle::code(),
                    }],
                });
            }
            // A resolved screen is monospaced text: the code block is the
            // node that prints it, with no highlighting to apply.
            out.push(BlockNode::Code {
                lines: lines
                    .iter()
                    .map(|line| {
                        vec![CodeSpan {
                            text: line.clone(),
                            color: (0, 0, 0),
                        }]
                    })
                    .collect(),
            });
        }
        Content::Toc { entries, title, .. } => {
            if let Some(title) = title {
                out.push(BlockNode::Heading {
                    level: 1,
                    runs: vec![InlineRun::Text {
                        text: title.clone(),
                        style: TextStyle::heading(),
                    }],
                });
            }
            out.push(BlockNode::Toc {
                entries: entries
                    .iter()
                    .map(
                        |ContentTocEntry {
                             depth,
                             title,
                             target,
                             number,
                         }| TocLine {
                            depth: *depth,
                            title: title.clone(),
                            page: target.clone(),
                            number: number.clone().unwrap_or_default(),
                        },
                    )
                    .collect(),
            });
        }
        Content::Footnotes { notes, title, .. } => {
            if let Some(title) = title {
                out.push(BlockNode::Heading {
                    level: 2,
                    runs: vec![InlineRun::Text {
                        text: title.clone(),
                        style: TextStyle::heading(),
                    }],
                });
            }
            for ContentFootnote { marker, text } in notes {
                let mut runs = vec![InlineRun::Text {
                    text: format!("{marker}. "),
                    style: TextStyle::body(),
                }];
                runs.extend(patterns.render_runs(doc, text));
                out.push(BlockNode::Paragraph { runs });
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
                out.push(BlockNode::Paragraph {
                    runs: patterns.render_runs(doc, kicker),
                });
            }
            out.push(BlockNode::Heading {
                level: 1,
                runs: vec![InlineRun::Text {
                    text: title.clone(),
                    style: TextStyle::heading(),
                }],
            });
            if let Some(subtitle) = subtitle {
                out.push(BlockNode::Paragraph {
                    runs: patterns.render_runs(doc, subtitle),
                });
            }
            if let Some(meta) = chapter_meta_line(reading_time, updated, version) {
                out.push(BlockNode::Paragraph {
                    runs: vec![InlineRun::Text {
                        text: meta,
                        style: TextStyle::body(),
                    }],
                });
            }
        }
        // A step-reveal group has no steps in print: its body renders in
        // place, exactly as the `fragment` block does.
        Content::Fragment { body, .. } => {
            for node in body {
                collect_content(doc, node, patterns, base_dir, out);
            }
        }
        // Presenter-only commentary, declared as omitted from printed
        // output — so the printed output omits it.
        Content::SpeakerNotes { .. } => {}
    }
}

/// The accent colour for a callout kind (mirrors the `--callout-accent`
/// values in `lib/callout.wcl`).
fn accent(kind: Option<CalloutKind>) -> (u8, u8, u8) {
    match kind {
        Some(CalloutKind::Note) | Some(CalloutKind::Info) => (94, 129, 172),
        Some(CalloutKind::Tip) => (136, 192, 208),
        Some(CalloutKind::Warning) => (208, 135, 112),
        Some(CalloutKind::Error) => (191, 97, 106),
        Some(CalloutKind::Success) => (163, 190, 140),
        None => (136, 136, 136),
    }
}

/// Flatten list items into indented marked lines, recursing through each
/// item's nested content. `prefix` carries the parent's number so a
/// numbered sublist counts as "1.2".
///
/// A `ListLine` holds inline runs, so prose nested under an item becomes a
/// marker-less line at the item's indent; content the flow model can't
/// hold there (a code sample, an image) goes to `beneath`, which the
/// caller places after the list rather than dropping.
// The walker's own context (doc / patterns / base_dir) plus the numbering
// state a nested list carries (ordered / depth / prefix) plus the two sinks.
// Bundling them would name a type nothing else wants.
#[allow(clippy::too_many_arguments)]
fn collect_list(
    doc: &Document,
    items: &[crate::content::ContentListItem],
    ordered: bool,
    depth: u8,
    prefix: &str,
    patterns: &InlinePatterns,
    base_dir: Option<&Path>,
    lines: &mut Vec<ListLine>,
    beneath: &mut Vec<BlockNode>,
) {
    for (i, item) in items.iter().enumerate() {
        let num = if prefix.is_empty() {
            (i + 1).to_string()
        } else {
            format!("{prefix}.{}", i + 1)
        };
        lines.push(ListLine {
            depth,
            marker: if ordered {
                format!("{num}.")
            } else {
                super::collect::bullet(depth).to_string()
            },
            runs: patterns.render_runs(doc, &item.text),
        });
        for nested in item.blocks.iter().flatten() {
            if let Content::List { items, style, .. } = nested {
                let sub_ordered = matches!(style, Some(ListStyle::Numbered));
                collect_list(
                    doc,
                    items,
                    sub_ordered,
                    depth + 1,
                    if sub_ordered && ordered { &num } else { "" },
                    patterns,
                    base_dir,
                    lines,
                    beneath,
                );
                continue;
            }
            let mut nodes = Vec::new();
            collect_content(doc, nested, patterns, base_dir, &mut nodes);
            for node in nodes {
                match node {
                    BlockNode::Paragraph { runs } => lines.push(ListLine {
                        depth: depth + 1,
                        marker: String::new(),
                        runs,
                    }),
                    other => beneath.push(other),
                }
            }
        }
    }
}
