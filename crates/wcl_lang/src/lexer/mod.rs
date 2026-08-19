//! The lexer: source bytes in, [`Token`]s out.
//!
//! Beyond the usual tokenization, it does two things worth knowing. It
//! resolves numeric type suffixes (`8080u32`) into typed [`NumberLit`]s
//! at lex time, so the parser never re-reads a literal's text. And it
//! collects comments and blank lines as [`Trivia`] attached to the token
//! that follows them, which is how `wcl fmt` round-trips a file without
//! losing what the author wrote between items.
//!
//! This file is the dispatch state machine — one byte of lookahead
//! deciding what to scan next — plus the trivia collection that runs
//! between tokens. What it hands off to:
//!
//! - [`token`] — what a token *is*: the kinds, the already-resolved
//!   literal payloads, and the trivia each token carries.
//! - [`numbers`] then [`finalize`] — carving a numeric literal out of
//!   the text, then deciding its type and whether the value fits.
//! - [`strings`] — quoted and heredoc bodies, escapes and `${…}` parts,
//!   plus the prefix that decides which of those apply.

use crate::ast::{Span, Trivia};

mod finalize;
mod numbers;
mod strings;
#[cfg(test)]
mod tests;
mod token;

pub use token::{NumberLit, StringEncoding, StringLit, StringPart, Token, TokenKind};

use strings::StringPrefix;

#[derive(Debug, Clone, PartialEq)]
/// A lexing failure — an unterminated literal, a bad escape, a numeric
/// literal that does not fit its declared type.
pub struct LexError {
    /// Human-readable description of the failure.
    pub message: String,
    /// Source span of the offending text.
    pub span: Span,
}

/// Streaming lexer over a source string. Pull tokens with
/// [`Lexer::next_token`] until it yields [`TokenKind::Eof`].
pub struct Lexer<'a> {
    /// The source, as bytes — every token boundary in WCL is ASCII, so
    /// the scanner works bytewise and only decodes UTF-8 inside literals
    /// and identifiers.
    src: &'a [u8],
    /// Byte offset of the next unconsumed byte.
    pos: usize,
    /// Set once the lexer has emitted any non-Eof token. Used by
    /// `collect_trivia` to decide whether a comment can be a *trailing*
    /// (same-line) comment of a previous token, or — at the start of the
    /// file, with no previous token — must stay a leading comment.
    had_prev_token: bool,
    /// True when the previous emitted token was an opening delimiter
    /// (`{` / `[` / `(`). A comment on the same line as an opener (e.g.
    /// `union U {  # note`) belongs to the first member that follows, not
    /// to the opener — so it stays a *leading* comment, not a trailing one.
    prev_open_delim: bool,
}

impl<'a> Lexer<'a> {
    /// Start lexing `src` from the beginning.
    pub fn new(src: &'a str) -> Self {
        Self {
            src: src.as_bytes(),
            pos: 0,
            had_prev_token: false,
            prev_open_delim: false,
        }
    }

    /// Consume and return the next token, with any preceding trivia
    /// attached. Yields [`TokenKind::Eof`] at end of input, repeatedly.
    pub fn next_token(&mut self) -> Result<Token, LexError> {
        let (leading_trivia, preceded_by_newline, same_line_comment) = self.collect_trivia();
        let start = self.pos;
        let Some(c) = self.peek() else {
            return Ok(Token {
                kind: TokenKind::Eof,
                span: Span::new(start, start),
                leading_trivia,
                same_line_comment,
                preceded_by_newline,
            });
        };
        let mut tok = self.lex_after_trivia(start, c)?;
        tok.leading_trivia = leading_trivia;
        tok.same_line_comment = same_line_comment;
        tok.preceded_by_newline = preceded_by_newline;
        // A real token has now been produced: any comment that follows
        // it on the same line is a trailing comment of *this* token's
        // node, not a leading comment of the next.
        self.had_prev_token = true;
        self.prev_open_delim = matches!(
            tok.kind,
            TokenKind::LBrace | TokenKind::LBracket | TokenKind::LParen
        );
        Ok(tok)
    }

