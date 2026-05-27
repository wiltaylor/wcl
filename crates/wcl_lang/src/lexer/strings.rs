//! String- and heredoc-literal lexing. Extracted from `lexer.rs` so
//! the parent file can stay focused on the dispatch state machine.

use crate::ast::Span;

use super::{
    LexError, Lexer, StringEncoding, StringLit, StringPart, StringPrefix, Token, TokenKind,
    is_ident_cont, is_ident_start,
};

impl<'a> Lexer<'a> {
    pub(super) fn lex_string(
        &mut self,
        start: usize,
        prefix: StringPrefix,
    ) -> Result<Token, LexError> {
        self.pos += 1; // opening "
        let body_start = self.pos;
        let mut body = String::new();
        let mut parts: Vec<StringPart> = Vec::new();
        loop {
            // Interpolation slot detection. `${` flushes the literal
            // buffer and captures the brace-balanced slot.
            if prefix.interpolated && self.peek() == Some(b'$') && self.peek_at(1) == Some(b'{') {
                if !body.is_empty() {
                    parts.push(StringPart::Literal(std::mem::take(&mut body)));
                }
                let slot_start = self.pos;
                let text_start = slot_start + 2;
                let (text, slot_end) = self.scan_interp_slot(slot_start, text_start)?;
                self.pos = slot_end;
                parts.push(StringPart::Expr {
                    text,
                    span: Span::new(slot_start, slot_end),
                });
                continue;
            }
            let Some(c) = self.bump() else {
                return Err(LexError {
                    message: "unterminated string".into(),
                    span: Span::new(start, self.pos),
                });
            };
            match c {
                b'"' => break,
                b'\\' => {
                    let esc_start = self.pos - 1;
                    let Some(esc) = self.bump() else {
                        return Err(LexError {
                            message: "unterminated escape sequence".into(),
                            span: Span::new(esc_start, self.pos),
                        });
                    };
                    let Some(decoded) = decode_escape_char(esc, prefix.interpolated) else {
                        return Err(LexError {
                            message: format!("invalid escape '\\{}'", esc as char),
                            span: Span::new(esc_start, self.pos),
                        });
                    };
                    body.push(decoded);
                }
                b'\n' => {
                    return Err(LexError {
                        message: "newline in string literal".into(),
                        span: Span::new(start, self.pos),
                    });
                }
                other if other < 0x80 => body.push(other as char),
                other => {
                    // Non-ASCII byte: walk back and decode one UTF-8 char from
                    // the source so the body preserves multi-byte content.
                    let char_start = self.pos - 1;
                    let mut len = 1;
                    while self
                        .peek()
                        .map(|b| (b & 0b1100_0000) == 0b1000_0000)
                        .unwrap_or(false)
                    {
                        self.pos += 1;
                        len += 1;
                    }
                    let s = std::str::from_utf8(&self.src[char_start..char_start + len]).map_err(
                        |_| LexError {
                            message: "invalid UTF-8 in string literal".into(),
                            span: Span::new(char_start, char_start + len),
                        },
                    )?;
                    body.push_str(s);
                    let _ = other;
                }
            }
        }
        let body_end = self.pos - 1; // before closing quote
        let kind = if prefix.interpolated {
            if !body.is_empty() {
                parts.push(StringPart::Literal(body));
            }
            StringLit::Interpolated {
                encoding: prefix.encoding,
                parts,
                span: Span::new(start, self.pos),
            }
        } else {
            self.materialise_string(prefix.encoding, body, body_start, body_end)?
        };
        Ok(Token::new(TokenKind::Str(kind), Span::new(start, self.pos)))
    }

