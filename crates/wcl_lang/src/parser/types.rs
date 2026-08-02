//! Type-reference parsing: `&T`, `list<T>`, `tensor<T, [...]>`,
//! `fn(...) -> T`, named refs, builtin scalars.

use crate::ast::Span;
use crate::error::ParseError;
use crate::lexer::TokenKind;
use crate::value::{BuiltinType, TensorDim, TypeRef};

use super::{Parser, describe};

impl<'a> Parser<'a> {
    pub(super) fn parse_type_ref(&mut self) -> Result<(TypeRef, Span), ParseError> {
        self.enter_recursion()?;
        let result = self.parse_type_ref_inner();
        if result.is_ok() {
            self.leave_recursion();
        }
        result
    }

    fn parse_type_ref_inner(&mut self) -> Result<(TypeRef, Span), ParseError> {
        let head = self.peek()?;
        if matches!(head.kind, TokenKind::Amp) {
            let amp = self.bump()?;
            let (inner, inner_span) = self.parse_type_atom()?;
            let span = Span::new(amp.span.start, inner_span.end);
            Ok((TypeRef::Reference(Box::new(inner)), span))
        } else {
            self.parse_type_atom()
        }
    }

    fn parse_type_atom(&mut self) -> Result<(TypeRef, Span), ParseError> {
        // Contextual keywords introduced by an ident followed by a specific
        // opener: `list<...>`, `tensor<..., [...]>`, `fn(...) -> T`.
        let head_ident = match &self.peek()?.kind {
            TokenKind::Ident(s) => Some(s.clone()),
            _ => None,
        };
        if let Some(s) = head_ident {
            let next = &self.peek2()?.kind;
            match (s.as_str(), next) {
                ("list", TokenKind::Lt) => return self.parse_list_type(),
                ("tensor", TokenKind::Lt) => return self.parse_tensor_type(),
                ("fn", TokenKind::LParen) => return self.parse_function_type(),
                _ => {}
            }
        }

        let head = self.peek()?;
        if matches!(head.kind, TokenKind::Ident(_)) {
            let (path, path_span) = self.parse_path()?;
            if !matches!(self.peek()?.kind, TokenKind::Lt) {
                return Ok((path_to_type_ref(&path, Vec::new()), path_span));
            }
            let (args, args_span) = self.parse_type_args()?;
            let span = Span::new(path_span.start, args_span.end);
            Ok((path_to_type_ref(&path, args), span))
        } else {
            let span = head.span;
            let kind_desc = describe(&head.kind);
            Err(self.err(
                format!("expected type, found {kind_desc}"),
                span,
                "expected type",
            ))
        }
    }

    /// Parse the `<A, B>` suffix of a named type reference, positioned on
    /// the `<`. Purely syntactic: the arguments are recorded on the
    /// `TypeRef` and never checked against a declaration (there is no
    /// `type Foo<T>` form to check against).
    fn parse_type_args(&mut self) -> Result<(Vec<TypeRef>, Span), ParseError> {
        let lt = self.bump()?; // '<'
        let mut args: Vec<TypeRef> = Vec::new();
        loop {
            if matches!(self.peek()?.kind, TokenKind::Gt) {
                break;
            }
            let (arg, _) = self.parse_type_ref()?;
            args.push(arg);
            match self.peek()?.kind {
                TokenKind::Comma => {
                    self.bump()?;
                }
                TokenKind::Gt => break,
                _ => {
                    let tok = self.peek()?;
                    let span = tok.span;
                    let kind = describe(&tok.kind);
                    return Err(self.err(
                        format!("expected ',' or '>' in type arguments, found {kind}"),
                        span,
                        "expected ',' or '>'",
                    ));
                }
            }
        }
        let gt = self.expect(TokenKind::Gt, "expected '>' to close type arguments")?;
        let span = Span::new(lt.span.start, gt.span.end);
        if args.is_empty() {
            // An empty list isn't an argument list — there is nothing to
            // print, so `wcl fmt` would silently delete the `<>`. (A
            // *trailing* comma is accepted and normalised away, as it is
            // in tensor dimensions: that's formatting, not deletion.)
            return Err(self.err(
                "type argument list cannot be empty",
                span,
                "expected at least one type argument",
            ));
        }
        Ok((args, span))
    }

