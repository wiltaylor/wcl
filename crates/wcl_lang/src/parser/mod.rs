mod pattern;
mod types;

use miette::{NamedSource, SourceSpan};

use crate::ast::{
    BinOp, Block, Decorator, Expr, Field, FunctionLit, ImportDecl, Item, LetBinding, MatchArm,
    NamedArg, NamespaceDecl, Parameter, Pattern, Source, Span, SymbolEntry, SymbolSetDecl,
    TypeDecl, TypeField, UnaryOp, UnionDecl, UnionVariant, UseDecl, UseForm, UseItem, VariantArgs,
    VariantBody,
};
use crate::error::ParseError;
use crate::lexer::{LexError, Lexer, NumberLit, StringLit, Token, TokenKind};
use crate::symbols::{DuplicateSymbol, SymbolIndex, SymbolKind, SymbolPath, SymbolRecord};
use crate::value::TypeRef;

use pattern::{collect_binding_names, pattern_span};

pub struct Parser<'a> {
    file: String,
    /// Pre-built `NamedSource` used for every diagnostic. Cloning a
    /// `NamedSource` is cheap (its body is shared), so this is reused
    /// across all of this parse's error sites instead of reallocating
    /// the source string per call.
    named_src: std::sync::Arc<NamedSource<String>>,
    lexer: Lexer<'a>,
    peeked: Option<Token>,
    peeked2: Option<Token>,
    file_ns: Vec<String>,
    index: SymbolIndex,
    block_depth: u32,
    /// Trivia (comments + blank lines) captured at the start of the
    /// current `parse_item` call. Each sub-parser drains this via
    /// `take_item_trivia()` when it builds the final Item struct, so
    /// the round-trip printer can re-emit comments at their original
    /// positions. Fresh per Item: `parse_item` overwrites it on entry.
    current_item_trivia: Vec<crate::ast::Trivia>,
}

impl<'a> Parser<'a> {
    pub fn new(src: &'a str, file: impl Into<String>) -> Self {
        let file = file.into();
        let named_src = std::sync::Arc::new(NamedSource::new(file.clone(), src.to_string()));
        Self {
            file,
            named_src,
            lexer: Lexer::new(src),
            peeked: None,
            peeked2: None,
            file_ns: Vec::new(),
            index: SymbolIndex::default(),
            block_depth: 0,
            current_item_trivia: Vec::new(),
        }
    }

