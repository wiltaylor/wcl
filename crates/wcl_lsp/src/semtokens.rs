//! `textDocument/semanticTokens/full` handler. Drives `wcl_lang`'s
//! lexer over the document and emits a delta-encoded token stream
//! using the legend declared at initialize time.
//!
//! v1: each lexer token maps to exactly one semantic-token kind by
//! its `TokenKind` variant; we do not consult the AST. Comments are
//! omitted (editors highlight those via their own filetype rules);
//! interpolated strings emit as a single `string` span (sub-token
//! coloring inside `${...}` slots is deferred).

use tower_lsp::lsp_types::{SemanticToken, SemanticTokenType};
use wcl_lang::{Lexer, Span, StringLit, StringPart, TokenKind};

/// Token legend in the order LSP expects: each emitted token's
/// `token_type` is an index into this list. Add new categories at the
/// end so older clients don't reinterpret existing indices.
pub(crate) const LEGEND: &[SemanticTokenType] = &[
    SemanticTokenType::KEYWORD,     // 0
    SemanticTokenType::STRING,      // 1
    SemanticTokenType::NUMBER,      // 2
    SemanticTokenType::OPERATOR,    // 3
    SemanticTokenType::DECORATOR,   // 4 (the `@` marker)
    SemanticTokenType::TYPE,        // 5
    SemanticTokenType::VARIABLE,    // 6 (identifiers we can't classify)
    SemanticTokenType::ENUM_MEMBER, // 7 (`:symbol` literals)
];

const T_KEYWORD: u32 = 0;
const T_STRING: u32 = 1;
const T_NUMBER: u32 = 2;
const T_OPERATOR: u32 = 3;
const T_DECORATOR: u32 = 4;
const T_TYPE: u32 = 5;
const T_VARIABLE: u32 = 6;
const T_ENUM_MEMBER: u32 = 7;

/// Compute the delta-encoded semantic token stream for `source`. On
/// lex failure, returns an empty stream — diagnostics already report
/// the underlying error.
pub(crate) fn compute(source: &str) -> Vec<SemanticToken> {
    let mut tokens = Vec::new();
    let mut lex = Lexer::new(source);
    let mut prev_type: Option<TokenKind> = None;
    while let Ok(tok) = lex.next_token() {
        if matches!(tok.kind, TokenKind::Eof) {
            break;
        }
        // Interpolated strings get sub-token coloring — the slot
        // contents are real expressions and deserve distinct
        // highlighting from the surrounding literal bytes.
        if let TokenKind::Str(StringLit::Interpolated { parts, .. }) = &tok.kind {
            push_interpolated_tokens(&mut tokens, source, tok.span, parts);
        } else if let Some(ty) = classify(&tok.kind, prev_type.as_ref()) {
            push_token(&mut tokens, source, tok.span, ty);
        }
        prev_type = Some(tok.kind);
    }
    delta_encode(source, &tokens)
}

/// Walk the parts of an interpolated string literal and emit
/// `STRING` runs for the literal bytes, `OPERATOR` runs for the
/// `${` / `}` delimiters, and re-lexed token coloring for the slot
/// bodies. The lexer pre-records each `StringPart::Expr.span` in
/// absolute source coordinates, so the slot text can be re-tokenised
/// in place by offsetting the inner lexer's spans by `slot.start + 2`.
fn push_interpolated_tokens(
    out: &mut Vec<Raw>,
    source: &str,
    string_span: Span,
    parts: &[StringPart],
) {
    let mut cursor = string_span.start;
    for part in parts {
        if let StringPart::Expr { text, span } = part {
            // String bytes between the previous cursor and this slot.
            if span.start > cursor {
                push_token(out, source, Span::new(cursor, span.start), T_STRING);
            }
            // `${` opener and trailing `}` are 1- and 1-byte tokens
            // (the lexer guarantees `${...}` shape).
            let open_end = (span.start + 2).min(span.end);
            push_token(out, source, Span::new(span.start, open_end), T_OPERATOR);
            // Inner expression — re-lex against the slot text and
            // translate spans back to absolute source offsets.
            let inner_start = open_end;
            let inner_end = span.end.saturating_sub(1);
            if inner_end > inner_start {
                let _ = text; // `text` may differ from the source slice on escapes; prefer raw source.
                push_inner_tokens(out, source, inner_start, inner_end);
            }
            if span.end > inner_end {
                push_token(out, source, Span::new(inner_end, span.end), T_OPERATOR);
            }
            cursor = span.end;
        }
    }
    if cursor < string_span.end {
        push_token(out, source, Span::new(cursor, string_span.end), T_STRING);
    }
}

