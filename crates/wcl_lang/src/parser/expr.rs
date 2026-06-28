//! Pratt expression parser and its sub-productions. Extracted from
//! `parser/mod.rs` so the parent file can stay focused on the
//! top-level source / item driver and shared token helpers.

use crate::ast::{
    BinOp, CALL_BP, ElemTrivia, Expr, FunctionLit, LetBinding, MEMBER_BP, MatchArm, NamedArg,
    Parameter, Pattern, Span, Trivia, UNARY_BP, UnaryOp, VariantArgs,
};
use crate::error::ParseError;
use crate::lexer::{NumberLit, TokenKind};
use crate::value::TypeRef;

use super::Parser;
use super::describe;
use super::pattern::{collect_binding_names, pattern_span};

impl<'a> Parser<'a> {
    /// Literal-only value parser. Used by contexts that intentionally accept
    /// only primary tokens (decorator arguments, block labels) — full
    /// expressions go through [`parse_expr`].
    pub(super) fn parse_value_expr(&mut self) -> Result<(Expr, Span), ParseError> {
        let tok = self.bump()?;
        let span = tok.span;
        let expr = match tok.kind {
            TokenKind::Number(n) => number_to_expr(n),
            TokenKind::NumberWithUnit(b) => {
                let (value, unit) = *b;
                Expr::UnitLiteral { value, unit, span }
            }
            TokenKind::Str(s) => self.string_lit_to_expr(s, span)?,
            TokenKind::Bool(b) => Expr::Bool(b),
            TokenKind::Ident(s) => Expr::Identifier(s, span),
            TokenKind::Symbol(s) => Expr::Symbol(s),
            TokenKind::None => Expr::None,
            other => {
                return Err(self.err(
                    format!("expected value, found {}", describe(&other)),
                    span,
                    "expected value",
                ));
            }
        };
        Ok((expr, span))
    }

    /// Parse a block **label** that starts with a bare identifier,
    /// stitching it across byte-adjacent `-`/`/` connectors into one
    /// compound identifier (`class dgm-box {}`, `page reference/intro {}`).
    ///
    /// This exists only because the lexer (correctly) emits `-`/`/` as
    /// standalone `Dash`/`Slash` tokens so `a-b`/`a/b` stay arithmetic.
    /// Labels are not an expression context, so here we re-join the
    /// pieces from the source span. A connector is consumed only when it
    /// is immediately adjacent (no whitespace/newline) to the previous
    /// run *and* immediately followed by an `Ident`/`Number` — so a
    /// spaced `a - b`, a dangling `foo-`, or a trailing `foo-{` never
    /// stitch.
    pub(super) fn parse_label_ident(&mut self) -> Result<(Expr, Span), ParseError> {
        let first = self.bump()?; // the leading `Ident`
        let start = first.span.start;
        let mut end = first.span.end;
        loop {
            let conn = self.peek()?;
            if !matches!(conn.kind, TokenKind::Dash | TokenKind::Slash)
                || conn.preceded_by_newline
                || conn.span.start != end
            {
                break;
            }
            let conn_end = conn.span.end;
            let after = self.peek2()?;
            if after.span.start != conn_end
                || !matches!(after.kind, TokenKind::Ident(_) | TokenKind::Number(_))
            {
                break;
            }
            self.bump()?; // connector
            let cont = self.bump()?; // ident / number
            end = cont.span.end;
        }
        let span = Span::new(start, end);
        Ok((
            Expr::Identifier(self.src[start..end].to_string(), span),
            span,
        ))
    }

    /// Pratt expression parser. Used in any context where a full expression
    /// is allowed (field RHS, function-literal bodies, `let` initialisers,
    /// parenthesised sub-expressions, call arguments).
    pub(crate) fn parse_expr(&mut self) -> Result<(Expr, Span), ParseError> {
        self.parse_expr_bp(0)
    }

    /// Drive the parser as if the input were a single expression and
    /// nothing else (no trailing tokens before EOF). Used by the
    /// top-level `wcl_lang::parse_expr` entry point so hosts can parse
    /// a value-shaped argument (e.g. a CLI `set <value>`) without
    /// having to wrap it in a `field = ...` declaration.
    pub(crate) fn parse_expr_only(&mut self) -> Result<Expr, ParseError> {
        let (expr, _) = self.parse_expr()?;
        let tok = self.peek()?;
        if !matches!(tok.kind, TokenKind::Eof) {
            let span = tok.span;
            let found = describe(&tok.kind);
            return Err(self.err(
                format!("expected end of input after expression, found {found}"),
                span,
                "unexpected trailing token",
            ));
        }
        Ok(expr)
    }

