use tower_lsp::lsp_types::{Position, Range};
use ropey::Rope;
use wcl_core::span::Span;

/// Convert a WCL byte-offset Span to an LSP Range using a rope for line/column lookup.
pub fn span_to_lsp_range(span: Span, rope: &Rope) -> Range {
    let start = offset_to_lsp_position(span.start, rope);
    let end = offset_to_lsp_position(span.end, rope);
    Range { start, end }
}

/// Convert a byte offset to an LSP Position (0-indexed line, UTF-16 character offset).
///
/// Note: Ropey handles CRLF (`\r\n`) line endings transparently — `byte_to_line`
/// and `line_to_byte` account for them correctly. The per-character UTF-16 column
/// walk below also works because `\r` is a single char/byte and will be counted
/// before `\n` triggers the next line, but we only iterate up to `byte_diff`
/// within the current line so the `\r` is simply included in the column count
/// (which is correct — the `\r` sits before the line break).
pub fn offset_to_lsp_position(offset: usize, rope: &Rope) -> Position {
    let offset = offset.min(rope.len_bytes());
    let line = rope.byte_to_line(offset);
    let line_start_byte = rope.line_to_byte(line);
    let line_slice = rope.line(line);

    // Count UTF-16 code units from line start to offset
    let byte_diff = offset - line_start_byte;
    let mut utf16_col = 0u32;
    let mut bytes_consumed = 0usize;
    for ch in line_slice.chars() {
        if bytes_consumed >= byte_diff {
            break;
        }
        utf16_col += ch.len_utf16() as u32;
        bytes_consumed += ch.len_utf8();
    }

    Position {
        line: line as u32,
        character: utf16_col,
    }
}

/// Convert an LSP Position to a byte offset.
pub fn lsp_position_to_offset(pos: Position, rope: &Rope) -> usize {
    let line = pos.line as usize;
    if line >= rope.len_lines() {
        return rope.len_bytes();
    }
    let line_start_byte = rope.line_to_byte(line);
    let line_slice = rope.line(line);

    // Walk UTF-16 code units to find byte offset
    let target_utf16 = pos.character as usize;
    let mut utf16_col = 0usize;
    let mut byte_offset = 0usize;
    for ch in line_slice.chars() {
        if utf16_col >= target_utf16 {
            break;
        }
        utf16_col += ch.len_utf16();
        byte_offset += ch.len_utf8();
    }

    line_start_byte + byte_offset
}

#[cfg(test)]
mod tests {
    use super::*;
    use wcl_core::span::FileId;

    #[test]
    fn test_offset_position_roundtrip_ascii() {
        let text = "hello\nworld\nfoo";
        let rope = Rope::from_str(text);
        // 'w' is at byte 6, line 1, col 0
        let pos = offset_to_lsp_position(6, &rope);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);
        let back = lsp_position_to_offset(pos, &rope);
        assert_eq!(back, 6);
    }

    #[test]
    fn test_offset_position_roundtrip_multibyte() {
        let text = "a\u{00e9}b\nc"; // 'e with acute' is 2 bytes UTF-8, 1 UTF-16 unit
        let rope = Rope::from_str(text);
        // 'b' is at byte 3 (a=1, e-acute=2), line 0, utf16 col 2
        let pos = offset_to_lsp_position(3, &rope);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 2);
        let back = lsp_position_to_offset(pos, &rope);
        assert_eq!(back, 3);
    }

    #[test]
    fn test_span_to_range() {
        let text = "hello\nworld";
        let rope = Rope::from_str(text);
        let span = Span::new(FileId(0), 6, 11);
        let range = span_to_lsp_range(span, &rope);
        assert_eq!(range.start.line, 1);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.line, 1);
        assert_eq!(range.end.character, 5);
    }

    #[test]
    fn test_offset_at_end() {
        let text = "abc";
        let rope = Rope::from_str(text);
        let pos = offset_to_lsp_position(3, &rope);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 3);
    }

    #[test]
    fn test_crlf_line_endings() {
        let text = "hello\r\nworld\r\nfoo";
        let rope = Rope::from_str(text);

        // 'h' at byte 0 -> line 0, col 0
        let pos = offset_to_lsp_position(0, &rope);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 0);

        // 'w' is at byte 7 (h e l l o \r \n w) -> line 1, col 0
        let pos = offset_to_lsp_position(7, &rope);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 0);

        // 'f' is at byte 14 (hello\r\nworld\r\nf) -> line 2, col 0
        let pos = offset_to_lsp_position(14, &rope);
        assert_eq!(pos.line, 2);
        assert_eq!(pos.character, 0);

        // Roundtrip from LSP position back to byte offset
        let back = lsp_position_to_offset(Position { line: 1, character: 0 }, &rope);
        assert_eq!(back, 7);

        // Middle of second line: 'o' in "world" at byte 11 -> line 1, col 4
        let pos = offset_to_lsp_position(11, &rope);
        assert_eq!(pos.line, 1);
        assert_eq!(pos.character, 4);

        // Span across CRLF boundary
        let span = Span::new(FileId(0), 7, 12); // "world"
        let range = span_to_lsp_range(span, &rope);
        assert_eq!(range.start.line, 1);
        assert_eq!(range.start.character, 0);
        assert_eq!(range.end.line, 1);
        assert_eq!(range.end.character, 5);
    }

    #[test]
    fn test_offset_beyond_end_clamped() {
        let text = "ab";
        let rope = Rope::from_str(text);
        let pos = offset_to_lsp_position(100, &rope);
        assert_eq!(pos.line, 0);
        assert_eq!(pos.character, 2);
    }
}