/// Re-lex the byte range `[start, end)` of `source` as a standalone
/// expression and emit semantic tokens for each token using the
/// outer source's absolute offsets. Used for `${...}` slot bodies.
fn push_inner_tokens(out: &mut Vec<Raw>, source: &str, start: usize, end: usize) {
    let slice = &source[start..end];
    let mut lex = Lexer::new(slice);
    let mut prev_type: Option<TokenKind> = None;
    while let Ok(tok) = lex.next_token() {
        if matches!(tok.kind, TokenKind::Eof) {
            break;
        }
        if let Some(ty) = classify(&tok.kind, prev_type.as_ref()) {
            let abs = Span::new(start + tok.span.start, start + tok.span.end);
            push_token(out, source, abs, ty);
        }
        prev_type = Some(tok.kind);
    }
}

/// Intermediate single-token record (absolute byte span + token type).
struct Raw {
    span: Span,
    ty: u32,
}

fn push_token(out: &mut Vec<Raw>, source: &str, span: Span, ty: u32) {
    // LSP semantic tokens cannot span newlines — split multi-line
    // spans (heredocs, multi-line strings) into one record per line.
    let bytes = source.as_bytes();
    let mut start = span.start;
    let end = span.end.min(bytes.len());
    while start < end {
        let nl = bytes[start..end]
            .iter()
            .position(|&b| b == b'\n')
            .map(|p| start + p)
            .unwrap_or(end);
        if nl > start {
            out.push(Raw {
                span: Span::new(start, nl),
                ty,
            });
        }
        start = nl + 1;
    }
}

/// Map a `TokenKind` to a semantic-token type index. Returns `None`
/// for tokens we deliberately omit (e.g. EOF, plain whitespace —
/// already filtered by the lexer).
fn classify(kind: &TokenKind, prev: Option<&TokenKind>) -> Option<u32> {
    match kind {
        TokenKind::Bool(_) | TokenKind::None => Some(T_KEYWORD),
        TokenKind::If | TokenKind::Else | TokenKind::Match => Some(T_KEYWORD),
        TokenKind::Number(_) => Some(T_NUMBER),
        TokenKind::Str(_) => Some(T_STRING),
        TokenKind::Symbol(_) => Some(T_ENUM_MEMBER),
        TokenKind::At => Some(T_DECORATOR),
        TokenKind::Ident(name) => {
            // Words the parser treats as soft keywords. None of these
            // are reserved `TokenKind` variants — they come through as
            // `Ident` and we color them up here.
            if matches!(
                name.as_str(),
                "type"
                    | "union"
                    | "interface"
                    | "extends"
                    | "symbol_set"
                    | "connection"
                    | "fn"
                    | "let"
                    | "in"
                    | "import"
                    | "use"
                    | "as"
                    | "namespace"
                    | "true"
                    | "false"
                    | "none"
            ) {
                return Some(T_KEYWORD);
            }
            // Identifier directly after `@` colors as a type — it
            // names a decorator schema, and the schema is itself a
            // type declaration.
            if matches!(prev, Some(TokenKind::At)) {
                return Some(T_TYPE);
            }
            Some(T_VARIABLE)
        }
        TokenKind::Eq
        | TokenKind::EqEq
        | TokenKind::BangEq
        | TokenKind::Bang
        | TokenKind::Lt
        | TokenKind::LtEq
        | TokenKind::Gt
        | TokenKind::GtEq
        | TokenKind::AmpAmp
        | TokenKind::PipePipe
        | TokenKind::Amp
        | TokenKind::Pipe
        | TokenKind::Plus
        | TokenKind::Dash
        | TokenKind::Star
        | TokenKind::Slash
        | TokenKind::Percent
        | TokenKind::Arrow
        | TokenKind::FatArrow
        | TokenKind::Question
        | TokenKind::Dot
        | TokenKind::DotDot
        | TokenKind::ColonColon => Some(T_OPERATOR),
        // Punctuation we don't paint: braces, parens, brackets,
        // commas, colons, semicolons. The editor's default
        // foreground handles them fine and emitting them just bloats
        // the response.
        _ => None,
    }
}