    /// Scan a `${...}` slot, returning the raw text between `${` and
    /// the matching `}` (exclusive) plus the byte offset of the byte
    /// after the closing `}`. `slot_start` is the source offset of the
    /// `$`; `text_start` is the source offset of the first body byte
    /// (i.e. just after the `${`). Non-mutating: `self.pos` is left
    /// untouched so the heredoc body assembler can drive its own
    /// per-line cursor.
    fn scan_interp_slot(
        &self,
        slot_start: usize,
        text_start: usize,
    ) -> Result<(String, usize), LexError> {
        let mut pos = text_start;
        let mut depth: usize = 1;
        loop {
            let Some(&c) = self.src.get(pos) else {
                return Err(LexError {
                    message: "unterminated '${...}' interpolation slot".into(),
                    span: Span::new(slot_start, pos),
                });
            };
            match c {
                b'}' => {
                    depth -= 1;
                    if depth == 0 {
                        let text = std::str::from_utf8(&self.src[text_start..pos])
                            .map_err(|_| LexError {
                                message: "invalid UTF-8 in interpolation slot".into(),
                                span: Span::new(slot_start, pos),
                            })?
                            .to_string();
                        return Ok((text, pos + 1));
                    }
                    pos += 1;
                }
                b'{' => {
                    depth += 1;
                    pos += 1;
                }
                b'"' => {
                    pos += 1;
                    while let Some(&b) = self.src.get(pos) {
                        pos += 1;
                        if b == b'\\' {
                            if self.src.get(pos).is_some() {
                                pos += 1;
                            }
                            continue;
                        }
                        if b == b'"' {
                            break;
                        }
                        if b == b'\n' {
                            return Err(LexError {
                                message: "newline inside interpolation slot string".into(),
                                span: Span::new(slot_start, pos),
                            });
                        }
                    }
                }
                b'<' if self.src.get(pos + 1) == Some(&b'<')
                    && matches!(self.src.get(pos + 2), Some(&b) if is_ident_start(b)) =>
                {
                    return Err(LexError {
                        message: "heredoc literals are not allowed inside an interpolation slot"
                            .into(),
                        span: Span::new(pos, pos + 2),
                    });
                }
                b'\n' => {
                    return Err(LexError {
                        message: "interpolation slot must not span multiple lines".into(),
                        span: Span::new(slot_start, pos),
                    });
                }
                b'#' => {
                    while let Some(&b) = self.src.get(pos) {
                        if b == b'\n' {
                            break;
                        }
                        pos += 1;
                    }
                }
                b'/' if self.src.get(pos + 1) == Some(&b'/') => {
                    while let Some(&b) = self.src.get(pos) {
                        if b == b'\n' {
                            break;
                        }
                        pos += 1;
                    }
                }
                _ => {
                    pos += 1;
                }
            }
        }
    }

    /// Lex a heredoc literal. The leading `<<` (and any typed prefix) is
    /// already consumed; `self.pos` is at the first byte of the tag
    /// identifier. `start` covers the opener including prefix.
    ///
    /// Grammar:
    ///   `<<TAG\n` body `\n` (whitespace)* `TAG` (whitespace)* (`\n` | EOF)
    ///
    /// The body is escape-interpreted (same table as `"..."`), and
    /// common leading whitespace across non-blank lines is stripped.
    pub(super) fn lex_heredoc(
        &mut self,
        start: usize,
        prefix: StringPrefix,
    ) -> Result<Token, LexError> {
        let tag = self.lex_heredoc_opener(start, prefix.raw)?;
        let raw_lines = self.scan_heredoc_body(start, &tag)?;
        let min_indent = raw_lines
            .iter()
            .filter(|(_, l)| !is_blank(l))
            .map(|(_, l)| leading_ws_len(l))
            .min()
            .unwrap_or(0);
        let token_span = Span::new(start, self.pos);
        let body_end = self.pos;
        if prefix.raw {
            self.build_heredoc_raw(start, prefix, &raw_lines, min_indent, body_end, token_span)
        } else if prefix.interpolated {
            self.build_heredoc_interpolated(start, prefix, &raw_lines, min_indent, token_span)
        } else {
            self.build_heredoc_plain(start, prefix, &raw_lines, min_indent, body_end, token_span)
        }
    }

