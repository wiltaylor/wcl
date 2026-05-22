use crate::ast::{Span, Trivia};
use crate::numeric::{self, ParsedNumber};

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Ident(String),
    Bool(bool),
    Number(NumberLit),
    Str(StringLit),
    Symbol(String),
    None,
    If,
    Else,
    Match,
    Eq,
    EqEq,
    FatArrow,
    BangEq,
    Bang,
    Colon,
    ColonColon,
    Question,
    Amp,
    AmpAmp,
    Pipe,
    PipePipe,
    Dot,
    DotDot,
    Comma,
    Semi,
    Lt,
    LtEq,
    Gt,
    GtEq,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    At,
    LParen,
    RParen,
    Plus,
    Dash,
    Arrow,
    Star,
    Slash,
    Percent,
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NumberLit {
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    Isize(isize),

    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    Usize(usize),

    F32(f32),
    F64(f64),
}

impl NumberLit {
    /// Convert an integer literal to `u64`. Returns `None` for floats,
    /// for negative signed values, and for magnitudes that don't fit.
    pub fn as_u64(&self) -> Option<u64> {
        match *self {
            NumberLit::I8(v) if v >= 0 => Some(v as u64),
            NumberLit::I16(v) if v >= 0 => Some(v as u64),
            NumberLit::I32(v) if v >= 0 => Some(v as u64),
            NumberLit::I64(v) if v >= 0 => Some(v as u64),
            NumberLit::I128(v) if v >= 0 => u64::try_from(v).ok(),
            NumberLit::Isize(v) if v >= 0 => Some(v as u64),
            NumberLit::U8(v) => Some(v as u64),
            NumberLit::U16(v) => Some(v as u64),
            NumberLit::U32(v) => Some(v as u64),
            NumberLit::U64(v) => Some(v),
            NumberLit::U128(v) => u64::try_from(v).ok(),
            NumberLit::Usize(v) => u64::try_from(v).ok(),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StringLit {
    Utf8(String),
    Ascii(String),
    Utf16(Vec<u16>),
    Utf32(Vec<char>),
    /// Opt-in interpolated literal (`$"…"`, `$ascii"…"`, `$<<TAG`, …).
    /// The body is split into already-escape-decoded literal chunks
    /// and raw source slices for `${expr}` slots that the parser later
    /// sub-parses into expressions.
    Interpolated {
        encoding: StringEncoding,
        parts: Vec<StringPart>,
        span: Span,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StringEncoding {
    Utf8,
    Ascii,
    Utf16,
    Utf32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum StringPart {
    /// Already-decoded body bytes between (or around) slots.
    Literal(String),
    /// Raw source text inside a `${...}` slot, plus the slot's full
    /// span (covering the `${` and `}`). The parser sub-parses this
    /// into an `Expr` at parse time.
    Expr { text: String, span: Span },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    /// Comments + blank-line breaks that appeared in the source
    /// immediately before this token. The parser pulls these onto the
    /// next Item it builds so the source printer can re-emit them in
    /// roughly the original position. Same-line trailing comments
    /// after a token end up here as the *next* token's leading trivia
    /// — a known simplification, see [`ast::Trivia`].
    pub leading_trivia: Vec<Trivia>,
}

impl Token {
    /// Build a Token with no leading trivia. Inner lex paths
    /// (`lex_string`, `lex_number`, …) use this; the outer
    /// `next_token` overwrites `leading_trivia` on whatever token they
    /// produce. Callers outside the lexer shouldn't need this.
    pub(crate) fn new(kind: TokenKind, span: Span) -> Self {
        Self {
            kind,
            span,
            leading_trivia: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub message: String,
    pub span: Span,
}

pub struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    pub fn new(src: &'a str) -> Self {
        Self {
            src: src.as_bytes(),
            pos: 0,
        }
    }

    pub fn next_token(&mut self) -> Result<Token, LexError> {
        let leading_trivia = self.collect_trivia();
        let start = self.pos;
        let Some(c) = self.peek() else {
            return Ok(Token {
                kind: TokenKind::Eof,
                span: Span::new(start, start),
                leading_trivia,
            });
        };
        let mut tok = self.lex_after_trivia(start, c)?;
        tok.leading_trivia = leading_trivia;
        Ok(tok)
    }

    fn lex_after_trivia(&mut self, start: usize, c: u8) -> Result<Token, LexError> {
        match c {
            b'=' => match self.peek_at(1) {
                Some(b'=') => {
                    self.pos += 2;
                    Ok(Token::new(TokenKind::EqEq, Span::new(start, self.pos)))
                }
                Some(b'>') => {
                    self.pos += 2;
                    Ok(Token::new(TokenKind::FatArrow, Span::new(start, self.pos)))
                }
                _ => Ok(self.single(start, TokenKind::Eq)),
            },
            b'!' => Ok(self.two_or_one(start, b'=', TokenKind::BangEq, TokenKind::Bang)),
            b'<' => {
                // Bare heredoc opener: `<<TAG`. Typed-prefix forms
                // (`ascii<<TAG`, …) route via `lex_ident_or_typed`.
                if self.peek_at(1) == Some(b'<')
                    && matches!(self.peek_at(2), Some(c) if is_ident_start(c))
                {
                    self.pos += 2; // consume `<<`
                    return self.lex_heredoc(start, StringPrefix::plain(StringEncoding::Utf8));
                }
                Ok(self.two_or_one(start, b'=', TokenKind::LtEq, TokenKind::Lt))
            }
            b'>' => Ok(self.two_or_one(start, b'=', TokenKind::GtEq, TokenKind::Gt)),
            b'&' => Ok(self.two_or_one(start, b'&', TokenKind::AmpAmp, TokenKind::Amp)),
            b'|' => Ok(self.two_or_one(start, b'|', TokenKind::PipePipe, TokenKind::Pipe)),
            b':' => {
                // `::` for variant paths wins over single `:`.
                if self.peek_at(1) == Some(b':') {
                    self.pos += 2;
                    return Ok(Token::new(
                        TokenKind::ColonColon,
                        Span::new(start, self.pos),
                    ));
                }
                // Tight `:foo` (no whitespace) → Symbol literal.
                if matches!(self.peek_at(1), Some(c) if is_ident_start(c)) {
                    self.pos += 1; // ':'
                    let name_start = self.pos;
                    while matches!(self.peek(), Some(c) if is_ident_cont(c)) {
                        self.pos += 1;
                    }
                    let name = std::str::from_utf8(&self.src[name_start..self.pos])
                        .expect("ident is ASCII")
                        .to_string();
                    Ok(Token::new(
                        TokenKind::Symbol(name),
                        Span::new(start, self.pos),
                    ))
                } else {
                    Ok(self.single(start, TokenKind::Colon))
                }
            }
            b'?' => Ok(self.single(start, TokenKind::Question)),
            b'.' => {
                if self.peek_at(1) == Some(b'.') {
                    self.pos += 2;
                    Ok(Token::new(TokenKind::DotDot, Span::new(start, self.pos)))
                } else {
                    Ok(self.single(start, TokenKind::Dot))
                }
            }
            b',' => Ok(self.single(start, TokenKind::Comma)),
            b';' => Ok(self.single(start, TokenKind::Semi)),
            b'[' => Ok(self.single(start, TokenKind::LBracket)),
            b']' => Ok(self.single(start, TokenKind::RBracket)),
            b'@' => Ok(self.single(start, TokenKind::At)),
            b'(' => Ok(self.single(start, TokenKind::LParen)),
            b')' => Ok(self.single(start, TokenKind::RParen)),
            b'{' => Ok(self.single(start, TokenKind::LBrace)),
            b'}' => Ok(self.single(start, TokenKind::RBrace)),
            b'+' => Ok(self.single(start, TokenKind::Plus)),
            b'*' => Ok(self.single(start, TokenKind::Star)),
            b'/' => Ok(self.single(start, TokenKind::Slash)),
            b'%' => Ok(self.single(start, TokenKind::Percent)),
            b'"' => self.lex_string(start, StringPrefix::plain(StringEncoding::Utf8)),
            b'$' => self.lex_dollar_prefix(start),
            b'-' => {
                // `->` always wins.
                if self.peek_at(1) == Some(b'>') {
                    self.pos += 2;
                    Ok(Token::new(TokenKind::Arrow, Span::new(start, self.pos)))
                } else if matches!(self.peek_at(1), Some(b'0'..=b'9'))
                    && (start == 0 || is_pre_value_separator(self.src[start - 1]))
                {
                    // Tight signed-number form: only when nothing value-shaped
                    // is immediately to the left, so `a - 1` and `a-1` both
                    // parse as subtraction once expressions are evaluated.
                    self.lex_number(start)
                } else {
                    Ok(self.single(start, TokenKind::Dash))
                }
            }
            b'0'..=b'9' => self.lex_number(start),
            c if is_ident_start(c) => self.lex_ident_or_typed(start),
            other => Err(LexError {
                message: format!("unexpected character {:?}", other as char),
                span: Span::new(start, start + 1),
            }),
        }
    }

    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.src.get(self.pos + offset).copied()
    }

    /// Advance through a digit run that allows `_` as a thousands separator.
    /// Underscores may only follow a digit; a leading `_` (or `_` after `_`)
    /// is a hard error. Reports whether at least one digit was consumed and
    /// whether the run ended on `_`. Callers decide what to do with both.
    fn scan_digits_with_underscores<F>(&mut self, is_digit: F) -> Result<DigitScan, LexError>
    where
        F: Fn(u8) -> bool,
    {
        let mut had_digit = false;
        let mut trailing_underscore = false;
        while let Some(c) = self.peek() {
            if c == b'_' {
                if !had_digit {
                    return Err(LexError {
                        message: "underscore must follow a digit".into(),
                        span: Span::new(self.pos, self.pos + 1),
                    });
                }
                trailing_underscore = true;
                self.pos += 1;
            } else if is_digit(c) {
                had_digit = true;
                trailing_underscore = false;
                self.pos += 1;
            } else {
                break;
            }
        }
        Ok(DigitScan {
            had_digit,
            trailing_underscore,
        })
    }

    fn bump(&mut self) -> Option<u8> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    fn single(&mut self, start: usize, kind: TokenKind) -> Token {
        self.pos += 1;
        Token::new(kind, Span::new(start, self.pos))
    }

    fn two_or_one(&mut self, start: usize, follow: u8, two: TokenKind, one: TokenKind) -> Token {
        if self.peek_at(1) == Some(follow) {
            self.pos += 2;
            Token::new(two, Span::new(start, self.pos))
        } else {
            self.pos += 1;
            Token::new(one, Span::new(start, self.pos))
        }
    }

    /// Walk through whitespace and comments, capturing each significant
    /// fragment as a [`Trivia`] entry. Returns the accumulated trivia;
    /// `self.pos` is left at the first non-trivia byte. Replaces the
    /// older `skip_trivia` (which silently discarded everything).
    ///
    /// Comment payload is stored without the leading `#` or `//` and
    /// without the trailing newline. Multiple consecutive blank-line
    /// breaks collapse to a single [`Trivia::BlankLine`] — canonical
    /// output emits at most one blank between Items.
    fn collect_trivia(&mut self) -> Vec<Trivia> {
        let mut out = Vec::new();
        let mut newlines_in_run = 0usize;
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\r') => {
                    self.pos += 1;
                }
                Some(b'\n') => {
                    self.pos += 1;
                    newlines_in_run += 1;
                    // Two consecutive newlines (with only spaces/tabs
                    // between) indicate a blank line. Subsequent
                    // newlines in the same run don't add more breaks
                    // — canonical output is one blank max.
                    if newlines_in_run == 2 {
                        out.push(Trivia::BlankLine);
                    }
                }
                Some(b'#') => {
                    let text = self.consume_line_comment(1);
                    out.push(Trivia::LineComment(text));
                    // The skipped line ended on a newline — that
                    // newline is "consumed" by the comment, so the
                    // run counter resets for any *additional* blank
                    // lines that follow.
                    newlines_in_run = 1;
                }
                Some(b'/') if self.peek_at(1) == Some(b'/') => {
                    let text = self.consume_line_comment(2);
                    out.push(Trivia::LineComment(text));
                    newlines_in_run = 1;
                }
                _ => break,
            }
        }
        out
    }

    /// Consume `marker_len` prefix bytes (1 for `#`, 2 for `//`), then
    /// everything up to and including the next `\n` (or EOF). Returns
    /// the comment payload between the prefix and the newline,
    /// stripped of trailing whitespace.
    fn consume_line_comment(&mut self, marker_len: usize) -> String {
        self.pos += marker_len;
        let text_start = self.pos;
        while let Some(c) = self.peek() {
            self.pos += 1;
            if c == b'\n' {
                let body = &self.src[text_start..self.pos - 1];
                return std::str::from_utf8(body).unwrap_or("").trim().to_string();
            }
        }
        // EOF inside a comment — payload runs to end of source.
        std::str::from_utf8(&self.src[text_start..self.pos])
            .unwrap_or("")
            .trim()
            .to_string()
    }

    /// Handle the `$`-prefix opener: `$"…"`, `$<<TAG`,
    /// `$ascii"…"`, `$utf16<<TAG`, etc. `self.pos` is on the `$`.
    fn lex_dollar_prefix(&mut self, start: usize) -> Result<Token, LexError> {
        self.pos += 1; // consume `$`
        // Optional encoding name (ascii / utf16 / utf32). Bare $" or
        // $<< default to utf8.
        let encoding_start = self.pos;
        let mut encoding = StringEncoding::Utf8;
        if matches!(self.peek(), Some(c) if is_ident_start(c)) {
            while matches!(self.peek(), Some(c) if is_ident_cont(c)) {
                self.pos += 1;
            }
            let text =
                std::str::from_utf8(&self.src[encoding_start..self.pos]).expect("ident is ASCII");
            match StringPrefix::encoding_from_text(text) {
                Some(enc) => encoding = enc,
                None => {
                    return Err(LexError {
                        message: format!("unknown string encoding '{text}' after '$' prefix"),
                        span: Span::new(encoding_start, self.pos),
                    });
                }
            }
        }
        let prefix = StringPrefix::interp(encoding);
        match self.peek() {
            Some(b'"') => self.lex_string(start, prefix),
            Some(b'<')
                if self.peek_at(1) == Some(b'<')
                    && matches!(self.peek_at(2), Some(c) if is_ident_start(c)) =>
            {
                self.pos += 2; // consume `<<`
                self.lex_heredoc(start, prefix)
            }
            _ => Err(LexError {
                message: "expected '\"' or '<<' after '$' string prefix".into(),
                span: Span::new(start, self.pos),
            }),
        }
    }

    fn lex_string(&mut self, start: usize, prefix: StringPrefix) -> Result<Token, LexError> {
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
    fn lex_heredoc(&mut self, start: usize, prefix: StringPrefix) -> Result<Token, LexError> {
        let tag = self.lex_heredoc_opener(start)?;
        let raw_lines = self.scan_heredoc_body(start, &tag)?;
        let min_indent = raw_lines
            .iter()
            .filter(|(_, l)| !is_blank(l))
            .map(|(_, l)| leading_ws_len(l))
            .min()
            .unwrap_or(0);
        let token_span = Span::new(start, self.pos);
        let body_end = self.pos;
        if prefix.interpolated {
            self.build_heredoc_interpolated(start, prefix, &raw_lines, min_indent, token_span)
        } else {
            self.build_heredoc_plain(start, prefix, &raw_lines, min_indent, body_end, token_span)
        }
    }

    /// Parse the heredoc opener: the tag identifier, same-line trivia
    /// (spaces, tabs, line comments), and the terminating `\n`. Leaves
    /// `self.pos` at the first byte of the body. Returns the tag.
    fn lex_heredoc_opener(&mut self, start: usize) -> Result<String, LexError> {
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

    fn lex_number(&mut self, start: usize) -> Result<Token, LexError> {
        let neg = if self.peek() == Some(b'-') {
            self.pos += 1;
            true
        } else {
            false
        };

        // Detect base prefix (only after optional sign, at start of digits).
        let base = match (self.peek(), self.peek_at(1)) {
            (Some(b'0'), Some(b'x' | b'X')) => {
                self.pos += 2;
                16
            }
            (Some(b'0'), Some(b'b' | b'B')) => {
                self.pos += 2;
                2
            }
            (Some(b'0'), Some(b'o' | b'O')) => {
                self.pos += 2;
                8
            }
            _ => 10,
        };

        let body_start = self.pos;
        let body_scan = self.scan_digits_with_underscores(|c| is_digit_in_base(c, base))?;
        if !body_scan.had_digit {
            // If the next character is alphanumeric, treat it as a bad digit
            // for the chosen base (more helpful than "expected digits").
            if let Some(c) = self.peek()
                && c.is_ascii_alphanumeric()
            {
                return Err(LexError {
                    message: format!("invalid digit '{}' for base {base}", c as char),
                    span: Span::new(self.pos, self.pos + 1),
                });
            }
            return Err(LexError {
                message: "expected digits in numeric literal".into(),
                span: Span::new(start, self.pos),
            });
        }
        if body_scan.trailing_underscore {
            return Err(LexError {
                message: "trailing underscore in numeric literal".into(),
                span: Span::new(self.pos - 1, self.pos),
            });
        }
        let body_end = self.pos;

        let mut is_float = false;
        let mut frac_end = body_end;
        if base == 10 && self.peek() == Some(b'.') && matches!(self.peek_at(1), Some(b'0'..=b'9')) {
            is_float = true;
            self.pos += 1; // consume '.'
            let frac_scan = self.scan_digits_with_underscores(|c| c.is_ascii_digit())?;
            if frac_scan.trailing_underscore {
                return Err(LexError {
                    message: "trailing underscore in numeric literal".into(),
                    span: Span::new(self.pos - 1, self.pos),
                });
            }
            frac_end = self.pos;
        }

        let mut exponent_text: Option<String> = None;
        if base == 10 && is_float && matches!(self.peek(), Some(b'e' | b'E')) {
            self.pos += 1; // consume 'e'
            let exp_start = self.pos;
            if matches!(self.peek(), Some(b'+' | b'-')) {
                self.pos += 1;
            }
            let exp_scan = self.scan_digits_with_underscores(|c| c.is_ascii_digit())?;
            if !exp_scan.had_digit {
                return Err(LexError {
                    message: "expected digits in exponent".into(),
                    span: Span::new(exp_start, self.pos),
                });
            }
            exponent_text = Some(
                std::str::from_utf8(&self.src[exp_start..self.pos])
                    .expect("ASCII exponent")
                    .replace('_', ""),
            );
        }

        let suffix_start = self.pos;
        while matches!(self.peek(), Some(c) if c.is_ascii_alphanumeric()) {
            self.pos += 1;
        }
        let suffix =
            std::str::from_utf8(&self.src[suffix_start..self.pos]).expect("suffix is ASCII");
        let literal_span = Span::new(start, self.pos);

        // Build the cleaned body string for the numeric helper.
        let body_text = std::str::from_utf8(&self.src[body_start..body_end]).expect("ASCII digits");
        let body_clean = body_text.replace('_', "");
        let body_for_finalize: String = if is_float {
            let frac_text =
                std::str::from_utf8(&self.src[body_end..frac_end]).expect("ASCII digits");
            let frac_clean = frac_text.replace('_', "");
            format!("{body_clean}{frac_clean}")
        } else {
            body_clean.clone()
        };

        let parsed = ParsedNumber {
            neg,
            base,
            body: &body_for_finalize,
            exponent: exponent_text.as_deref(),
            is_float,
            suffix,
        };

        numeric::finalize(parsed)
            .map(|n| Token::new(TokenKind::Number(n), literal_span))
            .map_err(|e| LexError {
                message: e.message,
                span: literal_span,
            })
    }

    fn lex_ident_or_typed(&mut self, start: usize) -> Result<Token, LexError> {
        while matches!(self.peek(), Some(c) if is_ident_cont(c)) {
            self.pos += 1;
        }
        let text = std::str::from_utf8(&self.src[start..self.pos]).expect("ident is ASCII");

        // Check for typed-string prefix: ident immediately followed by `"`.
        if self.peek() == Some(b'"')
            && let Some(encoding) = StringPrefix::encoding_from_text(text)
        {
            return self.lex_string(start, StringPrefix::plain(encoding));
        }

        // Typed heredoc prefix: `ascii<<TAG`, `utf16<<TAG`, etc.
        if self.peek() == Some(b'<')
            && self.peek_at(1) == Some(b'<')
            && matches!(self.peek_at(2), Some(c) if is_ident_start(c))
            && let Some(encoding) = StringPrefix::encoding_from_text(text)
        {
            self.pos += 2; // consume `<<`
            return self.lex_heredoc(start, StringPrefix::plain(encoding));
        }

        let span = Span::new(start, self.pos);
        let kind = match text {
            "true" => TokenKind::Bool(true),
            "false" => TokenKind::Bool(false),
            "none" => TokenKind::None,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "match" => TokenKind::Match,
            _ => TokenKind::Ident(text.to_string()),
        };
        Ok(Token::new(kind, span))
    }
}

#[derive(Debug, Clone, Copy)]
struct StringPrefix {
    encoding: StringEncoding,
    interpolated: bool,
}

impl StringPrefix {
    fn plain(encoding: StringEncoding) -> Self {
        Self {
            encoding,
            interpolated: false,
        }
    }

    fn interp(encoding: StringEncoding) -> Self {
        Self {
            encoding,
            interpolated: true,
        }
    }

    fn encoding_from_text(text: &str) -> Option<StringEncoding> {
        match text {
            "utf8" => Some(StringEncoding::Utf8),
            "ascii" => Some(StringEncoding::Ascii),
            "utf16" => Some(StringEncoding::Utf16),
            "utf32" => Some(StringEncoding::Utf32),
            _ => None,
        }
    }
}

struct DigitScan {
    had_digit: bool,
    trailing_underscore: bool,
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

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_ident_cont(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

fn is_digit_in_base(c: u8, base: u32) -> bool {
    (c as char).is_digit(base)
}

/// Returns true if `b` is the kind of byte that *cannot* end a value
/// expression — whitespace or a punctuation token that opens a fresh
/// value context. Used to decide whether a `-` immediately followed by
/// digits should be folded into a signed numeric literal or emitted as
/// a standalone `Dash` (binary subtraction / unary negation handled by
/// the parser).
fn is_pre_value_separator(b: u8) -> bool {
    matches!(
        b,
        b' ' | b'\t'
            | b'\n'
            | b'\r'
            | b'='
            | b'('
            | b'['
            | b'{'
            | b':'
            | b','
            | b';'
            | b'<'
            | b'>'
            | b'+'
            | b'-'
            | b'*'
            | b'/'
            | b'%'
            | b'&'
            | b'|'
            | b'!'
            | b'?'
            | b'@'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(src: &str) -> Vec<TokenKind> {
        let mut lex = Lexer::new(src);
        let mut out = Vec::new();
        loop {
            let t = lex.next_token().expect("lex error");
            let done = matches!(t.kind, TokenKind::Eof);
            out.push(t.kind);
            if done {
                break;
            }
        }
        out
    }

    fn one(src: &str) -> TokenKind {
        let mut lex = Lexer::new(src);
        let t = lex.next_token().expect("lex");
        t.kind
    }

    #[test]
    fn default_int_is_i64() {
        assert_eq!(one("42"), TokenKind::Number(NumberLit::I64(42)));
    }

    #[test]
    fn default_float_is_f64() {
        assert_eq!(one("1.25"), TokenKind::Number(NumberLit::F64(1.25)));
    }

    #[test]
    fn typed_int_suffix() {
        assert_eq!(one("8080i32"), TokenKind::Number(NumberLit::I32(8080)));
        assert_eq!(one("200u8"), TokenKind::Number(NumberLit::U8(200)));
        assert_eq!(one("-128i8"), TokenKind::Number(NumberLit::I8(-128)));
    }

    #[test]
    fn typed_float_suffix() {
        assert_eq!(one("1.5f32"), TokenKind::Number(NumberLit::F32(1.5)));
    }

    #[test]
    fn underscores_in_digits() {
        assert_eq!(
            one("1_000_000"),
            TokenKind::Number(NumberLit::I64(1_000_000))
        );
    }

    #[test]
    fn hex_bin_oct_bases() {
        assert_eq!(one("0xFFu8"), TokenKind::Number(NumberLit::U8(255)));
        assert_eq!(
            one("0b1010_1100u8"),
            TokenKind::Number(NumberLit::U8(0b1010_1100))
        );
        assert_eq!(one("0o755u16"), TokenKind::Number(NumberLit::U16(0o755)));
        // unsuffixed hex defaults to i64
        assert_eq!(one("0x10"), TokenKind::Number(NumberLit::I64(16)));
    }

    #[test]
    fn overflow_errors_with_literal_span() {
        let mut lex = Lexer::new("200i8");
        let err = lex.next_token().unwrap_err();
        assert!(err.message.contains("out of range"));
        assert_eq!(err.span, Span::new(0, 5));
    }

    #[test]
    fn negative_unsigned_errors() {
        let mut lex = Lexer::new("-1u32");
        let err = lex.next_token().unwrap_err();
        assert!(err.message.contains("unsigned"));
    }

    #[test]
    fn unknown_suffix_errors() {
        let mut lex = Lexer::new("1zz");
        let err = lex.next_token().unwrap_err();
        assert!(err.message.contains("unknown"));
    }

    #[test]
    fn invalid_digit_for_base() {
        let mut lex = Lexer::new("0b2");
        let err = lex.next_token().unwrap_err();
        assert!(err.message.contains("invalid digit"));
    }

    #[test]
    fn trailing_underscore_rejected() {
        let mut lex = Lexer::new("1_000_");
        let err = lex.next_token().unwrap_err();
        assert!(err.message.contains("trailing"));
    }

    #[test]
    fn float_exponent_requires_decimal_point() {
        // Per the lexer rules: float mode is triggered by '.' followed by digit.
        // `2e3` therefore lexes as int `2` followed by ident `e3` — which yields
        // an unknown-suffix lex error. That keeps the grammar simple.
        let mut lex = Lexer::new("2e3");
        let err = lex.next_token().unwrap_err();
        assert!(err.message.contains("unknown"));
    }

    #[test]
    fn float_with_explicit_decimal_and_exponent() {
        assert_eq!(one("1.5e3"), TokenKind::Number(NumberLit::F64(1500.0)));
        assert_eq!(one("2.0e-3"), TokenKind::Number(NumberLit::F64(0.002)));
    }

    #[test]
    fn default_utf8_string() {
        assert_eq!(
            one(r#""hello""#),
            TokenKind::Str(StringLit::Utf8("hello".into()))
        );
    }

    #[test]
    fn explicit_utf8_prefix() {
        assert_eq!(
            one(r#"utf8"hi""#),
            TokenKind::Str(StringLit::Utf8("hi".into()))
        );
    }

    #[test]
    fn ascii_prefix_validates() {
        assert_eq!(
            one(r#"ascii"alpha""#),
            TokenKind::Str(StringLit::Ascii("alpha".into()))
        );
        let mut lex = Lexer::new("ascii\"héllo\"");
        let err = lex.next_token().unwrap_err();
        assert!(err.message.contains("non-ASCII"));
    }

    #[test]
    fn utf16_prefix_encodes() {
        let TokenKind::Str(StringLit::Utf16(v)) = one(r#"utf16"hi""#) else {
            panic!("expected utf16 string")
        };
        assert_eq!(v, vec![0x68, 0x69]);
    }

    #[test]
    fn utf32_prefix_decodes() {
        let TokenKind::Str(StringLit::Utf32(v)) = one(r#"utf32"hi""#) else {
            panic!("expected utf32 string")
        };
        assert_eq!(v, vec!['h', 'i']);
    }

    #[test]
    fn ident_named_utf16_followed_by_equals_is_ident() {
        // `utf16 = 1` should lex as Ident("utf16"), Eq, Number(I64(1)).
        assert_eq!(
            tokens("utf16 = 1"),
            vec![
                TokenKind::Ident("utf16".into()),
                TokenKind::Eq,
                TokenKind::Number(NumberLit::I64(1)),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_block_punctuation_and_bool() {
        assert_eq!(
            tokens("service { on = true }"),
            vec![
                TokenKind::Ident("service".into()),
                TokenKind::LBrace,
                TokenKind::Ident("on".into()),
                TokenKind::Eq,
                TokenKind::Bool(true),
                TokenKind::RBrace,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_comments_are_skipped() {
        assert_eq!(
            tokens("# hash\n// slash\nx = 1"),
            vec![
                TokenKind::Ident("x".into()),
                TokenKind::Eq,
                TokenKind::Number(NumberLit::I64(1)),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn unterminated_string_errors() {
        let mut lex = Lexer::new(r#"x = "abc"#);
        assert!(matches!(
            lex.next_token().unwrap().kind,
            TokenKind::Ident(_)
        ));
        assert!(matches!(lex.next_token().unwrap().kind, TokenKind::Eq));
        let err = lex.next_token().unwrap_err();
        assert!(err.message.contains("unterminated"));
    }

    #[test]
    fn punctuation_colon_and_question() {
        assert_eq!(
            tokens("name: utf8?"),
            vec![
                TokenKind::Ident("name".into()),
                TokenKind::Colon,
                TokenKind::Ident("utf8".into()),
                TokenKind::Question,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_generic_and_bracket_tokens() {
        assert_eq!(
            tokens("tensor<f32, [3, 4]>"),
            vec![
                TokenKind::Ident("tensor".into()),
                TokenKind::Lt,
                TokenKind::Ident("f32".into()),
                TokenKind::Comma,
                TokenKind::LBracket,
                TokenKind::Number(NumberLit::I64(3)),
                TokenKind::Comma,
                TokenKind::Number(NumberLit::I64(4)),
                TokenKind::RBracket,
                TokenKind::Gt,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_at_token() {
        assert_eq!(
            tokens("@foo"),
            vec![
                TokenKind::At,
                TokenKind::Ident("foo".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_parens() {
        assert_eq!(
            tokens("(1, 2)"),
            vec![
                TokenKind::LParen,
                TokenKind::Number(NumberLit::I64(1)),
                TokenKind::Comma,
                TokenKind::Number(NumberLit::I64(2)),
                TokenKind::RParen,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_symbol_literal_tight() {
        assert_eq!(
            tokens(":red"),
            vec![TokenKind::Symbol("red".into()), TokenKind::Eof]
        );
    }

    #[test]
    fn lex_symbol_with_underscore_and_digit() {
        assert_eq!(
            tokens(":foo_bar2"),
            vec![TokenKind::Symbol("foo_bar2".into()), TokenKind::Eof]
        );
    }

    #[test]
    fn lex_colon_with_space() {
        assert_eq!(
            tokens("name: utf8"),
            vec![
                TokenKind::Ident("name".into()),
                TokenKind::Colon,
                TokenKind::Ident("utf8".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_colon_alone_at_eof() {
        assert_eq!(
            tokens("x :"),
            vec![
                TokenKind::Ident("x".into()),
                TokenKind::Colon,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn dot_separates_path_segments() {
        assert_eq!(
            tokens("foo.bar.baz"),
            vec![
                TokenKind::Ident("foo".into()),
                TokenKind::Dot,
                TokenKind::Ident("bar".into()),
                TokenKind::Dot,
                TokenKind::Ident("baz".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn comma_lexes_as_comma() {
        assert_eq!(
            tokens("a , b"),
            vec![
                TokenKind::Ident("a".into()),
                TokenKind::Comma,
                TokenKind::Ident("b".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn amp_lexes_as_amp() {
        assert_eq!(
            tokens("&User"),
            vec![
                TokenKind::Amp,
                TokenKind::Ident("User".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn none_keyword_lexes_distinctly_from_ident() {
        assert_eq!(one("none"), TokenKind::None);
        assert_eq!(one("nonexistent"), TokenKind::Ident("nonexistent".into()));
    }

    #[test]
    fn ascii_byte_below_0x80_validates() {
        // Confirm 0x7F (DEL) is accepted as ASCII.
        let mut lex = Lexer::new("ascii\"\x7F\"");
        let t = lex.next_token().unwrap();
        assert!(matches!(t.kind, TokenKind::Str(StringLit::Ascii(_))));
    }

    #[test]
    fn lex_arithmetic_operators() {
        assert_eq!(
            tokens("+ - * / %"),
            vec![
                TokenKind::Plus,
                TokenKind::Dash,
                TokenKind::Star,
                TokenKind::Slash,
                TokenKind::Percent,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_compound_eq_and_inequality() {
        assert_eq!(
            tokens("= == != !"),
            vec![
                TokenKind::Eq,
                TokenKind::EqEq,
                TokenKind::BangEq,
                TokenKind::Bang,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_compound_lt_gt() {
        assert_eq!(
            tokens("< <= > >="),
            vec![
                TokenKind::Lt,
                TokenKind::LtEq,
                TokenKind::Gt,
                TokenKind::GtEq,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_logical_ops() {
        assert_eq!(
            tokens("&& || & |"),
            vec![
                TokenKind::AmpAmp,
                TokenKind::PipePipe,
                TokenKind::Amp,
                TokenKind::Pipe,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_arrow_vs_dash() {
        assert_eq!(
            tokens("-> -"),
            vec![TokenKind::Arrow, TokenKind::Dash, TokenKind::Eof]
        );
    }

    #[test]
    fn lex_semi_token() {
        assert_eq!(
            tokens("a ; b"),
            vec![
                TokenKind::Ident("a".into()),
                TokenKind::Semi,
                TokenKind::Ident("b".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_signed_number_after_ident_is_subtraction() {
        // `a-1` and `a - 1` should both lex with `-` as Dash so the parser
        // can treat them as subtraction once expressions exist.
        assert_eq!(
            tokens("a-1"),
            vec![
                TokenKind::Ident("a".into()),
                TokenKind::Dash,
                TokenKind::Number(NumberLit::I64(1)),
                TokenKind::Eof,
            ]
        );
        assert_eq!(
            tokens("a - 1"),
            vec![
                TokenKind::Ident("a".into()),
                TokenKind::Dash,
                TokenKind::Number(NumberLit::I64(1)),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_signed_number_at_start_or_after_separator() {
        // After whitespace at start-of-file: signed literal.
        assert_eq!(
            tokens("-5"),
            vec![TokenKind::Number(NumberLit::I64(-5)), TokenKind::Eof]
        );
        // After `=` (no value to its left): signed literal.
        assert_eq!(
            tokens("x=-5"),
            vec![
                TokenKind::Ident("x".into()),
                TokenKind::Eq,
                TokenKind::Number(NumberLit::I64(-5)),
                TokenKind::Eof,
            ]
        );
        // After `(`: signed literal.
        assert_eq!(
            tokens("(-5)"),
            vec![
                TokenKind::LParen,
                TokenKind::Number(NumberLit::I64(-5)),
                TokenKind::RParen,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_signed_number_after_value_terminator_is_subtraction() {
        // After `)`: subtraction. After `]`: subtraction. After `}`: subtraction.
        assert_eq!(
            tokens(")-1"),
            vec![
                TokenKind::RParen,
                TokenKind::Dash,
                TokenKind::Number(NumberLit::I64(1)),
                TokenKind::Eof,
            ]
        );
    }

    // ---- heredocs --------------------------------------------------

    fn lex_str(src: &str) -> StringLit {
        match one(src) {
            TokenKind::Str(s) => s,
            other => panic!("expected Str, got {other:?}"),
        }
    }

    #[test]
    fn heredoc_basic_two_line_body() {
        let s = "<<END\nfirst\nsecond\nEND\n";
        assert_eq!(lex_str(s), StringLit::Utf8("first\nsecond\n".into()));
    }

    #[test]
    fn heredoc_strips_common_indent() {
        // 4-space common indent across non-blank lines. Blank line in
        // the middle stays blank in the output.
        let s = "<<END\n    foo\n\n    bar\n    END\n";
        assert_eq!(lex_str(s), StringLit::Utf8("foo\n\nbar\n".into()));
    }

    #[test]
    fn heredoc_indent_ignores_blank_lines() {
        // The leading-whitespace-only line should not contribute to the
        // minimum-indent calculation.
        let s = "<<END\n  foo\n  \n  bar\nEND\n";
        assert_eq!(lex_str(s), StringLit::Utf8("foo\n\nbar\n".into()));
    }

    #[test]
    fn heredoc_interprets_escapes() {
        let s = "<<END\nhi\\tthere\\nline\nEND\n";
        assert_eq!(lex_str(s), StringLit::Utf8("hi\tthere\nline\n".into()));
    }

    #[test]
    fn heredoc_ascii_prefix_validates() {
        let s = "ascii<<END\nplain ascii\nEND\n";
        assert_eq!(lex_str(s), StringLit::Ascii("plain ascii\n".into()));
    }

    #[test]
    fn heredoc_ascii_prefix_rejects_non_ascii() {
        let s = "ascii<<END\nplain\u{2713}\nEND\n";
        let mut lex = Lexer::new(s);
        let err = lex.next_token().unwrap_err();
        assert!(err.message.contains("non-ASCII"), "got: {}", err.message);
    }

    #[test]
    fn heredoc_utf16_prefix_encodes_body() {
        let s = "utf16<<END\nhi\nEND\n";
        // body = "hi\n" → UTF-16: [0x68, 0x69, 0x0a]
        assert_eq!(lex_str(s), StringLit::Utf16(vec![0x68, 0x69, 0x0a]));
    }

    #[test]
    fn heredoc_utf32_prefix_encodes_body() {
        let s = "utf32<<END\nhi\nEND\n";
        assert_eq!(lex_str(s), StringLit::Utf32(vec!['h', 'i', '\n']));
    }

    #[test]
    fn heredoc_unterminated_errors() {
        let mut lex = Lexer::new("<<END\nfoo\nbar\n");
        let err = lex.next_token().unwrap_err();
        assert!(
            err.message.contains("unterminated heredoc"),
            "got: {}",
            err.message
        );
    }

    #[test]
    fn heredoc_junk_after_tag_errors() {
        let mut lex = Lexer::new("<<END oops\nfoo\nEND\n");
        let err = lex.next_token().unwrap_err();
        assert!(
            err.message.contains("unexpected text after heredoc tag"),
            "got: {}",
            err.message
        );
    }

    #[test]
    fn heredoc_comment_after_tag_is_fine() {
        // A trailing `# comment` on the opener line is trivia.
        let s = "<<END  # a comment\nfoo\nEND\n";
        assert_eq!(lex_str(s), StringLit::Utf8("foo\n".into()));
    }

    #[test]
    fn heredoc_empty_body() {
        let s = "<<END\nEND\n";
        assert_eq!(lex_str(s), StringLit::Utf8(String::new()));
    }

    #[test]
    fn heredoc_closer_with_leading_whitespace() {
        // Closer may be indented; common-indent strip still applies to
        // the body using the minimum across non-blank body lines.
        let s = "<<END\n    foo\n    bar\n    END\n";
        assert_eq!(lex_str(s), StringLit::Utf8("foo\nbar\n".into()));
    }

    #[test]
    fn heredoc_pair_in_one_source() {
        let src = "<<A\none\nA\n<<B\ntwo\nB\n";
        let toks = tokens(src);
        assert_eq!(toks.len(), 3); // two strings + Eof
        assert_eq!(toks[0], TokenKind::Str(StringLit::Utf8("one\n".into())));
        assert_eq!(toks[1], TokenKind::Str(StringLit::Utf8("two\n".into())));
    }

    #[test]
    fn double_lt_with_non_ident_still_lt_lt() {
        // `<<` followed by non-ident-start should not be misread as a
        // heredoc — keep the existing two `Lt` token sequence.
        let toks = tokens("<<=");
        assert_eq!(toks[0], TokenKind::Lt);
        assert_eq!(toks[1], TokenKind::LtEq);
    }

    // ---- interpolation ---------------------------------------------

    fn interp_parts(src: &str) -> Vec<StringPart> {
        match one(src) {
            TokenKind::Str(StringLit::Interpolated { parts, .. }) => parts,
            other => panic!("expected Interpolated, got {other:?}"),
        }
    }

    #[test]
    fn interp_string_basic_slot() {
        let parts = interp_parts(r#"$"hi ${name}!""#);
        assert_eq!(parts.len(), 3);
        match &parts[0] {
            StringPart::Literal(s) => assert_eq!(s, "hi "),
            other => panic!("[0]: {other:?}"),
        }
        match &parts[1] {
            StringPart::Expr { text, .. } => assert_eq!(text, "name"),
            other => panic!("[1]: {other:?}"),
        }
        match &parts[2] {
            StringPart::Literal(s) => assert_eq!(s, "!"),
            other => panic!("[2]: {other:?}"),
        }
    }

    #[test]
    fn interp_string_multiple_slots() {
        let parts = interp_parts(r#"$"${a}-${b}""#);
        assert_eq!(parts.len(), 3);
        match &parts[0] {
            StringPart::Expr { text, .. } => assert_eq!(text, "a"),
            other => panic!("[0]: {other:?}"),
        }
        match &parts[1] {
            StringPart::Literal(s) => assert_eq!(s, "-"),
            other => panic!("[1]: {other:?}"),
        }
        match &parts[2] {
            StringPart::Expr { text, .. } => assert_eq!(text, "b"),
            other => panic!("[2]: {other:?}"),
        }
    }

    #[test]
    fn interp_slot_with_braces_and_strings() {
        // Brace balance + string-skip inside the slot.
        let parts = interp_parts(r#"$"x=${foo({y = "hi"})}""#);
        assert_eq!(parts.len(), 2);
        match &parts[1] {
            StringPart::Expr { text, .. } => {
                assert_eq!(text, r#"foo({y = "hi"})"#);
            }
            other => panic!("[1]: {other:?}"),
        }
    }

    #[test]
    fn interp_dollar_escape_emits_literal_dollar() {
        let parts = interp_parts(r#"$"price=\$5""#);
        assert_eq!(parts.len(), 1);
        match &parts[0] {
            StringPart::Literal(s) => assert_eq!(s, "price=$5"),
            other => panic!("[0]: {other:?}"),
        }
    }

    #[test]
    fn plain_string_treats_dollar_as_literal() {
        // No `$` prefix → `$` and `${...}` are plain bytes.
        let s = one(r#""price=$5 ${x}""#);
        assert_eq!(s, TokenKind::Str(StringLit::Utf8("price=$5 ${x}".into())));
    }

    #[test]
    fn interp_typed_ascii_carries_encoding() {
        let toks = tokens(r#"$ascii"id=${k}""#);
        match &toks[0] {
            TokenKind::Str(StringLit::Interpolated { encoding, .. }) => {
                assert_eq!(*encoding, StringEncoding::Ascii);
            }
            other => panic!("expected ascii interpolated, got {other:?}"),
        }
    }

    #[test]
    fn interp_heredoc_collects_slots_per_line() {
        let src = "$<<END\n  port=${cfg.port}\n  END\n";
        let parts = interp_parts(src);
        // ["port=", Expr("cfg.port"), "\n"]
        assert!(matches!(parts[0], StringPart::Literal(ref s) if s == "port="));
        assert!(matches!(parts[1], StringPart::Expr { ref text, .. } if text == "cfg.port"));
        // The trailing literal carries the joining newline.
        match parts.last().unwrap() {
            StringPart::Literal(s) => assert_eq!(s, "\n"),
            other => panic!("last: {other:?}"),
        }
    }

    #[test]
    fn interp_slot_unterminated_errors() {
        let mut lex = Lexer::new(r#"$"hi ${name"#);
        let err = lex.next_token().unwrap_err();
        assert!(
            err.message.contains("unterminated") || err.message.contains("slot"),
            "got: {}",
            err.message
        );
    }

    #[test]
    fn interp_slot_rejects_newline() {
        let mut lex = Lexer::new("$\"hi ${name\n}\"");
        let err = lex.next_token().unwrap_err();
        assert!(
            err.message.contains("multiple lines") || err.message.contains("newline"),
            "got: {}",
            err.message
        );
    }

    #[test]
    fn interp_slot_rejects_nested_heredoc() {
        let mut lex = Lexer::new("$\"${<<X\nhi\nX\n}\"");
        let err = lex.next_token().unwrap_err();
        assert!(
            err.message.contains("heredoc literals are not allowed"),
            "got: {}",
            err.message
        );
    }

    #[test]
    fn dollar_alone_errors() {
        let mut lex = Lexer::new("$ hello");
        let err = lex.next_token().unwrap_err();
        assert!(
            err.message.contains("expected '\"' or '<<'"),
            "got: {}",
            err.message
        );
    }

    #[test]
    fn dollar_with_unknown_encoding_errors() {
        let mut lex = Lexer::new(r#"$bogus"x""#);
        let err = lex.next_token().unwrap_err();
        assert!(
            err.message.contains("unknown string encoding"),
            "got: {}",
            err.message
        );
    }
}
