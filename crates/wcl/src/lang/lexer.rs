//! WCL lexer — tokenizes WCL source text into a flat [`Token`] stream.
//!
//! Uses nom 8 parser combinators internally. The public entry point is [`lex`].

use crate::lang::diagnostic::Diagnostic;
use crate::lang::span::{FileId, Span};

// ── Token types ──────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq)]
pub enum TokenKind {
    // Literals
    /// Standard identifier: `[a-zA-Z_][a-zA-Z0-9_]*`
    Ident(String),
    /// Identifier literal (may contain hyphens).
    /// Emitted when the scanned word contains at least one hyphen.
    IdentifierLit(String),
    /// Fully-resolved string content (escapes expanded; interpolation
    /// markers `${` are preserved verbatim for the parser to handle).
    StringLit(String),
    IntLit(i64),
    FloatLit(f64),
    BoolLit(bool),
    NullLit,
    /// Symbol literal: `:name`
    SymbolLit(String),
    /// Date literal: `d"2024-03-15"`
    DateLit(String),
    /// Duration literal: `dur"P1Y2M3D"`
    DurationLit(String),

    /// Heredoc value.
    Heredoc {
        content: String,
        /// `<<-` indented form strips leading whitespace.
        indented: bool,
        /// `<<'TAG'` raw form — no escape processing or interpolation.
        raw: bool,
    },

    // Keywords
    Let,
    Partial,
    Macro,
    Schema,
    Table,
    Import,
    Export,
    Query,
    Ref,
    For,
    In,
    If,
    Else,
    When,
    Inject,
    Set,
    Remove,
    SelfKw,
    Validation,
    DecoratorSchema,
    Declare,
    Update,
    SymbolSet,

    // Delimiters
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    LParen,
    RParen,

    // Punctuation
    Equals,
    Comma,
    Pipe,
    Dot,
    DotDot,
    Hash,
    At,
    Colon,
    Question,
    Semicolon,

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    EqEq,
    Neq,
    Lt,
    Gt,
    Lte,
    Gte,
    Match,
    And,
    Or,
    Not,
    FatArrow,

    // String interpolation start `${`
    InterpStart,

    /// Fragment of an interpolated string (text between `${...}` spans).
    StringFragment(String),

    // Comments
    LineComment(String),
    BlockComment(String),
    DocComment(String),

    // Special
    Newline,
    Eof,
}

#[derive(Debug, Clone)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    fn new(kind: TokenKind, file: FileId, start: usize, end: usize) -> Self {
        Token {
            kind,
            span: Span::new(file, start, end),
        }
    }
}

// ── Internal lexer state ──────────────────────────────────────────────────────

struct Lexer<'a> {
    input: &'a str,
    /// Current byte position within `input`.
    pos: usize,
    file: FileId,
    errors: Vec<Diagnostic>,
}

impl<'a> Lexer<'a> {
    fn new(input: &'a str, file: FileId) -> Self {
        Lexer {
            input,
            pos: 0,
            file,
            errors: Vec::new(),
        }
    }

    // ── Low-level helpers ────────────────────────────────────────────────

