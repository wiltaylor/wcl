//! The lexer: source bytes in, [`Token`]s out.
//!
//! Beyond the usual tokenization, it does two things worth knowing. It
//! resolves numeric type suffixes (`8080u32`) into typed [`NumberLit`]s
//! at lex time, so the parser never re-reads a literal's text. And it
//! collects comments and blank lines as [`Trivia`] attached to the token
//! that follows them, which is how `wcl fmt` round-trips a file without
//! losing what the author wrote between items.

use crate::ast::{Span, Trivia};

mod finalize;
mod numbers;
mod strings;

#[derive(Debug, Clone, PartialEq)]
/// One lexical token's payload.
pub enum TokenKind {
    /// A bare identifier or keyword-like name.
    Ident(String),
    /// `true` or `false`.
    Bool(bool),
    /// A numeric literal, already resolved to its suffixed type.
    Number(NumberLit),
    /// A numeric literal carrying a **literal unit** suffix (`5MiB`, `3km`):
    /// the magnitude plus the unit name, resolved against the declared type
    /// at evaluation time. Boxed so `TokenKind` (and thus `Token`, which
    /// parse-recursion frames hold) stays small.
    NumberWithUnit(Box<(NumberLit, String)>),
    /// A string literal in any of the supported encodings.
    Str(StringLit),
    /// A symbol literal, written `:name`.
    Symbol(String),
    /// The `none` keyword.
    None,
    /// The `if` keyword.
    If,
    /// The `else` keyword.
    Else,
    /// The `match` keyword.
    Match,
    /// `=`
    Eq,
    /// `==`
    EqEq,
    /// `=>`
    FatArrow,
    /// `!=`
    BangEq,
    /// `!`
    Bang,
    /// `:`
    Colon,
    /// `::`, the namespace qualifier.
    ColonColon,
    /// `?`, marking an optional declaration.
    Question,
    /// `??` — the none-coalescing operator.
    QuestionQuestion,
    /// `&`, forming a reference type.
    Amp,
    /// `&&`
    AmpAmp,
    /// `|`, the table row delimiter.
    Pipe,
    /// `||`
    PipePipe,
    /// `.`
    Dot,
    /// `..`, the rest pattern.
    DotDot,
    /// `,`
    Comma,
    /// `;`
    Semi,
    /// `<`
    Lt,
    /// `<=`
    LtEq,
    /// `>`
    Gt,
    /// `>=`
    GtEq,
    /// `[`
    LBracket,
    /// `]`
    RBracket,
    /// `{`
    LBrace,
    /// `}`
    RBrace,
    /// `@`, introducing a decorator.
    At,
    /// `(`
    LParen,
    /// `)`
    RParen,
    /// `+`
    Plus,
    /// `-`
    Dash,
    /// `->`, used by connections and return types.
    Arrow,
    /// `*`, marking a repeated slot.
    Star,
    /// `/`
    Slash,
    /// `%`
    Percent,
    /// End of input.
    Eof,
}

#[derive(Debug, Clone, PartialEq)]
/// A numeric literal, resolved to the type its suffix names. An
/// unsuffixed integer lexes as [`NumberLit::I64`] and an unsuffixed
/// float as [`NumberLit::F64`].
pub enum NumberLit {
    /// Signed 8-bit.
    I8(i8),
    /// Signed 16-bit.
    I16(i16),
    /// Signed 32-bit.
    I32(i32),
    /// Signed 64-bit — the unsuffixed integer default.
    I64(i64),
    /// Signed 128-bit.
    I128(i128),
    /// Pointer-sized signed.
    Isize(isize),

    /// Unsigned 8-bit.
    U8(u8),
    /// Unsigned 16-bit.
    U16(u16),
    /// Unsigned 32-bit.
    U32(u32),
    /// Unsigned 64-bit.
    U64(u64),
    /// Unsigned 128-bit.
    U128(u128),
    /// Pointer-sized unsigned.
    Usize(usize),

    /// 32-bit float.
    F32(f32),
    /// 64-bit float — the unsuffixed float default.
    F64(f64),
}

impl NumberLit {
    /// Convert an integer literal to `u64`. Returns `None` for floats,
    /// for negative signed values, and for magnitudes that don't fit.
    pub fn as_u64(&self) -> Option<u64> {
        crate::numeric::numeric_as_u64!(self, NumberLit)
    }
}