    /// Parse the heredoc opener: the tag identifier, same-line trivia
    /// (spaces, tabs, line comments), and the terminating `\n`. Leaves
    /// `self.pos` at the first byte of the body. Returns the tag.
    fn lex_heredoc_opener(&mut self, start: usize, raw: bool) -> Result<String, LexError> {
        // Raw heredocs quote the tag (`<<'TAG'`); the cursor is at the
        // opening `'`. Consume it; the closer line is still the bare tag.
        if raw {
            self.pos += 1; // opening `'`
        }
        let tag_start = self.pos;
        while matches!(self.peek(), Some(c) if is_ident_cont(c)) {
            self.pos += 1;
        }
        if self.pos == tag_start {
            return Err(LexError {
                message: "heredoc tag must be a non-empty identifier".into(),
                span: Span::new(start, self.pos),
            });
        }
        let tag = std::str::from_utf8(&self.src[tag_start..self.pos])
            .expect("ident is ASCII")
            .to_string();
        if raw {
            if self.peek() != Some(b'\'') {
                return Err(LexError {
                    message: "raw heredoc tag must be closed with a single quote (<<'TAG')".into(),
                    span: Span::new(start, self.pos),
                });
            }
            self.pos += 1; // closing `'`
        }

        // Same-line trailing trivia. Anything other than ws / line
        // comment is a hard error — the user almost certainly meant to
        // type the body on the next line.
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\r') => self.pos += 1,
                Some(b'#') => self.skip_line_to_newline(),
                Some(b'/') if self.peek_at(1) == Some(b'/') => self.skip_line_to_newline(),
                Some(b'\n') | None => break,
                Some(_) => {
                    return Err(LexError {
                        message: "unexpected text after heredoc tag".into(),
                        span: Span::new(self.pos, self.pos + 1),
                    });
                }
            }
        }

        if self.peek() != Some(b'\n') {
            return Err(LexError {
                message: format!("unterminated heredoc starting with '<<{tag}'"),
                span: Span::new(start, self.pos),
            });
        }
        self.pos += 1; // consume opener-line `\n`
        Ok(tag)
    }

    /// Capture raw source lines until we see a line whose trimmed
    /// contents are exactly the tag. Each entry tracks both the
    /// line's byte slice and its source-byte offset so interpolation
    /// slot spans line up with the outer source. Consumes the closer
    /// line and its trailing newline.
    fn scan_heredoc_body(
        &mut self,
        start: usize,
        tag: &str,
    ) -> Result<Vec<(usize, &'a [u8])>, LexError> {
        let mut raw_lines: Vec<(usize, &'a [u8])> = Vec::new();
        loop {
            let line_start = self.pos;
            while let Some(c) = self.peek() {
                if c == b'\n' {
                    break;
                }
                self.pos += 1;
            }
            let line_end = self.pos;
            let line = &self.src[line_start..line_end];

            if line_is_closer(line, tag) {
                if self.peek() == Some(b'\n') {
                    self.pos += 1;
                }
                return Ok(raw_lines);
            }

            // EOF without finding the closer → unterminated.
            if self.peek().is_none() {
                return Err(LexError {
                    message: format!("unterminated heredoc starting with '<<{tag}'"),
                    span: Span::new(start, self.pos),
                });
            }
            raw_lines.push((line_start, line));
            self.pos += 1; // consume the `\n` between lines
        }
    }

    /// Splits the body on `${...}` slots into alternating literal /
    /// Expr parts. Slot spans are anchored at the outer source's byte
    /// offsets so diagnostics from sub-parses point at the right
    /// place.
    fn build_heredoc_interpolated(
        &self,
        start: usize,
        prefix: StringPrefix,
        raw_lines: &[(usize, &[u8])],
        min_indent: usize,
        token_span: Span,
    ) -> Result<Token, LexError> {
        let mut parts: Vec<StringPart> = Vec::new();
        let mut buf = String::new();
        for (line_src_start, line) in raw_lines {
            let trim = if is_blank(line) {
                line.len()
            } else {
                min_indent.min(line.len())
            };
            let stripped_start = line_src_start + trim;
            let stripped = &line[trim..];
            self.interpret_line_with_interp(stripped, stripped_start, start, &mut buf, &mut parts)?;
            buf.push('\n');
        }
        if !buf.is_empty() {
            parts.push(StringPart::Literal(buf));
        }
        Ok(Token::new(
            TokenKind::Str(StringLit::Interpolated {
                encoding: prefix.encoding,
                parts,
                span: token_span,
            }),
            token_span,
        ))
    }

    /// Plain (non-interpolated) body: escape-decode each indent-
    /// stripped line, then hand off to the shared encoding
    /// materialiser (ASCII validation, UTF-16/UTF-32 encoding).
    fn build_heredoc_plain(
        &self,
        start: usize,
        prefix: StringPrefix,
        raw_lines: &[(usize, &[u8])],
        min_indent: usize,
        body_end: usize,
        token_span: Span,
    ) -> Result<Token, LexError> {
        let mut body = String::new();
        for (_, line) in raw_lines {
            let stripped: &[u8] = if is_blank(line) {
                &[]
            } else {
                &line[min_indent.min(line.len())..]
            };
            interpret_escapes_into(stripped, start, &mut body)?;
            body.push('\n');
        }
        let kind = self.materialise_string(prefix.encoding, body, start, body_end)?;
        Ok(Token::new(TokenKind::Str(kind), token_span))
    }

    /// Raw (`<<'TAG'`) body: indent-stripped lines copied verbatim — no
    /// escape decoding, no `${...}` interpolation. The source is valid
    /// UTF-8 and indent stripping only removes leading ASCII whitespace,
    /// so each stripped line stays a valid UTF-8 slice.
    fn build_heredoc_raw(
        &self,
        start: usize,
        prefix: StringPrefix,
        raw_lines: &[(usize, &[u8])],
        min_indent: usize,
        body_end: usize,
        token_span: Span,
    ) -> Result<Token, LexError> {
        let mut body = String::new();
        for (line_src_start, line) in raw_lines {
            let stripped: &[u8] = if is_blank(line) {
                &[]
            } else {
                &line[min_indent.min(line.len())..]
            };
            let s = std::str::from_utf8(stripped).map_err(|_| LexError {
                message: "invalid UTF-8 in raw heredoc".into(),
                span: Span::new(*line_src_start, line_src_start + stripped.len()),
            })?;
            body.push_str(s);
            body.push('\n');
        }
        let kind = self.materialise_string(prefix.encoding, body, start, body_end)?;
        Ok(Token::new(TokenKind::Str(kind), token_span))
    }

    /// Walk a heredoc body line that may contain `${...}` slots.
    /// `line_src_start` is the byte offset of the line's first byte
    /// in `self.src` (so slot spans are aligned with the outer
    /// source). Appends escape-decoded text into `buf` and slot
    /// captures into `parts` (flushing `buf` to a Literal whenever a
    /// slot is encountered).
    fn interpret_line_with_interp(
        &self,
        line: &[u8],
        line_src_start: usize,
        opener: usize,
        buf: &mut String,
        parts: &mut Vec<StringPart>,
    ) -> Result<(), LexError> {
        let mut i = 0;
        while i < line.len() {
            // Slot opener: `${`.
            if line[i] == b'$' && line.get(i + 1) == Some(&b'{') {
                if !buf.is_empty() {
                    parts.push(StringPart::Literal(std::mem::take(buf)));
                }
                let slot_start = line_src_start + i;
                let text_start = slot_start + 2;
                let (text, slot_end) = self.scan_interp_slot(slot_start, text_start)?;
                parts.push(StringPart::Expr {
                    text,
                    span: Span::new(slot_start, slot_end),
                });
                // Skip past the closing `}` in the line slice.
                i = slot_end - line_src_start;
                continue;
            }
            let c = line[i];
            match c {
                b'\\' => {
                    let esc_pos = i;
                    let Some(&esc) = line.get(i + 1) else {
                        return Err(LexError {
                            message: "unterminated escape sequence in heredoc".into(),
                            span: Span::new(opener, opener + 1),
                        });
                    };
                    match esc {
                        b'"' => buf.push('"'),
                        b'\\' => buf.push('\\'),
                        b'n' => buf.push('\n'),
                        b't' => buf.push('\t'),
                        b'r' => buf.push('\r'),
                        b'$' => buf.push('$'),
                        other => {
                            return Err(LexError {
                                message: format!("invalid escape '\\{}'", other as char),
                                span: Span::new(opener + esc_pos, opener + esc_pos + 2),
                            });
                        }
                    }
                    i += 2;
                }
                b if b < 0x80 => {
                    buf.push(b as char);
                    i += 1;
                }
                _ => {
                    let char_start = i;
                    let mut len = 1;
                    while line
                        .get(i + len)
                        .map(|b| (b & 0b1100_0000) == 0b1000_0000)
                        .unwrap_or(false)
                    {
                        len += 1;
                    }
                    let slice = &line[char_start..char_start + len];
                    let s = std::str::from_utf8(slice).map_err(|_| LexError {
                        message: "invalid UTF-8 in heredoc body".into(),
                        span: Span::new(opener + char_start, opener + char_start + len),
                    })?;
                    buf.push_str(s);
                    i += len;
                }
            }
        }
        Ok(())
    }

    /// Skip from the current position to (but not including) the next
    /// newline. Used inside heredoc-opener trivia scanning, where we
    /// must preserve the body-starting `\n` for the caller to consume.
    fn skip_line_to_newline(&mut self) {
        while let Some(c) = self.peek() {
            if c == b'\n' {
                break;
            }
            self.pos += 1;
        }
    }

    fn materialise_string(
        &self,
        encoding: StringEncoding,
        body: String,
        body_start: usize,
        body_end: usize,
    ) -> Result<StringLit, LexError> {
        match encoding {
            StringEncoding::Utf8 => Ok(StringLit::Utf8(body)),
            StringEncoding::Ascii => {
                if let Some(bad) = body.char_indices().find(|(_, c)| (*c as u32) >= 0x80) {
                    let (offset, _) = bad;
                    let start = body_start + offset;
                    return Err(LexError {
                        message: "non-ASCII character in ascii string literal".into(),
                        span: Span::new(start, start + 1),
                    });
                }
                Ok(StringLit::Ascii(body))
            }
            StringEncoding::Utf16 => {
                let _ = body_end;
                Ok(StringLit::Utf16(body.encode_utf16().collect()))
            }
            StringEncoding::Utf32 => Ok(StringLit::Utf32(body.chars().collect())),
        }
    }
}