    fn remaining(&self) -> &'a str {
        &self.input[self.pos..]
    }

    fn peek(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    fn peek2(&self) -> Option<char> {
        let mut chars = self.remaining().chars();
        chars.next();
        chars.next()
    }

    fn starts_with(&self, s: &str) -> bool {
        self.remaining().starts_with(s)
    }

    fn advance(&mut self, bytes: usize) {
        self.pos += bytes;
    }

    fn advance_char(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.pos += c.len_utf8();
        Some(c)
    }

    fn make_tok(&self, kind: TokenKind, start: usize) -> Token {
        Token::new(kind, self.file, start, self.pos)
    }

    // ── Whitespace / newline skipping ─────────────────────────────────────

    /// Skip spaces and tabs; return the number of newlines encountered.
    /// Newlines are NOT skipped — they are returned as `Newline` tokens.
    fn skip_inline_whitespace(&mut self) {
        loop {
            match self.peek() {
                Some(' ') | Some('\t') => {
                    self.advance_char();
                }
                Some('\r') => {
                    // Peek at next character for CR+LF
                    if self.starts_with("\r\n") {
                        break; // stop — newline follows
                    } else {
                        break; // bare CR treated as newline
                    }
                }
                _ => break,
            }
        }
    }

    // ── Token-level parsers ───────────────────────────────────────────────

    /// Lex the next token (or return `None` at EOF).
    fn next_token(&mut self) -> Option<Token> {
        // Skip spaces/tabs
        self.skip_inline_whitespace();

        let start = self.pos;
        let c = self.peek()?;

        // Newlines
        if c == '\n' {
            self.advance_char();
            return Some(self.make_tok(TokenKind::Newline, start));
        }
        if c == '\r' {
            self.advance_char();
            if self.peek() == Some('\n') {
                self.advance_char();
            }
            return Some(self.make_tok(TokenKind::Newline, start));
        }

        // Comments — check before `/` single char
        if self.starts_with("///") {
            return Some(self.lex_doc_comment(start));
        }
        if self.starts_with("//") {
            return Some(self.lex_line_comment(start));
        }
        if self.starts_with("/*") {
            return Some(self.lex_block_comment(start));
        }

        // Heredoc  <<
        if self.starts_with("<<") {
            return Some(self.lex_heredoc(start));
        }

        // String
        if c == '"' {
            return Some(self.lex_string(start));
        }

        // Numbers
        if c.is_ascii_digit() {
            return Some(self.lex_number(start));
        }

        // Identifiers / keywords
        if c.is_ascii_alphabetic() || c == '_' {
            return Some(self.lex_ident_or_keyword(start));
        }

        // Multi-char operators (must be checked before their single-char prefixes)
        if self.starts_with("${") {
            self.advance(2);
            return Some(self.make_tok(TokenKind::InterpStart, start));
        }
        if self.starts_with("==") {
            self.advance(2);
            return Some(self.make_tok(TokenKind::EqEq, start));
        }
        if self.starts_with("!=") {
            self.advance(2);
            return Some(self.make_tok(TokenKind::Neq, start));
        }
        if self.starts_with("<=") {
            self.advance(2);
            return Some(self.make_tok(TokenKind::Lte, start));
        }
        if self.starts_with(">=") {
            self.advance(2);
            return Some(self.make_tok(TokenKind::Gte, start));
        }
        if self.starts_with("=~") {
            self.advance(2);
            return Some(self.make_tok(TokenKind::Match, start));
        }
        if self.starts_with("&&") {
            self.advance(2);
            return Some(self.make_tok(TokenKind::And, start));
        }
        if self.starts_with("||") {
            self.advance(2);
            return Some(self.make_tok(TokenKind::Or, start));
        }
        if self.starts_with("=>") {
            self.advance(2);
            return Some(self.make_tok(TokenKind::FatArrow, start));
        }
        if self.starts_with("..") {
            self.advance(2);
            return Some(self.make_tok(TokenKind::DotDot, start));
        }

        // Symbol literal — `:name` where name starts with [a-zA-Z_]
        if c == ':' {
            if let Some(next) = self.peek2() {
                if next.is_ascii_alphabetic() || next == '_' {
                    self.advance_char(); // consume ':'
                    let name_start = self.pos;
                    while let Some(ch) = self.peek() {
                        if ch.is_ascii_alphanumeric() || ch == '_' {
                            self.advance_char();
                        } else {
                            break;
                        }
                    }
                    let name = self.input[name_start..self.pos].to_string();
                    return Some(self.make_tok(TokenKind::SymbolLit(name), start));
                }
            }
        }

        // Single-char tokens
        self.advance_char();
        let kind = match c {
            '{' => TokenKind::LBrace,
            '}' => TokenKind::RBrace,
            '[' => TokenKind::LBracket,
            ']' => TokenKind::RBracket,
            '(' => TokenKind::LParen,
            ')' => TokenKind::RParen,
            '=' => TokenKind::Equals,
            ',' => TokenKind::Comma,
            '|' => TokenKind::Pipe,
            '.' => TokenKind::Dot,
            '#' => TokenKind::Hash,
            '@' => TokenKind::At,
            ':' => TokenKind::Colon,
            '?' => TokenKind::Question,
            ';' => TokenKind::Semicolon,
            '+' => TokenKind::Plus,
            '-' => TokenKind::Minus,
            '*' => TokenKind::Star,
            '/' => TokenKind::Slash,
            '%' => TokenKind::Percent,
            '<' => TokenKind::Lt,
            '>' => TokenKind::Gt,
            '!' => TokenKind::Not,
            other => {
                self.errors.push(Diagnostic::error(
                    format!("unexpected character: {:?}", other),
                    Span::new(self.file, start, self.pos),
                ));
                return self.next_token();
            }
        };
        Some(self.make_tok(kind, start))
    }

    // ── Comment lexers ────────────────────────────────────────────────────

    fn lex_doc_comment(&mut self, start: usize) -> Token {
        // consume until end of line
        let text_start = self.pos;
        while let Some(c) = self.peek() {
            if c == '\n' || c == '\r' {
                break;
            }
            self.advance_char();
        }
        let text = self.input[text_start..self.pos].to_string();
        self.make_tok(TokenKind::DocComment(text), start)
    }

    fn lex_line_comment(&mut self, start: usize) -> Token {
        let text_start = self.pos;
        while let Some(c) = self.peek() {
            if c == '\n' || c == '\r' {
                break;
            }
            self.advance_char();
        }
        let text = self.input[text_start..self.pos].to_string();
        self.make_tok(TokenKind::LineComment(text), start)
    }

    fn lex_block_comment(&mut self, start: usize) -> Token {
        let text_start = self.pos;
        // consume opening `/*`
        self.advance(2);
        let mut depth = 1usize;
        loop {
            if self.remaining().is_empty() {
                self.errors.push(Diagnostic::error(
                    "unterminated block comment",
                    Span::new(self.file, start, self.pos),
                ));
                break;
            }
            if self.starts_with("/*") {
                self.advance(2);
                depth += 1;
            } else if self.starts_with("*/") {
                self.advance(2);
                depth -= 1;
                if depth == 0 {
                    break;
                }
            } else {
                self.advance_char();
            }
        }
        let text = self.input[text_start..self.pos].to_string();
        self.make_tok(TokenKind::BlockComment(text), start)
    }

    // ── String lexer ──────────────────────────────────────────────────────

    fn lex_string(&mut self, start: usize) -> Token {
        // consume opening `"`
        self.advance_char();
        let mut content = String::new();
        loop {
            match self.peek() {
                None => {
                    self.errors.push(
                        Diagnostic::error(
                            "unterminated string literal",
                            Span::new(self.file, start, self.pos),
                        )
                        .with_code("E003"),
                    );
                    break;
                }
                Some('"') => {
                    self.advance_char();
                    break;
                }
                Some('\\') => {
                    self.advance_char(); // consume `\`
                    match self.advance_char() {
                        Some('\\') => content.push('\\'),
                        Some('"') => content.push('"'),
                        Some('n') => content.push('\n'),
                        Some('r') => content.push('\r'),
                        Some('t') => content.push('\t'),
                        Some('u') => {
                            if let Some(ch) = self.lex_unicode_escape(4, start) {
                                content.push(ch);
                            }
                        }
                        Some('U') => {
                            if let Some(ch) = self.lex_unicode_escape(8, start) {
                                content.push(ch);
                            }
                        }
                        Some(other) => {
                            self.errors.push(Diagnostic::error(
                                format!("unknown escape sequence: \\{}", other),
                                Span::new(self.file, self.pos - other.len_utf8() - 1, self.pos),
                            ));
                            content.push('\\');
                            content.push(other);
                        }
                        None => {
                            self.errors.push(Diagnostic::error(
                                "unexpected end of file in escape sequence",
                                Span::new(self.file, start, self.pos),
                            ));
                        }
                    }
                }
                Some('$') if self.peek2() == Some('{') => {
                    // Preserve `${` verbatim so the parser can handle interpolation.
                    content.push('$');
                    self.advance_char();
                    content.push('{');
                    self.advance_char();
                }
                Some(c) => {
                    content.push(c);
                    self.advance_char();
                }
            }
        }
        self.make_tok(TokenKind::StringLit(content), start)
    }

    fn lex_unicode_escape(&mut self, digits: usize, err_start: usize) -> Option<char> {
        let mut hex = String::with_capacity(digits);
        for _ in 0..digits {
            match self.peek() {
                Some(c) if c.is_ascii_hexdigit() => {
                    hex.push(c);
                    self.advance_char();
                }
                _ => {
                    self.errors.push(Diagnostic::error(
                        format!("expected {} hex digits in unicode escape", digits),
                        Span::new(self.file, err_start, self.pos),
                    ));
                    return None;
                }
            }
        }
        let code = u32::from_str_radix(&hex, 16).unwrap();
        match char::from_u32(code) {
            Some(c) => Some(c),
            None => {
                self.errors.push(Diagnostic::error(
                    format!("invalid unicode code point: U+{:X}", code),
                    Span::new(self.file, err_start, self.pos),
                ));
                None
            }
        }
    }

    // ── Heredoc lexer ─────────────────────────────────────────────────────

    fn lex_heredoc(&mut self, start: usize) -> Token {
        // consume `<<`
        self.advance(2);

        let indented = if self.peek() == Some('-') {
            self.advance_char();
            true
        } else {
            false
        };

        let raw = if self.peek() == Some('\'') {
            self.advance_char();
            true
        } else {
            false
        };

        // Read the delimiter tag (identifier characters)
        let tag_start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                self.advance_char();
            } else {
                break;
            }
        }
        let tag = self.input[tag_start..self.pos].to_string();

        if raw && self.peek() == Some('\'') {
            self.advance_char(); // consume closing `'`
        }

        if tag.is_empty() {
            self.errors.push(Diagnostic::error(
                "heredoc delimiter tag is empty",
                Span::new(self.file, start, self.pos),
            ));
            return self.make_tok(
                TokenKind::Heredoc {
                    content: String::new(),
                    indented,
                    raw,
                },
                start,
            );
        }

        // Skip to end of opening line (including newline)
        while let Some(c) = self.peek() {
            if c == '\n' {
                self.advance_char();
                break;
            } else if c == '\r' {
                self.advance_char();
                if self.peek() == Some('\n') {
                    self.advance_char();
                }
                break;
            } else {
                self.advance_char();
            }
        }

        // Accumulate lines until we see the closing tag on its own line.
        let mut lines: Vec<String> = Vec::new();
        let mut closing_indent = 0usize;
        let mut found_close = false;

        loop {
            if self.remaining().is_empty() {
                self.errors.push(
                    Diagnostic::error(
                        format!("unterminated heredoc (expected closing {})", tag),
                        Span::new(self.file, start, self.pos),
                    )
                    .with_code("E003"),
                );
                break;
            }

            // Collect this line (up to but not including the newline)
            let line_start = self.pos;
            while let Some(c) = self.peek() {
                if c == '\n' || c == '\r' {
                    break;
                }
                self.advance_char();
            }
            let line = self.input[line_start..self.pos].to_string();

            // Consume the newline
            if self.peek() == Some('\r') {
                self.advance_char();
            }
            if self.peek() == Some('\n') {
                self.advance_char();
            }

            // Check if this is the closing marker
            let trimmed = line.trim_end();
            let trimmed_start = trimmed.trim_start();
            if trimmed_start == tag {
                closing_indent = line.len() - line.trim_start().len();
                found_close = true;
                break;
            }

            lines.push(line);
        }
        let _ = found_close;

        // Build content
        let content = if indented && closing_indent > 0 {
            lines
                .iter()
                .map(|l| {
                    if l.len() >= closing_indent {
                        l[closing_indent..].to_string()
                    } else {
                        l.trim_start().to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            lines.join("\n")
        };

        // Apply escape processing to non-raw heredocs
        let content = if raw {
            content
        } else {
            self.process_heredoc_escapes(content, start)
        };

        self.make_tok(
            TokenKind::Heredoc {
                content,
                indented,
                raw,
            },
            start,
        )
    }

    fn process_heredoc_escapes(&mut self, s: String, err_start: usize) -> String {
        let mut out = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\\' {
                match chars.next() {
                    Some('\\') => out.push('\\'),
                    Some('"') => out.push('"'),
                    Some('n') => out.push('\n'),
                    Some('r') => out.push('\r'),
                    Some('t') => out.push('\t'),
                    Some('u') => {
                        let mut hex = String::new();
                        for _ in 0..4 {
                            match chars.peek() {
                                Some(&hc) if hc.is_ascii_hexdigit() => {
                                    hex.push(hc);
                                    chars.next();
                                }
                                _ => break,
                            }
                        }
                        if hex.len() == 4 {
                            let code = u32::from_str_radix(&hex, 16).unwrap();
                            if let Some(ch) = char::from_u32(code) {
                                out.push(ch);
                            } else {
                                self.errors.push(Diagnostic::error(
                                    format!("invalid unicode code point: U+{:X}", code),
                                    Span::new(self.file, err_start, self.pos),
                                ));
                            }
                        } else {
                            out.push('\\');
                            out.push('u');
                            out.push_str(&hex);
                        }
                    }
                    Some('U') => {
                        let mut hex = String::new();
                        for _ in 0..8 {
                            match chars.peek() {
                                Some(&hc) if hc.is_ascii_hexdigit() => {
                                    hex.push(hc);
                                    chars.next();
                                }
                                _ => break,
                            }
                        }
                        if hex.len() == 8 {
                            let code = u32::from_str_radix(&hex, 16).unwrap();
                            if let Some(ch) = char::from_u32(code) {
                                out.push(ch);
                            } else {
                                self.errors.push(Diagnostic::error(
                                    format!("invalid unicode code point: U+{:X}", code),
                                    Span::new(self.file, err_start, self.pos),
                                ));
                            }
                        } else {
                            out.push('\\');
                            out.push('U');
                            out.push_str(&hex);
                        }
                    }
                    Some(other) => {
                        out.push('\\');
                        out.push(other);
                    }
                    None => {}
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    // ── Number lexer ──────────────────────────────────────────────────────

    fn lex_number(&mut self, start: usize) -> Token {
        // Check prefix
        if self.starts_with("0x") || self.starts_with("0X") {
            self.advance(2);
            let hex_start = self.pos;
            while let Some(c) = self.peek() {
                if c.is_ascii_hexdigit() || c == '_' {
                    self.advance_char();
                } else {
                    break;
                }
            }
            let raw = &self.input[hex_start..self.pos];
            let clean: String = raw.chars().filter(|&c| c != '_').collect();
            match i64::from_str_radix(&clean, 16) {
                Ok(n) => return self.make_tok(TokenKind::IntLit(n), start),
                Err(_) => {
                    // Try as u64 then cast (for large hex values fitting in i64 bits)
                    match u64::from_str_radix(&clean, 16) {
                        Ok(n) => return self.make_tok(TokenKind::IntLit(n as i64), start),
                        Err(_) => {
                            self.errors.push(Diagnostic::error(
                                format!("invalid hexadecimal literal: 0x{}", clean),
                                Span::new(self.file, start, self.pos),
                            ));
                            return self.make_tok(TokenKind::IntLit(0), start);
                        }
                    }
                }
            }
        }

        if self.starts_with("0o") || self.starts_with("0O") {
            self.advance(2);
            let oct_start = self.pos;
            while let Some(c) = self.peek() {
                if matches!(c, '0'..='7') || c == '_' {
                    self.advance_char();
                } else {
                    break;
                }
            }
            let raw = &self.input[oct_start..self.pos];
            let clean: String = raw.chars().filter(|&c| c != '_').collect();
            match i64::from_str_radix(&clean, 8) {
                Ok(n) => return self.make_tok(TokenKind::IntLit(n), start),
                Err(_) => {
                    self.errors.push(Diagnostic::error(
                        format!("invalid octal literal: 0o{}", clean),
                        Span::new(self.file, start, self.pos),
                    ));
                    return self.make_tok(TokenKind::IntLit(0), start);
                }
            }
        }

        if self.starts_with("0b") || self.starts_with("0B") {
            self.advance(2);
            let bin_start = self.pos;
            while let Some(c) = self.peek() {
                if c == '0' || c == '1' || c == '_' {
                    self.advance_char();
                } else {
                    break;
                }
            }
            let raw = &self.input[bin_start..self.pos];
            let clean: String = raw.chars().filter(|&c| c != '_').collect();
            match i64::from_str_radix(&clean, 2) {
                Ok(n) => return self.make_tok(TokenKind::IntLit(n), start),
                Err(_) => {
                    self.errors.push(Diagnostic::error(
                        format!("invalid binary literal: 0b{}", clean),
                        Span::new(self.file, start, self.pos),
                    ));
                    return self.make_tok(TokenKind::IntLit(0), start);
                }
            }
        }

        // Decimal integer or float
        let num_start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_digit() || c == '_' {
                self.advance_char();
            } else {
                break;
            }
        }

        // Check for float: digit(s).digit(s)[eE...]
        let is_float = self.peek() == Some('.')
            && self
                .remaining()
                .chars()
                .nth(1)
                .map(|c| c.is_ascii_digit())
                .unwrap_or(false);

        if is_float {
            self.advance_char(); // consume '.'
            while let Some(c) = self.peek() {
                if c.is_ascii_digit() || c == '_' {
                    self.advance_char();
                } else {
                    break;
                }
            }
            // Optional exponent
            if matches!(self.peek(), Some('e') | Some('E')) {
                self.advance_char();
                if matches!(self.peek(), Some('+') | Some('-')) {
                    self.advance_char();
                }
                while let Some(c) = self.peek() {
                    if c.is_ascii_digit() || c == '_' {
                        self.advance_char();
                    } else {
                        break;
                    }
                }
            }
            let raw = &self.input[num_start..self.pos];
            let clean: String = raw.chars().filter(|&c| c != '_').collect();
            match clean.parse::<f64>() {
                Ok(f) => return self.make_tok(TokenKind::FloatLit(f), start),
                Err(_) => {
                    self.errors.push(Diagnostic::error(
                        format!("invalid float literal: {}", clean),
                        Span::new(self.file, start, self.pos),
                    ));
                    return self.make_tok(TokenKind::FloatLit(0.0), start);
                }
            }
        }

        let raw = &self.input[num_start..self.pos];
        let clean: String = raw.chars().filter(|&c| c != '_').collect();
        match clean.parse::<i64>() {
            Ok(n) => self.make_tok(TokenKind::IntLit(n), start),
            Err(_) => {
                // May be too large for i64; try u64
                match clean.parse::<u64>() {
                    Ok(n) => self.make_tok(TokenKind::IntLit(n as i64), start),
                    Err(_) => {
                        self.errors.push(Diagnostic::error(
                            format!("integer literal out of range: {}", clean),
                            Span::new(self.file, start, self.pos),
                        ));
                        self.make_tok(TokenKind::IntLit(0), start)
                    }
                }
            }
        }
    }

    // ── Prefix string literal lexer (d"..." and dur"...") ─────────────────

    fn lex_prefix_string_literal(&mut self, prefix: &str, start: usize) -> Token {
        // Skip the opening quote
        self.advance_char(); // consume '"'
        let content_start = self.pos;

        while let Some(c) = self.peek() {
            if c == '"' {
                let content = self.input[content_start..self.pos].to_string();
                self.advance_char(); // consume closing '"'
                let kind = if prefix == "d" {
                    TokenKind::DateLit(content)
                } else {
                    TokenKind::DurationLit(content)
                };
                return self.make_tok(kind, start);
            } else if c == '\n' || c == '\r' {
                break;
            } else {
                self.advance_char();
            }
        }
        // Unterminated string
        self.errors.push(Diagnostic::error(
            "unterminated prefix string literal",
            Span::new(self.file, start, self.pos),
        ));
        self.make_tok(
            if prefix == "d" {
                TokenKind::DateLit(self.input[content_start..self.pos].to_string())
            } else {
                TokenKind::DurationLit(self.input[content_start..self.pos].to_string())
            },
            start,
        )
    }

    // ── Identifier / keyword lexer ────────────────────────────────────────

    fn lex_ident_or_keyword(&mut self, start: usize) -> Token {
        // Consume [a-zA-Z_][a-zA-Z0-9_-]* but note: hyphens are only kept
        // when they appear between identifier characters, not as trailing.
        let word_start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_ascii_alphanumeric() || c == '_' {
                self.advance_char();
            } else if c == '-' {
                // Only consume hyphen if followed by alphanumeric or _
                // (to avoid consuming "ident-" where `-` is a minus operator)
                let next = self.remaining().chars().nth(1);
                if next
                    .map(|nc| nc.is_ascii_alphanumeric() || nc == '_')
                    .unwrap_or(false)
                {
                    self.advance_char();
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        let word = &self.input[word_start..self.pos];

        // Check for date/duration prefix literals: d"..." and dur"..."
        if (word == "d" || word == "dur") && self.peek() == Some('"') {
            return self.lex_prefix_string_literal(word, start);
        }

        // Check keywords first (keywords cannot contain hyphens)
        let kind = match word {
            "let" => TokenKind::Let,
            "partial" => TokenKind::Partial,
            "macro" => TokenKind::Macro,
            "schema" => TokenKind::Schema,
            "table" => TokenKind::Table,
            "import" => TokenKind::Import,
            "export" => TokenKind::Export,
            "query" => TokenKind::Query,
            "ref" => TokenKind::Ref,
            "for" => TokenKind::For,
            "in" => TokenKind::In,
            "true" => TokenKind::BoolLit(true),
            "false" => TokenKind::BoolLit(false),
            "null" => TokenKind::NullLit,
            "if" => TokenKind::If,
            "else" => TokenKind::Else,
            "when" => TokenKind::When,
            "inject" => TokenKind::Inject,
            "set" => TokenKind::Set,
            "remove" => TokenKind::Remove,
            "self" => TokenKind::SelfKw,
            "validation" => TokenKind::Validation,
            "decorator_schema" => TokenKind::DecoratorSchema,
            "declare" => TokenKind::Declare,
            "update" => TokenKind::Update,
            "symbol_set" => TokenKind::SymbolSet,
            other => {
                if other.contains('-') {
                    TokenKind::IdentifierLit(other.to_string())
                } else {
                    TokenKind::Ident(other.to_string())
                }
            }
        };
        self.make_tok(kind, start)
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// Tokenize `input` and return a flat token stream ending with [`TokenKind::Eof`].
///
/// Diagnostics (errors/warnings encountered during lexing) are returned
/// separately. If there are errors, the token stream may still be partially
/// valid and callers can choose to continue parsing for further error recovery.
pub fn lex(input: &str, file_id: FileId) -> Result<Vec<Token>, Vec<Diagnostic>> {
    // Strip UTF-8 BOM if present (§3.1)
    let input = input.strip_prefix('\u{FEFF}').unwrap_or(input);
    let mut lexer = Lexer::new(input, file_id);
    let mut tokens = Vec::new();

    while lexer.pos < lexer.input.len() {
        if let Some(tok) = lexer.next_token() {
            tokens.push(tok);
        }
    }

    let eof_pos = lexer.pos;
    tokens.push(Token::new(TokenKind::Eof, file_id, eof_pos, eof_pos));

    if lexer.errors.is_empty() {
        Ok(tokens)
    } else {
        Err(lexer.errors)
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> FileId {
        FileId(0)
    }

    fn tokens_ok(src: &str) -> Vec<Token> {
        lex(src, file()).expect("expected no lex errors")
    }

    fn token_kinds_ok(src: &str) -> Vec<TokenKind> {
        tokens_ok(src).into_iter().map(|t| t.kind).collect()
    }

    // ── Keywords ─────────────────────────────────────────────────────────

    #[test]
    fn keywords() {
        let src = "let partial macro schema table import export query ref for in if else when inject set remove self validation decorator_schema update symbol_set";
        let ks = token_kinds_ok(src);
        assert_eq!(
            ks,
            vec![
                TokenKind::Let,
                TokenKind::Partial,
                TokenKind::Macro,
                TokenKind::Schema,
                TokenKind::Table,
                TokenKind::Import,
                TokenKind::Export,
                TokenKind::Query,
                TokenKind::Ref,
                TokenKind::For,
                TokenKind::In,
                TokenKind::If,
                TokenKind::Else,
                TokenKind::When,
                TokenKind::Inject,
                TokenKind::Set,
                TokenKind::Remove,
                TokenKind::SelfKw,
                TokenKind::Validation,
                TokenKind::DecoratorSchema,
                TokenKind::Update,
                TokenKind::SymbolSet,
                TokenKind::Eof,
            ]
        );
    }

    // ── Bool / null literals ──────────────────────────────────────────────

    #[test]
    fn bool_and_null() {
        let ks = token_kinds_ok("true false null");
        assert_eq!(
            ks,
            vec![
                TokenKind::BoolLit(true),
                TokenKind::BoolLit(false),
                TokenKind::NullLit,
                TokenKind::Eof,
            ]
        );
    }

    // ── Identifiers ───────────────────────────────────────────────────────

    #[test]
    fn plain_identifier() {
        let ks = token_kinds_ok("my_var _private camelCase");
        assert_eq!(
            ks,
            vec![
                TokenKind::Ident("my_var".into()),
                TokenKind::Ident("_private".into()),
                TokenKind::Ident("camelCase".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn identifier_lit_with_hyphens() {
        let ks = token_kinds_ok("svc-payments node-01 my-id");
        assert_eq!(
            ks,
            vec![
                TokenKind::IdentifierLit("svc-payments".into()),
                TokenKind::IdentifierLit("node-01".into()),
                TokenKind::IdentifierLit("my-id".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn hyphen_at_end_is_separate_minus() {
        // `foo-` → Ident("foo") then Minus (trailing hyphen not consumed as part of ident)
        let ks = token_kinds_ok("foo-");
        assert_eq!(
            ks,
            vec![
                TokenKind::Ident("foo".into()),
                TokenKind::Minus,
                TokenKind::Eof,
            ]
        );
    }

    // ── Numbers ───────────────────────────────────────────────────────────

    #[test]
    fn decimal_integers() {
        let ks = token_kinds_ok("0 42 1000");
        assert_eq!(
            ks,
            vec![
                TokenKind::IntLit(0),
                TokenKind::IntLit(42),
                TokenKind::IntLit(1000),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn decimal_with_underscores() {
        let ks = token_kinds_ok("1_000_000");
        assert_eq!(ks, vec![TokenKind::IntLit(1_000_000), TokenKind::Eof]);
    }

    #[test]
    fn hex_literals() {
        let ks = token_kinds_ok("0xFF 0x00 0xDEAD_BEEF");
        assert_eq!(
            ks,
            vec![
                TokenKind::IntLit(0xFF),
                TokenKind::IntLit(0x00),
                TokenKind::IntLit(0xDEAD_BEEF),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn octal_literals() {
        let ks = token_kinds_ok("0o755 0o0 0o777");
        assert_eq!(
            ks,
            vec![
                TokenKind::IntLit(0o755),
                TokenKind::IntLit(0),
                TokenKind::IntLit(0o777),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn binary_literals() {
        let ks = token_kinds_ok("0b1010 0b0 0b1111_1111");
        assert_eq!(
            ks,
            vec![
                TokenKind::IntLit(0b1010),
                TokenKind::IntLit(0),
                TokenKind::IntLit(0b1111_1111),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn float_literals() {
        let ks = token_kinds_ok("3.14 1.0e10 6.022e23 1.0e-10");
        assert_eq!(
            ks,
            vec![
                TokenKind::FloatLit(3.14),
                TokenKind::FloatLit(1.0e10),
                TokenKind::FloatLit(6.022e23),
                TokenKind::FloatLit(1.0e-10),
                TokenKind::Eof,
            ]
        );
    }

    // ── Strings ───────────────────────────────────────────────────────────

    #[test]
    fn empty_string() {
        let ks = token_kinds_ok(r#""""#);
        assert_eq!(ks, vec![TokenKind::StringLit("".into()), TokenKind::Eof]);
    }

    #[test]
    fn simple_string() {
        let ks = token_kinds_ok(r#""hello world""#);
        assert_eq!(
            ks,
            vec![TokenKind::StringLit("hello world".into()), TokenKind::Eof]
        );
    }

    #[test]
    fn string_escapes() {
        // "\\n\\t\\r\\\\\\\""  →  \n\t\r\"
        let src = r#""\n\t\r\\\"" "#;
        let ks = token_kinds_ok(src);
        assert_eq!(ks[0], TokenKind::StringLit("\n\t\r\\\"".into()),);
    }

    #[test]
    fn string_unicode_escape_4() {
        // "\u0041" == "A"
        let src = r#""\u0041""#;
        let ks = token_kinds_ok(src);
        assert_eq!(ks[0], TokenKind::StringLit("A".into()));
    }

    #[test]
    fn string_unicode_escape_8() {
        // "\U00000041" == "A"
        let src = r#""\U00000041""#;
        let ks = token_kinds_ok(src);
        assert_eq!(ks[0], TokenKind::StringLit("A".into()));
    }

    #[test]
    fn string_with_interpolation_marker() {
        // Interpolation `${name}` is preserved verbatim in the string content
        let src = r#""Hello ${name}!""#;
        let ks = token_kinds_ok(src);
        assert_eq!(ks[0], TokenKind::StringLit("Hello ${name}!".into()));
    }

    // ── Heredocs ──────────────────────────────────────────────────────────

    #[test]
    fn heredoc_basic() {
        let src = "<<EOF\nhello\nworld\nEOF\n";
        let ks = token_kinds_ok(src);
        assert_eq!(
            ks[0],
            TokenKind::Heredoc {
                content: "hello\nworld".into(),
                indented: false,
                raw: false,
            }
        );
    }

    #[test]
    fn heredoc_indented() {
        let src = "<<-EOF\n    hello\n    world\n    EOF\n";
        let ks = token_kinds_ok(src);
        assert_eq!(
            ks[0],
            TokenKind::Heredoc {
                content: "hello\nworld".into(),
                indented: true,
                raw: false,
            }
        );
    }

    #[test]
    fn heredoc_raw() {
        let src = "<<'EOF'\nno ${interp} here\nEOF\n";
        let ks = token_kinds_ok(src);
        assert_eq!(
            ks[0],
            TokenKind::Heredoc {
                content: "no ${interp} here".into(),
                indented: false,
                raw: true,
            }
        );
    }

    #[test]
    fn heredoc_with_escape_in_non_raw() {
        let src = "<<EOF\nhello\\nworld\nEOF\n";
        let ks = token_kinds_ok(src);
        assert_eq!(
            ks[0],
            TokenKind::Heredoc {
                content: "hello\nworld".into(),
                indented: false,
                raw: false,
            }
        );
    }

    // ── Comments ─────────────────────────────────────────────────────────

    #[test]
    fn line_comment() {
        let ks = token_kinds_ok("// this is a comment\n");
        assert_eq!(ks[0], TokenKind::LineComment("// this is a comment".into()));
    }

    #[test]
    fn doc_comment() {
        let ks = token_kinds_ok("/// doc comment\n");
        assert_eq!(ks[0], TokenKind::DocComment("/// doc comment".into()));
    }

    #[test]
    fn block_comment_simple() {
        let ks = token_kinds_ok("/* hello */");
        assert_eq!(ks[0], TokenKind::BlockComment("/* hello */".into()));
    }

    #[test]
    fn block_comment_nested() {
        let ks = token_kinds_ok("/* outer /* inner */ still outer */");
        assert_eq!(
            ks[0],
            TokenKind::BlockComment("/* outer /* inner */ still outer */".into())
        );
    }

    #[test]
    fn block_comment_multiline() {
        let src = "/* line1\n   line2\n   line3 */";
        let ks = token_kinds_ok(src);
        assert_eq!(ks[0], TokenKind::BlockComment(src.into()));
    }

    // ── Operators and punctuation ─────────────────────────────────────────

    #[test]
    fn single_char_operators() {
        let ks = token_kinds_ok("+ - * / % < > ! | . # @ : ? ;");
        assert_eq!(
            ks,
            vec![
                TokenKind::Plus,
                TokenKind::Minus,
                TokenKind::Star,
                TokenKind::Slash,
                TokenKind::Percent,
                TokenKind::Lt,
                TokenKind::Gt,
                TokenKind::Not,
                TokenKind::Pipe,
                TokenKind::Dot,
                TokenKind::Hash,
                TokenKind::At,
                TokenKind::Colon,
                TokenKind::Question,
                TokenKind::Semicolon,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn multi_char_operators() {
        let ks = token_kinds_ok("== != <= >= =~ && || => .. ${");
        assert_eq!(
            ks,
            vec![
                TokenKind::EqEq,
                TokenKind::Neq,
                TokenKind::Lte,
                TokenKind::Gte,
                TokenKind::Match,
                TokenKind::And,
                TokenKind::Or,
                TokenKind::FatArrow,
                TokenKind::DotDot,
                TokenKind::InterpStart,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn delimiters() {
        let ks = token_kinds_ok("{ } [ ] ( )");
        assert_eq!(
            ks,
            vec![
                TokenKind::LBrace,
                TokenKind::RBrace,
                TokenKind::LBracket,
                TokenKind::RBracket,
                TokenKind::LParen,
                TokenKind::RParen,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn equals_vs_eqeq() {
        let ks = token_kinds_ok("= ==");
        assert_eq!(ks, vec![TokenKind::Equals, TokenKind::EqEq, TokenKind::Eof]);
    }

    #[test]
    fn dot_vs_dotdot() {
        let ks = token_kinds_ok(". ..");
        assert_eq!(ks, vec![TokenKind::Dot, TokenKind::DotDot, TokenKind::Eof]);
    }

    // ── Newlines ─────────────────────────────────────────────────────────

    #[test]
    fn newlines_emitted() {
        let ks = token_kinds_ok("a\nb\n");
        assert_eq!(
            ks,
            vec![
                TokenKind::Ident("a".into()),
                TokenKind::Newline,
                TokenKind::Ident("b".into()),
                TokenKind::Newline,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn crlf_newline() {
        let ks = token_kinds_ok("a\r\nb");
        assert_eq!(
            ks,
            vec![
                TokenKind::Ident("a".into()),
                TokenKind::Newline,
                TokenKind::Ident("b".into()),
                TokenKind::Eof,
            ]
        );
    }

    // ── Spans ─────────────────────────────────────────────────────────────

    #[test]
    fn span_tracks_offsets() {
        let src = "foo bar";
        let toks = tokens_ok(src);
        // "foo" at 0..3
        assert_eq!(toks[0].span.start, 0);
        assert_eq!(toks[0].span.end, 3);
        // "bar" at 4..7
        assert_eq!(toks[1].span.start, 4);
        assert_eq!(toks[1].span.end, 7);
    }

    #[test]
    fn span_for_string_includes_quotes() {
        let src = r#""hello""#;
        let toks = tokens_ok(src);
        assert_eq!(toks[0].span.start, 0);
        assert_eq!(toks[0].span.end, 7);
    }

    // ── Edge cases ────────────────────────────────────────────────────────

    #[test]
    fn empty_input() {
        let ks = token_kinds_ok("");
        assert_eq!(ks, vec![TokenKind::Eof]);
    }

    #[test]
    fn whitespace_only() {
        let ks = token_kinds_ok("   \t  ");
        assert_eq!(ks, vec![TokenKind::Eof]);
    }

    #[test]
    fn string_only_escapes() {
        let src = r#""\n\t\r""#;
        let ks = token_kinds_ok(src);
        assert_eq!(ks[0], TokenKind::StringLit("\n\t\r".into()));
    }

    #[test]
    fn decorator_schema_keyword() {
        let ks = token_kinds_ok("decorator_schema");
        assert_eq!(ks[0], TokenKind::DecoratorSchema);
    }

    #[test]
    fn symbol_literal() {
        let ks = token_kinds_ok(":GET :relational :unix_socket");
        assert_eq!(
            ks,
            vec![
                TokenKind::SymbolLit("GET".into()),
                TokenKind::SymbolLit("relational".into()),
                TokenKind::SymbolLit("unix_socket".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn symbol_literal_not_colon() {
        // `x: string` — colon followed by space, should remain Colon
        let ks = token_kinds_ok("x: string");
        assert_eq!(
            ks,
            vec![
                TokenKind::Ident("x".into()),
                TokenKind::Colon,
                TokenKind::Ident("string".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn symbol_literal_in_ternary() {
        // `a ? b : c` — colon before space and ident
        let ks = token_kinds_ok("a ? b : c");
        assert_eq!(
            ks,
            vec![
                TokenKind::Ident("a".into()),
                TokenKind::Question,
                TokenKind::Ident("b".into()),
                TokenKind::Colon,
                TokenKind::Ident("c".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn fat_arrow_not_equals_gt() {
        let ks = token_kinds_ok("=>");
        assert_eq!(ks[0], TokenKind::FatArrow);
    }

    #[test]
    fn interp_start_in_expression_context() {
        // `${` outside a string is just the InterpStart token
        let ks = token_kinds_ok("${x}");
        assert_eq!(
            ks,
            vec![
                TokenKind::InterpStart,
                TokenKind::Ident("x".into()),
                TokenKind::RBrace,
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn number_followed_by_dot_is_not_float() {
        // `42.foo` → IntLit(42), Dot, Ident("foo")  (not a float, second char is 'f')
        let ks = token_kinds_ok("42.foo");
        assert_eq!(
            ks,
            vec![
                TokenKind::IntLit(42),
                TokenKind::Dot,
                TokenKind::Ident("foo".into()),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn float_with_exponent_sign() {
        let ks = token_kinds_ok("1.5e+3 2.0e-5");
        assert_eq!(
            ks,
            vec![
                TokenKind::FloatLit(1.5e3),
                TokenKind::FloatLit(2.0e-5),
                TokenKind::Eof,
            ]
        );
    }

    #[test]
    fn mixed_expression() {
        let src = "port = 8080 // default port\n";
        let ks = token_kinds_ok(src);
        assert_eq!(
            ks,
            vec![
                TokenKind::Ident("port".into()),
                TokenKind::Equals,
                TokenKind::IntLit(8080),
                TokenKind::LineComment("// default port".into()),
                TokenKind::Newline,
                TokenKind::Eof,
            ]
        );
    }
}
