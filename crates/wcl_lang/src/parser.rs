use miette::{NamedSource, SourceSpan};

use crate::ast::{
    Block, Decorator, Expr, Field, Item, NamedArg, NamespaceDecl, Source, Span, SymbolEntry,
    SymbolSetDecl, TypeDecl, TypeField, UnionDecl, UnionVariant, UseDecl, UseForm, UseItem,
    VariantBody,
};
use crate::error::ParseError;
use crate::lexer::{LexError, Lexer, NumberLit, StringLit, Token, TokenKind};
use crate::value::{BuiltinType, TensorDim, TypeRef};

pub struct Parser<'a> {
    src: &'a str,
    file: String,
    lexer: Lexer<'a>,
    peeked: Option<Token>,
    peeked2: Option<Token>,
}

impl<'a> Parser<'a> {
    pub fn new(src: &'a str, file: impl Into<String>) -> Self {
        Self {
            src,
            file: file.into(),
            lexer: Lexer::new(src),
            peeked: None,
            peeked2: None,
        }
    }

    pub fn parse_source(&mut self) -> Result<Source, ParseError> {
        let mut items = Vec::new();
        while !matches!(self.peek()?.kind, TokenKind::Eof) {
            items.push(self.parse_item()?);
        }
        Ok(Source { items })
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
        if let Some(first) = first_ident
            && matches!(self.peek2()?.kind, TokenKind::Ident(_))
        {
            match first.as_str() {
                "type" => return self.parse_type_decl(decorators),
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
                        let (value, value_span) = self.parse_value_expr()?;
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
                        let (value, _) = self.parse_value_expr()?;
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
            fields,
            decorators,
            span: Span::new(start, rbrace.span.end),
        }))
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
            TokenKind::Amp | TokenKind::Ident(_) => {
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
        // Contextual keyword: `list<...>` or `tensor<..., [...]>`.
        let head_ident = match &self.peek()?.kind {
            TokenKind::Ident(s) => Some(s.clone()),
            _ => None,
        };
        if let Some(s) = head_ident
            && (s == "list" || s == "tensor")
            && matches!(self.peek2()?.kind, TokenKind::Lt)
        {
            return if s == "list" {
                self.parse_list_type()
            } else {
                self.parse_tensor_type()
            };
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

    fn parse_list_type(&mut self) -> Result<(TypeRef, Span), ParseError> {
        let start = self.bump()?.span.start; // 'list'
        self.expect(TokenKind::Lt, "expected '<' after 'list'")?;
        let (inner, _) = self.parse_type_ref()?;
        let gt = self.expect(TokenKind::Gt, "expected '>' to close list<...>")?;
        Ok((
            TypeRef::List(Box::new(inner)),
            Span::new(start, gt.span.end),
        ))
    }

    fn parse_tensor_type(&mut self) -> Result<(TypeRef, Span), ParseError> {
        let start = self.bump()?.span.start; // 'tensor'
        self.expect(TokenKind::Lt, "expected '<' after 'tensor'")?;
        let (element, _) = self.parse_type_ref()?;
        self.expect(TokenKind::Comma, "expected ',' after tensor element type")?;
        let lbracket = self.expect(TokenKind::LBracket, "expected '[' for tensor dimensions")?;

        let mut dims: Vec<TensorDim> = Vec::new();
        loop {
            if matches!(self.peek()?.kind, TokenKind::RBracket) {
                break;
            }
            dims.push(self.parse_tensor_dim()?);
            match self.peek()?.kind {
                TokenKind::Comma => {
                    self.bump()?;
                }
                TokenKind::RBracket => break,
                _ => {
                    let p = self.peek()?;
                    let span = p.span;
                    let kind = describe(&p.kind);
                    return Err(self.err(
                        format!("expected ',' or ']' in tensor dimensions, found {kind}"),
                        span,
                        "expected ',' or ']'",
                    ));
                }
            }
        }
        let rbracket = self.expect(TokenKind::RBracket, "expected ']' to close tensor dims")?;
        if dims.is_empty() {
            return Err(self.err(
                "tensor must have at least one dimension",
                Span::new(lbracket.span.start, rbracket.span.end),
                "expected at least one dimension",
            ));
        }
        let gt = self.expect(TokenKind::Gt, "expected '>' to close tensor<...>")?;
        Ok((
            TypeRef::Tensor {
                element: Box::new(element),
                dims,
            },
            Span::new(start, gt.span.end),
        ))
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
        let (expr, value_span) = self.parse_value_expr()?;
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

fn describe(t: &TokenKind) -> String {
    match t {
        TokenKind::Ident(s) => format!("identifier '{s}'"),
        TokenKind::Str(_) => "string".to_string(),
        TokenKind::Number(_) => "number".to_string(),
        TokenKind::Bool(_) => "boolean".to_string(),
        TokenKind::Symbol(_) => "symbol literal".to_string(),
        TokenKind::None => "'none'".to_string(),
        TokenKind::Eq => "'='".to_string(),
        TokenKind::Colon => "':'".to_string(),
        TokenKind::Question => "'?'".to_string(),
        TokenKind::Amp => "'&'".to_string(),
        TokenKind::Dot => "'.'".to_string(),
        TokenKind::Comma => "','".to_string(),
        TokenKind::Lt => "'<'".to_string(),
        TokenKind::Gt => "'>'".to_string(),
        TokenKind::LBracket => "'['".to_string(),
        TokenKind::RBracket => "']'".to_string(),
        TokenKind::At => "'@'".to_string(),
        TokenKind::LParen => "'('".to_string(),
        TokenKind::RParen => "')'".to_string(),
        TokenKind::LBrace => "'{'".to_string(),
        TokenKind::RBrace => "'}'".to_string(),
        TokenKind::Eof => "end of file".to_string(),
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(src: &str) -> Source {
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
        let s = parse("type Item {} union Wrap { Boxed &Item }");
        let u = union_decls(&s.items)[0];
        match &u.variants[0].body {
            VariantBody::TypeRef { ty, .. } => assert_eq!(
                *ty,
                TypeRef::Reference(Box::new(TypeRef::Named(vec!["Item".into()])))
            ),
            _ => panic!("expected TypeRef body"),
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
}