/// True iff the line contains only ASCII whitespace (spaces, tabs,
/// CR). Heredoc indent stripping ignores blank lines when computing
/// the minimum prefix.
fn is_blank(line: &[u8]) -> bool {
    line.iter().all(|b| matches!(*b, b' ' | b'\t' | b'\r'))
}

/// Number of leading ASCII whitespace bytes on the line.
fn leading_ws_len(line: &[u8]) -> usize {
    line.iter()
        .take_while(|b| matches!(**b, b' ' | b'\t'))
        .count()
}

/// True iff the line is a heredoc closer: leading whitespace, then
/// the exact tag, then trailing whitespace (CRs allowed).
fn line_is_closer(line: &[u8], tag: &str) -> bool {
    let lead = leading_ws_len(line);
    let rest = &line[lead..];
    if !rest.starts_with(tag.as_bytes()) {
        return false;
    }
    let after = &rest[tag.len()..];
    after.iter().all(|b| matches!(*b, b' ' | b'\t' | b'\r'))
}

/// Decode a single backslash-escape byte into its char value, sharing
/// the table between `lex_string` and `interpret_escapes_into`.
/// Returns `None` for unknown escapes (callers report the error so the
/// span fits the local cursor).
fn decode_escape_char(esc: u8, allow_dollar: bool) -> Option<char> {
    match esc {
        b'"' => Some('"'),
        b'\\' => Some('\\'),
        b'n' => Some('\n'),
        b't' => Some('\t'),
        b'r' => Some('\r'),
        b'$' if allow_dollar => Some('$'),
        _ => None,
    }
}