    fn parse_expr_bp(&mut self, min_bp: u8) -> Result<(Expr, Span), ParseError> {
        self.enter_recursion()?;
        let result = self.parse_expr_bp_inner(min_bp);
        if result.is_ok() {
            self.leave_recursion();
        }
        result
    }

    fn parse_expr_bp_inner(&mut self, min_bp: u8) -> Result<(Expr, Span), ParseError> {
        let (mut lhs, mut span) = self.parse_prefix()?;
        loop {
            let kind = self.peek()?.kind.clone();
            // Postfix call: `expr(args)`.
            if matches!(kind, TokenKind::LParen) {
                if CALL_BP < min_bp {
                    break;
                }
                let (call_expr, call_span) = self.parse_call_tail(lhs, span)?;
                lhs = call_expr;
                span = call_span;
                continue;
            }
            // Postfix member access: `expr.IDENT`.
            if matches!(kind, TokenKind::Dot) {
                if MEMBER_BP < min_bp {
                    break;
                }
                self.bump()?; // '.'
                let name_tok = self.bump()?;
                let name = match name_tok.kind {
                    TokenKind::Ident(s) => s,
                    // An integer segment addresses a block by a numeric
                    // `@inline(0)` label (`steps.1` → the step labelled 1 — a
                    // label match, NOT positional indexing). Suffix-free, to
                    // match `Value::as_path_segment`. A float has no label
                    // meaning and stays an error.
                    TokenKind::Number(ref n) => {
                        match crate::numeric::numeric_as_path_segment!(n, NumberLit) {
                            Some(seg) => seg,
                            None => {
                                return Err(self.err(
                                    "expected identifier or integer after '.', found a float"
                                        .to_string(),
                                    name_tok.span,
                                    "expected identifier",
                                ));
                            }
                        }
                    }
                    other => {
                        return Err(self.err(
                            format!("expected identifier after '.', found {}", describe(&other)),
                            name_tok.span,
                            "expected identifier",
                        ));
                    }
                };
                let new_span = Span::new(span.start, name_tok.span.end);
                lhs = Expr::Member {
                    recv: Box::new(lhs),
                    name,
                    span: new_span,
                };
                span = new_span;
                continue;
            }
            // Variant construction: `Path::Variant args?`. The LHS must
            // be a pure dotted path (Identifier / Member chain).
            if matches!(kind, TokenKind::ColonColon) {
                // Variant construction binds like member access.
                const VARIANT_BP: u8 = MEMBER_BP;
                if VARIANT_BP < min_bp {
                    break;
                }
                let Some(type_path) = flatten_path_expr(&lhs) else {
                    let p = self.peek()?;
                    let span = p.span;
                    return Err(self.err(
                        "'::' is only valid after a type path",
                        span,
                        "expected a type name on the left of '::'",
                    ));
                };
                self.bump()?; // '::'
                let v_tok = self.bump()?;
                let TokenKind::Ident(v_name) = v_tok.kind else {
                    return Err(self.err(
                        format!(
                            "expected variant name after '::', found {}",
                            describe(&v_tok.kind)
                        ),
                        v_tok.span,
                        "expected variant name",
                    ));
                };
                let (args, args_end) = self.parse_variant_args(v_tok.span.end)?;
                let new_span = Span::new(span.start, args_end);
                lhs = Expr::Variant {
                    type_path,
                    variant: v_name,
                    args,
                    span: new_span,
                };
                span = new_span;
                continue;
            }
            let Some((lbp, rbp, op)) = bin_op_info(&kind) else {
                break;
            };
            if lbp < min_bp {
                break;
            }
            self.bump()?; // consume operator
            let (rhs, rhs_span) = self.parse_expr_bp(rbp)?;
            let new_span = Span::new(span.start, rhs_span.end);
            lhs = Expr::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
                span: new_span,
            };
            span = new_span;
        }
        Ok((lhs, span))
    }

    fn parse_prefix(&mut self) -> Result<(Expr, Span), ParseError> {
        let kind = self.peek()?.kind.clone();
        match kind {
            TokenKind::Dash => {
                let tok = self.bump()?;
                let (operand, operand_span) = self.parse_expr_bp(UNARY_BP)?;
                let span = Span::new(tok.span.start, operand_span.end);
                Ok((
                    Expr::Unary {
                        op: UnaryOp::Neg,
                        operand: Box::new(operand),
                        span,
                    },
                    span,
                ))
            }
            TokenKind::Bang => {
                let tok = self.bump()?;
                let (operand, operand_span) = self.parse_expr_bp(UNARY_BP)?;
                let span = Span::new(tok.span.start, operand_span.end);
                Ok((
                    Expr::Unary {
                        op: UnaryOp::Not,
                        operand: Box::new(operand),
                        span,
                    },
                    span,
                ))
            }
            _ => self.parse_atom(),
        }
    }

    fn parse_atom(&mut self) -> Result<(Expr, Span), ParseError> {
        // `fn (` triggers a function literal.
        if let TokenKind::Ident(s) = &self.peek()?.kind
            && s == "fn"
            && matches!(self.peek2()?.kind, TokenKind::LParen)
        {
            return self.parse_function_literal();
        }
        // Contextual keyword `try` starts a try/catch expression. Like
        // `parent` / `self` below, it is a keyword only in expression
        // position — fields named `try` keep parsing.
        if let TokenKind::Ident(s) = &self.peek()?.kind
            && s == "try"
        {
            return self.parse_try_expr();
        }
        // Contextual keywords `parent` and `self` only act as keywords
        // in expression position. Anywhere else they remain regular
        // identifiers, so existing source (e.g. `parent: &User?` field
        // declarations) keeps working.
        let contextual_kw = match &self.peek()?.kind {
            TokenKind::Ident(s) if s == "parent" => Some(false), // false = parent
            TokenKind::Ident(s) if s == "self" => Some(true),    // true = self
            _ => None,
        };
        if let Some(is_self) = contextual_kw {
            let tok = self.bump()?;
            let expr = if is_self {
                Expr::SelfKw(tok.span)
            } else {
                Expr::ParentKw(tok.span)
            };
            return Ok((expr, tok.span));
        }
        let kind = self.peek()?.kind.clone();
        match kind {
            TokenKind::If => self.parse_if_expr(),
            TokenKind::Match => self.parse_match_expr(),
            TokenKind::LParen => self.parse_paren_expr(),
            TokenKind::LBrace => self.parse_brace_atom(),
            TokenKind::LBracket => self.parse_list_literal(),
            _ => self.parse_value_expr(),
        }
    }

    fn parse_if_expr(&mut self) -> Result<(Expr, Span), ParseError> {
        let if_tok = self.bump()?; // 'if'
        let start = if_tok.span.start;
        // `if let` form: pattern-binding conditional.
        if matches!(&self.peek()?.kind, TokenKind::Ident(s) if s == "let") {
            self.bump()?; // 'let'
            let pattern = self.parse_pattern()?;
            self.expect(TokenKind::Eq, "expected '=' after pattern in 'if let'")?;
            let (scrut, _) = self.parse_expr()?;
            let (then_block, then_span) = self.parse_block_expr()?;
            self.expect(TokenKind::Else, "'if let' requires an 'else' branch")?;
            let (else_block, else_span) = self.parse_if_or_block()?;
            let span = Span::new(start, else_span.end);
            return Ok((
                Expr::IfLet {
                    pattern,
                    scrut: Box::new(scrut),
                    then_block: Box::new(then_block),
                    else_block: Box::new(else_block),
                    span,
                },
                Span::new(start, then_span.end.max(else_span.end)),
            ));
        }
        // Plain `if cond { ... } else { ... }`.
        let (cond, _) = self.parse_expr()?;
        let (then_block, then_span) = self.parse_block_expr()?;
        self.expect(TokenKind::Else, "'if' requires an 'else' branch")?;
        let (else_block, else_span) = self.parse_if_or_block()?;
        let span = Span::new(start, else_span.end);
        Ok((
            Expr::If {
                cond: Box::new(cond),
                then_block: Box::new(then_block),
                else_block: Box::new(else_block),
                span,
            },
            Span::new(start, then_span.end.max(else_span.end)),
        ))
    }

    /// After `else`, the source can be either a block (`{ … }`) or
    /// another `if`/`if let` (for chaining). Pick based on the next
    /// token.
    fn parse_if_or_block(&mut self) -> Result<(Expr, Span), ParseError> {
        if matches!(self.peek()?.kind, TokenKind::If) {
            self.parse_if_expr()
        } else {
            self.parse_block_expr()
        }
    }

    fn parse_match_expr(&mut self) -> Result<(Expr, Span), ParseError> {
        let m_tok = self.bump()?; // 'match'
        let start = m_tok.span.start;
        let (scrut, _) = self.parse_expr()?;
        self.expect(TokenKind::LBrace, "expected '{' after match scrutinee")?;
        let mut arms: Vec<MatchArm> = Vec::new();
        while !matches!(self.peek()?.kind, TokenKind::RBrace) {
            arms.push(self.parse_match_arm()?);
            match self.peek()?.kind {
                TokenKind::Comma => {
                    self.bump()?;
                }
                TokenKind::RBrace => {}
                _ => {
                    let p = self.peek()?;
                    let span = p.span;
                    let kind = describe(&p.kind);
                    return Err(self.err(
                        format!("expected ',' or '}}' between match arms, found {kind}"),
                        span,
                        "expected ',' or '}'",
                    ));
                }
            }
            // After the optional comma, an inline comment on the next
            // token (the next arm, or `}`) trails this arm.
            if let Some(c) = self.take_same_line_comment()?
                && let Some(last) = arms.last_mut()
            {
                last.trailing_comment = Some(c);
            }
        }
        let rbrace = self.expect(TokenKind::RBrace, "expected '}' to close match")?;
        // Comments on their own lines after the last arm, before `}`.
        let trailing_trivia = rbrace.leading_trivia.clone();
        let span = Span::new(start, rbrace.span.end);
        // Structural exhaustiveness: the last arm must be a single
        // irrefutable pattern (Wildcard or Binding) and have no guard.
        match arms.last() {
            Some(last) if last.guard.is_none() && last.patterns.len() == 1 => {
                if !matches!(
                    last.patterns[0],
                    Pattern::Wildcard(_) | Pattern::Binding { .. }
                ) {
                    return Err(self.err(
                        "match must end with a wildcard or binding arm",
                        last.span,
                        "this arm is refutable; add `_ => …` at the end",
                    ));
                }
            }
            Some(last) => {
                return Err(self.err(
                    "match must end with a wildcard or binding arm with no guard",
                    last.span,
                    "make the final arm `_ => …` (no alternation, no guard)",
                ));
            }
            None => {
                return Err(self.err(
                    "match expression must have at least one arm",
                    span,
                    "expected at least one arm",
                ));
            }
        }
        Ok((
            Expr::Match {
                scrut: Box::new(scrut),
                arms,
                trailing_trivia,
                span,
            },
            span,
        ))
    }

    fn parse_match_arm(&mut self) -> Result<MatchArm, ParseError> {
        // Comments above this arm, before its first pattern.
        let leading_trivia = self.peek_leading_trivia()?;
        let first = self.parse_pattern()?;
        let arm_start = pattern_span(&first).start;
        let mut patterns = vec![first];
        while matches!(self.peek()?.kind, TokenKind::Pipe) {
            self.bump()?; // '|'
            patterns.push(self.parse_pattern()?);
        }
        // Optional guard: `if expr`.
        let guard = if matches!(self.peek()?.kind, TokenKind::If) {
            self.bump()?;
            let (g, _) = self.parse_expr()?;
            Some(g)
        } else {
            None
        };
        // Alternatives must bind the same names — keeps `name` in the
        // body unambiguous across alternatives.
        if patterns.len() > 1 {
            let first_names = collect_binding_names(&patterns[0]);
            for alt in &patterns[1..] {
                let names = collect_binding_names(alt);
                if names != first_names {
                    return Err(self.err(
                        "match alternatives bind different names",
                        pattern_span(alt),
                        "each `|` alternative must introduce the same bindings",
                    ));
                }
            }
        }
        self.expect(TokenKind::FatArrow, "expected '=>' after match pattern")?;
        let (body, body_span) = self.parse_expr()?;
        let span = Span::new(arm_start, body_span.end);
        Ok(MatchArm {
            patterns,
            guard,
            body,
            span,
            leading_trivia,
            trailing_comment: None,
        })
    }

    fn parse_list_literal(&mut self) -> Result<(Expr, Span), ParseError> {
        let lb = self.bump()?; // '['
        let mut elements = Vec::new();
        let mut elem_trivia: Vec<ElemTrivia> = Vec::new();
        if !matches!(self.peek()?.kind, TokenKind::RBracket) {
            loop {
                let leading = self.peek_leading_trivia()?;
                let (e, _) = self.parse_expr()?;
                elements.push(e);
                elem_trivia.push(ElemTrivia {
                    leading,
                    trailing: None,
                });
                match self.peek()?.kind {
                    TokenKind::Comma => {
                        self.bump()?;
                        self.attach_elem_trailing(&mut elem_trivia)?;
                        if matches!(self.peek()?.kind, TokenKind::RBracket) {
                            break;
                        }
                    }
                    TokenKind::RBracket => {
                        self.attach_elem_trailing(&mut elem_trivia)?;
                        break;
                    }
                    _ => {
                        let p = self.peek()?;
                        let span = p.span;
                        let kind = describe(&p.kind);
                        return Err(self.err(
                            format!("expected ',' or ']' in list literal, found {kind}"),
                            span,
                            "expected ',' or ']'",
                        ));
                    }
                }
            }
        }
        let rb = self.expect(TokenKind::RBracket, "expected ']' to close list literal")?;
        let trailing_trivia = rb.leading_trivia.clone();
        let span = Span::new(lb.span.start, rb.span.end);
        Ok((
            Expr::ListLit {
                elements,
                elem_trivia,
                trailing_trivia,
                span,
            },
            span,
        ))
    }

    /// Attach the next token's same-line comment (if any) to the most
    /// recent element-trivia entry — the inline trailing comment of a
    /// list element or call argument.
    fn attach_elem_trailing(&mut self, elem_trivia: &mut [ElemTrivia]) -> Result<(), ParseError> {
        if let Some(c) = self.take_same_line_comment()?
            && let Some(t) = elem_trivia.last_mut()
        {
            t.trailing = Some(c);
        }
        Ok(())
    }

    fn parse_paren_expr(&mut self) -> Result<(Expr, Span), ParseError> {
        let lparen = self.bump()?; // '('
        let (inner, _) = self.parse_expr()?;
        let rparen = self.expect(TokenKind::RParen, "expected ')'")?;
        let span = Span::new(lparen.span.start, rparen.span.end);
        Ok((
            Expr::Paren {
                inner: Box::new(inner),
                span,
            },
            span,
        ))
    }

    /// `{ … }` in atom position. Disambiguates a bare record literal
    /// (`{ name: value, … }`) from a block expression. The lookahead is
    /// unambiguous: a block expression never begins `Ident :` (a `:`
    /// only appears in field-name / type-annotation position).
    fn parse_brace_atom(&mut self) -> Result<(Expr, Span), ParseError> {
        let lbrace = self.bump()?; // '{'
        if matches!(self.peek()?.kind, TokenKind::Ident(_))
            && matches!(self.peek2()?.kind, TokenKind::Colon)
        {
            let (fields, end, trailing_trivia) = self.parse_record_fields()?;
            let span = Span::new(lbrace.span.start, end);
            return Ok((
                Expr::Record {
                    fields,
                    trailing_trivia,
                    span,
                },
                span,
            ));
        }
        self.parse_block_body(lbrace.span.start)
    }

    pub(super) fn parse_block_expr(&mut self) -> Result<(Expr, Span), ParseError> {
        let lbrace = self.bump()?; // '{'
        self.parse_block_body(lbrace.span.start)
    }

    /// Parse a block expression's body (zero-or-more `let` bindings then
    /// a mandatory tail expression) up to and including the closing `}`.
    /// Assumes the opening `{` at `start` has already been consumed.
    fn parse_block_body(&mut self, start: usize) -> Result<(Expr, Span), ParseError> {
        let mut lets = Vec::new();
        while matches!(&self.peek()?.kind, TokenKind::Ident(s) if s == "let") {
            lets.push(self.parse_let_binding()?);
        }
        // Empty `{}` is not a valid expression — we need a tail expression.
        if matches!(self.peek()?.kind, TokenKind::RBrace) {
            let span = self.peek()?.span;
            return Err(self.err(
                "block expression requires a final expression",
                span,
                "expected expression",
            ));
        }
        // Comments between the last binding and the tail print above the
        // tail; any after the tail (before `}`) join them there too.
        let mut trailing_trivia = self.peek_leading_trivia()?;
        let (tail, _) = self.parse_expr()?;
        if let Some(c) = self.take_same_line_comment()? {
            trailing_trivia.push(Trivia::LineComment(c));
        }
        let rbrace = self.expect(TokenKind::RBrace, "expected '}' to close block")?;
        trailing_trivia.extend(rbrace.leading_trivia.iter().cloned());
        let span = Span::new(start, rbrace.span.end);
        Ok((
            Expr::Block {
                lets,
                tail: Box::new(tail),
                trailing_trivia,
                span,
            },
            span,
        ))
    }

    fn parse_let_binding(&mut self) -> Result<LetBinding, ParseError> {
        // Comments above this binding, before `let`.
        let leading_trivia = self.peek_leading_trivia()?;
        let let_tok = self.bump()?; // 'let'
        let name_tok = self.bump()?;
        let TokenKind::Ident(name) = name_tok.kind else {
            return Err(self.err(
                format!(
                    "expected name after 'let', found {}",
                    describe(&name_tok.kind)
                ),
                name_tok.span,
                "expected identifier",
            ));
        };
        self.expect(TokenKind::Eq, "expected '=' after let name")?;
        let (value, value_span) = self.parse_expr()?;
        self.expect(TokenKind::Semi, "expected ';' after let binding")?;
        // An inline comment after the `;` trails this binding.
        let trailing_comment = self.take_same_line_comment()?;
        let span = Span::new(let_tok.span.start, value_span.end);
        Ok(LetBinding {
            name,
            value,
            span,
            leading_trivia,
            trailing_comment,
        })
    }

    /// Parse `(expr)` / `{ name: expr, … }` / nothing as variant args.
    /// `default_end` is used when there are no args (the variant is a
    /// unit constructor like `Shape::Empty`).
    fn parse_variant_args(
        &mut self,
        default_end: usize,
    ) -> Result<(VariantArgs, usize), ParseError> {
        match self.peek()?.kind {
            TokenKind::LParen => {
                self.bump()?;
                let (e, _) = self.parse_expr()?;
                let rp = self.expect(TokenKind::RParen, "expected ')' after variant payload")?;
                Ok((VariantArgs::Positional(Box::new(e)), rp.span.end))
            }
            TokenKind::LBrace => {
                self.bump()?;
                let (fields, end, trailing_trivia) = self.parse_record_fields()?;
                Ok((
                    VariantArgs::Record {
                        fields,
                        trailing_trivia,
                    },
                    end,
                ))
            }
            _ => Ok((VariantArgs::Unit, default_end)),
        }
    }

    /// Parse a comma-separated `name: value` field list up to and
    /// including the closing `}`. Assumes the opening `{` has already
    /// been consumed. Shared by variant constructors (`T::V { … }`) and
    /// bare record literals (`{ … }`); returns the fields plus the end
    /// offset of the closing brace.
    fn parse_record_fields(&mut self) -> Result<(Vec<NamedArg>, usize, Vec<Trivia>), ParseError> {
        let mut fields: Vec<NamedArg> = Vec::new();
        while !matches!(self.peek()?.kind, TokenKind::RBrace) {
            let leading_trivia = self.peek_leading_trivia()?;
            let name_tok = self.bump()?;
            let TokenKind::Ident(fname) = name_tok.kind else {
                return Err(self.err(
                    format!(
                        "expected field name in record, found {}",
                        describe(&name_tok.kind)
                    ),
                    name_tok.span,
                    "expected field name",
                ));
            };
            self.expect(TokenKind::Colon, "expected ':' after field name in record")?;
            let (value, value_span) = self.parse_expr()?;
            fields.push(NamedArg {
                name: fname,
                value,
                span: Span::new(name_tok.span.start, value_span.end),
                leading_trivia,
                trailing_comment: None,
            });
            match self.peek()?.kind {
                TokenKind::Comma => {
                    self.bump()?;
                    self.attach_field_trailing(&mut fields)?;
                }
                TokenKind::RBrace => {
                    self.attach_field_trailing(&mut fields)?;
                    break;
                }
                _ => {
                    let p = self.peek()?;
                    let span = p.span;
                    let kind = describe(&p.kind);
                    return Err(self.err(
                        format!("expected ',' or '}}' in record, found {kind}"),
                        span,
                        "expected ',' or '}'",
                    ));
                }
            }
        }
        let rb = self.expect(TokenKind::RBrace, "expected '}' to close record")?;
        let trailing_trivia = rb.leading_trivia.clone();
        Ok((fields, rb.span.end, trailing_trivia))
    }

    /// Attach the next token's same-line comment (if any) to the most
    /// recent record/variant field as its inline trailing comment.
    fn attach_field_trailing(&mut self, fields: &mut [NamedArg]) -> Result<(), ParseError> {
        if let Some(c) = self.take_same_line_comment()?
            && let Some(f) = fields.last_mut()
        {
            f.trailing_comment = Some(c);
        }
        Ok(())
    }

    fn parse_call_tail(
        &mut self,
        callee: Expr,
        callee_span: Span,
    ) -> Result<(Expr, Span), ParseError> {
        self.bump()?; // '('
        let mut args = Vec::new();
        let mut arg_trivia: Vec<ElemTrivia> = Vec::new();
        if !matches!(self.peek()?.kind, TokenKind::RParen) {
            loop {
                let leading = self.peek_leading_trivia()?;
                let (e, _) = self.parse_expr()?;
                args.push(e);
                arg_trivia.push(ElemTrivia {
                    leading,
                    trailing: None,
                });
                match self.peek()?.kind {
                    TokenKind::Comma => {
                        self.bump()?;
                        self.attach_elem_trailing(&mut arg_trivia)?;
                        if matches!(self.peek()?.kind, TokenKind::RParen) {
                            break;
                        }
                    }
                    TokenKind::RParen => {
                        self.attach_elem_trailing(&mut arg_trivia)?;
                        break;
                    }
                    _ => {
                        let p = self.peek()?;
                        let span = p.span;
                        let kind = describe(&p.kind);
                        return Err(self.err(
                            format!("expected ',' or ')' in call arguments, found {kind}"),
                            span,
                            "expected ',' or ')'",
                        ));
                    }
                }
            }
        }
        let rparen = self.expect(TokenKind::RParen, "expected ')' to close call")?;
        let trailing_trivia = rparen.leading_trivia.clone();
        let span = Span::new(callee_span.start, rparen.span.end);
        Ok((
            Expr::Call {
                callee: Box::new(callee),
                args,
                arg_trivia,
                trailing_trivia,
                span,
            },
            span,
        ))
    }

    fn parse_fn_parameter(&mut self) -> Result<Parameter, ParseError> {
        // Comments above this parameter print on their own line in the
        // multi-line form. Trailing comment is filled in by the caller.
        let leading_trivia = self.peek_leading_trivia()?;
        let name_tok = self.bump()?;
        let param_start = name_tok.span.start;
        let TokenKind::Ident(pname) = name_tok.kind else {
            return Err(self.err(
                format!(
                    "expected parameter name, found {}",
                    describe(&name_tok.kind)
                ),
                name_tok.span,
                "expected parameter name",
            ));
        };
        self.expect(TokenKind::Colon, "expected ':' after parameter name")?;
        let (ty, ty_span) = self.parse_type_ref()?;
        Ok(Parameter {
            name: pname,
            ty,
            ty_span,
            span: Span::new(param_start, ty_span.end),
            leading_trivia,
            trailing_comment: None,
        })
    }

    /// `try body catch name => handler` (either side may be a `{ … }`
    /// block expression; the handler also accepts the block form
    /// without `=>`: `catch name { … }`). The `try` token has been
    /// matched but not consumed.
    fn parse_try_expr(&mut self) -> Result<(Expr, Span), ParseError> {
        let try_tok = self.bump()?; // consume 'try'
        let start = try_tok.span.start;
        let (body, _) = if matches!(self.peek()?.kind, TokenKind::LBrace) {
            self.parse_block_expr()?
        } else {
            self.parse_expr()?
        };
        let catch_tok = self.bump()?;
        match &catch_tok.kind {
            TokenKind::Ident(s) if s == "catch" => {}
            other => {
                return Err(self.err(
                    format!("expected 'catch' after try body, found {}", describe(other)),
                    catch_tok.span,
                    "a try expression is `try body catch name => handler`",
                ));
            }
        }
        let (binder, binder_span) = self.bump_ident("expected binding name after 'catch'")?;
        let (handler, handler_span) = match self.peek()?.kind {
            TokenKind::FatArrow => {
                self.bump()?; // consume '=>'
                if matches!(self.peek()?.kind, TokenKind::LBrace) {
                    self.parse_block_expr()?
                } else {
                    self.parse_expr()?
                }
            }
            TokenKind::LBrace => self.parse_block_expr()?,
            _ => {
                let span = self.peek()?.span;
                return Err(self.err(
                    "expected '=>' or '{' after the catch binding",
                    span,
                    "write `catch name => expr` or `catch name { … }`",
                ));
            }
        };
        let span = Span::new(start, handler_span.end);
        Ok((
            Expr::Try {
                body: Box::new(body),
                binder,
                binder_span,
                handler: Box::new(handler),
                span,
            },
            span,
        ))
    }

    fn parse_function_literal(&mut self) -> Result<(Expr, Span), ParseError> {
        let fn_tok = self.bump()?; // 'fn'
        self.parse_function_tail(fn_tok.span.start)
    }

    /// Parse a function literal's parameter list, return type and body,
    /// with the `fn` token already consumed. `start` is the `fn` token's
    /// start offset, anchoring the literal's span. Shared by expression
    /// literals (`fn(…) -> T body`) and `fn name(…)` items.
    pub(super) fn parse_function_tail(&mut self, start: usize) -> Result<(Expr, Span), ParseError> {
        self.expect(TokenKind::LParen, "expected '(' after 'fn'")?;
        let mut params: Vec<Parameter> = Vec::new();
        let mut trailing_trivia = Vec::new();
        if !matches!(self.peek()?.kind, TokenKind::RParen) {
            loop {
                params.push(self.parse_fn_parameter()?);
                let attach =
                    |this: &mut Self, params: &mut Vec<Parameter>| -> Result<(), ParseError> {
                        if let Some(c) = this.take_same_line_comment()?
                            && let Some(last) = params.last_mut()
                        {
                            last.trailing_comment = Some(c);
                        }
                        Ok(())
                    };
                match self.peek()?.kind {
                    TokenKind::Comma => {
                        self.bump()?;
                        attach(self, &mut params)?;
                        if matches!(self.peek()?.kind, TokenKind::RParen) {
                            break;
                        }
                    }
                    TokenKind::RParen => {
                        attach(self, &mut params)?;
                        break;
                    }
                    _ => {
                        let p = self.peek()?;
                        let span = p.span;
                        let kind = describe(&p.kind);
                        return Err(self.err(
                            format!("expected ',' or ')' in parameter list, found {kind}"),
                            span,
                            "expected ',' or ')'",
                        ));
                    }
                }
            }
        }
        let rparen = self.expect(TokenKind::RParen, "expected ')' to close parameter list")?;
        trailing_trivia.extend(rparen.leading_trivia.iter().cloned());
        self.expect(TokenKind::Arrow, "expected '->' before return type")?;
        let (return_ty, return_ty_span) = self.parse_type_ref()?;
        let (body, body_span) = if matches!(self.peek()?.kind, TokenKind::LBrace) {
            self.parse_block_expr()?
        } else {
            self.parse_expr()?
        };
        let span = Span::new(start, body_span.end);
        Ok((
            Expr::Function(FunctionLit {
                params,
                return_ty,
                return_ty_span,
                body: std::sync::Arc::new(body),
                span,
                trailing_trivia,
            }),
            span,
        ))
    }

    pub(super) fn parse_function_type(&mut self) -> Result<(TypeRef, Span), ParseError> {
        let fn_tok = self.bump()?; // 'fn'
        let start = fn_tok.span.start;
        self.expect(TokenKind::LParen, "expected '(' in fn type")?;
        let params = self.parse_comma_separated(
            |k| matches!(k, TokenKind::RParen),
            "')'",
            "fn type",
            |p| p.parse_type_ref().map(|(t, _)| t),
        )?;
        self.expect(
            TokenKind::RParen,
            "expected ')' to close fn type parameters",
        )?;
        self.expect(TokenKind::Arrow, "expected '->' in fn type")?;
        let (return_ty, ret_span) = self.parse_type_ref()?;
        let span = Span::new(start, ret_span.end);
        Ok((
            TypeRef::Function {
                params,
                return_ty: Box::new(return_ty),
            },
            span,
        ))
    }
}

