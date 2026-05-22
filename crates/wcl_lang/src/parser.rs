use miette::{NamedSource, SourceSpan};

use crate::ast::{
    BinOp, Block, Decorator, Expr, Field, FunctionLit, ImportDecl, Item, LetBinding, MatchArm,
    NamedArg, NamespaceDecl, Parameter, Pattern, Source, Span, SymbolEntry, SymbolSetDecl,
    TypeDecl, TypeField, UnaryOp, UnionDecl, UnionVariant, UseDecl, UseForm, UseItem, VariantArgs,
    VariantBody, VariantPatArgs,
};
use crate::error::ParseError;
use crate::lexer::{LexError, Lexer, NumberLit, StringLit, Token, TokenKind};
use crate::symbols::{DuplicateSymbol, SymbolIndex, SymbolKind, SymbolPath, SymbolRecord};
use crate::value::{BuiltinType, TensorDim, TypeRef};

pub struct Parser<'a> {
    src: &'a str,
    file: String,
    lexer: Lexer<'a>,
    peeked: Option<Token>,
    peeked2: Option<Token>,
    file_ns: Vec<String>,
    index: SymbolIndex,
}

impl<'a> Parser<'a> {
    pub fn new(src: &'a str, file: impl Into<String>) -> Self {
        Self {
            src,
            file: file.into(),
            lexer: Lexer::new(src),
            peeked: None,
            peeked2: None,
            file_ns: Vec::new(),
            index: SymbolIndex::default(),
        }
    }

    pub fn parse_source(&mut self) -> Result<(Source, SymbolIndex), ParseError> {
        let mut items = Vec::new();
        while !matches!(self.peek()?.kind, TokenKind::Eof) {
            let item_idx = items.len();
            let item = self.parse_item()?;
            self.register_item(&item, item_idx)?;
            items.push(item);
        }
        Ok((Source { items }, std::mem::take(&mut self.index)))
    }

    /// Register a freshly-parsed top-level item (and its immediate
    /// members) with the symbol index. Function-internal names and
    /// items nested inside `Block`s are not indexed yet.
    fn register_item(&mut self, item: &Item, item_index: usize) -> Result<(), ParseError> {
        match item {
            Item::NamespaceDecl(n) => {
                self.file_ns = n.path.clone();
            }
            Item::UseDecl(_) => {}
            Item::Import(_) => {}
            Item::Table(_) => {}
            Item::TypeDecl(t) => {
                let members = t.fields.iter().map(|f| (f.name.as_str(), f.span));
                self.register_decl_with_members(
                    item_index,
                    &t.name,
                    t.span,
                    SymbolKind::TypeDecl,
                    "type",
                    "field",
                    members,
                    |parent_fqn| SymbolKind::TypeField { parent_fqn },
                )?;
            }
            Item::InterfaceDecl(i) => {
                let members = i.fields.iter().map(|f| (f.name.as_str(), f.span));
                self.register_decl_with_members(
                    item_index,
                    &i.name,
                    i.span,
                    SymbolKind::InterfaceDecl,
                    "interface",
                    "field",
                    members,
                    |parent_fqn| SymbolKind::InterfaceField { parent_fqn },
                )?;
            }
            Item::UnionDecl(u) => {
                let members = u.variants.iter().map(|v| (v.name.as_str(), v.span));
                self.register_decl_with_members(
                    item_index,
                    &u.name,
                    u.span,
                    SymbolKind::UnionDecl,
                    "union",
                    "variant",
                    members,
                    |parent_fqn| SymbolKind::UnionVariant { parent_fqn },
                )?;
            }
            Item::SymbolSetDecl(s) => {
                let members = s.symbols.iter().map(|sy| (sy.name.as_str(), sy.span));
                self.register_decl_with_members(
                    item_index,
                    &s.name,
                    s.span,
                    SymbolKind::SymbolSetDecl,
                    "symbol_set",
                    "symbol",
                    members,
                    |parent_fqn| SymbolKind::SymbolEntry { parent_fqn },
                )?;
            }
            Item::Field(f) => {
                let fqn = self.join_fqn(std::slice::from_ref(&f.name));
                self.try_insert(SymbolRecord {
                    fqn,
                    kind: SymbolKind::Field,
                    span: f.span,
                    path: SymbolPath {
                        item_index,
                        member_index: None,
                    },
                })?;
            }
            Item::Block(b) => {
                self.index.push_block(
                    b.kind.clone(),
                    SymbolPath {
                        item_index,
                        member_index: None,
                    },
                );
            }
        }
        Ok(())
    }

