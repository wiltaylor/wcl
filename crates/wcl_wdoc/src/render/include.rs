//! The include pass: file-backed code listings.
//!
//! A `code` block may carry its listing inline (`source`) or name a file
//! to read it from (`source_file`). This pass runs between lowering and
//! the backends: it turns every `Content::Code` that names a file into
//! one that carries text, so no backend ever learns that a listing can
//! come from disk. By the time a node reaches HTML, Markdown or PDF, its
//! `source` is the code.
//!
//! Reading here rather than in each backend is what keeps the three
//! outputs honest about one file — the alternative is three reads that
//! can disagree, which is the drift this feature exists to remove.
//!
//! A read that fails is a **build failure**, never a warning and never an
//! empty card. The point of naming a file is that the build tracks it; a
//! listing that has moved, been renamed, or lost its anchor is exactly
//! the stale-documentation case the inline form could not detect, so it
//! stops the build with a message naming the file.

use std::path::{Path, PathBuf};

use crate::content::{Content, ContentListItem};

/// Why a listing could not be read. Rendered into the build error, so the
/// text is the whole diagnostic a reader gets.
#[derive(Debug)]
pub(crate) struct IncludeError(String);

impl IncludeError {
    /// The message, ready to put in front of a person.
    pub(crate) fn into_message(self) -> String {
        self.0
    }
}

