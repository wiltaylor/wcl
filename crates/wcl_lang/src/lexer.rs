use crate::ast::Span;
use crate::numeric::{self, ParsedNumber};

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Ident(String),
    Bool(bool),
    Number(NumberLit),
    Str(StringLit),
    Symbol(String),
    None,
    Eq,
    Colon,
    Question,
    Amp,
    Dot,
    Comma,
    Lt,
    Gt,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
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

#[derive(Debug, Clone, PartialEq)]
pub enum StringLit {
    Utf8(String),
    Ascii(String),
    Utf16(Vec<u16>),
    Utf32(Vec<char>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
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
        self.skip_trivia();
        let start = self.pos;
        let Some(c) = self.peek() else {
            return Ok(Token {
                kind: TokenKind::Eof,
                span: Span::new(start, start),
            });
        };
        match c {
            b'=' => {
                self.pos += 1;
                Ok(Token {
                    kind: TokenKind::Eq,
                    span: Span::new(start, self.pos),
                })
            }
            b':' => {
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
                    Ok(Token {
                        kind: TokenKind::Symbol(name),
                        span: Span::new(start, self.pos),
                    })
                } else {
                    self.pos += 1;
                    Ok(Token {
                        kind: TokenKind::Colon,
                        span: Span::new(start, self.pos),
                    })
                }
            }
            b'?' => {
                self.pos += 1;
                Ok(Token {
                    kind: TokenKind::Question,
                    span: Span::new(start, self.pos),
                })
            }
            b'&' => {
                self.pos += 1;
                Ok(Token {
                    kind: TokenKind::Amp,
                    span: Span::new(start, self.pos),
                })
            }
            b'.' => {
                self.pos += 1;
                Ok(Token {
                    kind: TokenKind::Dot,
                    span: Span::new(start, self.pos),
                })
            }
            b',' => {
                self.pos += 1;
                Ok(Token {
                    kind: TokenKind::Comma,
                    span: Span::new(start, self.pos),
                })
            }
            b'<' => {
                self.pos += 1;
                Ok(Token {
                    kind: TokenKind::Lt,
                    span: Span::new(start, self.pos),
                })
            }
            b'>' => {
                self.pos += 1;
                Ok(Token {
                    kind: TokenKind::Gt,
                    span: Span::new(start, self.pos),
                })
            }
            b'[' => {
                self.pos += 1;
                Ok(Token {
                    kind: TokenKind::LBracket,
                    span: Span::new(start, self.pos),
                })
            }
            b']' => {
                self.pos += 1;
                Ok(Token {
                    kind: TokenKind::RBracket,
                    span: Span::new(start, self.pos),
                })
            }
            b'{' => {
                self.pos += 1;
                Ok(Token {
                    kind: TokenKind::LBrace,
                    span: Span::new(start, self.pos),
                })
            }
            b'}' => {
                self.pos += 1;
                Ok(Token {
                    kind: TokenKind::RBrace,
                    span: Span::new(start, self.pos),
                })
            }
            b'"' => self.lex_string(start, StringPrefix::Utf8),
            b'-' if matches!(self.peek_at(1), Some(b'0'..=b'9')) => self.lex_number(start),
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

    fn bump(&mut self) -> Option<u8> {
        let c = self.peek()?;
        self.pos += 1;
        Some(c)
    }

    fn skip_trivia(&mut self) {
        loop {
            match self.peek() {
                Some(b' ' | b'\t' | b'\n' | b'\r') => {
                    self.pos += 1;
                }
                Some(b'#') => self.skip_line(),
                Some(b'/') if self.peek_at(1) == Some(b'/') => self.skip_line(),
                _ => break,
            }
        }
    }

    fn skip_line(&mut self) {
        while let Some(c) = self.peek() {
            self.pos += 1;
            if c == b'\n' {
                break;
            }
        }
    }

    fn lex_string(&mut self, start: usize, prefix: StringPrefix) -> Result<Token, LexError> {
        self.pos += 1; // opening "
        let body_start = self.pos;
        let mut body = String::new();
        loop {
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
                    match esc {
                        b'"' => body.push('"'),
                        b'\\' => body.push('\\'),
                        b'n' => body.push('\n'),
                        b't' => body.push('\t'),
                        b'r' => body.push('\r'),
                        other => {
                            return Err(LexError {
                                message: format!("invalid escape '\\{}'", other as char),
                                span: Span::new(esc_start, self.pos),
                            });
                        }
                    }
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
        let kind = self.materialise_string(prefix, body, body_start, body_end)?;
        Ok(Token {
            kind: TokenKind::Str(kind),
            span: Span::new(start, self.pos),
        })
    }

    fn materialise_string(
        &self,
        prefix: StringPrefix,
        body: String,
        body_start: usize,
        body_end: usize,
    ) -> Result<StringLit, LexError> {
        match prefix {
            StringPrefix::Utf8 => Ok(StringLit::Utf8(body)),
            StringPrefix::Ascii => {
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
            StringPrefix::Utf16 => {
                let _ = body_end;
                Ok(StringLit::Utf16(body.encode_utf16().collect()))
            }
            StringPrefix::Utf32 => Ok(StringLit::Utf32(body.chars().collect())),
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
        let mut prev_was_underscore = false;
        let mut had_digit = false;
        while let Some(c) = self.peek() {
            if c == b'_' {
                if !had_digit {
                    return Err(LexError {
                        message: "underscore must follow a digit".into(),
                        span: Span::new(self.pos, self.pos + 1),
                    });
                }
                prev_was_underscore = true;
                self.pos += 1;
            } else if is_digit_in_base(c, base) {
                had_digit = true;
                prev_was_underscore = false;
                self.pos += 1;
            } else {
                break;
            }
        }
        if !had_digit {
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
        if prev_was_underscore {
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
            let mut frac_had_digit = false;
            let mut frac_prev_underscore = false;
            while let Some(c) = self.peek() {
                if c == b'_' {
                    if !frac_had_digit {
                        return Err(LexError {
                            message: "underscore must follow a digit".into(),
                            span: Span::new(self.pos, self.pos + 1),
                        });
                    }
                    frac_prev_underscore = true;
                    self.pos += 1;
                } else if c.is_ascii_digit() {
                    frac_had_digit = true;
                    frac_prev_underscore = false;
                    self.pos += 1;
                } else {
                    break;
                }
            }
            if frac_prev_underscore {
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
            let digits_start = self.pos;
            while let Some(c) = self.peek() {
                if c == b'_' {
                    if self.pos == digits_start {
                        return Err(LexError {
                            message: "underscore must follow a digit".into(),
                            span: Span::new(self.pos, self.pos + 1),
                        });
                    }
                    self.pos += 1;
                } else if c.is_ascii_digit() {
                    self.pos += 1;
                } else {
                    break;
                }
            }
            if self.pos == digits_start {
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
            .map(|n| Token {
                kind: TokenKind::Number(n),
                span: literal_span,
            })
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
            && let Some(prefix) = StringPrefix::from_text(text)
        {
            return self.lex_string(start, prefix);
        }

        let span = Span::new(start, self.pos);
        let kind = match text {
            "true" => TokenKind::Bool(true),
            "false" => TokenKind::Bool(false),
            "none" => TokenKind::None,
            _ => TokenKind::Ident(text.to_string()),
        };
        Ok(Token { kind, span })
    }
}

#[derive(Debug, Clone, Copy)]
enum StringPrefix {
    Utf8,
    Ascii,
    Utf16,
    Utf32,
}

impl StringPrefix {
    fn from_text(text: &str) -> Option<Self> {
        match text {
            "utf8" => Some(StringPrefix::Utf8),
            "ascii" => Some(StringPrefix::Ascii),
            "utf16" => Some(StringPrefix::Utf16),
            "utf32" => Some(StringPrefix::Utf32),
            _ => None,
        }
    }
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
}
