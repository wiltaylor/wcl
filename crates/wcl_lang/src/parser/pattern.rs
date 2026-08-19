//! Pattern parsing — match arm patterns, qualified and unqualified
//! variant forms, record-pattern bodies.
//!
//! Held out from [`super`] (the main parser module) because variant
//! patterns and their record-body parser form one cohesive group with
//! no external coupling beyond the shared [`Parser`] token helpers.

use crate::ast::{Pattern, Span, VariantPatArgs};
use crate::error::ParseError;
use crate::lexer::TokenKind;

use super::{Parser, describe};

/// `(fields, has_rest, close_brace_end)`. Returned by
/// `Parser::parse_record_pattern_fields` so both qualified and
/// unqualified variant patterns can share the body parser.
type RecordPatBody = (Vec<(String, Pattern)>, bool, usize);

impl<'a> Parser<'a> {
    /// Parse one pattern, guarding recursion depth.
    pub(super) fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        self.enter_recursion()?;
        let result = self.parse_pattern_inner();
        if result.is_ok() {
            self.leave_recursion();
        }
        result
    }

    /// Parse one pattern, without the depth guard.
    fn parse_pattern_inner(&mut self) -> Result<Pattern, ParseError> {
        let tok = self.peek()?;
        match &tok.kind {
            TokenKind::Ident(s) if s == "_" => {
                let t = self.bump()?;
                Ok(Pattern::Wildcard(t.span))
            }
            TokenKind::Ident(_) => {
                // Could be: Binding, At pattern, Variant (path :: Variant),
                // or *unqualified* variant (`Name(...)` / `Name { ... }`).
                // Decide by peek2:
                //   - peek2 == `@` → At
                //   - peek2 == `::` → Variant (single-segment qualified)
                //   - peek2 == `.`  → multi-segment path; consume to find `::`
                //   - peek2 == `(` or `{` → unqualified variant (no type_path)
                //   - else → Binding
                let p2 = self.peek2()?.kind.clone();
                match p2 {
                    TokenKind::At => {
                        let (name, name_span) = self.bump_ident("expected binding name")?;
                        self.bump()?; // '@'
                        let inner = self.parse_pattern()?;
                        let span = Span::new(name_span.start, pattern_span(&inner).end);
                        Ok(Pattern::At {
                            name,
                            inner: Box::new(inner),
                            span,
                        })
                    }
                    TokenKind::ColonColon | TokenKind::Dot => self.parse_variant_pattern(),
                    TokenKind::LParen | TokenKind::LBrace => {
                        self.parse_unqualified_variant_pattern()
                    }
                    _ => {
                        let (name, span) = self.bump_ident("expected binding name")?;
                        Ok(Pattern::Binding { name, span })
                    }
                }
            }
            TokenKind::Bool(_)
            | TokenKind::Number(_)
            | TokenKind::Str(_)
            | TokenKind::Symbol(_) => {
                // Bump and dispatch on the owned kind so the literal
                // extraction is unambiguous to the compiler — avoids the
                // `let-else unreachable!()` shape that peek-then-bump
                // would otherwise require.
                let t = self.bump()?;
                let span = t.span;
                match t.kind {
                    TokenKind::Bool(b) => Ok(Pattern::LiteralBool(b, span)),
                    TokenKind::Number(n) => Ok(Pattern::LiteralNumber { lit: n, span }),
                    TokenKind::Str(crate::lexer::StringLit::Utf8(text)) => {
                        Ok(Pattern::LiteralUtf8(text, span))
                    }
                    TokenKind::Str(crate::lexer::StringLit::Ascii(text)) => {
                        Ok(Pattern::LiteralAscii(text, span))
                    }
                    TokenKind::Str(other) => Err(self.err(
                        format!(
                            "string patterns require utf8 or ascii literals, got {}",
                            match other {
                                crate::lexer::StringLit::Utf16(_) => "utf16",
                                crate::lexer::StringLit::Utf32(_) => "utf32",
                                _ => "string",
                            }
                        ),
                        span,
                        "unsupported string-pattern kind",
                    )),
                    TokenKind::Symbol(s) => Ok(Pattern::LiteralSymbol(s, span)),
                    // Outer arm guard already restricted us to these
                    // four kinds; any other is a structural bug.
                    _ => unreachable!("literal-pattern arm guard"),
                }
            }
            TokenKind::None => {
                let t = self.bump()?;
                Ok(Pattern::LiteralNone(t.span))
            }
            _ => {
                let span = tok.span;
                let kind = describe(&tok.kind);
                Err(self.err(
                    format!("expected pattern, found {kind}"),
                    span,
                    "expected pattern",
                ))
            }
        }
    }

    /// Parse `Path::Variant variant_pattern_args?`. Called when the
    /// outer pattern parser sees `Ident` followed by `.` or `::`.
    fn parse_variant_pattern(&mut self) -> Result<Pattern, ParseError> {
        let (path, path_span) = self.parse_path()?;
        self.expect(
            TokenKind::ColonColon,
            "expected '::' after type path in pattern",
        )?;
        let (v_name, v_span) = self.bump_ident("expected variant name after '::'")?;
        let (args, args_end) = self.parse_variant_pat_args(v_span.end)?;
        Ok(Pattern::Variant {
            type_path: path,
            variant: v_name,
            args,
            span: Span::new(path_span.start, args_end),
        })
    }

    /// Parse `Name(...)` / `Name { ... }` as an *unqualified* variant
    /// pattern. The matcher resolves the variant via the scrutinee's
    /// runtime union at match time.
    fn parse_unqualified_variant_pattern(&mut self) -> Result<Pattern, ParseError> {
        let (v_name, name_span) = self.bump_ident("expected variant name")?;
        let (args, args_end) = self.parse_variant_pat_args(name_span.end)?;
        Ok(Pattern::Variant {
            type_path: Vec::new(), // unqualified — matcher uses scrutinee's union
            variant: v_name,
            args,
            span: Span::new(name_span.start, args_end),
        })
    }

    /// Parse the args trailer common to both qualified and unqualified
    /// variant patterns: `(inner_pattern)`, `{ field: pat, .., }`, or
    /// no trailer at all (unit form). `name_end` is the end-pos of the
    /// preceding variant name; when there's no trailer it doubles as
    /// the args end-pos.
    fn parse_variant_pat_args(
        &mut self,
        name_end: usize,
    ) -> Result<(VariantPatArgs, usize), ParseError> {
        match self.peek()?.kind {
            TokenKind::LParen => {
                self.bump()?;
                let inner = self.parse_pattern()?;
                let rp = self.expect(TokenKind::RParen, "expected ')' after variant pattern")?;
                Ok((VariantPatArgs::Positional(Box::new(inner)), rp.span.end))
            }
            TokenKind::LBrace => {
                self.bump()?;
                let (fields, rest, brace_end) = self.parse_record_pattern_fields()?;
                Ok((VariantPatArgs::Record { fields, rest }, brace_end))
            }
            _ => Ok((VariantPatArgs::Unit, name_end)),
        }
    }

    /// Parse the body of a record-pattern `{ field: pat, ..,  }` after
    /// the opening brace has already been consumed. Returns the field
    /// list, whether a `..` rest pattern was present, and the end-pos
    /// of the closing `}`.
    fn parse_record_pattern_fields(&mut self) -> Result<RecordPatBody, ParseError> {
        let mut fields: Vec<(String, Pattern)> = Vec::new();
        let mut rest = false;
        while !matches!(self.peek()?.kind, TokenKind::RBrace) {
            if matches!(self.peek()?.kind, TokenKind::DotDot) {
                self.bump()?;
                rest = true;
                // Trailing comma after `..` is allowed.
                if matches!(self.peek()?.kind, TokenKind::Comma) {
                    self.bump()?;
                }
                break;
            }
            let (fname, fname_span) = self.bump_ident("expected field name in record pattern")?;
            let inner = if matches!(self.peek()?.kind, TokenKind::Colon) {
                self.bump()?;
                self.parse_pattern()?
            } else {
                // `{ name }` shorthand → bind by the field's own name.
                Pattern::Binding {
                    name: fname.clone(),
                    span: fname_span,
                }
            };
            fields.push((fname, inner));
            match self.peek()?.kind {
                TokenKind::Comma => {
                    self.bump()?;
                }
                TokenKind::RBrace => break,
                _ => {
                    let p = self.peek()?;
                    let span = p.span;
                    let kind = describe(&p.kind);
                    return Err(self.err(
                        format!("expected ',' or '}}' in record pattern, found {kind}"),
                        span,
                        "expected ',' or '}'",
                    ));
                }
            }
        }
        let rb = self.expect(TokenKind::RBrace, "expected '}' to close record pattern")?;
        Ok((fields, rest, rb.span.end))
    }
}