/// Walk an `Identifier` / `Member` chain and produce the dotted path
/// it names. Returns `None` if the expression is not a pure path (e.g.
/// it contains a call or arithmetic).
fn flatten_path_expr(expr: &Expr) -> Option<Vec<String>> {
    fn walk(e: &Expr, out: &mut Vec<String>) -> bool {
        match e {
            Expr::Identifier(s, _) => {
                out.push(s.clone());
                true
            }
            Expr::Member { recv, name, .. } => {
                if !walk(recv, out) {
                    return false;
                }
                out.push(name.clone());
                true
            }
            _ => false,
        }
    }
    let mut out = Vec::new();
    if walk(expr, &mut out) {
        Some(out)
    } else {
        None
    }
}

fn number_to_expr(n: NumberLit) -> Expr {
    match n {
        NumberLit::I8(v) => Expr::I8(v),
        NumberLit::I16(v) => Expr::I16(v),
        NumberLit::I32(v) => Expr::I32(v),
        NumberLit::I64(v) => Expr::I64(v),
        NumberLit::I128(v) => Expr::I128(v),
        NumberLit::Isize(v) => Expr::Isize(v),
        NumberLit::U8(v) => Expr::U8(v),
        NumberLit::U16(v) => Expr::U16(v),
        NumberLit::U32(v) => Expr::U32(v),
        NumberLit::U64(v) => Expr::U64(v),
        NumberLit::U128(v) => Expr::U128(v),
        NumberLit::Usize(v) => Expr::Usize(v),
        NumberLit::F32(v) => Expr::F32(v),
        NumberLit::F64(v) => Expr::F64(v),
    }
}

/// Token → binary-operator mapping for the Pratt parser. Returns
/// `(left_bp, right_bp, op)` for binary operators; `None` for anything
/// that doesn't bind as an infix operator. Binding powers come from
/// [`BinOp::binding_power`], the shared parser/formatter table.
fn bin_op_info(k: &TokenKind) -> Option<(u8, u8, BinOp)> {
    let op = match k {
        TokenKind::QuestionQuestion => BinOp::Coalesce,
        TokenKind::PipePipe => BinOp::Or,
        TokenKind::AmpAmp => BinOp::And,
        TokenKind::EqEq => BinOp::Eq,
        TokenKind::BangEq => BinOp::Ne,
        TokenKind::Lt => BinOp::Lt,
        TokenKind::LtEq => BinOp::Le,
        TokenKind::Gt => BinOp::Gt,
        TokenKind::GtEq => BinOp::Ge,
        TokenKind::Plus => BinOp::Add,
        TokenKind::Dash => BinOp::Sub,
        TokenKind::Star => BinOp::Mul,
        TokenKind::Slash => BinOp::Div,
        TokenKind::Percent => BinOp::Mod,
        _ => return None,
    };
    let (lbp, rbp) = op.binding_power();
    Some((lbp, rbp, op))
}