/// Resolve every file-backed listing in `node`, in place.
///
/// Recurses through the content nodes that carry other content — a
/// listing can sit inside a callout body, a column, a list item or a
/// slide fragment, and it may have got there from a custom block's
/// `lower` rather than from a `code` block. The first failure wins and
/// stops the walk; the build is over either way.
pub(crate) fn resolve_content(
    node: &mut Content,
    base_dir: Option<&Path>,
) -> Result<(), IncludeError> {
    match node {
        Content::Code {
            source,
            source_file,
            anchor,
            lines,
            dedent,
            ..
        } => {
            let text = resolve_listing(
                source.as_deref(),
                source_file.as_deref(),
                anchor.as_deref(),
                lines.as_deref(),
                dedent.unwrap_or(false),
                base_dir,
            )?;
            *source = Some(text);
            // The selectors have done their work. Clearing them keeps the
            // node's meaning single: `source` is the listing, full stop.
            *source_file = None;
            *anchor = None;
            *lines = None;
            *dedent = None;
            Ok(())
        }
        Content::Callout { body, .. } | Content::Fragment { body, .. } => {
            resolve_each(body, base_dir)
        }
        Content::SpeakerNotes { body, .. } => resolve_each(body, base_dir),
        Content::Columns { columns, .. } => {
            for column in columns.iter_mut() {
                resolve_each(column, base_dir)?;
            }
            Ok(())
        }
        Content::List { items, .. } => {
            for item in items.iter_mut() {
                resolve_item(item, base_dir)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Resolve a list of content nodes in place.
fn resolve_each(nodes: &mut [Content], base_dir: Option<&Path>) -> Result<(), IncludeError> {
    for node in nodes.iter_mut() {
        resolve_content(node, base_dir)?;
    }
    Ok(())
}

/// Resolve the nested blocks a list item may carry.
fn resolve_item(item: &mut ContentListItem, base_dir: Option<&Path>) -> Result<(), IncludeError> {
    if let Some(blocks) = item.blocks.as_mut() {
        resolve_each(blocks, base_dir)?;
    }
    Ok(())
}

/// The listing text for one `Code` node: the inline `source` as authored,
/// or the selected region of `source_file`.
///
/// Exactly one of the two must be present. Neither leaves nothing to
/// render; both would make the build pick a winner, and the wrong guess
/// is a listing that silently disagrees with the file it names.
fn resolve_listing(
    source: Option<&str>,
    source_file: Option<&str>,
    anchor: Option<&str>,
    lines: Option<&str>,
    dedent: bool,
    base_dir: Option<&Path>,
) -> Result<String, IncludeError> {
    let path = match (source, source_file) {
        (Some(text), None) => return Ok(text.to_string()),
        (None, Some(path)) => path,
        (Some(_), Some(path)) => {
            return Err(IncludeError(format!(
                "code block sets both `source` and `source_file` (\"{path}\") — \
                 give the listing once, as text or as a file"
            )));
        }
        (None, None) => {
            return Err(IncludeError(
                "code block has no listing — set `source` for inline text, \
                 or `source_file` to read one from a file"
                    .to_string(),
            ));
        }
    };

    let full = resolve_path(path, base_dir);
    let text = std::fs::read_to_string(&full).map_err(|e| {
        IncludeError(format!(
            "code block cannot read `source_file` \"{path}\" ({}): {e}",
            full.display()
        ))
    })?;

    let region = strip_markers(&select_region(&text, path, anchor, lines)?);
    Ok(if dedent {
        dedent_region(&region)
    } else {
        region
    })
}

/// A `source_file` path against the directory of the document that names
/// it. A relative path may climb out of the document tree — a manual's
/// code usually lives beside it rather than under it — and an absolute
/// path is taken as given.
fn resolve_path(path: &str, base_dir: Option<&Path>) -> PathBuf {
    match base_dir {
        Some(dir) => dir.join(path),
        None => PathBuf::from(path),
    }
}

/// Cut the requested region out of a file's text.
///
/// `anchor` and `lines` are two ways to say the same thing, so asking for
/// both is an error rather than a precedence rule nobody would remember.
fn select_region(
    text: &str,
    path: &str,
    anchor: Option<&str>,
    lines: Option<&str>,
) -> Result<String, IncludeError> {
    match (anchor, lines) {
        (Some(_), Some(_)) => Err(IncludeError(format!(
            "code block sets both `anchor` and `lines` on \"{path}\" — \
             select the region one way"
        ))),
        (Some(name), None) => select_anchor(text, path, name),
        (None, Some(range)) => select_lines(text, path, range),
        (None, None) => Ok(text.to_string()),
    }
}

/// The lines between `ANCHOR: name` and `ANCHOR_END: name`.
///
/// The markers are matched anywhere on a line so they can sit in whatever
/// comment syntax the file's language uses — `// ANCHOR: x`, `# ANCHOR: x`
/// and `<!-- ANCHOR: x -->` all work, and none of them needs wdoc to know
/// the language. The marker lines themselves are dropped by
/// [`strip_markers`], which every mode runs.
fn select_anchor(text: &str, path: &str, name: &str) -> Result<String, IncludeError> {
    let open = format!("ANCHOR: {name}");
    let close = format!("ANCHOR_END: {name}");

    let mut kept: Vec<&str> = Vec::new();
    let mut inside = false;
    let mut closed = false;
    for line in text.lines() {
        if !inside {
            if line.contains(&open) {
                inside = true;
            }
            continue;
        }
        if line.contains(&close) {
            closed = true;
            break;
        }
        kept.push(line);
    }

    if !inside {
        return Err(IncludeError(format!(
            "code block asks for anchor \"{name}\" in \"{path}\", \
             which has no `ANCHOR: {name}` marker"
        )));
    }
    if !closed {
        return Err(IncludeError(format!(
            "anchor \"{name}\" in \"{path}\" is never closed — \
             add an `ANCHOR_END: {name}` marker"
        )));
    }
    Ok(joined(&kept))
}

/// A 1-based inclusive line range: `"12-30"`, `"12-"` (to the end),
/// `"-30"` (from the start) or `"12"` (one line).
///
/// A range that runs past the end of the file is an error rather than a
/// silent truncation — a listing addressed by line number is the form
/// most likely to rot when the file it quotes grows or shrinks, and
/// quietly rendering fewer lines than were asked for is exactly the drift
/// this feature is meant to catch.
fn select_lines(text: &str, path: &str, range: &str) -> Result<String, IncludeError> {
    let all: Vec<&str> = text.lines().collect();
    let bad = || {
        IncludeError(format!(
            "code block has an unreadable `lines` range \"{range}\" for \"{path}\" — \
             write it as \"12-30\", \"12-\", \"-30\" or \"12\""
        ))
    };

    let spec = range.trim();
    let (start, end) = match spec.split_once('-') {
        None => {
            let n: usize = spec.parse().map_err(|_| bad())?;
            (n, n)
        }
        Some((from, to)) => {
            let from = from.trim();
            let to = to.trim();
            let start = if from.is_empty() {
                1
            } else {
                from.parse().map_err(|_| bad())?
            };
            let end = if to.is_empty() {
                all.len()
            } else {
                to.parse().map_err(|_| bad())?
            };
            (start, end)
        }
    };

    if start == 0 || end < start {
        return Err(bad());
    }
    if end > all.len() {
        return Err(IncludeError(format!(
            "code block asks for lines {start}-{end} of \"{path}\", \
             which has {} line{}",
            all.len(),
            if all.len() == 1 { "" } else { "s" }
        )));
    }
    Ok(joined(&all[start - 1..end]))
}

/// Drop the marker lines.
///
/// An `ANCHOR:` comment is bookkeeping addressed to wdoc, not code anyone
/// reading the manual wants to see — so it never survives into a listing,
/// whether the listing selected an anchor, a line range, or the whole
/// file. A nested or overlapping anchor's markers go the same way, which
/// is what lets regions nest. (A page that wants to *show* the markers —
/// this chapter, for one — writes its listing inline.)
fn strip_markers(text: &str) -> String {
    let kept: Vec<&str> = text
        .lines()
        .filter(|l| !l.contains("ANCHOR: ") && !l.contains("ANCHOR_END: "))
        .collect();
    joined(&kept)
}

/// Strip the indentation every non-blank line shares, so a region lifted
/// out of a nested scope reads flush left.
fn dedent_region(text: &str) -> String {
    let indent = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.len() - l.trim_start().len())
        .min()
        .unwrap_or(0);
    if indent == 0 {
        return text.to_string();
    }
    let stripped: Vec<&str> = text
        .lines()
        .map(|l| {
            if l.len() >= indent {
                &l[indent..]
            } else {
                l.trim_start()
            }
        })
        .collect();
    joined(&stripped)
}

/// Join selected lines back into a listing with a trailing newline — the
/// shape a whole file read off disk already has, so a region and a whole
/// file render the same.
fn joined(lines: &[&str]) -> String {
    if lines.is_empty() {
        return String::new();
    }
    let mut out = lines.join("\n");
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_source_passes_through() {
        let text = resolve_listing(Some("fn x() {}"), None, None, None, false, None)
            .expect("inline listing");
        assert_eq!(text, "fn x() {}");
    }

    #[test]
    fn a_listing_must_be_given_exactly_once() {
        assert!(resolve_listing(None, None, None, None, false, None).is_err());
        assert!(resolve_listing(Some("x"), Some("f.rs"), None, None, false, None).is_err());
    }

    #[test]
    fn anchor_selects_between_its_markers() {
        let file = "before\n// ANCHOR: a\nkept\n// ANCHOR_END: a\nafter\n";
        assert_eq!(select_anchor(file, "f.rs", "a").expect("anchor"), "kept\n");
    }

    #[test]
    fn nested_anchors_leave_no_markers_behind() {
        let file = "// ANCHOR: outer\none\n// ANCHOR: inner\ntwo\n// ANCHOR_END: inner\n// ANCHOR_END: outer\n";
        let region = select_anchor(file, "f.rs", "outer").expect("anchor");
        assert_eq!(strip_markers(&region), "one\ntwo\n");
    }

    #[test]
    fn an_unknown_or_unclosed_anchor_is_an_error() {
        assert!(select_anchor("nothing here\n", "f.rs", "a").is_err());
        assert!(select_anchor("// ANCHOR: a\nkept\n", "f.rs", "a").is_err());
    }

    #[test]
    fn line_ranges_are_inclusive_and_one_based() {
        let file = "one\ntwo\nthree\nfour\n";
        assert_eq!(
            select_lines(file, "f", "2-3").expect("range"),
            "two\nthree\n"
        );
        assert_eq!(select_lines(file, "f", "3").expect("one"), "three\n");
        assert_eq!(
            select_lines(file, "f", "3-").expect("open"),
            "three\nfour\n"
        );
        assert_eq!(select_lines(file, "f", "-2").expect("head"), "one\ntwo\n");
    }

    #[test]
    fn a_range_off_the_end_or_backwards_is_an_error() {
        let file = "one\ntwo\n";
        assert!(select_lines(file, "f", "1-9").is_err());
        assert!(select_lines(file, "f", "2-1").is_err());
        assert!(select_lines(file, "f", "0-1").is_err());
        assert!(select_lines(file, "f", "two").is_err());
    }

    #[test]
    fn markers_never_survive_into_a_listing() {
        let file = "// ANCHOR: a\nkept\n// ANCHOR_END: a\n";
        assert_eq!(strip_markers(file), "kept\n");
    }

    #[test]
    fn dedent_strips_the_shared_indent_only() {
        let region = "    fn inner() {\n        7\n    }\n";
        assert_eq!(dedent_region(region), "fn inner() {\n    7\n}\n");
    }

    #[test]
    fn dedent_ignores_blank_lines_when_measuring() {
        let region = "    a\n\n    b\n";
        assert_eq!(dedent_region(region), "a\n\nb\n");
    }
}