    /// Parse a `keyword<body>` form: bump the keyword, expect `<`, run
    /// `parse_body`, expect `>`. Used by `list<T>` and `tensor<T, [...]>`.
    /// `keyword` is folded into the open/close error messages.
    pub(super) fn parse_angle_bracketed<F, R>(
        &mut self,
        keyword: &'static str,
        parse_body: F,
    ) -> Result<(R, Span), ParseError>
    where
        F: FnOnce(&mut Self) -> Result<R, ParseError>,
    {
        let start = self.bump()?.span.start;
        self.expect(TokenKind::Lt, &format!("expected '<' after '{keyword}'"))?;
        let body = parse_body(self)?;
        let gt = self.expect(
            TokenKind::Gt,
            &format!("expected '>' to close {keyword}<...>"),
        )?;
        Ok((body, Span::new(start, gt.span.end)))
    }

    fn parse_list_type(&mut self) -> Result<(TypeRef, Span), ParseError> {
        self.parse_angle_bracketed("list", |p| {
            let (inner, _) = p.parse_type_ref()?;
            Ok(TypeRef::List(Box::new(inner)))
        })
    }

    fn parse_tensor_type(&mut self) -> Result<(TypeRef, Span), ParseError> {
        self.parse_angle_bracketed("tensor", |p| {
            let (element, _) = p.parse_type_ref()?;
            p.expect(TokenKind::Comma, "expected ',' after tensor element type")?;
            let lbracket = p.expect(TokenKind::LBracket, "expected '[' for tensor dimensions")?;
            let mut dims: Vec<TensorDim> = Vec::new();
            loop {
                if matches!(p.peek()?.kind, TokenKind::RBracket) {
                    break;
                }
                dims.push(p.parse_tensor_dim()?);
                match p.peek()?.kind {
                    TokenKind::Comma => {
                        p.bump()?;
                    }
                    TokenKind::RBracket => break,
                    _ => {
                        let tok = p.peek()?;
                        let span = tok.span;
                        let kind = describe(&tok.kind);
                        return Err(p.err(
                            format!("expected ',' or ']' in tensor dimensions, found {kind}"),
                            span,
                            "expected ',' or ']'",
                        ));
                    }
                }
            }
            let rbracket = p.expect(TokenKind::RBracket, "expected ']' to close tensor dims")?;
            if dims.is_empty() {
                return Err(p.err(
                    "tensor must have at least one dimension",
                    Span::new(lbracket.span.start, rbracket.span.end),
                    "expected at least one dimension",
                ));
            }
            Ok(TypeRef::Tensor {
                element: Box::new(element),
                dims,
            })
        })
    }

    fn parse_tensor_dim(&mut self) -> Result<TensorDim, ParseError> {
        let tok = self.bump()?;
        let span = tok.span;
        match tok.kind {
            TokenKind::Number(n) => n.as_u64().map(TensorDim::Fixed).ok_or_else(|| {
                self.err(
                    "tensor dimensions must be non-negative integers or symbolic identifiers",
                    span,
                    "invalid dimension",
                )
            }),
            TokenKind::Ident(name) => Ok(TensorDim::Symbolic(name)),
            other => Err(self.err(
                format!(
                    "expected dimension (integer or identifier), found {}",
                    describe(&other)
                ),
                span,
                "expected dimension",
            )),
        }
    }
}

/// Promote a parsed path into either a builtin scalar (`u32`, `utf8`,
/// …) or a named-type reference carrying `args`.
///
/// A builtin never takes type arguments, so `u32<T>` stays a named
/// reference — an unknown type, reported by the usual resolution pass
/// rather than by a special case here.
pub(super) fn path_to_type_ref(path: &[String], args: Vec<TypeRef>) -> TypeRef {
    if args.is_empty()
        && path.len() == 1
        && let Some(b) = BuiltinType::from_name(&path[0])
    {
        return TypeRef::Builtin(b);
    }
    TypeRef::Named {
        path: path.to_vec(),
        args,
    }
}
