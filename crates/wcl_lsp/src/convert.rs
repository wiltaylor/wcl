//! Span ↔ LSP position conversion.
//!
//! `wcl_lang::Span` is a half-open byte range. LSP positions are
//! `(line, character)` pairs, where `character` counts code units of the
//! negotiated [`PositionEncoding`]: UTF-8 bytes when the client offers
//! them, otherwise UTF-16 code units (the protocol's mandatory default,
//! and the only encoding VS Code speaks). Lines break at `\n` only.

use std::path::{Path, PathBuf};

use ropey::Rope;
use tower_lsp_server::ls_types::{ClientCapabilities, Position, PositionEncodingKind, Range, Uri};
use wcl_lang::Span;

/// The filesystem path a `file:` URI names. `None` for any other
/// scheme (`untitled:`, `vscode-notebook-cell:` …), which has no path
/// on disk to import from or read back.
pub(crate) fn uri_to_path(uri: &Uri) -> Option<PathBuf> {
    if !uri.scheme().as_str().eq_ignore_ascii_case("file") {
        return None;
    }
    uri.to_file_path().map(std::borrow::Cow::into_owned)
}

/// The `file:` URI for an absolute path. `None` when the path is
/// relative and cannot be canonicalised.
pub(crate) fn path_to_uri(path: &Path) -> Option<Uri> {
    Uri::from_file_path(path)
}

/// The unit an LSP `character` counts. Negotiated once, in
/// `initialize`, and applied to every position crossing the wire.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum PositionEncoding {
    /// Bytes of UTF-8 — offered by some clients, native to `wcl_lang`.
    Utf8,
    /// UTF-16 code units — what every client must support.
    #[default]
    Utf16,
}

impl PositionEncoding {
    /// Pick UTF-8 when the client lists it in
    /// `general.positionEncodings`, otherwise fall back to UTF-16.
    pub(crate) fn negotiate(capabilities: &ClientCapabilities) -> Self {
        let offers_utf8 = capabilities
            .general
            .as_ref()
            .and_then(|general| general.position_encodings.as_ref())
            .is_some_and(|offered| offered.contains(&PositionEncodingKind::UTF8));
        if offers_utf8 { Self::Utf8 } else { Self::Utf16 }
    }

    /// The protocol name advertised back in the server capabilities.
    pub(crate) fn kind(self) -> PositionEncodingKind {
        match self {
            Self::Utf8 => PositionEncodingKind::UTF8,
            Self::Utf16 => PositionEncodingKind::UTF16,
        }
    }

    /// Code units `text` occupies in this encoding.
    pub(crate) fn units(self, text: &str) -> usize {
        match self {
            Self::Utf8 => text.len(),
            Self::Utf16 => text.encode_utf16().count(),
        }
    }
}

/// Line starts of one text snapshot, so each conversion costs a binary
/// search plus a walk of a single line rather than a scan from the top
/// of the file. Build one per snapshot and reuse it for every span.
pub(crate) struct LineIndex<'a> {
    /// The snapshot being indexed.
    text: &'a str,
    /// Byte offset of the first byte of every line.
    line_starts: Vec<usize>,
    /// Unit `character` counts in.
    encoding: PositionEncoding,
}

impl<'a> LineIndex<'a> {
    /// Index `text` for conversions in `encoding`.
    pub(crate) fn new(text: &'a str, encoding: PositionEncoding) -> Self {
        let line_starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();
        Self {
            text,
            line_starts,
            encoding,
        }
    }

    /// The encoding `character` counts in.
    pub(crate) fn encoding(&self) -> PositionEncoding {
        self.encoding
    }

    /// Byte offset → LSP position. Offsets past the end clamp to the end
    /// of the text; an offset inside a multi-byte character snaps back to
    /// that character's first byte.
    pub(crate) fn position(&self, offset: usize) -> Position {
        let offset = self.text.floor_char_boundary(offset);
        let line = self.line_starts.partition_point(|&start| start <= offset) - 1;
        let start = self.line_starts[line];
        Position {
            line: line as u32,
            character: self.encoding.units(&self.text[start..offset]) as u32,
        }
    }

    /// LSP position → byte offset. A line past the end clamps to the end
    /// of the text, a character past the end of its line clamps to the
    /// line's `\n`, and a character inside a multi-unit character (the
    /// middle of a UTF-8 sequence or a surrogate pair) snaps back to that
    /// character's first byte — so the result is always a char boundary.
    pub(crate) fn offset(&self, pos: Position) -> usize {
        let Some(&start) = self.line_starts.get(pos.line as usize) else {
            return self.text.len();
        };
        let end = self
            .line_starts
            .get(pos.line as usize + 1)
            .map_or(self.text.len(), |next| next - 1);
        let line = &self.text[start..end];
        let wanted = pos.character as usize;
        match self.encoding {
            PositionEncoding::Utf8 => start + line.floor_char_boundary(wanted),
            PositionEncoding::Utf16 => {
                let mut units = 0;
                for (i, c) in line.char_indices() {
                    units += c.len_utf16();
                    if units > wanted {
                        return start + i;
                    }
                }
                end
            }
        }
    }

    /// Byte [`Span`] → LSP [`Range`].
    pub(crate) fn range(&self, span: Span) -> Range {
        Range {
            start: self.position(span.start),
            end: self.position(span.end),
        }
    }

    /// Range covering the whole text — full-document formatting edits.
    pub(crate) fn full_range(&self) -> Range {
        Range {
            start: Position::new(0, 0),
            end: self.position(self.text.len()),
        }
    }
}