    /// Drain the trivia captured at the start of the current
    /// `parse_item` call. Sub-parsers call this exactly once, at the
    /// moment they construct their `ast::Item` variant.
    pub(super) fn take_item_trivia(&mut self) -> Vec<crate::ast::Trivia> {
        std::mem::take(&mut self.current_item_trivia)
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
            Item::ConnectionDecl(c) => {
                let fqn = self.join_fqn(&c.name);
                self.try_insert(SymbolRecord {
                    fqn,
                    kind: SymbolKind::ConnectionDecl,
                    span: c.span,
                    path: SymbolPath {
                        item_index,
                        member_index: None,
                    },
                })?;
            }
            Item::Connection(_) => {}
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
            Err(DuplicateSymbol {
                first_span,
                second_span,
                ..
            }) => Err(ParseError::syntax_with_related(
                msg,
                (*self.named_src).clone(),
                SourceSpan::new(
                    second_span.start.into(),
                    second_span.end - second_span.start,
                ),
                "duplicate declaration".to_string(),
                SourceSpan::new(first_span.start.into(), first_span.end - first_span.start),
                "first declared here".to_string(),
            )),
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
        // Harvest leading trivia (comments + blank lines) from the
        // first token of this item, before parse_decorators consumes
        // it. Sub-parsers drain via `take_item_trivia()` when they
        // build their Item, so the source printer can re-emit
        // comments at their original positions.
        self.current_item_trivia = self.peek()?.leading_trivia.clone();
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
        if let Some(ref first) = first_ident
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
                    if self.block_depth > 0 {
                        let span = self.peek()?.span;
                        return Err(self.err(
                            format!("'{first}' declarations are only allowed at the top level"),
                            span,
                            "move to the file's top level",
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
        // `connection NAME :` — schema declaration. Two-token lookahead
        // distinguishes it from a bare identifier followed by other
        // syntax. Statements (`NAME -> NAME`) use the bare-ident path
        // below.
        if let Some(first) = first_ident.as_deref()
            && first == "connection"
            && matches!(self.peek2()?.kind, TokenKind::Ident(_))
        {
            if !decorators.is_empty() {
                let span = decorators[0].span;
                return Err(self.err(
                    "decorators are not allowed on connection declarations",
                    span,
                    "remove decorator",
                ));
            }
            return self.parse_connection_decl();
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
            TokenKind::Arrow => {
                if !decorators.is_empty() {
                    let span = decorators[0].span;
                    return Err(self.err(
                        "decorators are not allowed on connection statements",
                        span,
                        "remove decorator",
                    ));
                }
                self.parse_connection_stmt(name, Span::new(span_start, tok.span.end))
            }
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
                        let (arg_name, name_span) = self.bump_ident("expected argument name")?;
                        let arg_start = name_span.start;
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
            TokenKind::Str(s) => self.string_lit_to_expr(s, span)?,
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
        let args = self.parse_comma_separated(
            |k| matches!(k, TokenKind::RParen),
            "')'",
            "call arguments",
            |p| p.parse_expr().map(|(e, _)| e),
        )?;
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

    fn parse_fn_parameter(&mut self) -> Result<Parameter, ParseError> {
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
        })
    }

    fn parse_function_literal(&mut self) -> Result<(Expr, Span), ParseError> {
        let fn_tok = self.bump()?; // 'fn'
        self.expect(TokenKind::LParen, "expected '(' after 'fn'")?;
        let params = self.parse_comma_separated(
            |k| matches!(k, TokenKind::RParen),
            "')'",
            "parameter list",
            Self::parse_fn_parameter,
        )?;
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

    /// Greedy path parser: `IDENT (. IDENT)*`.
    ///
    /// Refuses to consume a `Dot` if the next token after it is not an
    /// identifier — that way `foo.bar.{...}` parses as path `[foo, bar]`
    /// with the `.{` left for the caller (`parse_use_decl`).
    pub(super) fn parse_path(&mut self) -> Result<(Vec<String>, Span), ParseError> {
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
            let (seg, seg_span) = self.bump_ident("expected identifier after '.'")?;
            end = seg_span.end;
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
            leading_trivia: self.take_item_trivia(),
        }))
    }

    fn parse_table_item(&mut self) -> Result<Item, ParseError> {
        // Already peeked: IDENT followed by Colon. Consume both.
        let (field_name, name_span) = self.bump_ident("expected table field name")?;
        let start = name_span.start;
        self.expect(TokenKind::Colon, "expected ':' after table field name")?;

        let mut rows = Vec::new();
        let mut end = name_span.end;
        while matches!(self.peek()?.kind, TokenKind::Pipe) {
            let row = self.parse_table_row()?;
            end = row.span.end;
            rows.push(row);
        }

        Ok(Item::Table(crate::ast::TableItem {
            field_name,
            rows,
            span: Span::new(start, end),
            leading_trivia: self.take_item_trivia(),
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
            leading_trivia: self.take_item_trivia(),
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
                leading_trivia: self.take_item_trivia(),
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
            leading_trivia: self.take_item_trivia(),
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
        self.expect_brace_after("type name", &name)?;
        let (fields, rbrace_span) = self.parse_brace_members(
            "unexpected end of file inside type declaration",
            Self::parse_type_field,
        )?;
        Ok(Item::TypeDecl(TypeDecl {
            name,
            extends,
            fields,
            decorators,
            span: Span::new(start, rbrace_span.end),
            leading_trivia: self.take_item_trivia(),
        }))
    }

    fn parse_interface_decl(&mut self, decorators: Vec<Decorator>) -> Result<Item, ParseError> {
        let kw = self.bump()?; // 'interface'
        let start = kw.span.start;
        let (name, _name_span) = self.parse_path()?;
        let extends = self.parse_extends_clause()?;
        self.expect_brace_after("interface name", &name)?;
        let (fields, rbrace_span) = self.parse_brace_members(
            "unexpected end of file inside interface declaration",
            Self::parse_type_field,
        )?;
        Ok(Item::InterfaceDecl(crate::ast::InterfaceDecl {
            name,
            extends,
            fields,
            decorators,
            span: Span::new(start, rbrace_span.end),
            leading_trivia: self.take_item_trivia(),
        }))
    }

    /// Consume the opening `{` of a `type` / `interface` / `union`
    /// declaration body, producing the existing rich error message
    /// (which interpolates the declaration's name) when the token isn't a
    /// `{`.
    fn expect_brace_after(&mut self, label: &str, name: &[String]) -> Result<(), ParseError> {
        let lbrace = self.bump()?;
        if !matches!(lbrace.kind, TokenKind::LBrace) {
            return Err(self.err(
                format!(
                    "expected '{{' after {label} '{}', found {}",
                    name.join("."),
                    describe(&lbrace.kind)
                ),
                lbrace.span,
                "expected '{'",
            ));
        }
        Ok(())
    }

    /// Parse a comma-separated list, allowing a single optional trailing
    /// comma before the close token. Assumes the open delimiter has
    /// already been consumed and consumes the matching close. The
    /// `context` string is interpolated into error messages
    /// (`"expected ',' or {close_desc} in {context}"`).
    fn parse_comma_separated<T>(
        &mut self,
        is_close: fn(&TokenKind) -> bool,
        close_desc: &str,
        context: &str,
        mut parse_item: impl FnMut(&mut Self) -> Result<T, ParseError>,
    ) -> Result<Vec<T>, ParseError> {
        let mut items = Vec::new();
        if is_close(&self.peek()?.kind) {
            return Ok(items);
        }
        loop {
            items.push(parse_item(self)?);
            match self.peek()?.kind {
                TokenKind::Comma => {
                    self.bump()?;
                    if is_close(&self.peek()?.kind) {
                        break;
                    }
                }
                ref k if is_close(k) => break,
                _ => {
                    let p = self.peek()?;
                    let span = p.span;
                    let kind = describe(&p.kind);
                    return Err(self.err(
                        format!("expected ',' or {close_desc} in {context}, found {kind}"),
                        span,
                        format!("expected ',' or {close_desc}"),
                    ));
                }
            }
        }
        Ok(items)
    }

    /// Drive a brace-delimited member loop. Assumes the opening `{` has
    /// already been consumed; reads members via `parse_member` until it
    /// sees `}` (which it consumes) or `Eof` (which errors with
    /// `eof_message`). Returns the parsed members and the span of the
    /// closing brace.
    fn parse_brace_members<T>(
        &mut self,
        eof_message: &str,
        mut parse_member: impl FnMut(&mut Self) -> Result<T, ParseError>,
    ) -> Result<(Vec<T>, Span), ParseError> {
        let mut items = Vec::new();
        loop {
            let p = self.peek()?;
            match p.kind {
                TokenKind::RBrace => break,
                TokenKind::Eof => {
                    let span = p.span;
                    return Err(self.err(eof_message, span, "expected '}'"));
                }
                _ => items.push(parse_member(self)?),
            }
        }
        let rbrace = self.bump()?;
        Ok((items, rbrace.span))
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
        self.expect_brace_after("union name", &name)?;
        let (variants, rbrace_span) = self.parse_brace_members(
            "unexpected end of file inside union declaration",
            Self::parse_variant_decl,
        )?;
        Ok(Item::UnionDecl(UnionDecl {
            name,
            extends,
            variants,
            decorators,
            span: Span::new(start, rbrace_span.end),
            leading_trivia: self.take_item_trivia(),
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
                let (fields, rbrace_span) = self.parse_brace_members(
                    "unexpected end of file inside variant body",
                    Self::parse_type_field,
                )?;
                Ok((VariantBody::Record(fields), rbrace_span.end))
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
            leading_trivia: self.take_item_trivia(),
        }))
    }

    /// Parse `connection NAME : TypeRef -> TypeRef : Path`.
    /// The leading `connection` keyword has not yet been consumed.
    fn parse_connection_decl(&mut self) -> Result<Item, ParseError> {
        let kw = self.bump()?; // `connection`
        let start = kw.span.start;
        let (name, _) = self.parse_path()?;
        self.expect(TokenKind::Colon, "expected ':' after connection name")?;
        let (source, source_span) = self.parse_type_ref()?;
        self.expect(
            TokenKind::Arrow,
            "expected '->' between source and destination types",
        )?;
        let (destination, destination_span) = self.parse_type_ref()?;
        self.expect(
            TokenKind::Colon,
            "expected ':' before connection kind symbol_set",
        )?;
        let (kind_set, kind_set_span) = self.parse_path()?;
        let end = kind_set_span.end;
        Ok(Item::ConnectionDecl(crate::ast::ConnectionDecl {
            name,
            source,
            source_span,
            destination,
            destination_span,
            kind_set,
            kind_set_span,
            span: Span::new(start, end),
            leading_trivia: self.take_item_trivia(),
        }))
    }

    /// Parse a connection statement starting at `lhs -> rhs [':' sym]`.
    /// The lhs ident has already been consumed; `lhs_span` covers it.
    fn parse_connection_stmt(&mut self, lhs: String, lhs_span: Span) -> Result<Item, ParseError> {
        self.expect(TokenKind::Arrow, "expected '->' in connection statement")?;
        let rhs_tok = self.bump()?;
        let TokenKind::Ident(rhs) = rhs_tok.kind else {
            return Err(self.err(
                format!(
                    "expected identifier after '->', found {}",
                    describe(&rhs_tok.kind)
                ),
                rhs_tok.span,
                "expected identifier",
            ));
        };
        let rhs_span = rhs_tok.span;
        let mut end = rhs_span.end;
        // Optional kind annotation. The lexer produces a single
        // `Symbol(name)` token for the tight `:name` form, or a
        // separate `Colon` + `Ident` pair when whitespace intervenes.
        // Clone the symbol payload out of peek so we can bump without
        // having to re-destructure (and without leaving a load-bearing
        // `unreachable!()` between the peek and the bump).
        let pre_sym = match &self.peek()?.kind {
            TokenKind::Symbol(s) => Some(s.clone()),
            _ => None,
        };
        let (kind, kind_span) = if let Some(sym) = pre_sym {
            let span = self.bump()?.span;
            end = span.end;
            (Some(sym), Some(span))
        } else if matches!(self.peek()?.kind, TokenKind::Colon) {
            self.bump()?; // ':'
            let (sym, sym_span) = self.bump_ident("expected symbol identifier after ':'")?;
            end = sym_span.end;
            (Some(sym), Some(sym_span))
        } else {
            (None, None)
        };
        Ok(Item::Connection(crate::ast::ConnectionStmt {
            lhs,
            lhs_span,
            rhs,
            rhs_span,
            kind,
            kind_span,
            span: Span::new(lhs_span.start, end),
            leading_trivia: self.take_item_trivia(),
        }))
    }

    pub(super) fn expect(&mut self, kind: TokenKind, msg: &str) -> Result<Token, ParseError> {
        let tok = self.bump()?;
        if std::mem::discriminant(&tok.kind) == std::mem::discriminant(&kind) {
            Ok(tok)
        } else {
            let span = tok.span;
            let found = describe(&tok.kind);
            Err(self.err(format!("{msg}, found {found}"), span, "unexpected token"))
        }
    }

    /// Bump and destructure an `Ident` token, returning `(name, span)`.
    /// On any other token kind, build a "expected identifier" error
    /// using `msg` as the surface context (e.g. `"expected identifier
    /// after '.'"`). Replaces the recurring `peek → bump → let-else
    /// unreachable!()` pattern.
    pub(super) fn bump_ident(&mut self, msg: &str) -> Result<(String, Span), ParseError> {
        let tok = self.bump()?;
        let span = tok.span;
        if let TokenKind::Ident(name) = tok.kind {
            Ok((name, span))
        } else {
            let found = describe(&tok.kind);
            Err(self.err(format!("{msg}, found {found}"), span, "expected identifier"))
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
            leading_trivia: self.take_item_trivia(),
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
        self.block_depth += 1;
        let body_result = (|| -> Result<Vec<Item>, ParseError> {
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
            Ok(items)
        })();
        self.block_depth -= 1;
        let items = body_result?;
        let rbrace = self.bump()?;
        Ok(Item::Block(Block {
            kind,
            labels,
            items,
            decorators,
            span: Span::new(start, rbrace.span.end),
            leading_trivia: self.take_item_trivia(),
        }))
    }

    pub(super) fn peek(&mut self) -> Result<&Token, ParseError> {
        if self.peeked.is_none() {
            self.peeked = Some(self.next_lex()?);
        }
        Ok(self.peeked.as_ref().expect("just set"))
    }

    pub(super) fn peek2(&mut self) -> Result<&Token, ParseError> {
        self.peek()?;
        if self.peeked2.is_none() {
            self.peeked2 = Some(self.next_lex()?);
        }
        Ok(self.peeked2.as_ref().expect("just set"))
    }

    pub(super) fn bump(&mut self) -> Result<Token, ParseError> {
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

    /// Build an `Expr` from a `StringLit` token. Plain forms map
    /// one-to-one to the existing string-typed `Expr` variants; the
    /// interpolated form sub-parses each `${expr}` slot using a fresh
    /// `Parser` over a leading-padded copy of the original source so
    /// span offsets stay aligned with the outer file.
    fn string_lit_to_expr(&self, lit: StringLit, _span: Span) -> Result<Expr, ParseError> {
        // Plain encodings short-circuit. Only the interpolated form
        // needs the slot-by-slot sub-parse, so destructure here rather
        // than splitting into a helper that leaves an unreachable arm.
        let (encoding, parts, lit_span) = match lit {
            StringLit::Utf8(s) => return Ok(Expr::Utf8(s)),
            StringLit::Ascii(s) => return Ok(Expr::Ascii(s)),
            StringLit::Utf16(v) => return Ok(Expr::Utf16(v)),
            StringLit::Utf32(v) => return Ok(Expr::Utf32(v)),
            StringLit::Interpolated {
                encoding,
                parts,
                span,
            } => (encoding, parts, span),
        };
        let mut out_parts: Vec<crate::ast::TemplatePart> = Vec::with_capacity(parts.len());
        for part in parts {
            match part {
                crate::lexer::StringPart::Literal(s) => {
                    out_parts.push(crate::ast::TemplatePart::Literal(s));
                }
                crate::lexer::StringPart::Expr {
                    text,
                    span: slot_span,
                } => {
                    let expr = self.sub_parse_slot(&text, slot_span)?;
                    out_parts.push(crate::ast::TemplatePart::Expr(Box::new(expr)));
                }
            }
        }
        Ok(Expr::InterpolatedString {
            encoding,
            parts: out_parts,
            span: lit_span,
        })
    }

    /// Sub-parse one `${...}` slot. Constructs a fresh parser over a
    /// leading-padded copy of the slot text so error spans land in the
    /// outer source's coordinates without explicit span rewriting.
    ///
    /// Any error from the sub-parser is re-issued against the outer
    /// `NamedSource` (so the rendered snippet shows the *real* file,
    /// not the padded duplicate the sub-parser sees) and prefixed with
    /// `"in interpolation slot:"` so the user can tell the diagnostic
    /// came from inside a `${…}` rather than the surrounding text.
    fn sub_parse_slot(&self, text: &str, slot_span: Span) -> Result<Expr, ParseError> {
        // Pad the slot text with spaces so the sub-parser's byte
        // offsets line up with the outer source.
        let padded = format!("{}{}", " ".repeat(slot_span.start), text);
        let mut sub = Parser::new(&padded, self.file.clone());
        // Skip the padding via the lexer's whitespace handling: the
        // first peek/bump will land on the first real byte of the
        // slot text. Then parse one expression.
        let (expr, _) = sub.parse_expr().map_err(|e| self.wrap_slot_error(e))?;
        let trailing = sub.peek().map_err(|e| self.wrap_slot_error(e))?;
        match &trailing.kind {
            TokenKind::Eof => Ok(expr),
            other => {
                let msg = format!(
                    "in interpolation slot: unexpected token {}",
                    describe(other)
                );
                Err(self.err(msg, slot_span, "extra tokens after expression"))
            }
        }
    }

    /// Convert a sub-parser's `ParseError` (which references the
    /// padded slot source) into one rooted in the outer document's
    /// `NamedSource`, prefixing the message with the interpolation
    /// context.
    fn wrap_slot_error(&self, e: ParseError) -> ParseError {
        match e {
            ParseError::Syntax(inner) => ParseError::syntax(
                format!("in interpolation slot: {}", inner.message),
                (*self.named_src).clone(),
                inner.span,
                inner.label,
            ),
            other => other,
        }
    }

    pub(super) fn err(
        &self,
        message: impl Into<String>,
        span: Span,
        label: impl Into<String>,
    ) -> ParseError {
        let len = span.len().max(1);
        ParseError::syntax(
            message.into(),
            (*self.named_src).clone(),
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

pub(super) fn describe(t: &TokenKind) -> String {
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
#[path = "../parser_tests.rs"]
mod tests;