/// The source span of any pattern, whatever its form.
pub(super) fn pattern_span(p: &Pattern) -> Span {
    match p {
        Pattern::Wildcard(s)
        | Pattern::LiteralBool(_, s)
        | Pattern::LiteralUtf8(_, s)
        | Pattern::LiteralAscii(_, s)
        | Pattern::LiteralSymbol(_, s)
        | Pattern::LiteralNone(s) => *s,
        Pattern::Binding { span, .. }
        | Pattern::At { span, .. }
        | Pattern::LiteralNumber { span, .. }
        | Pattern::Variant { span, .. } => *span,
    }
}

/// Collect every binding name the pattern introduces. Used to enforce
/// that all `|` alternatives bind the same names.
pub(super) fn collect_binding_names(p: &Pattern) -> std::collections::BTreeSet<String> {
    use std::collections::BTreeSet;
    fn walk(p: &Pattern, out: &mut BTreeSet<String>) {
        match p {
            Pattern::Binding { name, .. } => {
                out.insert(name.clone());
            }
            Pattern::At { name, inner, .. } => {
                out.insert(name.clone());
                walk(inner, out);
            }
            Pattern::Variant { args, .. } => match args {
                VariantPatArgs::Unit => {}
                VariantPatArgs::Positional(inner) => walk(inner, out),
                VariantPatArgs::Record { fields, .. } => {
                    for (_, p) in fields {
                        walk(p, out);
                    }
                }
            },
            _ => {}
        }
    }
    let mut s = BTreeSet::new();
    walk(p, &mut s);
    s
}