    /// Lex one token given the first byte `c`, once trivia has already
    /// been collected. The dispatch table at the heart of the lexer.
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
                // Raw heredoc opener: `<<'TAG'` — body taken verbatim.
                if self.peek_at(1) == Some(b'<') && self.peek_at(2) == Some(b'\'') {
                    self.pos += 2; // consume `<<`, leaving the cursor at `'`
                    return self.lex_heredoc(start, StringPrefix::raw_utf8());
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
            b'?' => {
                if self.peek_at(1) == Some(b'?') {
                    self.pos += 2;
                    Ok(Token::new(
                        TokenKind::QuestionQuestion,
                        Span::new(start, self.pos),
                    ))
                } else {
                    Ok(self.single(start, TokenKind::Question))
                }
            }
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

    /// The next byte without consuming it.
    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    /// The byte `offset` positions ahead, without consuming anything.
    fn peek_at(&self, offset: usize) -> Option<u8> {
        self.src.get(self.pos + offset).copied()
    }

    /// Consume and return the next byte.
    fn bump(&mut self) -> Option<u8> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    /// Emit a one-byte token, consuming that byte.
    fn single(&mut self, start: usize, kind: TokenKind) -> Token {
        self.pos += 1;
        Token::new(kind, Span::new(start, self.pos))
    }

    /// Emit `two` when the next byte is `follow`, else `one` — the
    /// maximal-munch rule behind pairs like `=` / `==`.
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
    fn collect_trivia(&mut self) -> (Vec<Trivia>, bool, Option<String>) {
        let mut out = Vec::new();
        let mut newlines_in_run = 0usize;
        let mut saw_newline = false;
        let mut same_line: Option<String> = None;
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\r') => {
                    self.pos += 1;
                }
                Some(b'\n') => {
                    self.pos += 1;
                    newlines_in_run += 1;
                    saw_newline = true;
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
                    // A comment on the same line as the previous token (no
                    // newline yet in this run, and a previous token exists)
                    // is a trailing comment: divert it instead of pushing
                    // leading trivia. Either way the comment consumed its
                    // line's `\n`, so the run counter advances to 1 — a
                    // genuine blank line *after* the comment still registers.
                    if self.had_prev_token
                        && !self.prev_open_delim
                        && !saw_newline
                        && same_line.is_none()
                    {
                        same_line = Some(text);
                    } else {
                        out.push(Trivia::LineComment(text));
                    }
                    newlines_in_run = 1;
                    saw_newline = true;
                }
                Some(b'/') if self.peek_at(1) == Some(b'/') => {
                    let text = self.consume_line_comment(2);
                    if self.had_prev_token
                        && !self.prev_open_delim
                        && !saw_newline
                        && same_line.is_none()
                    {
                        same_line = Some(text);
                    } else {
                        out.push(Trivia::LineComment(text));
                    }
                    newlines_in_run = 1;
                    saw_newline = true;
                }
                _ => break,
            }
        }
        (out, saw_newline, same_line)
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

    /// Lex an identifier, a keyword, or a typed string literal whose
    /// encoding prefix looks like one (`ascii"…"`).
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

/// Whether `c` may begin an identifier.
fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

/// Whether `c` may continue an identifier.
fn is_ident_cont(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// Whether `s` can be written as a bare WCL identifier.
///
/// Every writer that builds source needs this — the formatter deciding
/// whether a key needs quoting, an editing UI validating a new block id.
/// It lives here, on the lexer's own [`is_ident_start`] / [`is_ident_cont`],
/// so a caller's idea of an identifier cannot drift from what the lexer
/// will actually accept back.
pub fn is_identifier(s: &str) -> bool {
    let mut bytes = s.bytes();
    matches!(bytes.next(), Some(c) if is_ident_start(c)) && bytes.all(is_ident_cont)
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
