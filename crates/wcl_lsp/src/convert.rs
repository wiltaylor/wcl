//! Span ↔ LSP position conversion.
//!
//! `wcl_lang::Span` is a half-open byte range. LSP positions are
//! `(line, character)` pairs. The server advertises UTF-8 position
//! encoding (`PositionEncodingKind::UTF8`), so `character` is a count
//! of bytes within the line — no UTF-16 dance needed.

use tower_lsp::lsp_types::{Position, Range};
use wcl_lang::Span;

/// Translate a byte-offset [`Span`] in `source` to an LSP [`Range`].
/// Offsets past the end of the source clamp to the final byte.
pub(crate) fn span_to_range(source: &str, span: Span) -> Range {
    Range {
        start: offset_to_position(source, span.start),
        end: offset_to_position(source, span.end),
    }
}

/// Translate a byte offset into a `(line, character)` LSP [`Position`].
/// `character` counts bytes within the line (UTF-8 position encoding).
pub(crate) fn offset_to_position(source: &str, offset: usize) -> Position {
    let clamped = offset.min(source.len());
    let (line, line_start) = crate::scan::line_and_start(source, offset);
    Position {
        line,
        character: (clamped - line_start) as u32,
    }
}

/// Range that covers the entire `source`. Used for full-document
/// formatting edits.
pub(crate) fn full_document_range(source: &str) -> Range {
    Range {
        start: Position {
            line: 0,
            character: 0,
        },
        end: offset_to_position(source, source.len()),
    }
}

/// Translate an LSP `(line, character)` [`Position`] into a byte
/// offset inside `source`. Positions past the end of the document
/// clamp to `source.len()`; positions past the end of a line clamp
/// to that line's terminator.
pub(crate) fn position_to_offset(source: &str, pos: Position) -> usize {
    let bytes = source.as_bytes();
    let mut line: u32 = 0;
    let mut line_start: usize = 0;
    for (i, &b) in bytes.iter().enumerate() {
        if line == pos.line {
            let line_end = bytes[line_start..]
                .iter()
                .position(|&c| c == b'\n')
                .map(|n| line_start + n)
                .unwrap_or(bytes.len());
            return (line_start + pos.character as usize).min(line_end);
        }
        if b == b'\n' {
            line += 1;
            line_start = i + 1;
        }
    }
    if line == pos.line {
        // Target line is the trailing (possibly empty) final line.
        return (line_start + pos.character as usize).min(bytes.len());
    }
    bytes.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_zero_is_origin() {
        let p = offset_to_position("hello", 0);
        assert_eq!(p.line, 0);
        assert_eq!(p.character, 0);
    }

    #[test]
    fn offset_past_newlines_advances_line() {
        let src = "ab\ncd\nef";
        let p = offset_to_position(src, 6); // 'e'
        assert_eq!(p.line, 2);
        assert_eq!(p.character, 0);
    }

    #[test]
    fn offset_clamped_to_end() {
        let src = "abc";
        let p = offset_to_position(src, 999);
        assert_eq!(p.line, 0);
        assert_eq!(p.character, 3);
    }

    #[test]
    fn position_round_trips_with_offset() {
        let src = "ab\ncdef\nghi";
        for offset in 0..=src.len() {
            let p = offset_to_position(src, offset);
            let round = position_to_offset(src, p);
            assert_eq!(round, offset, "offset {offset} round-tripped to {round}");
        }
    }

    #[test]
    fn position_past_line_end_clamps() {
        let src = "ab\ncdef";
        let off = position_to_offset(
            src,
            Position {
                line: 0,
                character: 99,
            },
        );
        assert_eq!(off, 2); // end of first line, before '\n'
    }

    #[test]
    fn position_past_document_clamps() {
        let src = "abc";
        let off = position_to_offset(
            src,
            Position {
                line: 99,
                character: 0,
            },
        );
        assert_eq!(off, src.len());
    }

    #[test]
    fn span_to_range_spans_two_lines() {
        let src = "ab\ncdef";
        let r = span_to_range(src, Span::new(1, 5));
        assert_eq!(
            r.start,
            Position {
                line: 0,
                character: 1
            }
        );
        assert_eq!(
            r.end,
            Position {
                line: 1,
                character: 2
            }
        );
    }
}