/// Apply the same escape table as `lex_string` to `line`, appending
/// the decoded chars to `out`. Validates UTF-8 for non-ASCII bytes.
fn interpret_escapes_into(line: &[u8], opener: usize, out: &mut String) -> Result<(), LexError> {
    let mut i = 0;
    while i < line.len() {
        let c = line[i];
        match c {
            b'\\' => {
                let esc_pos = i;
                let Some(&esc) = line.get(i + 1) else {
                    return Err(LexError {
                        message: "unterminated escape sequence in heredoc".into(),
                        span: Span::new(opener, opener + 1),
                    });
                };
                let Some(decoded) = decode_escape_char(esc, false) else {
                    return Err(LexError {
                        message: format!("invalid escape '\\{}'", esc as char),
                        span: Span::new(opener + esc_pos, opener + esc_pos + 2),
                    });
                };
                out.push(decoded);
                i += 2;
            }
            b if b < 0x80 => {
                out.push(b as char);
                i += 1;
            }
            _ => {
                // Multi-byte UTF-8: walk continuation bytes and copy
                // the original slice through std's validator so a bad
                // sequence becomes a structured error rather than a
                // panic.
                let char_start = i;
                let mut len = 1;
                while line
                    .get(i + len)
                    .map(|b| (b & 0b1100_0000) == 0b1000_0000)
                    .unwrap_or(false)
                {
                    len += 1;
                }
                let slice = &line[char_start..char_start + len];
                let s = std::str::from_utf8(slice).map_err(|_| LexError {
                    message: "invalid UTF-8 in heredoc body".into(),
                    span: Span::new(opener + char_start, opener + char_start + len),
                })?;
                out.push_str(s);
                i += len;
            }
        }
    }
    Ok(())
}
