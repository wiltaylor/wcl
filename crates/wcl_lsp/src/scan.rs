//! Shared byte-level text scanning for the LSP request handlers:
//! identifier-byte classification and offset → line/column counting.
//!
//! Several handlers (`navigation`, `resolve`, `convert`, `code_actions`)
//! need the same two primitives; keeping one copy here avoids the
//! `is_ident` / `is_ident_byte` and newline-counting drift the handlers
//! used to carry independently.

/// True for bytes that may appear inside a WCL identifier
/// (`[A-Za-z0-9_]`). Word-boundary detection and word slicing across the
/// navigation / resolve handlers all key off this.
pub(crate) fn is_ident_byte(b: u8) -> bool {
    b == b'_' || b.is_ascii_alphanumeric()
}

/// The zero-based line number containing `offset` and the byte offset of
/// that line's start, in a single pass over the bytes before `offset`
/// (clamped to the source length). `line` is the count of `\n` bytes
/// strictly before `offset`; `character` is then `offset - line_start`.
pub(crate) fn line_and_start(source: &str, offset: usize) -> (u32, usize) {
    let clamped = offset.min(source.len());
    let bytes = source.as_bytes();
    let mut line: u32 = 0;
    let mut line_start: usize = 0;
    for (i, &b) in bytes.iter().enumerate().take(clamped) {
        if b == b'\n' {
            line += 1;
            line_start = i + 1;
        }
    }
    (line, line_start)
}

/// The zero-based line number containing `offset` (the `line` half of
/// [`line_and_start`]).
pub(crate) fn line_for_offset(source: &str, offset: usize) -> u32 {
    line_and_start(source, offset).0
}