/// LSP position → char index in a rope, with the same clamping and
/// snapping as [`LineIndex::offset`]. Applies incremental edits without
/// materialising the buffer as a string per change event.
pub(crate) fn rope_char_index(rope: &Rope, pos: Position, encoding: PositionEncoding) -> usize {
    let line = pos.line as usize;
    if line >= rope.len_lines() {
        return rope.len_chars();
    }
    let start = rope.line_to_char(line);
    let end = if line + 1 < rope.len_lines() {
        rope.line_to_char(line + 1) - 1
    } else {
        rope.len_chars()
    };
    let wanted = pos.character as usize;
    match encoding {
        PositionEncoding::Utf8 => {
            let end_byte = rope.char_to_byte(end);
            rope.byte_to_char((rope.char_to_byte(start) + wanted).min(end_byte))
        }
        PositionEncoding::Utf16 => {
            let end_unit = rope.char_to_utf16_cu(end);
            rope.utf16_cu_to_char((rope.char_to_utf16_cu(start) + wanted).min(end_unit))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn utf8(text: &str) -> LineIndex<'_> {
        LineIndex::new(text, PositionEncoding::Utf8)
    }

    fn utf16(text: &str) -> LineIndex<'_> {
        LineIndex::new(text, PositionEncoding::Utf16)
    }

    #[test]
    fn offset_zero_is_origin() {
        assert_eq!(utf8("hello").position(0), Position::new(0, 0));
    }

    #[test]
    fn offset_past_newlines_advances_line() {
        assert_eq!(utf8("ab\ncd\nef").position(6), Position::new(2, 0));
    }

    #[test]
    fn offset_clamped_to_end() {
        assert_eq!(utf8("abc").position(999), Position::new(0, 3));
    }

    #[test]
    fn position_round_trips_with_offset() {
        let src = "ab\ncdef\nghi";
        for index in [utf8(src), utf16(src)] {
            for offset in 0..=src.len() {
                let round = index.offset(index.position(offset));
                assert_eq!(round, offset, "offset {offset} round-tripped to {round}");
            }
        }
    }

    #[test]
    fn position_past_line_end_clamps() {
        assert_eq!(utf8("ab\ncdef").offset(Position::new(0, 99)), 2);
        assert_eq!(utf16("ab\ncdef").offset(Position::new(0, 99)), 2);
    }

    #[test]
    fn position_past_document_clamps() {
        assert_eq!(utf8("abc").offset(Position::new(99, 0)), 3);
    }

    #[test]
    fn span_to_range_spans_two_lines() {
        let r = utf8("ab\ncdef").range(Span::new(1, 5));
        assert_eq!(r.start, Position::new(0, 1));
        assert_eq!(r.end, Position::new(1, 2));
    }

    #[test]
    fn non_ascii_columns_count_in_the_negotiated_unit() {
        // `é` is 2 bytes / 1 unit, `😀` 4 bytes / 2 units, `—` 3 bytes / 1 unit.
        let src = "x = \"é😀—\" y\n";
        let y = src.find('y').unwrap();
        assert_eq!(utf8(src).position(y), Position::new(0, 16));
        assert_eq!(utf16(src).position(y), Position::new(0, 11));
        assert_eq!(utf8(src).offset(Position::new(0, 16)), y);
        assert_eq!(utf16(src).offset(Position::new(0, 11)), y);
        for index in [utf8(src), utf16(src)] {
            for (offset, _) in src.char_indices() {
                assert_eq!(index.offset(index.position(offset)), offset);
            }
        }
    }

    #[test]
    fn mid_character_positions_snap_to_the_character_start() {
        let src = "é😀";
        // Byte 1 is inside `é`; UTF-16 unit 2 is inside the surrogate pair.
        assert_eq!(utf8(src).offset(Position::new(0, 1)), 0);
        assert_eq!(utf16(src).offset(Position::new(0, 2)), 2);
        assert_eq!(utf8(src).position(1), Position::new(0, 0));
        assert_eq!(utf16(src).position(3), Position::new(0, 1));
    }

    #[test]
    fn rope_index_agrees_with_line_index() {
        let src = "a é\n😀 — b\n\nlast";
        let rope = Rope::from_str(src);
        for encoding in [PositionEncoding::Utf8, PositionEncoding::Utf16] {
            let index = LineIndex::new(src, encoding);
            for line in 0..6 {
                for character in 0..12 {
                    let pos = Position::new(line, character);
                    let char_index = rope_char_index(&rope, pos, encoding);
                    assert_eq!(
                        rope.char_to_byte(char_index),
                        index.offset(pos),
                        "{encoding:?} {pos:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn negotiation_prefers_utf8_only_when_offered() {
        use tower_lsp_server::ls_types::GeneralClientCapabilities;
        let offering = |kinds: Vec<PositionEncodingKind>| ClientCapabilities {
            general: Some(GeneralClientCapabilities {
                position_encodings: Some(kinds),
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(
            PositionEncoding::negotiate(&ClientCapabilities::default()),
            PositionEncoding::Utf16
        );
        assert_eq!(
            PositionEncoding::negotiate(&offering(vec![PositionEncodingKind::UTF16])),
            PositionEncoding::Utf16
        );
        assert_eq!(
            PositionEncoding::negotiate(&offering(vec![
                PositionEncodingKind::UTF16,
                PositionEncodingKind::UTF8,
            ])),
            PositionEncoding::Utf8
        );
    }
}