    /// Register a top-level declaration plus its member list (fields,
    /// variants, or symbol entries). Used by `TypeDecl`, `InterfaceDecl`,
    /// `UnionDecl`, and `SymbolSetDecl`.
    ///
    /// `container_label` is the source-level keyword used in duplicate-member
    /// error messages (`"type"`, `"interface"`, etc.); `member_label` names
    /// what the member is (`"field"`, `"variant"`, `"symbol"`).
    /// `make_member_kind` builds the `SymbolKind` for each member from the
    /// parent FQN.
    #[allow(clippy::too_many_arguments)]
    fn register_decl_with_members<'b, I, K>(
        &mut self,
        item_index: usize,
        name: &[String],
        span: Span,
        parent_kind: SymbolKind,
        container_label: &str,
        member_label: &str,
        members: I,
        make_member_kind: K,
    ) -> Result<(), ParseError>
    where
        I: IntoIterator<Item = (&'b str, Span)>,
        K: Fn(String) -> SymbolKind,
    {
        let parent_fqn = self.join_fqn(name);
        self.try_insert(SymbolRecord {
            fqn: parent_fqn.clone(),
            kind: parent_kind,
            span,
            path: SymbolPath {
                item_index,
                member_index: None,
            },
        })?;
        for (mi, (member_name, member_span)) in members.into_iter().enumerate() {
            let fqn = format!("{parent_fqn}.{member_name}");
            self.try_insert_with_msg(
                SymbolRecord {
                    fqn,
                    kind: make_member_kind(parent_fqn.clone()),
                    span: member_span,
                    path: SymbolPath {
                        item_index,
                        member_index: Some(mi),
                    },
                },
                format!(
                    "duplicate {member_label} '{member_name}' in {container_label} '{}'",
                    name.join(".")
                ),
            )?;
        }
        Ok(())
    }

    fn try_insert(&mut self, rec: SymbolRecord) -> Result<(), ParseError> {
        let msg = format!("duplicate declaration '{}'", rec.fqn);
        self.try_insert_with_msg(rec, msg)
    }

    fn try_insert_with_msg(&mut self, rec: SymbolRecord, msg: String) -> Result<(), ParseError> {
        match self.index.insert(rec) {
            Ok(()) => Ok(()),
            Err(DuplicateSymbol { second_span, .. }) => {
                Err(self.err(msg, second_span, "duplicate declaration"))
            }
        }
    }

    fn join_fqn(&self, name: &[String]) -> String {
        if self.file_ns.is_empty() {
            name.join(".")
        } else {
            let mut parts = self.file_ns.clone();
            parts.extend(name.iter().cloned());
            parts.join(".")
        }
    }

    fn parse_item(&mut self) -> Result<Item, ParseError> {
        // Collect any leading @decorators first.
        let decorators = self.parse_decorators()?;

        // Two-token lookahead for `type IDENT`, `union IDENT`,
        // `namespace IDENT`, `use IDENT`, `symbol_set IDENT`.
        let first_ident = match &self.peek()?.kind {
            TokenKind::Ident(s) => Some(s.clone()),
            _ => None,
        };
        if let Some(first) = first_ident.as_deref()
            && first == "import"
            && matches!(self.peek2()?.kind, TokenKind::Str(_))
        {
            if !decorators.is_empty() {
                let span = decorators[0].span;
                return Err(self.err(
                    "decorators are not allowed on import statements",
                    span,
                    "remove decorator",
                ));
            }
            return self.parse_import_decl();
        }
        // Table-header form: `IDENT : | ... |` opens a table bound to
        // the parent's named field.
        if first_ident.is_some() && matches!(self.peek2()?.kind, TokenKind::Colon) {
            if !decorators.is_empty() {
                let span = decorators[0].span;
                return Err(self.err(
                    "decorators are not allowed on table headers",
                    span,
                    "remove decorator",
                ));
            }
            return self.parse_table_item();
        }
        if let Some(first) = first_ident
            && matches!(self.peek2()?.kind, TokenKind::Ident(_))
        {
            match first.as_str() {
                "type" => return self.parse_type_decl(decorators),
                "interface" => return self.parse_interface_decl(decorators),
                "union" => return self.parse_union_decl(decorators),
                "namespace" | "use" => {
                    if !decorators.is_empty() {
                        let span = decorators[0].span;
                        return Err(self.err(
                            "decorators are not allowed on namespace/use declarations",
                            span,
                            "remove decorator",
                        ));
                    }
                    return if first == "namespace" {
                        self.parse_namespace_decl()
                    } else {
                        self.parse_use_decl()
                    };
                }
                "symbol_set" => return self.parse_symbol_set_decl(decorators),
                _ => {}
            }
        }

        let tok = self.bump()?;
        let span_start = tok.span.start;
        let name = match tok.kind {
            TokenKind::Ident(n) => n,
            other => {
                return Err(self.err(
                    format!("expected identifier, found {}", describe(&other)),
                    tok.span,
                    "expected identifier",
                ));
            }
        };
        let next = self.peek()?;
        match &next.kind {
            TokenKind::Eq => self.parse_field(name, span_start, decorators),
            TokenKind::Str(_)
            | TokenKind::LBrace
            | TokenKind::Ident(_)
            | TokenKind::Number(_)
            | TokenKind::Bool(_)
            | TokenKind::Symbol(_)
            | TokenKind::None => self.parse_block(name, span_start, decorators),
            other => {
                let msg = format!(
                    "expected '=', label, or '{{' after identifier '{}', found {}",
                    name,
                    describe(other)
                );
                let span = next.span;
                Err(self.err(msg, span, "unexpected token"))
            }
        }
    }

    fn parse_decorators(&mut self) -> Result<Vec<Decorator>, ParseError> {
        let mut decorators = Vec::new();
        while matches!(self.peek()?.kind, TokenKind::At) {
            decorators.push(self.parse_decorator()?);
        }
        Ok(decorators)
    }

    fn parse_decorator(&mut self) -> Result<Decorator, ParseError> {
        let at = self.bump()?; // '@'
        let start = at.span.start;
        let (name, name_span) = self.parse_path()?;
        let mut positional = Vec::new();
        let mut named = Vec::new();
        let mut end = name_span.end;
        if matches!(self.peek()?.kind, TokenKind::LParen) {
            self.bump()?; // '('
            if !matches!(self.peek()?.kind, TokenKind::RParen) {
                let mut saw_named = false;
                loop {
                    let is_named = matches!(self.peek()?.kind, TokenKind::Ident(_))
                        && matches!(self.peek2()?.kind, TokenKind::Eq);
                    if is_named {
                        saw_named = true;
                        let name_tok = self.bump()?;
                        let arg_start = name_tok.span.start;
                        let TokenKind::Ident(arg_name) = name_tok.kind else {
                            unreachable!()
                        };
                        self.bump()?; // '='
                        let (value, value_span) = self.parse_expr()?;
                        named.push(NamedArg {
                            name: arg_name,
                            value,
                            span: Span::new(arg_start, value_span.end),
                        });
                    } else {
                        if saw_named {
                            let p = self.peek()?;
                            let span = p.span;
                            return Err(self.err(
                                "positional argument cannot follow named argument",
                                span,
                                "unexpected positional arg",
                            ));
                        }
                        let (value, _) = self.parse_expr()?;
                        positional.push(value);
                    }
                    match self.peek()?.kind {
                        TokenKind::Comma => {
                            self.bump()?;
                            if matches!(self.peek()?.kind, TokenKind::RParen) {
                                break;
                            }
                        }
                        TokenKind::RParen => break,
                        _ => {
                            let p = self.peek()?;
                            let span = p.span;
                            let kind = describe(&p.kind);
                            return Err(self.err(
                                format!("expected ',' or ')', found {kind}"),
                                span,
                                "expected ',' or ')'",
                            ));
                        }
                    }
                }
            }
            let rparen = self.expect(TokenKind::RParen, "expected ')'")?;
            end = rparen.span.end;
        }
        Ok(Decorator {
            name,
            positional,
            named,
            span: Span::new(start, end),
        })
    }

    /// Literal-only value parser. Used by contexts that intentionally accept
    /// only primary tokens (decorator arguments, block labels) — full
    /// expressions go through [`parse_expr`].
    fn parse_value_expr(&mut self) -> Result<(Expr, Span), ParseError> {
        let tok = self.bump()?;
        let span = tok.span;
        let expr = match tok.kind {
            TokenKind::Number(n) => number_to_expr(n),
            TokenKind::Str(s) => string_to_expr(s),
            TokenKind::Bool(b) => Expr::Bool(b),
            TokenKind::Ident(s) => Expr::Identifier(s),
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

    /// Pratt expression parser. Used in any context where a full expression
    /// is allowed (field RHS, function-literal bodies, `let` initialisers,
    /// parenthesised sub-expressions, call arguments).
    fn parse_expr(&mut self) -> Result<(Expr, Span), ParseError> {
        self.parse_expr_bp(0)
    }

    fn parse_expr_bp(&mut self, min_bp: u8) -> Result<(Expr, Span), ParseError> {
        let (mut lhs, mut span) = self.parse_prefix()?;
        loop {
            let kind = self.peek()?.kind.clone();
            // Postfix call: `expr(args)`.
            if matches!(kind, TokenKind::LParen) {
                const CALL_BP: u8 = 14;
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
                const MEMBER_BP: u8 = 15;
                if MEMBER_BP < min_bp {
                    break;
                }
                self.bump()?; // '.'
                let name_tok = self.bump()?;
                let name = match name_tok.kind {
                    TokenKind::Ident(s) => s,
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
                const VARIANT_BP: u8 = 15;
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
                let (operand, operand_span) = self.parse_expr_bp(13)?;
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
                let (operand, operand_span) = self.parse_expr_bp(13)?;
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
            TokenKind::LBrace => self.parse_block_expr(),
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
                TokenKind::RBrace => break,
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
        }
        let rbrace = self.expect(TokenKind::RBrace, "expected '}' to close match")?;
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
                span,
            },
            span,
        ))
    }

    fn parse_match_arm(&mut self) -> Result<MatchArm, ParseError> {
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
        })
    }

    fn parse_pattern(&mut self) -> Result<Pattern, ParseError> {
        let tok = self.peek()?;
        match &tok.kind {
            TokenKind::Ident(s) if s == "_" => {
                let t = self.bump()?;
                Ok(Pattern::Wildcard(t.span))
            }
            TokenKind::Ident(_) => {
                // Could be: Binding, At pattern, or Variant (path :: Variant).
                // Decide by peek2:
                //   - peek2 == `@` → At
                //   - peek2 == `::` → Variant (single-segment path)
                //   - peek2 == `.`  → multi-segment path; need to consume to find `::`
                //   - else → Binding
                let p2 = self.peek2()?.kind.clone();
                match p2 {
                    TokenKind::At => {
                        let t = self.bump()?;
                        let TokenKind::Ident(name) = t.kind else {
                            unreachable!()
                        };
                        self.bump()?; // '@'
                        let inner = self.parse_pattern()?;
                        let span = Span::new(t.span.start, pattern_span(&inner).end);
                        Ok(Pattern::At {
                            name,
                            inner: Box::new(inner),
                            span,
                        })
                    }
                    TokenKind::ColonColon | TokenKind::Dot => self.parse_variant_pattern(),
                    _ => {
                        let t = self.bump()?;
                        let TokenKind::Ident(name) = t.kind else {
                            unreachable!()
                        };
                        Ok(Pattern::Binding { name, span: t.span })
                    }
                }
            }
            TokenKind::Bool(_) => {
                let t = self.bump()?;
                let TokenKind::Bool(b) = t.kind else {
                    unreachable!()
                };
                Ok(Pattern::LiteralBool(b, t.span))
            }
            TokenKind::Number(_) => {
                let t = self.bump()?;
                let TokenKind::Number(n) = t.kind else {
                    unreachable!()
                };
                Ok(Pattern::LiteralNumber {
                    lit: n,
                    span: t.span,
                })
            }
            TokenKind::Str(_) => {
                let t = self.bump()?;
                let TokenKind::Str(s) = t.kind else {
                    unreachable!()
                };
                match s {
                    crate::lexer::StringLit::Utf8(text) => Ok(Pattern::LiteralUtf8(text, t.span)),
                    crate::lexer::StringLit::Ascii(text) => Ok(Pattern::LiteralAscii(text, t.span)),
                    other => Err(self.err(
                        format!(
                            "string patterns only support utf8/ascii for now, got {}",
                            match other {
                                crate::lexer::StringLit::Utf16(_) => "utf16",
                                crate::lexer::StringLit::Utf32(_) => "utf32",
                                _ => "string",
                            }
                        ),
                        t.span,
                        "unsupported string-pattern kind",
                    )),
                }
            }
            TokenKind::Symbol(_) => {
                let t = self.bump()?;
                let TokenKind::Symbol(s) = t.kind else {
                    unreachable!()
                };
                Ok(Pattern::LiteralSymbol(s, t.span))
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
        let mut end = v_tok.span.end;
        let args = match self.peek()?.kind {
            TokenKind::LParen => {
                self.bump()?;
                let inner = self.parse_pattern()?;
                let rp = self.expect(TokenKind::RParen, "expected ')' after variant pattern")?;
                end = rp.span.end;
                VariantPatArgs::Positional(Box::new(inner))
            }
            TokenKind::LBrace => {
                self.bump()?;
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
                    let name_tok = self.bump()?;
                    let TokenKind::Ident(fname) = name_tok.kind else {
                        return Err(self.err(
                            format!(
                                "expected field name in record pattern, found {}",
                                describe(&name_tok.kind)
                            ),
                            name_tok.span,
                            "expected field name",
                        ));
                    };
                    let inner = if matches!(self.peek()?.kind, TokenKind::Colon) {
                        self.bump()?;
                        self.parse_pattern()?
                    } else {
                        // `{ name }` shorthand → bind by the field's own name.
                        Pattern::Binding {
                            name: fname.clone(),
                            span: name_tok.span,
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
                end = rb.span.end;
                VariantPatArgs::Record { fields, rest }
            }
            _ => VariantPatArgs::Unit,
        };
        Ok(Pattern::Variant {
            type_path: path,
            variant: v_name,
            args,
            span: Span::new(path_span.start, end),
        })
    }

    fn parse_list_literal(&mut self) -> Result<(Expr, Span), ParseError> {
        let lb = self.bump()?; // '['
        let mut elements = Vec::new();
        if !matches!(self.peek()?.kind, TokenKind::RBracket) {
            loop {
                let (e, _) = self.parse_expr()?;
                elements.push(e);
                match self.peek()?.kind {
                    TokenKind::Comma => {
                        self.bump()?;
                        if matches!(self.peek()?.kind, TokenKind::RBracket) {
                            break;
                        }
                    }
                    TokenKind::RBracket => break,
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
        let span = Span::new(lb.span.start, rb.span.end);
        Ok((Expr::ListLit { elements, span }, span))
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

    fn parse_block_expr(&mut self) -> Result<(Expr, Span), ParseError> {
        let lbrace = self.bump()?; // '{'
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
        let (tail, _) = self.parse_expr()?;
        let rbrace = self.expect(TokenKind::RBrace, "expected '}' to close block")?;
        let span = Span::new(lbrace.span.start, rbrace.span.end);
        Ok((
            Expr::Block {
                lets,
                tail: Box::new(tail),
                span,
            },
            span,
        ))
    }

    fn parse_let_binding(&mut self) -> Result<LetBinding, ParseError> {
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
        let span = Span::new(let_tok.span.start, value_span.end);
        Ok(LetBinding { name, value, span })
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
                let mut fields: Vec<NamedArg> = Vec::new();
                while !matches!(self.peek()?.kind, TokenKind::RBrace) {
                    let name_tok = self.bump()?;
                    let TokenKind::Ident(fname) = name_tok.kind else {
                        return Err(self.err(
                            format!(
                                "expected field name in variant constructor, found {}",
                                describe(&name_tok.kind)
                            ),
                            name_tok.span,
                            "expected field name",
                        ));
                    };
                    self.expect(
                        TokenKind::Colon,
                        "expected ':' after field name in variant constructor",
                    )?;
                    let (value, value_span) = self.parse_expr()?;
                    fields.push(NamedArg {
                        name: fname,
                        value,
                        span: Span::new(name_tok.span.start, value_span.end),
                    });
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
                                format!(
                                    "expected ',' or '}}' in variant constructor, found {kind}"
                                ),
                                span,
                                "expected ',' or '}'",
                            ));
                        }
                    }
                }
                let rb = self.expect(
                    TokenKind::RBrace,
                    "expected '}' to close variant constructor",
                )?;
                Ok((VariantArgs::Record(fields), rb.span.end))
            }
            _ => Ok((VariantArgs::Unit, default_end)),
        }
    }

    fn parse_call_tail(
        &mut self,
        callee: Expr,
        callee_span: Span,
    ) -> Result<(Expr, Span), ParseError> {
        self.bump()?; // '('
        let mut args = Vec::new();
        if !matches!(self.peek()?.kind, TokenKind::RParen) {
            loop {
                let (arg, _) = self.parse_expr()?;
                args.push(arg);
                match self.peek()?.kind {
                    TokenKind::Comma => {
                        self.bump()?;
                        if matches!(self.peek()?.kind, TokenKind::RParen) {
                            break;
                        }
                    }
                    TokenKind::RParen => break,
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
        let span = Span::new(callee_span.start, rparen.span.end);
        Ok((
            Expr::Call {
                callee: Box::new(callee),
                args,
                span,
            },
            span,
        ))
    }

    fn parse_function_literal(&mut self) -> Result<(Expr, Span), ParseError> {
        let fn_tok = self.bump()?; // 'fn'
        self.expect(TokenKind::LParen, "expected '(' after 'fn'")?;
        let mut params: Vec<Parameter> = Vec::new();
        if !matches!(self.peek()?.kind, TokenKind::RParen) {
            loop {
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
                params.push(Parameter {
                    name: pname,
                    ty,
                    ty_span,
                    span: Span::new(param_start, ty_span.end),
                });
                match self.peek()?.kind {
                    TokenKind::Comma => {
                        self.bump()?;
                        if matches!(self.peek()?.kind, TokenKind::RParen) {
                            break;
                        }
                    }
                    TokenKind::RParen => break,
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
        self.expect(TokenKind::RParen, "expected ')' to close parameter list")?;
        self.expect(TokenKind::Arrow, "expected '->' before return type")?;
        let (return_ty, return_ty_span) = self.parse_type_ref()?;
        let (body, body_span) = if matches!(self.peek()?.kind, TokenKind::LBrace) {
            self.parse_block_expr()?
        } else {
            self.parse_expr()?
        };
        let span = Span::new(fn_tok.span.start, body_span.end);
        Ok((
            Expr::Function(FunctionLit {
                params,
                return_ty,
                return_ty_span,
                body: Box::new(body),
                span,
            }),
            span,
        ))
    }

    fn parse_function_type(&mut self) -> Result<(TypeRef, Span), ParseError> {
        let fn_tok = self.bump()?; // 'fn'
        let start = fn_tok.span.start;
        self.expect(TokenKind::LParen, "expected '(' in fn type")?;
        let mut params: Vec<TypeRef> = Vec::new();
        if !matches!(self.peek()?.kind, TokenKind::RParen) {
            loop {
                let (ty, _) = self.parse_type_ref()?;
                params.push(ty);
                match self.peek()?.kind {
                    TokenKind::Comma => {
                        self.bump()?;
                        if matches!(self.peek()?.kind, TokenKind::RParen) {
                            break;
                        }
                    }
                    TokenKind::RParen => break,
                    _ => {
                        let p = self.peek()?;
                        let span = p.span;
                        let kind = describe(&p.kind);
                        return Err(self.err(
                            format!("expected ',' or ')' in fn type, found {kind}"),
                            span,
                            "expected ',' or ')'",
                        ));
                    }
                }
            }
        }
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

    /// Greedy path parser: `IDENT (. IDENT)*`.
    ///
    /// Refuses to consume a `Dot` if the next token after it is not an
    /// identifier — that way `foo.bar.{...}` parses as path `[foo, bar]`
    /// with the `.{` left for the caller (`parse_use_decl`).
    fn parse_path(&mut self) -> Result<(Vec<String>, Span), ParseError> {
        let first = self.bump()?;
        let TokenKind::Ident(name) = first.kind else {
            let span = first.span;
            return Err(self.err(
                format!("expected identifier, found {}", describe(&first.kind)),
                span,
                "expected identifier",
            ));
        };
        let mut segments = vec![name];
        let start = first.span.start;
        let mut end = first.span.end;
        loop {
            if !matches!(self.peek()?.kind, TokenKind::Dot) {
                break;
            }
            // Look ahead one more — if the token after '.' isn't an ident,
            // leave the '.' for the caller.
            if !matches!(self.peek2()?.kind, TokenKind::Ident(_)) {
                break;
            }
            self.bump()?; // '.'
            let next = self.bump()?;
            let TokenKind::Ident(seg) = next.kind else {
                unreachable!("peek2 confirmed Ident");
            };
            end = next.span.end;
            segments.push(seg);
        }
        Ok((segments, Span::new(start, end)))
    }

    fn parse_namespace_decl(&mut self) -> Result<Item, ParseError> {
        let kw = self.bump()?; // 'namespace'
        let start = kw.span.start;
        let (path, path_span) = self.parse_path()?;
        Ok(Item::NamespaceDecl(NamespaceDecl {
            path,
            span: Span::new(start, path_span.end),
        }))
    }

    fn parse_table_item(&mut self) -> Result<Item, ParseError> {
        // Already peeked: IDENT followed by Colon. Consume both.
        let name_tok = self.bump()?;
        let start = name_tok.span.start;
        let field_name = match name_tok.kind {
            TokenKind::Ident(s) => s,
            _ => unreachable!("parse_table_item entered with non-Ident first token"),
        };
        self.expect(TokenKind::Colon, "expected ':' after table field name")?;

        let mut rows = Vec::new();
        let mut end = name_tok.span.end;
        while matches!(self.peek()?.kind, TokenKind::Pipe) {
            let row = self.parse_table_row()?;
            end = row.span.end;
            rows.push(row);
        }

        Ok(Item::Table(crate::ast::TableItem {
            field_name,
            rows,
            span: Span::new(start, end),
        }))
    }

    fn parse_table_row(&mut self) -> Result<crate::ast::Row, ParseError> {
        // Row grammar: `| (expr |)* (expr)?`
        //
        // The leading `|` is required. Each value is followed by a
        // `|` (which acts as either a separator or the trailing pipe;
        // we don't need to distinguish — the loop ends as soon as the
        // next token can't start an expression). Trailing pipe is
        // therefore effectively optional: after the last value, if no
        // `|` follows, the row ends without one.
        let lead = self.bump()?; // leading '|'
        let start = lead.span.start;
        let mut values = Vec::new();
        let mut end = lead.span.end;
        loop {
            if !is_expr_start(&self.peek()?.kind) {
                break;
            }
            let (v, v_span) = self.parse_expr()?;
            values.push(v);
            end = v_span.end;
            if matches!(self.peek()?.kind, TokenKind::Pipe) {
                let sep = self.bump()?;
                end = sep.span.end;
            } else {
                break;
            }
        }
        Ok(crate::ast::Row {
            values,
            span: Span::new(start, end),
        })
    }

    fn parse_import_decl(&mut self) -> Result<Item, ParseError> {
        let kw = self.bump()?; // 'import'
        let start = kw.span.start;
        let tok = self.bump()?;
        let path_span = tok.span;
        let path = match tok.kind {
            TokenKind::Str(StringLit::Utf8(s)) | TokenKind::Str(StringLit::Ascii(s)) => s,
            other => {
                return Err(self.err(
                    format!(
                        "expected string path after 'import', found {}",
                        describe(&other)
                    ),
                    path_span,
                    "expected string path",
                ));
            }
        };
        Ok(Item::Import(ImportDecl {
            path,
            path_span,
            span: Span::new(start, path_span.end),
        }))
    }

    fn parse_use_decl(&mut self) -> Result<Item, ParseError> {
        let kw = self.bump()?; // 'use'
        let start = kw.span.start;
        let (path, path_span) = self.parse_path()?;

        // Brace-list form: path '.' '{' use_item (',' use_item)* ','? '}'
        if matches!(self.peek()?.kind, TokenKind::Dot)
            && matches!(self.peek2()?.kind, TokenKind::LBrace)
        {
            self.bump()?; // '.'
            self.bump()?; // '{'
            let mut items = Vec::new();
            loop {
                if matches!(self.peek()?.kind, TokenKind::RBrace) {
                    break;
                }
                let item = self.parse_use_item()?;
                items.push(item);
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
                            format!("expected ',' or '}}', found {kind}"),
                            span,
                            "expected ',' or '}'",
                        ));
                    }
                }
            }
            let rbrace = self.bump()?;
            return Ok(Item::UseDecl(UseDecl {
                path,
                form: UseForm::List(items),
                span: Span::new(start, rbrace.span.end),
            }));
        }

        // Bare form: optional 'as' IDENT
        let mut end = path_span.end;
        let alias = if let TokenKind::Ident(s) = &self.peek()?.kind
            && s == "as"
        {
            self.bump()?; // 'as'
            let alias_tok = self.bump()?;
            let TokenKind::Ident(alias_name) = alias_tok.kind else {
                let span = alias_tok.span;
                return Err(self.err(
                    format!(
                        "expected alias identifier after 'as', found {}",
                        describe(&alias_tok.kind)
                    ),
                    span,
                    "expected identifier",
                ));
            };
            end = alias_tok.span.end;
            Some(alias_name)
        } else {
            None
        };
        Ok(Item::UseDecl(UseDecl {
            path,
            form: UseForm::Bare(alias),
            span: Span::new(start, end),
        }))
    }

    fn parse_use_item(&mut self) -> Result<UseItem, ParseError> {
        let name_tok = self.bump()?;
        let item_start = name_tok.span.start;
        let TokenKind::Ident(name) = name_tok.kind else {
            let span = name_tok.span;
            return Err(self.err(
                format!(
                    "expected item name in use list, found {}",
                    describe(&name_tok.kind)
                ),
                span,
                "expected identifier",
            ));
        };
        let mut end = name_tok.span.end;
        let alias = if let TokenKind::Ident(s) = &self.peek()?.kind
            && s == "as"
        {
            self.bump()?;
            let alias_tok = self.bump()?;
            let TokenKind::Ident(alias_name) = alias_tok.kind else {
                let span = alias_tok.span;
                return Err(self.err(
                    format!(
                        "expected alias identifier after 'as', found {}",
                        describe(&alias_tok.kind)
                    ),
                    span,
                    "expected identifier",
                ));
            };
            end = alias_tok.span.end;
            Some(alias_name)
        } else {
            None
        };
        Ok(UseItem {
            name,
            alias,
            span: Span::new(item_start, end),
        })
    }

    fn parse_type_decl(&mut self, decorators: Vec<Decorator>) -> Result<Item, ParseError> {
        let type_kw = self.bump()?; // 'type'
        let start = type_kw.span.start;
        let (name, _name_span) = self.parse_path()?;
        let extends = self.parse_extends_clause()?;
        let lbrace = self.bump()?;
        if !matches!(lbrace.kind, TokenKind::LBrace) {
            return Err(self.err(
                format!(
                    "expected '{{' after type name '{}', found {}",
                    name.join("."),
                    describe(&lbrace.kind)
                ),
                lbrace.span,
                "expected '{'",
            ));
        }
        let mut fields = Vec::new();
        loop {
            let p = self.peek()?;
            match p.kind {
                TokenKind::RBrace => break,
                TokenKind::Eof => {
                    let span = p.span;
                    return Err(self.err(
                        "unexpected end of file inside type declaration",
                        span,
                        "expected '}'",
                    ));
                }
                _ => fields.push(self.parse_type_field()?),
            }
        }
        let rbrace = self.bump()?;
        Ok(Item::TypeDecl(TypeDecl {
            name,
            extends,
            fields,
            decorators,
            span: Span::new(start, rbrace.span.end),
        }))
    }

    fn parse_interface_decl(&mut self, decorators: Vec<Decorator>) -> Result<Item, ParseError> {
        let kw = self.bump()?; // 'interface'
        let start = kw.span.start;
        let (name, _name_span) = self.parse_path()?;
        let extends = self.parse_extends_clause()?;
        let lbrace = self.bump()?;
        if !matches!(lbrace.kind, TokenKind::LBrace) {
            return Err(self.err(
                format!(
                    "expected '{{' after interface name '{}', found {}",
                    name.join("."),
                    describe(&lbrace.kind)
                ),
                lbrace.span,
                "expected '{'",
            ));
        }
        let mut fields = Vec::new();
        loop {
            let p = self.peek()?;
            match p.kind {
                TokenKind::RBrace => break,
                TokenKind::Eof => {
                    let span = p.span;
                    return Err(self.err(
                        "unexpected end of file inside interface declaration",
                        span,
                        "expected '}'",
                    ));
                }
                _ => fields.push(self.parse_type_field()?),
            }
        }
        let rbrace = self.bump()?;
        Ok(Item::InterfaceDecl(crate::ast::InterfaceDecl {
            name,
            extends,
            fields,
            decorators,
            span: Span::new(start, rbrace.span.end),
        }))
    }

    /// Optional `extends Path (, Path)*` clause used by `type` and
    /// `interface` declarations. Returns an empty vec when absent.
    /// Trailing commas and empty lists after the keyword are
    /// errors.
    fn parse_extends_clause(&mut self) -> Result<Vec<Vec<String>>, ParseError> {
        let is_extends = matches!(&self.peek()?.kind, TokenKind::Ident(s) if s == "extends");
        if !is_extends {
            return Ok(Vec::new());
        }
        let kw = self.bump()?; // 'extends'
        let mut parents = Vec::new();
        loop {
            // Disallow trailing comma / empty list: at least one path required.
            let needs_ident_error = !matches!(&self.peek()?.kind, TokenKind::Ident(_));
            if needs_ident_error {
                let tok = self.peek()?.clone();
                let kind = describe(&tok.kind);
                return Err(self.err(
                    format!("expected parent type or interface name after 'extends', found {kind}"),
                    tok.span,
                    "expected identifier",
                ));
            }
            let (path, _) = self.parse_path()?;
            parents.push(path);
            match self.peek()?.kind {
                TokenKind::Comma => {
                    self.bump()?;
                    continue;
                }
                _ => break,
            }
        }
        if parents.is_empty() {
            // Unreachable given the loop above always pushes once,
            // but kept for clarity.
            return Err(self.err(
                "'extends' must be followed by at least one parent name",
                kw.span,
                "empty extends clause",
            ));
        }
        Ok(parents)
    }

    fn parse_type_field(&mut self) -> Result<TypeField, ParseError> {
        let decorators = self.parse_decorators()?;
        let name_tok = self.bump()?;
        let field_start = name_tok.span.start;
        let TokenKind::Ident(field_name) = name_tok.kind else {
            return Err(self.err(
                format!("expected field name, found {}", describe(&name_tok.kind)),
                name_tok.span,
                "expected identifier",
            ));
        };
        let colon = self.bump()?;
        if !matches!(colon.kind, TokenKind::Colon) {
            return Err(self.err(
                format!(
                    "expected ':' after field name '{field_name}', found {}",
                    describe(&colon.kind)
                ),
                colon.span,
                "expected ':'",
            ));
        }
        let (ty, ty_span) = self.parse_type_ref()?;
        let mut optional = false;
        let mut end = ty_span.end;
        if matches!(self.peek()?.kind, TokenKind::Question) {
            let q = self.bump()?;
            optional = true;
            end = q.span.end;
        }
        Ok(TypeField {
            name: field_name,
            ty,
            ty_span,
            optional,
            decorators,
            span: Span::new(field_start, end),
        })
    }

    fn parse_union_decl(&mut self, decorators: Vec<Decorator>) -> Result<Item, ParseError> {
        let kw = self.bump()?; // 'union'
        let start = kw.span.start;
        let (name, _name_span) = self.parse_path()?;
        let extends = self.parse_extends_clause()?;
        let lbrace = self.bump()?;
        if !matches!(lbrace.kind, TokenKind::LBrace) {
            return Err(self.err(
                format!(
                    "expected '{{' after union name '{}', found {}",
                    name.join("."),
                    describe(&lbrace.kind)
                ),
                lbrace.span,
                "expected '{'",
            ));
        }
        let mut variants = Vec::new();
        loop {
            let p = self.peek()?;
            match p.kind {
                TokenKind::RBrace => break,
                TokenKind::Eof => {
                    let span = p.span;
                    return Err(self.err(
                        "unexpected end of file inside union declaration",
                        span,
                        "expected '}'",
                    ));
                }
                _ => variants.push(self.parse_variant_decl()?),
            }
        }
        let rbrace = self.bump()?;
        Ok(Item::UnionDecl(UnionDecl {
            name,
            extends,
            variants,
            decorators,
            span: Span::new(start, rbrace.span.end),
        }))
    }

    fn parse_variant_decl(&mut self) -> Result<UnionVariant, ParseError> {
        let decorators = self.parse_decorators()?;
        let name_tok = self.bump()?;
        let variant_start = name_tok.span.start;
        let TokenKind::Ident(variant_name) = name_tok.kind else {
            return Err(self.err(
                format!("expected variant name, found {}", describe(&name_tok.kind)),
                name_tok.span,
                "expected identifier",
            ));
        };
        let (body, body_end) = self.parse_variant_body()?;
        Ok(UnionVariant {
            name: variant_name,
            body,
            decorators,
            span: Span::new(variant_start, body_end),
        })
    }

    fn parse_variant_body(&mut self) -> Result<(VariantBody, usize), ParseError> {
        let head = self.peek()?;
        match head.kind {
            TokenKind::LBrace => {
                self.bump()?;
                let mut fields = Vec::new();
                loop {
                    let p = self.peek()?;
                    match p.kind {
                        TokenKind::RBrace => break,
                        TokenKind::Eof => {
                            let span = p.span;
                            return Err(self.err(
                                "unexpected end of file inside variant body",
                                span,
                                "expected '}'",
                            ));
                        }
                        _ => fields.push(self.parse_type_field()?),
                    }
                }
                let rbrace = self.bump()?;
                Ok((VariantBody::Record(fields), rbrace.span.end))
            }
            TokenKind::None => {
                let tok = self.bump()?;
                Ok((VariantBody::Unit, tok.span.end))
            }
            TokenKind::Amp => {
                let amp = self.bump()?; // '&'
                let (iface, iface_span) = self.parse_path()?;
                if matches!(self.peek()?.kind, TokenKind::Question) {
                    let q = self.peek()?;
                    let span = q.span;
                    return Err(self.err(
                        "'?' is not allowed on a variant body",
                        span,
                        "remove '?'",
                    ));
                }
                let span = Span::new(amp.span.start, iface_span.end);
                Ok((
                    VariantBody::InterfaceRef {
                        iface,
                        iface_span: span,
                    },
                    iface_span.end,
                ))
            }
            TokenKind::Ident(_) => {
                let (ty, ty_span) = self.parse_type_ref()?;
                // No optional `?` is permitted on a variant body type ref.
                if matches!(self.peek()?.kind, TokenKind::Question) {
                    let q = self.peek()?;
                    let span = q.span;
                    return Err(self.err(
                        "'?' is not allowed on a variant body",
                        span,
                        "remove '?'",
                    ));
                }
                Ok((VariantBody::TypeRef { ty, ty_span }, ty_span.end))
            }
            _ => {
                let span = head.span;
                let kind = describe(&head.kind);
                Err(self.err(
                    format!("expected variant body ('{{ ... }}', a type, or 'None'), found {kind}"),
                    span,
                    "expected variant body",
                ))
            }
        }
    }

    fn parse_symbol_set_decl(&mut self, decorators: Vec<Decorator>) -> Result<Item, ParseError> {
        let kw = self.bump()?; // 'symbol_set'
        let start = kw.span.start;
        let (name, _) = self.parse_path()?;
        self.expect(TokenKind::LBrace, "expected '{' after symbol_set name")?;
        let mut symbols: Vec<SymbolEntry> = Vec::new();
        loop {
            // Each entry may have its own decorators.
            let entry_decorators = self.parse_decorators()?;
            let p = self.peek()?;
            match &p.kind {
                TokenKind::RBrace => {
                    if !entry_decorators.is_empty() {
                        let span = entry_decorators[0].span;
                        return Err(self.err(
                            "decorators must be followed by a symbol name",
                            span,
                            "dangling decorator",
                        ));
                    }
                    break;
                }
                TokenKind::Eof => {
                    let span = p.span;
                    return Err(self.err(
                        "unexpected end of file inside symbol_set declaration",
                        span,
                        "expected '}'",
                    ));
                }
                TokenKind::Ident(_) => {
                    let tok = self.bump()?;
                    if let TokenKind::Ident(entry_name) = tok.kind {
                        symbols.push(SymbolEntry {
                            name: entry_name,
                            decorators: entry_decorators,
                            span: tok.span,
                        });
                    }
                }
                other => {
                    let span = p.span;
                    let kind = describe(other);
                    return Err(self.err(
                        format!("expected symbol name or '}}', found {kind}"),
                        span,
                        "expected symbol name",
                    ));
                }
            }
        }
        let rbrace = self.bump()?;
        Ok(Item::SymbolSetDecl(SymbolSetDecl {
            name,
            symbols,
            decorators,
            span: Span::new(start, rbrace.span.end),
        }))
    }

    fn parse_type_ref(&mut self) -> Result<(TypeRef, Span), ParseError> {
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
            Ok((path_to_type_ref(&path), path_span))
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

    /// Parse a `keyword<body>` form: bump the keyword, expect `<`, run
    /// `parse_body`, expect `>`. Used by `list<T>` and `tensor<T, [...]>`.
    /// `keyword` is folded into the open/close error messages.
    fn parse_angle_bracketed<F, R>(
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
            TokenKind::Number(n) => match n {
                NumberLit::I8(v) if v >= 0 => Ok(TensorDim::Fixed(v as u64)),
                NumberLit::I16(v) if v >= 0 => Ok(TensorDim::Fixed(v as u64)),
                NumberLit::I32(v) if v >= 0 => Ok(TensorDim::Fixed(v as u64)),
                NumberLit::I64(v) if v >= 0 => Ok(TensorDim::Fixed(v as u64)),
                NumberLit::I128(v) if v >= 0 && v <= u64::MAX as i128 => {
                    Ok(TensorDim::Fixed(v as u64))
                }
                NumberLit::Isize(v) if v >= 0 => Ok(TensorDim::Fixed(v as u64)),
                NumberLit::U8(v) => Ok(TensorDim::Fixed(v as u64)),
                NumberLit::U16(v) => Ok(TensorDim::Fixed(v as u64)),
                NumberLit::U32(v) => Ok(TensorDim::Fixed(v as u64)),
                NumberLit::U64(v) => Ok(TensorDim::Fixed(v)),
                NumberLit::Usize(v) => Ok(TensorDim::Fixed(v as u64)),
                _ => Err(self.err(
                    "tensor dimensions must be non-negative integers or symbolic identifiers",
                    span,
                    "invalid dimension",
                )),
            },
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

    fn expect(&mut self, kind: TokenKind, msg: &str) -> Result<Token, ParseError> {
        let tok = self.bump()?;
        if std::mem::discriminant(&tok.kind) == std::mem::discriminant(&kind) {
            Ok(tok)
        } else {
            let span = tok.span;
            let found = describe(&tok.kind);
            Err(self.err(format!("{msg}, found {found}"), span, "unexpected token"))
        }
    }

    fn parse_field(
        &mut self,
        name: String,
        start: usize,
        decorators: Vec<Decorator>,
    ) -> Result<Item, ParseError> {
        self.bump()?; // consume '='
        let (expr, value_span) = self.parse_expr()?;
        let span = Span::new(start, value_span.end);
        Ok(Item::Field(Field {
            name,
            expr,
            decorators,
            span,
        }))
    }

    fn parse_block(
        &mut self,
        kind: String,
        start: usize,
        decorators: Vec<Decorator>,
    ) -> Result<Item, ParseError> {
        // Labels are value expressions in positional slots. Their types are
        // determined by the schema's `@inline(N)`-decorated fields.
        let mut labels: Vec<Expr> = Vec::new();
        loop {
            let p = self.peek()?;
            match &p.kind {
                TokenKind::LBrace => break,
                TokenKind::Str(StringLit::Utf8(_))
                | TokenKind::Str(StringLit::Ascii(_))
                | TokenKind::Str(StringLit::Utf16(_))
                | TokenKind::Str(StringLit::Utf32(_))
                | TokenKind::Number(_)
                | TokenKind::Bool(_)
                | TokenKind::Symbol(_)
                | TokenKind::Ident(_)
                | TokenKind::None => {
                    let (expr, _) = self.parse_value_expr()?;
                    labels.push(expr);
                }
                other => {
                    let msg = format!("expected label or '{{', found {}", describe(other));
                    let span = p.span;
                    return Err(self.err(msg, span, "expected label or '{'"));
                }
            }
        }
        self.bump()?; // consume '{'
        let mut items = Vec::new();
        loop {
            let p = self.peek()?;
            match p.kind {
                TokenKind::RBrace => break,
                TokenKind::Eof => {
                    let span = p.span;
                    return Err(self.err(
                        "unexpected end of file inside block",
                        span,
                        "expected '}'",
                    ));
                }
                _ => items.push(self.parse_item()?),
            }
        }
        let rbrace = self.bump()?;
        Ok(Item::Block(Block {
            kind,
            labels,
            items,
            decorators,
            span: Span::new(start, rbrace.span.end),
        }))
    }

    fn peek(&mut self) -> Result<&Token, ParseError> {
        if self.peeked.is_none() {
            self.peeked = Some(self.next_lex()?);
        }
        Ok(self.peeked.as_ref().expect("just set"))
    }

    fn peek2(&mut self) -> Result<&Token, ParseError> {
        self.peek()?;
        if self.peeked2.is_none() {
            self.peeked2 = Some(self.next_lex()?);
        }
        Ok(self.peeked2.as_ref().expect("just set"))
    }

    fn bump(&mut self) -> Result<Token, ParseError> {
        if let Some(t) = self.peeked.take() {
            self.peeked = self.peeked2.take();
            Ok(t)
        } else {
            self.next_lex()
        }
    }

    fn next_lex(&mut self) -> Result<Token, ParseError> {
        self.lexer.next_token().map_err(|e| self.lex_to_parse(e))
    }

    fn lex_to_parse(&self, e: LexError) -> ParseError {
        let label = e.message.clone();
        self.err(e.message, e.span, label)
    }

    fn err(&self, message: impl Into<String>, span: Span, label: impl Into<String>) -> ParseError {
        let len = span.len().max(1);
        ParseError::syntax(
            message.into(),
            NamedSource::new(self.file.clone(), self.src.to_string()),
            SourceSpan::new(span.start.into(), len),
            label.into(),
        )
    }
}

/// Heuristic for "could this token start a row-value expression?".
///
/// Notably **excludes** `Ident` and `LBrace`: bare identifiers in a
/// row position are ambiguous with the start of the next item
/// (`meta { ... }` or `port = ...`), and `{` is similarly ambiguous
/// with a block. Hosts that need a textual literal in a row should
/// quote it (`| "alice" |`) or use a symbol (`| :alice |`).
fn is_expr_start(t: &TokenKind) -> bool {
    matches!(
        t,
        TokenKind::Number(_)
            | TokenKind::Str(_)
            | TokenKind::Bool(_)
            | TokenKind::Symbol(_)
            | TokenKind::None
            | TokenKind::LParen
            | TokenKind::LBracket
            | TokenKind::Dash
            | TokenKind::Bang
    )
}

fn describe(t: &TokenKind) -> String {
    match t {
        TokenKind::Ident(s) => format!("identifier '{s}'"),
        TokenKind::Str(_) => "string".to_string(),
        TokenKind::Number(_) => "number".to_string(),
        TokenKind::Bool(_) => "boolean".to_string(),
        TokenKind::Symbol(_) => "symbol literal".to_string(),
        TokenKind::None => "'none'".to_string(),
        TokenKind::Eq => "'='".to_string(),
        TokenKind::EqEq => "'=='".to_string(),
        TokenKind::BangEq => "'!='".to_string(),
        TokenKind::Bang => "'!'".to_string(),
        TokenKind::Colon => "':'".to_string(),
        TokenKind::Question => "'?'".to_string(),
        TokenKind::Amp => "'&'".to_string(),
        TokenKind::AmpAmp => "'&&'".to_string(),
        TokenKind::Pipe => "'|'".to_string(),
        TokenKind::PipePipe => "'||'".to_string(),
        TokenKind::Dot => "'.'".to_string(),
        TokenKind::Comma => "','".to_string(),
        TokenKind::Semi => "';'".to_string(),
        TokenKind::Lt => "'<'".to_string(),
        TokenKind::LtEq => "'<='".to_string(),
        TokenKind::Gt => "'>'".to_string(),
        TokenKind::GtEq => "'>='".to_string(),
        TokenKind::LBracket => "'['".to_string(),
        TokenKind::RBracket => "']'".to_string(),
        TokenKind::At => "'@'".to_string(),
        TokenKind::LParen => "'('".to_string(),
        TokenKind::RParen => "')'".to_string(),
        TokenKind::LBrace => "'{'".to_string(),
        TokenKind::RBrace => "'}'".to_string(),
        TokenKind::Plus => "'+'".to_string(),
        TokenKind::Dash => "'-'".to_string(),
        TokenKind::Arrow => "'->'".to_string(),
        TokenKind::Star => "'*'".to_string(),
        TokenKind::Slash => "'/'".to_string(),
        TokenKind::Percent => "'%'".to_string(),
        TokenKind::If => "'if'".to_string(),
        TokenKind::Else => "'else'".to_string(),
        TokenKind::Match => "'match'".to_string(),
        TokenKind::FatArrow => "'=>'".to_string(),
        TokenKind::ColonColon => "'::'".to_string(),
        TokenKind::DotDot => "'..'".to_string(),
        TokenKind::Eof => "end of file".to_string(),
    }
}

/// Walk an `Identifier` / `Member` chain and produce the dotted path
/// it names. Returns `None` if the expression is not a pure path (e.g.
/// it contains a call or arithmetic).
fn flatten_path_expr(expr: &Expr) -> Option<Vec<String>> {
    fn walk(e: &Expr, out: &mut Vec<String>) -> bool {
        match e {
            Expr::Identifier(s) => {
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

fn pattern_span(p: &Pattern) -> Span {
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
fn collect_binding_names(p: &Pattern) -> std::collections::BTreeSet<String> {
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

fn path_to_type_ref(path: &[String]) -> TypeRef {
    if path.len() == 1
        && let Some(b) = BuiltinType::from_name(&path[0])
    {
        return TypeRef::Builtin(b);
    }
    TypeRef::Named(path.to_vec())
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

fn string_to_expr(s: StringLit) -> Expr {
    match s {
        StringLit::Utf8(s) => Expr::Utf8(s),
        StringLit::Ascii(s) => Expr::Ascii(s),
        StringLit::Utf16(v) => Expr::Utf16(v),
        StringLit::Utf32(v) => Expr::Utf32(v),
    }
}

/// Binding powers and operator mapping for the Pratt parser. Returns
/// `(left_bp, right_bp, op)` for binary operators; `None` for anything
/// that doesn't bind as an infix operator.
fn bin_op_info(k: &TokenKind) -> Option<(u8, u8, BinOp)> {
    Some(match k {
        TokenKind::PipePipe => (1, 2, BinOp::Or),
        TokenKind::AmpAmp => (3, 4, BinOp::And),
        TokenKind::EqEq => (5, 6, BinOp::Eq),
        TokenKind::BangEq => (5, 6, BinOp::Ne),
        TokenKind::Lt => (7, 8, BinOp::Lt),
        TokenKind::LtEq => (7, 8, BinOp::Le),
        TokenKind::Gt => (7, 8, BinOp::Gt),
        TokenKind::GtEq => (7, 8, BinOp::Ge),
        TokenKind::Plus => (9, 10, BinOp::Add),
        TokenKind::Dash => (9, 10, BinOp::Sub),
        TokenKind::Star => (11, 12, BinOp::Mul),
        TokenKind::Slash => (11, 12, BinOp::Div),
        TokenKind::Percent => (11, 12, BinOp::Mod),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Source {
        Parser::new(src, "test").parse_source().expect("parse ok").0
    }

    fn parse_with_index(src: &str) -> (Source, SymbolIndex) {
        Parser::new(src, "test").parse_source().expect("parse ok")
    }

    fn parse_err(src: &str) -> ParseError {
        Parser::new(src, "test")
            .parse_source()
            .expect_err("expected parse error")
    }

    fn field<'a>(items: &'a [Item], name: &str) -> &'a Field {
        items
            .iter()
            .find_map(|i| match i {
                Item::Field(f) if f.name == name => Some(f),
                _ => None,
            })
            .unwrap_or_else(|| panic!("no field '{name}'"))
    }

    fn blocks(items: &[Item]) -> Vec<&Block> {
        items
            .iter()
            .filter_map(|i| match i {
                Item::Block(b) => Some(b),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn parse_empty_document() {
        let s = parse("");
        assert!(s.items.is_empty());
    }

    #[test]
    fn parse_single_string_field() {
        let s = parse(r#"name = "alpha""#);
        assert_eq!(field(&s.items, "name").expr, Expr::Utf8("alpha".into()));
    }

    #[test]
    fn parse_mixed_scalar_fields() {
        let s = parse(
            r#"
            name = "alpha"
            count = 3
            ratio = 2.5
            enabled = true
            "#,
        );
        assert_eq!(field(&s.items, "name").expr, Expr::Utf8("alpha".into()));
        assert_eq!(field(&s.items, "count").expr, Expr::I64(3));
        assert_eq!(field(&s.items, "ratio").expr, Expr::F64(2.5));
        assert_eq!(field(&s.items, "enabled").expr, Expr::Bool(true));
    }

    #[test]
    fn parse_block_with_label() {
        let s = parse(
            r#"
            service "web" {
              port = 8080
              host = "0.0.0.0"
            }
            "#,
        );
        let blks = blocks(&s.items);
        let block = blks[0];
        assert_eq!(block.kind, "service");
        assert_eq!(block.labels, vec![Expr::Utf8("web".into())]);
        assert_eq!(field(&block.items, "port").expr, Expr::I64(8080));
        assert_eq!(
            field(&block.items, "host").expr,
            Expr::Utf8("0.0.0.0".into())
        );
    }

    #[test]
    fn parse_block_without_label() {
        let s = parse("metadata { region = \"us-east-1\" }");
        let block = blocks(&s.items)[0];
        assert_eq!(block.kind, "metadata");
        assert!(block.labels.is_empty());
    }

    #[test]
    fn parse_block_with_multiple_labels() {
        let s = parse(r#"resource "aws_s3_bucket" "logs" { acl = "private" }"#);
        let block = blocks(&s.items)[0];
        assert_eq!(block.kind, "resource");
        assert_eq!(
            block.labels,
            vec![
                Expr::Utf8("aws_s3_bucket".into()),
                Expr::Utf8("logs".into())
            ]
        );
    }

    #[test]
    fn parse_nested_blocks() {
        let s = parse(
            r#"
            service "web" {
              metadata {
                region = "us-east-1"
              }
            }
            "#,
        );
        let outer = blocks(&s.items)[0];
        let inner = blocks(&outer.items)[0];
        assert_eq!(inner.kind, "metadata");
        assert_eq!(
            field(&inner.items, "region").expr,
            Expr::Utf8("us-east-1".into())
        );
    }

    #[test]
    fn error_on_missing_value() {
        let err = parse_err("name =");
        match err {
            ParseError::Syntax(e) => assert!(e.message.contains("expected value")),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn error_on_unclosed_block() {
        let err = parse_err("service \"web\" { port = 1");
        match err {
            ParseError::Syntax(e) => assert!(e.message.contains("end of file")),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn error_when_first_token_is_not_ident() {
        let err = parse_err("= 1");
        match err {
            ParseError::Syntax(e) => assert!(e.message.contains("expected identifier")),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn spans_cover_full_field() {
        let src = r#"name = "alpha""#;
        let s = parse(src);
        let f = field(&s.items, "name");
        assert_eq!(&src[f.span.start..f.span.end], src);
    }

    fn type_decls(items: &[Item]) -> Vec<&TypeDecl> {
        items
            .iter()
            .filter_map(|i| match i {
                Item::TypeDecl(t) => Some(t),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn parse_simple_type_declaration() {
        let s = parse("type User { name: utf8 }");
        let t = type_decls(&s.items)[0];
        assert_eq!(t.name, vec!["User".to_string()]);
        assert_eq!(t.fields.len(), 1);
        assert_eq!(t.fields[0].name, "name");
        assert_eq!(t.fields[0].ty, TypeRef::Builtin(BuiltinType::Utf8));
        assert!(!t.fields[0].optional);
    }

    #[test]
    fn parse_type_with_optional_field() {
        let s = parse("type User { bio: utf8? age: u32? }");
        let t = type_decls(&s.items)[0];
        assert!(t.fields[0].optional);
        assert!(t.fields[1].optional);
        assert_eq!(t.fields[1].ty, TypeRef::Builtin(BuiltinType::U32));
    }

    #[test]
    fn parse_empty_type_body() {
        let s = parse("type Empty {}");
        let t = type_decls(&s.items)[0];
        assert_eq!(t.name, vec!["Empty".to_string()]);
        assert!(t.fields.is_empty());
    }

    #[test]
    fn parse_type_with_named_ref() {
        let s = parse("type Tree { parent: Tree? }");
        let t = type_decls(&s.items)[0];
        assert_eq!(t.fields[0].ty, TypeRef::Named(vec!["Tree".into()]));
        assert!(t.fields[0].optional);
    }

    #[test]
    fn parse_reference_type_to_named() {
        let s = parse("type Post { author: &User? }");
        let t = type_decls(&s.items)[0];
        assert_eq!(
            t.fields[0].ty,
            TypeRef::Reference(Box::new(TypeRef::Named(vec!["User".into()])))
        );
        assert!(t.fields[0].optional);
    }

    #[test]
    fn parse_reference_type_to_builtin() {
        let s = parse("type Score { value: &i32 }");
        let t = type_decls(&s.items)[0];
        assert_eq!(
            t.fields[0].ty,
            TypeRef::Reference(Box::new(TypeRef::Builtin(BuiltinType::I32)))
        );
        assert!(!t.fields[0].optional);
    }

    #[test]
    fn nested_reference_rejected() {
        let err = parse_err("type X { y: &&User }");
        match err {
            ParseError::Syntax(e) => {
                assert!(e.message.contains("expected type"), "{}", e.message)
            }
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn parse_bare_ident_as_reference_value() {
        let s = parse("owner = wil_taylor");
        assert_eq!(
            field(&s.items, "owner").expr,
            Expr::Identifier("wil_taylor".into())
        );
    }

    #[test]
    fn parse_none_as_value() {
        let s = parse("maybe = none");
        assert_eq!(field(&s.items, "maybe").expr, Expr::None);
    }

    #[test]
    fn contextual_keyword_field_named_type_still_works() {
        // `type` followed by `=` is just a field named "type".
        let s = parse("type = 1");
        assert_eq!(field(&s.items, "type").expr, Expr::I64(1));
    }

    #[test]
    fn contextual_keyword_block_with_kind_type_still_works() {
        let s = parse(r#"type "label" { x = 1 }"#);
        let block = blocks(&s.items)[0];
        assert_eq!(block.kind, "type");
        assert_eq!(block.labels, vec![Expr::Utf8("label".into())]);
    }

    #[test]
    fn type_decl_without_brace_errors() {
        let err = parse_err("type Foo = 1");
        match err {
            ParseError::Syntax(e) => assert!(e.message.contains("'{'"), "{}", e.message),
            _ => panic!("expected syntax error"),
        }
    }

    fn union_decls(items: &[Item]) -> Vec<&UnionDecl> {
        items
            .iter()
            .filter_map(|i| match i {
                Item::UnionDecl(u) => Some(u),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn parse_union_with_all_three_body_forms() {
        let s = parse(
            r#"
            type Point { x: f64 y: f64 }
            union Shape {
              Circle { center: Point radius: f64 }
              Polygon Point
              Empty none
            }
            "#,
        );
        let u = union_decls(&s.items)[0];
        assert_eq!(u.name, vec!["Shape".to_string()]);
        assert_eq!(u.variants.len(), 3);
        assert_eq!(u.variants[0].name, "Circle");
        assert!(matches!(u.variants[0].body, VariantBody::Record(_)));
        assert_eq!(u.variants[1].name, "Polygon");
        match &u.variants[1].body {
            VariantBody::TypeRef { ty, .. } => {
                assert_eq!(*ty, TypeRef::Named(vec!["Point".into()]))
            }
            _ => panic!("expected TypeRef body"),
        }
        assert_eq!(u.variants[2].name, "Empty");
        assert!(matches!(u.variants[2].body, VariantBody::Unit));
    }

    #[test]
    fn parse_empty_union() {
        let s = parse("union Nothing {}");
        let u = union_decls(&s.items)[0];
        assert_eq!(u.name, vec!["Nothing".to_string()]);
        assert!(u.variants.is_empty());
    }

    #[test]
    fn parse_reference_variant_body() {
        // `&Path` in variant body position now parses as InterfaceRef
        // — the variant payload is any value implementing the named
        // interface. Concrete type refs without `&` still parse as
        // TypeRef.
        let s = parse("type Item {} union Wrap { Boxed &Item }");
        let u = union_decls(&s.items)[0];
        match &u.variants[0].body {
            VariantBody::InterfaceRef { iface, .. } => {
                assert_eq!(*iface, vec!["Item".to_string()]);
            }
            other => panic!("expected InterfaceRef body, got {other:?}"),
        }
    }

    #[test]
    fn variant_body_question_mark_rejected() {
        let err = parse_err("type T {} union X { V T? }");
        match err {
            ParseError::Syntax(e) => assert!(
                e.message.contains("'?' is not allowed on a variant body"),
                "{}",
                e.message
            ),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn union_decl_without_brace_errors() {
        let err = parse_err("union Foo = 1");
        match err {
            ParseError::Syntax(e) => assert!(e.message.contains("'{'"), "{}", e.message),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn type_field_without_colon_errors() {
        let err = parse_err("type Foo { x utf8 }");
        match err {
            ParseError::Syntax(e) => assert!(e.message.contains("':'"), "{}", e.message),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn parse_list_type() {
        let s = parse("type Q { items: list<i32> }");
        let t = type_decls(&s.items)[0];
        assert_eq!(
            t.fields[0].ty,
            TypeRef::List(Box::new(TypeRef::Builtin(BuiltinType::I32)))
        );
    }

    #[test]
    fn parse_nested_list_type() {
        let s = parse("type Q { items: list<list<f32>> }");
        let t = type_decls(&s.items)[0];
        assert_eq!(
            t.fields[0].ty,
            TypeRef::List(Box::new(TypeRef::List(Box::new(TypeRef::Builtin(
                BuiltinType::F32
            )))))
        );
    }

    #[test]
    fn parse_list_of_reference() {
        let s = parse("type User {}\ntype Q { items: list<&User> }");
        let t = type_decls(&s.items)
            .into_iter()
            .find(|t| t.name == vec!["Q".to_string()])
            .unwrap();
        assert_eq!(
            t.fields[0].ty,
            TypeRef::List(Box::new(TypeRef::Reference(Box::new(TypeRef::Named(
                vec!["User".into()]
            )))))
        );
    }

    #[test]
    fn parse_optional_list() {
        let s = parse("type Q { items: list<i32>? }");
        let t = type_decls(&s.items)[0];
        assert!(t.fields[0].optional);
        assert_eq!(
            t.fields[0].ty,
            TypeRef::List(Box::new(TypeRef::Builtin(BuiltinType::I32)))
        );
    }

    #[test]
    fn parse_tensor_with_concrete_dims() {
        let s = parse("type Q { w: tensor<f32, [3, 4]> }");
        let t = type_decls(&s.items)[0];
        let TypeRef::Tensor { element, dims } = &t.fields[0].ty else {
            panic!("expected tensor");
        };
        assert_eq!(**element, TypeRef::Builtin(BuiltinType::F32));
        assert_eq!(dims, &vec![TensorDim::Fixed(3), TensorDim::Fixed(4)]);
    }

    #[test]
    fn parse_tensor_with_symbolic_dim() {
        let s = parse("type Q { w: tensor<f32, [N, 128]> }");
        let t = type_decls(&s.items)[0];
        let TypeRef::Tensor { dims, .. } = &t.fields[0].ty else {
            panic!("expected tensor");
        };
        assert_eq!(
            dims,
            &vec![TensorDim::Symbolic("N".into()), TensorDim::Fixed(128)]
        );
    }

    #[test]
    fn parse_tensor_single_dim() {
        let s = parse("type Q { w: tensor<u8, [256]> }");
        let t = type_decls(&s.items)[0];
        let TypeRef::Tensor { dims, .. } = &t.fields[0].ty else {
            panic!("expected tensor");
        };
        assert_eq!(dims, &vec![TensorDim::Fixed(256)]);
    }

    #[test]
    fn parse_tensor_trailing_comma_in_dims() {
        let s = parse("type Q { w: tensor<f32, [3, 4,]> }");
        let t = type_decls(&s.items)[0];
        let TypeRef::Tensor { dims, .. } = &t.fields[0].ty else {
            panic!("expected tensor");
        };
        assert_eq!(dims.len(), 2);
    }

    #[test]
    fn tensor_requires_at_least_one_dim() {
        let err = parse_err("type Q { w: tensor<f32, []> }");
        match err {
            ParseError::Syntax(e) => assert!(
                e.message.contains("at least one dimension"),
                "{}",
                e.message
            ),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn tensor_missing_close_gt_errors() {
        let err = parse_err("type Q { w: tensor<f32, [4] }");
        match err {
            ParseError::Syntax(e) => assert!(e.message.contains("'>'"), "{}", e.message),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn list_keyword_as_type_name_still_works() {
        // A user type named `list` is OK; field: list (no '<') resolves to it.
        let s = parse("type list {}\ntype Q { x: list }");
        let q = type_decls(&s.items)
            .into_iter()
            .find(|t| t.name == vec!["Q".to_string()])
            .unwrap();
        assert_eq!(q.fields[0].ty, TypeRef::Named(vec!["list".into()]));
    }

    #[test]
    fn parse_decorator_no_args() {
        let s = parse("@hidden\ntype X {}");
        let t = type_decls(&s.items)[0];
        assert_eq!(t.decorators.len(), 1);
        assert_eq!(t.decorators[0].name, vec!["hidden".to_string()]);
        assert!(t.decorators[0].positional.is_empty());
        assert!(t.decorators[0].named.is_empty());
    }

    #[test]
    fn parse_decorator_empty_parens() {
        let s = parse("@hidden()\ntype X {}");
        let t = type_decls(&s.items)[0];
        assert_eq!(t.decorators.len(), 1);
        assert!(t.decorators[0].positional.is_empty());
    }

    #[test]
    fn parse_decorator_positional_args() {
        let s = parse("@range(1, 10)\ntype X {}");
        let t = type_decls(&s.items)[0];
        assert_eq!(
            t.decorators[0].positional,
            vec![Expr::I64(1), Expr::I64(10)]
        );
    }

    #[test]
    fn parse_decorator_named_args() {
        let s = parse("@validate(min = 1, max = 10)\ntype X {}");
        let t = type_decls(&s.items)[0];
        let d = &t.decorators[0];
        assert!(d.positional.is_empty());
        assert_eq!(d.named.len(), 2);
        assert_eq!(d.named[0].name, "min");
        assert_eq!(d.named[0].value, Expr::I64(1));
        assert_eq!(d.named[1].name, "max");
        assert_eq!(d.named[1].value, Expr::I64(10));
    }

    #[test]
    fn parse_decorator_mixed_args() {
        let s = parse("@range(0, max = 100)\ntype X {}");
        let t = type_decls(&s.items)[0];
        let d = &t.decorators[0];
        assert_eq!(d.positional, vec![Expr::I64(0)]);
        assert_eq!(d.named.len(), 1);
        assert_eq!(d.named[0].name, "max");
    }

    #[test]
    fn parse_decorator_positional_after_named_errors() {
        let err = parse_err("@x(min = 1, 5)\ntype X {}");
        match err {
            ParseError::Syntax(e) => assert!(
                e.message
                    .contains("positional argument cannot follow named"),
                "{}",
                e.message
            ),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn parse_decorator_trailing_comma() {
        let s = parse("@x(1, 2,)\ntype X {}");
        let t = type_decls(&s.items)[0];
        assert_eq!(t.decorators[0].positional.len(), 2);
    }

    #[test]
    fn parse_dotted_decorator() {
        let s = parse("@ui.color(:red)\ntype X {}");
        let t = type_decls(&s.items)[0];
        assert_eq!(
            t.decorators[0].name,
            vec!["ui".to_string(), "color".to_string()]
        );
        assert_eq!(t.decorators[0].positional, vec![Expr::Symbol("red".into())]);
    }

    #[test]
    fn parse_decorator_on_type_field() {
        let s = parse("type X { @max(64) name: utf8 }");
        let t = type_decls(&s.items)[0];
        assert_eq!(t.fields[0].decorators.len(), 1);
        assert_eq!(t.fields[0].decorators[0].name, vec!["max".to_string()]);
    }

    #[test]
    fn parse_decorator_on_variant() {
        let s = parse("union U { @hidden Circle { radius: f64 } }");
        let u = union_decls(&s.items)[0];
        assert_eq!(u.variants[0].decorators.len(), 1);
        assert_eq!(u.variants[0].decorators[0].name, vec!["hidden".to_string()]);
    }

    #[test]
    fn parse_decorator_on_symbol_entry() {
        let s = parse("symbol_set C { @default red green }");
        let set = symbol_set_decls(&s.items)[0];
        assert_eq!(set.symbols[0].decorators.len(), 1);
        assert!(set.symbols[1].decorators.is_empty());
    }

    #[test]
    fn parse_decorator_on_top_level_field() {
        let s = parse("@logged\nport = 8080");
        let f = field(&s.items, "port");
        assert_eq!(f.decorators.len(), 1);
        assert_eq!(f.decorators[0].name, vec!["logged".to_string()]);
    }

    #[test]
    fn parse_decorator_on_block() {
        let s = parse(r#"@logged service "web" { port = 8080 }"#);
        let b = blocks(&s.items)[0];
        assert_eq!(b.decorators.len(), 1);
        assert_eq!(b.decorators[0].name, vec!["logged".to_string()]);
    }

    #[test]
    fn parse_multiple_stacked_decorators() {
        let s = parse("@a @b\ntype X {}");
        let t = type_decls(&s.items)[0];
        assert_eq!(t.decorators.len(), 2);
        assert_eq!(t.decorators[0].name, vec!["a".to_string()]);
        assert_eq!(t.decorators[1].name, vec!["b".to_string()]);
    }

    #[test]
    fn decorator_on_namespace_errors() {
        let err = parse_err("@x\nnamespace foo");
        match err {
            ParseError::Syntax(e) => assert!(
                e.message.contains("not allowed on namespace/use"),
                "{}",
                e.message
            ),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn decorator_on_use_errors() {
        let err = parse_err("type X {}\n@x use X");
        match err {
            ParseError::Syntax(e) => assert!(
                e.message.contains("not allowed on namespace/use"),
                "{}",
                e.message
            ),
            _ => panic!("expected syntax error"),
        }
    }

    fn symbol_set_decls(items: &[Item]) -> Vec<&SymbolSetDecl> {
        items
            .iter()
            .filter_map(|i| match i {
                Item::SymbolSetDecl(s) => Some(s),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn parse_symbol_set_decl() {
        let s = parse("symbol_set Color { red green blue }");
        let set = symbol_set_decls(&s.items)[0];
        assert_eq!(set.name, vec!["Color".to_string()]);
        assert_eq!(
            set.symbols
                .iter()
                .map(|e| e.name.clone())
                .collect::<Vec<_>>(),
            vec!["red".to_string(), "green".to_string(), "blue".to_string()]
        );
    }

    #[test]
    fn parse_empty_symbol_set() {
        let s = parse("symbol_set Empty {}");
        let set = symbol_set_decls(&s.items)[0];
        assert!(set.symbols.is_empty());
    }

    #[test]
    fn parse_dotted_symbol_set_name() {
        let s = parse("symbol_set foo.bar.X { a b }");
        let set = symbol_set_decls(&s.items)[0];
        assert_eq!(
            set.name,
            vec!["foo".to_string(), "bar".to_string(), "X".to_string()]
        );
        assert_eq!(set.symbols.len(), 2);
    }

    #[test]
    fn parse_symbol_value() {
        let s = parse("tag = :wide");
        assert_eq!(field(&s.items, "tag").expr, Expr::Symbol("wide".into()));
    }

    #[test]
    fn parse_symbol_typed_field() {
        let s = parse("type Q { tag: symbol }");
        let t = type_decls(&s.items)[0];
        assert_eq!(t.fields[0].ty, TypeRef::Builtin(BuiltinType::Symbol));
    }

    #[test]
    fn parse_named_symbol_set_field() {
        let s = parse("symbol_set C { x }\ntype Q { f: C }");
        let q = type_decls(&s.items)
            .into_iter()
            .find(|t| t.name == vec!["Q".to_string()])
            .unwrap();
        assert_eq!(q.fields[0].ty, TypeRef::Named(vec!["C".into()]));
    }

    #[test]
    fn symbol_set_requires_brace() {
        let err = parse_err("symbol_set Foo = 1");
        match err {
            ParseError::Syntax(e) => assert!(e.message.contains("'{'"), "{}", e.message),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn parse_namespace_declaration() {
        let s = parse("namespace foo.bar");
        match &s.items[0] {
            Item::NamespaceDecl(n) => {
                assert_eq!(n.path, vec!["foo".to_string(), "bar".to_string()])
            }
            _ => panic!("expected NamespaceDecl"),
        }
    }

    #[test]
    fn parse_use_bare_no_alias() {
        let s = parse("use foo.bar.Baz");
        match &s.items[0] {
            Item::UseDecl(u) => {
                assert_eq!(
                    u.path,
                    vec!["foo".to_string(), "bar".to_string(), "Baz".to_string()]
                );
                assert!(matches!(u.form, UseForm::Bare(None)));
            }
            _ => panic!("expected UseDecl"),
        }
    }

    #[test]
    fn parse_use_bare_with_alias() {
        let s = parse("use foo.bar.Baz as MyBaz");
        match &s.items[0] {
            Item::UseDecl(u) => match &u.form {
                UseForm::Bare(Some(a)) => assert_eq!(a, "MyBaz"),
                _ => panic!("expected Bare(Some)"),
            },
            _ => panic!("expected UseDecl"),
        }
    }

    #[test]
    fn parse_use_brace_list() {
        let s = parse("use foo.bar.{X, Y as Z}");
        match &s.items[0] {
            Item::UseDecl(u) => {
                assert_eq!(u.path, vec!["foo".to_string(), "bar".to_string()]);
                let UseForm::List(items) = &u.form else {
                    panic!("expected List");
                };
                assert_eq!(items.len(), 2);
                assert_eq!(items[0].name, "X");
                assert_eq!(items[0].alias, None);
                assert_eq!(items[1].name, "Y");
                assert_eq!(items[1].alias.as_deref(), Some("Z"));
            }
            _ => panic!("expected UseDecl"),
        }
    }

    #[test]
    fn parse_use_brace_trailing_comma() {
        let s = parse("use foo.bar.{X, Y,}");
        match &s.items[0] {
            Item::UseDecl(u) => match &u.form {
                UseForm::List(items) => assert_eq!(items.len(), 2),
                _ => panic!("expected List"),
            },
            _ => panic!("expected UseDecl"),
        }
    }

    #[test]
    fn parse_use_brace_empty_list() {
        let s = parse("use foo.bar.{}");
        match &s.items[0] {
            Item::UseDecl(u) => match &u.form {
                UseForm::List(items) => assert!(items.is_empty()),
                _ => panic!("expected List"),
            },
            _ => panic!("expected UseDecl"),
        }
    }

    #[test]
    fn parse_dotted_type_decl() {
        let s = parse("type a.b.X {}");
        match &s.items[0] {
            Item::TypeDecl(t) => assert_eq!(
                t.name,
                vec!["a".to_string(), "b".to_string(), "X".to_string()]
            ),
            _ => panic!("expected TypeDecl"),
        }
    }

    #[test]
    fn parse_dotted_type_ref() {
        let s = parse("type Q { f: a.b.X }");
        match &s.items[0] {
            Item::TypeDecl(t) => assert_eq!(
                t.fields[0].ty,
                TypeRef::Named(vec!["a".to_string(), "b".to_string(), "X".to_string()])
            ),
            _ => panic!("expected TypeDecl"),
        }
    }

    #[test]
    fn parse_dotted_reference_type() {
        let s = parse("type Q { f: &a.b.X? }");
        match &s.items[0] {
            Item::TypeDecl(t) => assert_eq!(
                t.fields[0].ty,
                TypeRef::Reference(Box::new(TypeRef::Named(vec![
                    "a".to_string(),
                    "b".to_string(),
                    "X".to_string()
                ])))
            ),
            _ => panic!("expected TypeDecl"),
        }
    }

    #[test]
    fn path_trailing_dot_errors() {
        let err = parse_err("namespace foo.");
        match err {
            ParseError::Syntax(e) => {
                assert!(e.message.contains("expected identifier"), "{}", e.message)
            }
            _ => panic!("expected syntax error"),
        }
    }

    // ─── Functions & expressions ────────────────────────────────────────

    #[test]
    fn parse_function_literal_bare_body() {
        let s = parse("double = fn(x: i32) -> i32 x * 2");
        let f = field(&s.items, "double");
        let Expr::Function(lit) = &f.expr else {
            panic!("expected function literal")
        };
        assert_eq!(lit.params.len(), 1);
        assert_eq!(lit.params[0].name, "x");
        assert_eq!(lit.params[0].ty, TypeRef::Builtin(BuiltinType::I32));
        assert_eq!(lit.return_ty, TypeRef::Builtin(BuiltinType::I32));
        let Expr::Binary { op, .. } = &*lit.body else {
            panic!("expected binary body")
        };
        assert_eq!(*op, BinOp::Mul);
    }

    #[test]
    fn parse_function_literal_block_body() {
        let s = parse("sum_squared = fn(x: i32, y: i32) -> i32 {\n  let s = x + y;\n  s * s\n}");
        let f = field(&s.items, "sum_squared");
        let Expr::Function(lit) = &f.expr else {
            panic!("expected function literal")
        };
        assert_eq!(lit.params.len(), 2);
        let Expr::Block { lets, tail, .. } = &*lit.body else {
            panic!("expected block body")
        };
        assert_eq!(lets.len(), 1);
        assert_eq!(lets[0].name, "s");
        let Expr::Binary { op, .. } = &**tail else {
            panic!("expected binary tail")
        };
        assert_eq!(*op, BinOp::Mul);
    }

    #[test]
    fn parse_function_with_no_params() {
        let s = parse("k = fn() -> i32 42");
        let f = field(&s.items, "k");
        let Expr::Function(lit) = &f.expr else {
            panic!("expected function literal")
        };
        assert!(lit.params.is_empty());
        assert!(matches!(&*lit.body, Expr::I64(42)));
    }

    #[test]
    fn parse_function_type_in_field() {
        let s = parse("type Handler { on_click: fn(i32) -> bool on_drag: fn(i32, i32) -> bool }");
        let t = type_decls(&s.items)[0];
        let TypeRef::Function { params, return_ty } = &t.fields[0].ty else {
            panic!("expected fn type")
        };
        assert_eq!(params.len(), 1);
        assert_eq!(params[0], TypeRef::Builtin(BuiltinType::I32));
        assert_eq!(**return_ty, TypeRef::Builtin(BuiltinType::Bool));
        let TypeRef::Function { params, .. } = &t.fields[1].ty else {
            panic!("expected fn type")
        };
        assert_eq!(params.len(), 2);
    }

    #[test]
    fn parse_function_type_zero_args() {
        let s = parse("type T { thunk: fn() -> i32 }");
        let t = type_decls(&s.items)[0];
        let TypeRef::Function { params, return_ty } = &t.fields[0].ty else {
            panic!("expected fn type")
        };
        assert!(params.is_empty());
        assert_eq!(**return_ty, TypeRef::Builtin(BuiltinType::I32));
    }

    #[test]
    fn parse_call_expression() {
        let s = parse("y = f(1, 2)");
        let f = field(&s.items, "y");
        let Expr::Call { callee, args, .. } = &f.expr else {
            panic!("expected call")
        };
        assert!(matches!(&**callee, Expr::Identifier(n) if n == "f"));
        assert_eq!(args.len(), 2);
    }

    #[test]
    fn parse_arithmetic_precedence() {
        // 1 + 2 * 3 should bind as 1 + (2 * 3).
        let s = parse("a = 1 + 2 * 3");
        let f = field(&s.items, "a");
        let Expr::Binary {
            op: BinOp::Add,
            lhs,
            rhs,
            ..
        } = &f.expr
        else {
            panic!("expected top-level Add")
        };
        assert!(matches!(&**lhs, Expr::I64(1)));
        let Expr::Binary { op: BinOp::Mul, .. } = &**rhs else {
            panic!("expected nested Mul on rhs")
        };
    }

    #[test]
    fn parse_comparison_and_logical() {
        // x > 100 && x < 1000 should bind as (x > 100) && (x < 1000).
        let s = parse("ok = x > 100 && x < 1000");
        let f = field(&s.items, "ok");
        let Expr::Binary {
            op: BinOp::And,
            lhs,
            rhs,
            ..
        } = &f.expr
        else {
            panic!("expected top-level And")
        };
        assert!(matches!(&**lhs, Expr::Binary { op: BinOp::Gt, .. }));
        assert!(matches!(&**rhs, Expr::Binary { op: BinOp::Lt, .. }));
    }

    #[test]
    fn parse_unary_neg_and_not() {
        let s = parse("a = -x\nb = !flag");
        let a = field(&s.items, "a");
        let Expr::Unary {
            op: UnaryOp::Neg, ..
        } = &a.expr
        else {
            panic!("expected unary neg")
        };
        let b = field(&s.items, "b");
        let Expr::Unary {
            op: UnaryOp::Not, ..
        } = &b.expr
        else {
            panic!("expected unary not")
        };
    }

    #[test]
    fn parse_paren_expression() {
        let s = parse("x = (1 + 2) * 3");
        let f = field(&s.items, "x");
        let Expr::Binary {
            op: BinOp::Mul,
            lhs,
            ..
        } = &f.expr
        else {
            panic!("expected top-level Mul")
        };
        assert!(matches!(&**lhs, Expr::Paren { .. }));
    }

    #[test]
    fn parse_function_literal_with_call_in_body() {
        let s = parse("apply = fn(n: i32) -> i32 add(n, 1)");
        let f = field(&s.items, "apply");
        let Expr::Function(lit) = &f.expr else {
            panic!("expected function")
        };
        assert!(matches!(&*lit.body, Expr::Call { .. }));
    }

    #[test]
    fn parse_function_returning_function() {
        let s = parse("k = fn(x: i32) -> fn(i32) -> i32 fn(y: i32) -> i32 x + y");
        let f = field(&s.items, "k");
        let Expr::Function(outer) = &f.expr else {
            panic!("expected outer fn")
        };
        let TypeRef::Function { .. } = &outer.return_ty else {
            panic!("outer return should be fn type")
        };
        assert!(matches!(&*outer.body, Expr::Function(_)));
    }

    #[test]
    fn parse_block_let_bindings_then_tail() {
        // Standalone block expression (no surrounding fn).
        let s = parse("x = { let a = 1; let b = 2; a + b }");
        let f = field(&s.items, "x");
        let Expr::Block { lets, tail, .. } = &f.expr else {
            panic!("expected block")
        };
        assert_eq!(lets.len(), 2);
        assert!(matches!(&**tail, Expr::Binary { op: BinOp::Add, .. }));
    }

    #[test]
    fn missing_arrow_errors() {
        let err = parse_err("f = fn(x: i32) i32 x");
        match err {
            ParseError::Syntax(e) => assert!(e.message.contains("'->'"), "{}", e.message),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn missing_return_type_errors() {
        // `fn(x: i32) ->` with nothing after the arrow should fail to parse
        // a type ref.
        let err = parse_err("f = fn(x: i32) -> ");
        match err {
            ParseError::Syntax(e) => assert!(
                e.message.contains("expected type") || e.message.contains("end of file"),
                "{}",
                e.message
            ),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn trailing_semi_on_block_tail_errors() {
        let err = parse_err("x = { let a = 1; a; }");
        match err {
            ParseError::Syntax(_) => {}
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn empty_block_expression_errors() {
        let err = parse_err("x = {}");
        match err {
            ParseError::Syntax(e) => {
                assert!(e.message.contains("final expression"), "{}", e.message)
            }
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn field_named_fn_still_parses_as_field() {
        let s = parse("fn = 1");
        assert_eq!(field(&s.items, "fn").expr, Expr::I64(1));
    }

    #[test]
    fn parse_empty_list_literal() {
        let s = parse("x = []");
        let f = field(&s.items, "x");
        let Expr::ListLit { elements, .. } = &f.expr else {
            panic!("expected list literal, got {:?}", f.expr)
        };
        assert!(elements.is_empty());
    }

    #[test]
    fn parse_list_literal_with_elements() {
        let s = parse("x = [1, 2, 3]");
        let f = field(&s.items, "x");
        let Expr::ListLit { elements, .. } = &f.expr else {
            panic!("expected list literal")
        };
        assert_eq!(elements.len(), 3);
        assert_eq!(elements[0], Expr::I64(1));
        assert_eq!(elements[2], Expr::I64(3));
    }

    #[test]
    fn parse_nested_list_literal() {
        let s = parse("x = [[1, 2], [3, 4]]");
        let f = field(&s.items, "x");
        let Expr::ListLit { elements, .. } = &f.expr else {
            panic!("expected outer list literal")
        };
        assert_eq!(elements.len(), 2);
        let Expr::ListLit {
            elements: inner, ..
        } = &elements[0]
        else {
            panic!("expected inner list literal")
        };
        assert_eq!(inner.len(), 2);
        assert_eq!(inner[0], Expr::I64(1));
    }

    #[test]
    fn parse_list_literal_trailing_comma() {
        let s = parse("x = [1, 2,]");
        let f = field(&s.items, "x");
        let Expr::ListLit { elements, .. } = &f.expr else {
            panic!("expected list literal")
        };
        assert_eq!(elements.len(), 2);
    }

    fn table_items(items: &[Item]) -> Vec<&crate::ast::TableItem> {
        items
            .iter()
            .filter_map(|i| match i {
                Item::Table(t) => Some(t),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn parse_table_with_one_row() {
        let s = parse(r#"db x { users: | "a" | 30 | true | }"#);
        let b = blocks(&s.items)[0];
        let tables = table_items(&b.items);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].field_name, "users");
        assert_eq!(tables[0].rows.len(), 1);
        assert_eq!(tables[0].rows[0].values.len(), 3);
    }

    #[test]
    fn parse_table_with_multiple_rows() {
        let s = parse(
            r#"
            db x {
              users:
                | "a" | 30 | true |
                | "b" | 25 | false |
                | "c" | 42 | true |
            }
            "#,
        );
        let b = blocks(&s.items)[0];
        let tables = table_items(&b.items);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].rows.len(), 3);
    }

    #[test]
    fn parse_table_trailing_pipe_optional() {
        let s = parse(r#"db x { users: | "a" | 30 }"#);
        let b = blocks(&s.items)[0];
        let tables = table_items(&b.items);
        assert_eq!(tables.len(), 1);
        assert_eq!(tables[0].rows[0].values.len(), 2);
    }

    #[test]
    fn parse_empty_table_header() {
        let s = parse(r#"db x { users: }"#);
        let b = blocks(&s.items)[0];
        let tables = table_items(&b.items);
        assert_eq!(tables.len(), 1);
        assert!(tables[0].rows.is_empty());
    }

    #[test]
    fn parse_table_alongside_other_items() {
        let s = parse(
            r#"
            db x {
              port = 8080
              users:
                | "a" | 1 |
              meta { region = "us" }
            }
            "#,
        );
        let b = blocks(&s.items)[0];
        assert!(
            table_items(&b.items)
                .iter()
                .any(|t| t.field_name == "users")
        );
        // Other items still parse.
        let inner_blocks: Vec<&Block> = b
            .items
            .iter()
            .filter_map(|i| {
                if let Item::Block(b) = i {
                    Some(b)
                } else {
                    None
                }
            })
            .collect();
        assert!(inner_blocks.iter().any(|b| b.kind == "meta"));
    }

    #[test]
    fn parser_rejects_decorator_on_table_header() {
        let err = parse_err(r#"db x { @logged users: | 1 | }"#);
        match err {
            ParseError::Syntax(e) => assert!(
                e.message.contains("decorators are not allowed on table"),
                "{}",
                e.message
            ),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn parse_list_literal_of_strings() {
        let s = parse(r#"x = ["a", "b"]"#);
        let f = field(&s.items, "x");
        let Expr::ListLit { elements, .. } = &f.expr else {
            panic!("expected list literal")
        };
        assert_eq!(elements[0], Expr::Utf8("a".into()));
        assert_eq!(elements[1], Expr::Utf8("b".into()));
    }

    // ─── Symbol index ────────────────────────────────────────────────

    #[test]
    fn index_includes_top_level_decls_and_members() {
        let (_, idx) = parse_with_index(
            r#"
            type User { name: utf8 age: u32 }
            union Shape { Circle { r: f64 } Square none }
            symbol_set Color { red green }
            port = 8080
            "#,
        );
        let rec = idx.lookup("User").expect("User indexed");
        assert!(matches!(rec.kind, SymbolKind::TypeDecl));
        let rec = idx.lookup("User.name").expect("User.name indexed");
        assert!(matches!(rec.kind, SymbolKind::TypeField { .. }));
        let rec = idx.lookup("Shape.Circle").expect("Shape.Circle indexed");
        assert!(matches!(rec.kind, SymbolKind::UnionVariant { .. }));
        let rec = idx.lookup("Color.red").expect("Color.red indexed");
        assert!(matches!(rec.kind, SymbolKind::SymbolEntry { .. }));
        let rec = idx.lookup("port").expect("port indexed");
        assert!(matches!(rec.kind, SymbolKind::Field));
    }

    #[test]
    fn index_composes_with_file_namespace() {
        let (_, idx) = parse_with_index("namespace foo\ntype Bar { x: i32 }");
        assert!(idx.lookup("foo.Bar").is_some());
        assert!(idx.lookup("foo.Bar.x").is_some());
        // Without the namespace prefix the entries should NOT be found.
        assert!(idx.lookup("Bar").is_none());
        assert!(idx.lookup("Bar.x").is_none());
    }

    #[test]
    fn index_tracks_blocks_by_kind() {
        let (_, idx) = parse_with_index(
            r#"
            service "a" { port = 1 }
            service "b" { port = 2 }
            metadata { region = "us" }
            "#,
        );
        assert_eq!(idx.blocks_with_kind("service").len(), 2);
        assert_eq!(idx.blocks_with_kind("metadata").len(), 1);
        assert_eq!(idx.blocks_with_kind("unknown").len(), 0);
    }

    #[test]
    fn parser_rejects_duplicate_top_level_field() {
        let err = parse_err("port = 1\nport = 2");
        match err {
            ParseError::Syntax(e) => assert!(
                e.message.contains("duplicate declaration 'port'"),
                "{}",
                e.message
            ),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn parser_rejects_field_and_typedecl_with_same_fqn() {
        let err = parse_err("Foo = 1\ntype Foo {}");
        match err {
            ParseError::Syntax(e) => assert!(
                e.message.contains("duplicate declaration 'Foo'"),
                "{}",
                e.message
            ),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn parser_rejects_duplicate_type_field() {
        let err = parse_err("type Foo { x: i32 x: u8 }");
        match err {
            ParseError::Syntax(e) => assert!(
                e.message.contains("duplicate field 'x' in type 'Foo'"),
                "{}",
                e.message
            ),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn parser_rejects_duplicate_variant() {
        let err = parse_err("union X { A none A none }");
        match err {
            ParseError::Syntax(e) => assert!(
                e.message.contains("duplicate variant 'A' in union 'X'"),
                "{}",
                e.message
            ),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn parse_top_level_import() {
        let s = parse(r#"import "./foo.wcl""#);
        match &s.items[0] {
            Item::Import(i) => assert_eq!(i.path, "./foo.wcl"),
            _ => panic!("expected Item::Import"),
        }
    }

    #[test]
    fn parse_block_level_import() {
        let s = parse(r#"service "web" { import "./x.wcl" }"#);
        let b = blocks(&s.items)[0];
        assert!(matches!(b.items.first(), Some(Item::Import(_))));
    }

    #[test]
    fn parser_rejects_decorator_on_import() {
        let err = parse_err(r#"@logged import "./p.wcl""#);
        match err {
            ParseError::Syntax(e) => assert!(
                e.message.contains("decorators are not allowed on import"),
                "{}",
                e.message
            ),
            _ => panic!("expected syntax error"),
        }
    }

    #[test]
    fn parser_rejects_duplicate_symbol_entry() {
        let err = parse_err("symbol_set C { a a }");
        match err {
            ParseError::Syntax(e) => assert!(
                e.message.contains("duplicate symbol 'a' in symbol_set 'C'"),
                "{}",
                e.message
            ),
            _ => panic!("expected syntax error"),
        }
    }
}