/// Convert a sorted list of absolute-position `Raw` tokens into the
/// LSP delta encoding: each token is `(delta_line, delta_start_char,
/// length, token_type, token_modifiers)`, where deltas are relative
/// to the previous emitted token. Columns are UTF-8 byte offsets
/// within the line (we advertise UTF-8 position encoding).
fn delta_encode(source: &str, raws: &[Raw]) -> Vec<SemanticToken> {
    let mut out = Vec::with_capacity(raws.len());
    let mut prev_line: u32 = 0;
    let mut prev_col: u32 = 0;
    for r in raws {
        let pos = crate::convert::offset_to_position(source, r.span.start);
        let length = (r.span.end - r.span.start) as u32;
        let delta_line = pos.line - prev_line;
        let delta_start = if delta_line == 0 {
            pos.character - prev_col
        } else {
            pos.character
        };
        out.push(SemanticToken {
            delta_line,
            delta_start,
            length,
            token_type: r.ty,
            token_modifiers_bitset: 0,
        });
        prev_line = pos.line;
        prev_col = pos.character;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn types_emitted(source: &str) -> Vec<u32> {
        compute(source).into_iter().map(|t| t.token_type).collect()
    }

    #[test]
    fn keywords_and_strings_classified() {
        let src = "type Foo {\n  v: utf8\n}\n";
        let types = types_emitted(src);
        assert!(types.contains(&T_KEYWORD), "no keyword in {types:?}");
        assert!(types.contains(&T_VARIABLE), "no variable in {types:?}");
    }

    #[test]
    fn decorator_marker_and_name() {
        let src = "@block(\"x\")\ntype Y {}\n";
        let toks = compute(src);
        // First emitted token should be the `@` decorator marker.
        assert_eq!(toks[0].token_type, T_DECORATOR);
        // The very next token (Ident "block" after `@`) should be a type.
        assert_eq!(toks[1].token_type, T_TYPE);
    }

    #[test]
    fn number_and_symbol_classified() {
        let src = "x = 42i32\ny = :gold\n";
        let types = types_emitted(src);
        assert!(types.contains(&T_NUMBER));
        assert!(types.contains(&T_ENUM_MEMBER));
    }

    #[test]
    fn interpolated_string_colors_slot_contents() {
        let src = "x = $\"hello ${y + 1}\"\n";
        // Find what types appear between the slot's `${` and `}`.
        let toks = compute(src);
        // We expect: variable color for `y`, operator for `+`, number for `1`.
        let types: Vec<u32> = toks.iter().map(|t| t.token_type).collect();
        assert!(types.contains(&T_STRING), "no STRING in {types:?}");
        assert!(types.contains(&T_VARIABLE), "no VARIABLE in {types:?}");
        assert!(types.contains(&T_NUMBER), "no NUMBER in {types:?}");
        assert!(types.contains(&T_OPERATOR), "no OPERATOR in {types:?}");
    }

    #[test]
    fn delta_encoding_is_relative() {
        let src = "a = 1\nb = 2\n";
        let toks = compute(src);
        // First token (Ident "a") sits at line 0, col 0.
        assert_eq!(toks[0].delta_line, 0);
        assert_eq!(toks[0].delta_start, 0);
        // Subsequent same-line tokens have delta_line == 0.
        // The line break to "b" should have delta_line == 1.
        let line_breaks = toks.iter().filter(|t| t.delta_line > 0).count();
        assert_eq!(line_breaks, 1);
    }
}