#[derive(Debug, Clone, PartialEq)]
/// A string literal, carried in its declared encoding.
pub enum StringLit {
    /// A UTF-8 literal.
    Utf8(String),
    /// An `ascii"…"` literal.
    Ascii(String),
    /// A `utf16"…"` literal, as code units.
    Utf16(Vec<u16>),
    /// A `utf32"…"` literal, as scalar values.
    Utf32(Vec<char>),
    /// Opt-in interpolated literal (`$"…"`, `$ascii"…"`, `$<<TAG`, …).
    /// The body is split into already-escape-decoded literal chunks
    /// and raw source slices for `${expr}` slots that the parser later
    /// sub-parses into expressions.
    Interpolated {
        /// Encoding the concatenated result is re-encoded to.
        encoding: StringEncoding,
        /// Literal chunks interleaved with `${…}` slots.
        parts: Vec<StringPart>,
        /// Source span of the whole literal.
        span: Span,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// The encoding a string literal declares.
pub enum StringEncoding {
    /// UTF-8, the default.
    Utf8,
    /// ASCII — the lexer rejects non-ASCII bytes in the body.
    Ascii,
    /// UTF-16.
    Utf16,
    /// UTF-32.
    Utf32,
}

#[derive(Debug, Clone, PartialEq)]
/// One segment of an interpolated string literal.
pub enum StringPart {
    /// Already-decoded body bytes between (or around) slots.
    Literal(String),
    /// Raw source text inside a `${...}` slot, plus the slot's full
    /// span (covering the `${` and `}`). The parser sub-parses this
    /// into an `Expr` at parse time.
    Expr {
        /// Raw source text between the braces.
        text: String,
        /// Span covering the whole slot, `${` and `}` included.
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
/// One token: its payload, where it came from, and the trivia that
/// preceded it.
pub struct Token {
    /// What kind of token this is, and its payload.
    pub kind: TokenKind,
    /// Source span of the token text.
    pub span: Span,
    /// Comments + blank-line breaks that appeared in the source
    /// immediately before this token. The parser pulls these onto the
    /// next Item it builds so the source printer can re-emit them in
    /// roughly the original position. Same-line trailing comments
    /// after a token end up here as the *next* token's leading trivia
    /// — a known simplification, see [`ast::Trivia`].
    pub leading_trivia: Vec<Trivia>,
    /// A line comment that appeared AFTER the previous token, on the
    /// same line as it (before any intervening newline). Diverted here
    /// — rather than into `leading_trivia` — so the parser can re-attach
    /// it as the *trailing* comment of the node that ended that line,
    /// keeping inline comments inline through a round-trip. Populated
    /// only when a previous token exists, so a comment at the very start
    /// of the file stays leading.
    pub same_line_comment: Option<String>,
    /// Whether at least one newline (or line comment, which terminates a
    /// line) was skipped between the previous token and this one. The
    /// parser uses this so that a block's label loop can end at a line
    /// break, enabling the `kind labels…` (no `{}`) empty-body form.
    pub preceded_by_newline: bool,
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
            same_line_comment: None,
            preceded_by_newline: false,
        }
    }
}

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

#[derive(Debug, Clone, Copy)]
/// A parsed string-literal prefix: which encoding it names, and whether
/// it opts into `${…}` interpolation.
struct StringPrefix {
    /// The encoding the prefix names.
    encoding: StringEncoding,
    /// `true` when the prefix carried a leading `$`.
    interpolated: bool,
    /// Raw body: no escape processing and no `${…}` interpolation. Only
    /// the `<<'TAG'` heredoc opener sets this; the body is taken
    /// verbatim (handy for backslash-heavy text like LaTeX or regexes).
    raw: bool,
}

impl StringPrefix {
    /// A non-interpolating prefix in the given encoding.
    fn plain(encoding: StringEncoding) -> Self {
        Self {
            encoding,
            interpolated: false,
            raw: false,
        }
    }

    /// An interpolating (`$`-marked) prefix in the given encoding.
    fn interp(encoding: StringEncoding) -> Self {
        Self {
            encoding,
            interpolated: true,
            raw: false,
        }
    }

    /// A raw (`<<'TAG'`) UTF-8 heredoc: literal body, no escapes/interp.
    fn raw_utf8() -> Self {
        Self {
            encoding: StringEncoding::Utf8,
            interpolated: false,
            raw: true,
        }
    }

    /// Map an encoding keyword (`ascii`, `utf16`, …) to its encoding,
    /// or `None` when the text names no encoding.
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
    fn literal_unit_suffix() {
        // An unrecognised suffix lexes as a unit-bearing number, magnitude
        // defaulting to i64 (int) / f64 (float).
        assert_eq!(
            one("5MiB"),
            TokenKind::NumberWithUnit(Box::new((NumberLit::I64(5), "MiB".to_string())))
        );
        assert_eq!(
            one("512KiB"),
            TokenKind::NumberWithUnit(Box::new((NumberLit::I64(512), "KiB".to_string())))
        );
        assert_eq!(
            one("1.5km"),
            TokenKind::NumberWithUnit(Box::new((NumberLit::F64(1.5), "km".to_string())))
        );
        assert_eq!(
            one("-5MiB"),
            TokenKind::NumberWithUnit(Box::new((NumberLit::I64(-5), "MiB".to_string())))
        );
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
    fn unknown_suffix_becomes_unit() {
        // An unrecognised suffix is no longer a lex error — it lexes as a
        // literal unit (resolved later against the declared type).
        assert_eq!(
            one("1zz"),
            TokenKind::NumberWithUnit(Box::new((NumberLit::I64(1), "zz".to_string())))
        );
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
        // Per the lexer rules: float mode is triggered by '.' followed by
        // digit. `2e3` is therefore not a float; `e3` is taken as a suffix,
        // and since it isn't a numeric type suffix it lexes as a literal
        // unit named `e3` (which fails later unless such a unit is declared)
        // — write `2.0e3` for scientific notation. Keeps the grammar simple.
        assert_eq!(
            one("2e3"),
            TokenKind::NumberWithUnit(Box::new((NumberLit::I64(2), "e3".to_string())))
        );
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
    fn lex_kebab_and_path_label_token_streams() {
        // The label-stitching parser relies on `-`/`/` staying standalone
        // Dash/Slash tokens, with a trailing digit staying a Number.
        assert_eq!(
            tokens("dgm-box"),
            vec![
                TokenKind::Ident("dgm".into()),
                TokenKind::Dash,
                TokenKind::Ident("box".into()),
                TokenKind::Eof,
            ]
        );
        assert_eq!(
            tokens("data-series-1"),
            vec![
                TokenKind::Ident("data".into()),
                TokenKind::Dash,
                TokenKind::Ident("series".into()),
                TokenKind::Dash,
                TokenKind::Number(NumberLit::I64(1)),
                TokenKind::Eof,
            ]
        );
        assert_eq!(
            tokens("api/v1/users"),
            vec![
                TokenKind::Ident("api".into()),
                TokenKind::Slash,
                TokenKind::Ident("v1".into()),
                TokenKind::Slash,
                TokenKind::Ident("users".into()),
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
    fn raw_heredoc_takes_body_verbatim() {
        // `<<'TAG'` — backslashes are literal, no escape interpretation,
        // so LaTeX (`\frac`, `\theta`) survives unmangled. A plain
        // heredoc would reject `\f` / turn `\t` into a tab.
        let s = "<<'TEX'\n\\frac{a}{b} \\theta\nTEX\n";
        assert_eq!(lex_str(s), StringLit::Utf8("\\frac{a}{b} \\theta\n".into()));
    }

    #[test]
    fn raw_heredoc_ignores_interpolation_slots() {
        // `${…}` is literal in a raw heredoc, not an interpolation slot.
        let s = "<<'RAW'\na ${b} c\nRAW\n";
        assert_eq!(lex_str(s), StringLit::Utf8("a ${b} c\n".into()));
    }

    #[test]
    fn raw_heredoc_strips_common_indent() {
        let s = "<<'RAW'\n    \\foo\n\n    \\bar\n    RAW\n";
        assert_eq!(lex_str(s), StringLit::Utf8("\\foo\n\n\\bar\n".into()));
    }

    #[test]
    fn raw_heredoc_unclosed_tag_quote_errors() {
        let mut lex = Lexer::new("<<'TEX\n\\frac\nTEX\n");
        let err = lex.next_token().unwrap_err();
        assert!(err.message.contains("single quote"), "got: {}", err.message);
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
