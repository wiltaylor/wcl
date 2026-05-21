use crate::ast::Span;

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    Ident(String),
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Eq,
    LBrace,
    RBrace,
    Eof,
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
            b'"' => self.lex_string(start),
            b'-' if matches!(self.peek_at(1), Some(b'0'..=b'9')) => self.lex_number(start),
            b'0'..=b'9' => self.lex_number(start),
            c if is_ident_start(c) => self.lex_ident(start),
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

    fn lex_string(&mut self, start: usize) -> Result<Token, LexError> {
        self.pos += 1; // opening "
        let mut s = String::new();
        loop {
            let Some(c) = self.bump() else {
                return Err(LexError {
                    message: "unterminated string".into(),
                    span: Span::new(start, self.pos),
                });
            };
            match c {
                b'"' => {
                    return Ok(Token {
                        kind: TokenKind::String(s),
                        span: Span::new(start, self.pos),
                    });
                }
                b'\\' => {
                    let esc_start = self.pos - 1;
                    let Some(esc) = self.bump() else {
                        return Err(LexError {
                            message: "unterminated escape sequence".into(),
                            span: Span::new(esc_start, self.pos),
                        });
                    };
                    match esc {
                        b'"' => s.push('"'),
                        b'\\' => s.push('\\'),
                        b'n' => s.push('\n'),
                        b't' => s.push('\t'),
                        b'r' => s.push('\r'),
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
                other => s.push(other as char),
            }
        }
    }

    fn lex_number(&mut self, start: usize) -> Result<Token, LexError> {
        if self.peek() == Some(b'-') {
            self.pos += 1;
        }
        while matches!(self.peek(), Some(b'0'..=b'9')) {
            self.pos += 1;
        }
        let mut is_float = false;
        if self.peek() == Some(b'.') && matches!(self.peek_at(1), Some(b'0'..=b'9')) {
            is_float = true;
            self.pos += 1;
            while matches!(self.peek(), Some(b'0'..=b'9')) {
                self.pos += 1;
            }
        }
        let span = Span::new(start, self.pos);
        let text =
            std::str::from_utf8(&self.src[start..self.pos]).expect("number literal is ASCII");
        if is_float {
            text.parse::<f64>()
                .map(|f| Token {
                    kind: TokenKind::Float(f),
                    span,
                })
                .map_err(|e| LexError {
                    message: format!("invalid float literal: {e}"),
                    span,
                })
        } else {
            text.parse::<i64>()
                .map(|n| Token {
                    kind: TokenKind::Int(n),
                    span,
                })
                .map_err(|e| LexError {
                    message: format!("invalid integer literal: {e}"),
                    span,
                })
        }
    }

    fn lex_ident(&mut self, start: usize) -> Result<Token, LexError> {
        while matches!(self.peek(), Some(c) if is_ident_cont(c)) {
            self.pos += 1;
        }
        let span = Span::new(start, self.pos);
        let text = std::str::from_utf8(&self.src[start..self.pos]).expect("ident is ASCII");
        let kind = match text {
            "true" => TokenKind::Bool(true),
            "false" => TokenKind::Bool(false),
            _ => TokenKind::Ident(text.to_string()),
        };
        Ok(Token { kind, span })
    }
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_ident_cont(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
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

    #[test]
    fn lex_field_with_string() {
        assert_eq!(
            tokens(r#"name = "alpha""#),
            vec![
                TokenKind::Ident("name".into()),
                TokenKind::Eq,
                TokenKind::String("alpha".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_numbers() {
        assert_eq!(
            tokens("a = 42 b = -7 c = 2.5"),
            vec![
                TokenKind::Ident("a".into()),
                TokenKind::Eq,
                TokenKind::Int(42),
                TokenKind::Ident("b".into()),
                TokenKind::Eq,
                TokenKind::Int(-7),
                TokenKind::Ident("c".into()),
                TokenKind::Eq,
                TokenKind::Float(2.5),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_booleans() {
        assert_eq!(
            tokens("on = true off = false"),
            vec![
                TokenKind::Ident("on".into()),
                TokenKind::Eq,
                TokenKind::Bool(true),
                TokenKind::Ident("off".into()),
                TokenKind::Eq,
                TokenKind::Bool(false),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_block_punctuation() {
        assert_eq!(
            tokens("service { }"),
            vec![
                TokenKind::Ident("service".into()),
                TokenKind::LBrace,
                TokenKind::RBrace,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_comments_are_skipped() {
        assert_eq!(
            tokens("# hash comment\n// slash comment\nx = 1"),
            vec![
                TokenKind::Ident("x".into()),
                TokenKind::Eq,
                TokenKind::Int(1),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_string_escapes() {
        assert_eq!(
            tokens(r#"x = "a\nb\tc\\d\"""#),
            vec![
                TokenKind::Ident("x".into()),
                TokenKind::Eq,
                TokenKind::String("a\nb\tc\\d\"".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn lex_unterminated_string_errors_with_span() {
        let mut lex = Lexer::new(r#"x = "abc"#);
        // ident, eq are ok
        assert!(matches!(
            lex.next_token().unwrap().kind,
            TokenKind::Ident(_)
        ));
        assert!(matches!(lex.next_token().unwrap().kind, TokenKind::Eq));
        let err = lex.next_token().unwrap_err();
        assert!(err.message.contains("unterminated"));
        assert_eq!(err.span.start, 4);
    }

    #[test]
    fn lex_unexpected_character_reports_position() {
        let mut lex = Lexer::new("x @ 1");
        assert!(matches!(
            lex.next_token().unwrap().kind,
            TokenKind::Ident(_)
        ));
        let err = lex.next_token().unwrap_err();
        assert_eq!(err.span, Span::new(2, 3));
    }
}
