//! The vocabulary the lexer hands the parser.
//!
//! [`Token`] is one lexical unit plus the [`Trivia`] collected in front
//! of it, and [`TokenKind`] is what it is. The literal payloads are
//! resolved here rather than left as text: a numeric suffix has already
//! become a typed [`NumberLit`] and a string prefix a [`StringEncoding`]
//! by the time the parser sees them, so nothing downstream re-reads a
//! literal's characters.
//!
//! The scanner that produces these lives in [`lexer`](super); this file
//! is only what it produces.

use crate::ast::{Span, Trivia};

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
