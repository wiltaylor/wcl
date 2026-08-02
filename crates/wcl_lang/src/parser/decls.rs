//! Declaration parsers (decorators, type/interface/union/symbol_set, imports,
//! use, table, namespace, connections, field, block). Extracted from
//! `parser/mod.rs` so the parent file can stay focused on top-level
//! source driving and shared token helpers.

use crate::ast::{
    Block, Decorator, Expr, Field, ImportDecl, Item, NamedArg, NamespaceDecl, Span, SymbolEntry,
    SymbolSetDecl, TypeDecl, TypeField, UnionDecl, UnionVariant, UseDecl, UseForm, UseItem,
    VariantBody,
};
use crate::error::ParseError;
use crate::lexer::{StringLit, TokenKind};
use crate::value::{BuiltinType, TypeRef};

use super::Parser;
use super::describe;
use super::is_expr_start;

/// `true` when `decorators` contains an `@default` (used to reject
/// the redundant combination of an inline `=` default with a
/// `@default(...)` decorator on the same field).
fn has_default_decorator(decorators: &[Decorator]) -> bool {
    decorators
        .iter()
        .any(|d| d.name.len() == 1 && d.name[0] == "default")
}

/// Whether a block carries a bare `@schemaless` decorator. Mirrors
/// `doc::schema_check::has_schemaless`, kept local so the parser need not
/// depend on the doc layer. Drives string-literal field-key acceptance in
/// `parse_block`.
fn decorator_is_schemaless(decorators: &[Decorator]) -> bool {
    decorators
        .iter()
        .any(|d| d.name.len() == 1 && d.name[0] == "schemaless")
}

/// Infer a `TypeRef` from an expression used as an inline default in
/// a type-body field declaration (`name = expr`). Covers the cases
/// where the user could plausibly want type inference: function
/// literals (carry full signature), primitive literals, list / record
/// literals built from them. Returns `None` when inference would
/// require lookup (identifiers, calls, member access, etc.) — the
/// parser then reports a focused error.
pub(super) fn infer_type_from_expr(expr: &Expr) -> Option<TypeRef> {
    match expr {
        Expr::Bool(_) => Some(TypeRef::Builtin(BuiltinType::Bool)),
        Expr::I8(_) => Some(TypeRef::Builtin(BuiltinType::I8)),
        Expr::I16(_) => Some(TypeRef::Builtin(BuiltinType::I16)),
        Expr::I32(_) => Some(TypeRef::Builtin(BuiltinType::I32)),
        Expr::I64(_) => Some(TypeRef::Builtin(BuiltinType::I64)),
        Expr::I128(_) => Some(TypeRef::Builtin(BuiltinType::I128)),
        Expr::Isize(_) => Some(TypeRef::Builtin(BuiltinType::Isize)),
        Expr::U8(_) => Some(TypeRef::Builtin(BuiltinType::U8)),
        Expr::U16(_) => Some(TypeRef::Builtin(BuiltinType::U16)),
        Expr::U32(_) => Some(TypeRef::Builtin(BuiltinType::U32)),
        Expr::U64(_) => Some(TypeRef::Builtin(BuiltinType::U64)),
        Expr::U128(_) => Some(TypeRef::Builtin(BuiltinType::U128)),
        Expr::Usize(_) => Some(TypeRef::Builtin(BuiltinType::Usize)),
        Expr::F32(_) => Some(TypeRef::Builtin(BuiltinType::F32)),
        Expr::F64(_) => Some(TypeRef::Builtin(BuiltinType::F64)),
        Expr::Utf8(_) => Some(TypeRef::Builtin(BuiltinType::Utf8)),
        Expr::Ascii(_) => Some(TypeRef::Builtin(BuiltinType::Ascii)),
        Expr::Symbol(_) => Some(TypeRef::Builtin(BuiltinType::Symbol)),
        Expr::Function(lit) => Some(TypeRef::Function {
            params: lit.params.iter().map(|p| p.ty.clone()).collect(),
            return_ty: Box::new(lit.return_ty.clone()),
        }),
        Expr::ListLit { elements, .. } => {
            // Pick the first element's inferred type; fall back to
            // utf8 for empty lists (rare but plausible for stubs).
            let inner = elements
                .first()
                .and_then(infer_type_from_expr)
                .unwrap_or(TypeRef::Builtin(BuiltinType::Utf8));
            Some(TypeRef::List(Box::new(inner)))
        }
        _ => None,
    }
}

impl<'a> Parser<'a> {
    pub(super) fn parse_decorators(&mut self) -> Result<Vec<Decorator>, ParseError> {
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
                            leading_trivia: Vec::new(),
                            trailing_comment: None,
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
            name_span,
            positional,
            named,
            span: Span::new(start, end),
        })
    }

    pub(super) fn parse_namespace_decl(&mut self) -> Result<Item, ParseError> {
        let kw = self.bump()?; // 'namespace'
        let start = kw.span.start;
        let (path, path_span) = self.parse_path()?;
        Ok(Item::NamespaceDecl(NamespaceDecl {
            path,
            span: Span::new(start, path_span.end),
            leading_trivia: self.take_item_trivia(),
            trailing_comment: None,
        }))
    }

    pub(super) fn parse_table_item(&mut self) -> Result<Item, ParseError> {
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
            trailing_comment: None,
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

    pub(super) fn parse_import_decl(&mut self) -> Result<Item, ParseError> {
        let kw = self.bump()?; // 'import'
        let start = kw.span.start;

        // System form: `import < path >`. The bracketed path is not a
        // single token (it contains `/`, `.`, `-`, …), so we scan to the
        // matching `>` and recover the path by slicing the raw source
        // between the `<` and `>` spans.
        if matches!(self.peek()?.kind, TokenKind::Lt) {
            let lt = self.bump()?; // '<'
            loop {
                let t = self.peek()?;
                match t.kind {
                    TokenKind::Gt => break,
                    TokenKind::Eof => {
                        let eof_start = t.span.start;
                        return Err(self.err(
                            "unterminated system import: expected '>'",
                            Span::new(lt.span.start, eof_start),
                            "expected '>'",
                        ));
                    }
                    _ => {
                        self.bump()?;
                    }
                }
            }
            let gt = self.bump()?; // '>'
            let path = self.src[lt.span.end..gt.span.start].trim().to_string();
            let path_span = Span::new(lt.span.end, gt.span.start);
            if path.is_empty() {
                return Err(self.err(
                    "empty system import path: `import <>`",
                    Span::new(lt.span.start, gt.span.end),
                    "expected a path",
                ));
            }
            return Ok(Item::Import(ImportDecl {
                path,
                path_span,
                system: true,
                span: Span::new(start, gt.span.end),
                leading_trivia: self.take_item_trivia(),
                trailing_comment: None,
            }));
        }

        // Disk form: `import "path"`.
        let tok = self.bump()?;
        let path_span = tok.span;
        let path = match tok.kind {
            TokenKind::Str(StringLit::Utf8(s)) | TokenKind::Str(StringLit::Ascii(s)) => s,
            other => {
                return Err(self.err(
                    format!(
                        "expected string path or `<...>` after 'import', found {}",
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
            system: false,
            span: Span::new(start, path_span.end),
            leading_trivia: self.take_item_trivia(),
            trailing_comment: None,
        }))
    }

    pub(super) fn parse_use_decl(&mut self) -> Result<Item, ParseError> {
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
                trailing_comment: None,
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
            trailing_comment: None,
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

    pub(super) fn parse_type_decl(
        &mut self,
        decorators: Vec<Decorator>,
    ) -> Result<Item, ParseError> {
        let type_kw = self.bump()?; // 'type'
        let start = type_kw.span.start;
        let (name, _name_span) = self.parse_path()?;
        let extends = self.parse_extends_clause()?;
        // Alias form: `type Name = TypeRef`. A transparent name for the
        // target; constraint decorators on the alias apply wherever the
        // name is used. An alias can't also extend.
        if matches!(self.peek()?.kind, TokenKind::Eq) {
            if !extends.is_empty() {
                let span = self.peek()?.span;
                return Err(self.err(
                    "a type alias cannot have an extends clause",
                    span,
                    "remove `extends` or give the type a body",
                ));
            }
            self.bump()?; // consume '='
            let (target, target_span) = self.parse_type_ref()?;
            return Ok(Item::TypeDecl(TypeDecl {
                name,
                extends,
                alias: Some(target),
                fields: Vec::new(),
                decorators,
                span: Span::new(start, target_span.end),
                leading_trivia: self.take_item_trivia(),
                trailing_comment: None,
                trailing_trivia: Vec::new(),
            }));
        }
        self.expect_brace_after("type name", &name)?;
        let (fields, rbrace_span, trailing_trivia) = self.parse_brace_members(
            "unexpected end of file inside type declaration",
            Self::parse_type_field,
            |f: &mut TypeField, c| f.trailing_comment = Some(c),
        )?;
        Ok(Item::TypeDecl(TypeDecl {
            name,
            extends,
            alias: None,
            fields,
            decorators,
            span: Span::new(start, rbrace_span.end),
            leading_trivia: self.take_item_trivia(),
            trailing_comment: None,
            trailing_trivia,
        }))
    }

    pub(super) fn parse_interface_decl(
        &mut self,
        decorators: Vec<Decorator>,
    ) -> Result<Item, ParseError> {
        let kw = self.bump()?; // 'interface'
        let start = kw.span.start;
        let (name, _name_span) = self.parse_path()?;
        let extends = self.parse_extends_clause()?;
        self.expect_brace_after("interface name", &name)?;
        let (fields, rbrace_span, trailing_trivia) = self.parse_brace_members(
            "unexpected end of file inside interface declaration",
            Self::parse_type_field,
            |f: &mut TypeField, c| f.trailing_comment = Some(c),
        )?;
        Ok(Item::InterfaceDecl(crate::ast::InterfaceDecl {
            name,
            extends,
            fields,
            decorators,
            span: Span::new(start, rbrace_span.end),
            leading_trivia: self.take_item_trivia(),
            trailing_comment: None,
            trailing_trivia,
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
    pub(super) fn parse_comma_separated<T>(
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
        set_trailing: impl Fn(&mut T, String),
    ) -> Result<(Vec<T>, Span, Vec<crate::ast::Trivia>), ParseError> {
        let mut items = Vec::new();
        loop {
            let p = self.peek()?;
            match p.kind {
                TokenKind::RBrace => break,
                TokenKind::Eof => {
                    let span = p.span;
                    return Err(self.err(eof_message, span, "expected '}'"));
                }
                _ => {
                    items.push(parse_member(self)?);
                    // An inline comment after this member (carried on the
                    // next token, including the `}`) trails it.
                    if let Some(c) = self.take_same_line_comment()?
                        && let Some(last) = items.last_mut()
                    {
                        set_trailing(last, c);
                    }
                }
            }
        }
        let rbrace = self.bump()?;
        // Comments on their own lines after the last member, before `}`.
        let trailing_trivia = rbrace.leading_trivia;
        Ok((items, rbrace.span, trailing_trivia))
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
        // Capture comments above this field before decorators consume
        // the first token. Trailing comment is filled in by the caller.
        let leading_trivia = self.peek_leading_trivia()?;
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
        let next = self.bump()?;
        match next.kind {
            TokenKind::Colon => {
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
                    default_expr: None,
                    leading_trivia,
                    trailing_comment: None,
                })
            }
            TokenKind::Eq => {
                // Inline-default form: `name = expr`. Type is inferred
                // from the expression. Optional implicitly true — the
                // default guarantees a value, so an instance that
                // omits the field is not a violation.
                if has_default_decorator(&decorators) {
                    return Err(self.err(
                        format!(
                            "field '{field_name}' uses inline `=` default and `@default(...)` together; pick one"
                        ),
                        Span::new(field_start, next.span.end),
                        "redundant default",
                    ));
                }
                let (expr, expr_span) = self.parse_expr()?;
                let ty = infer_type_from_expr(&expr).ok_or_else(|| {
                    self.err(
                        format!(
                            "cannot infer type for inline default of '{field_name}'; declare it with `{field_name}: T` and use `@default(...)` instead"
                        ),
                        expr_span,
                        "type not inferable from this expression",
                    )
                })?;
                Ok(TypeField {
                    name: field_name,
                    ty,
                    ty_span: expr_span,
                    optional: true,
                    decorators,
                    span: Span::new(field_start, expr_span.end),
                    default_expr: Some(expr),
                    leading_trivia,
                    trailing_comment: None,
                })
            }
            _ => Err(self.err(
                format!(
                    "expected ':' or '=' after field name '{field_name}', found {}",
                    describe(&next.kind)
                ),
                next.span,
                "expected ':' or '='",
            )),
        }
    }

    pub(super) fn parse_union_decl(
        &mut self,
        decorators: Vec<Decorator>,
    ) -> Result<Item, ParseError> {
        let kw = self.bump()?; // 'union'
        let start = kw.span.start;
        let (name, _name_span) = self.parse_path()?;
        let extends = self.parse_extends_clause()?;
        self.expect_brace_after("union name", &name)?;
        let (variants, rbrace_span, trailing_trivia) = self.parse_brace_members(
            "unexpected end of file inside union declaration",
            Self::parse_variant_decl,
            |v: &mut UnionVariant, c| v.trailing_comment = Some(c),
        )?;
        Ok(Item::UnionDecl(UnionDecl {
            name,
            extends,
            variants,
            decorators,
            span: Span::new(start, rbrace_span.end),
            leading_trivia: self.take_item_trivia(),
            trailing_comment: None,
            trailing_trivia,
        }))
    }

    fn parse_variant_decl(&mut self) -> Result<UnionVariant, ParseError> {
        // Capture comments above this variant before decorators consume
        // the first token. Trailing comment is filled in by the caller.
        let leading_trivia = self.peek_leading_trivia()?;
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
            leading_trivia,
            trailing_comment: None,
        })
    }

    fn parse_variant_body(&mut self) -> Result<(VariantBody, usize), ParseError> {
        let head = self.peek()?;
        match head.kind {
            TokenKind::LBrace => {
                self.bump()?;
                let (fields, rbrace_span, trailing_trivia) = self.parse_brace_members(
                    "unexpected end of file inside variant body",
                    Self::parse_type_field,
                    |f: &mut TypeField, c| f.trailing_comment = Some(c),
                )?;
                Ok((
                    VariantBody::Record {
                        fields,
                        trailing_trivia,
                    },
                    rbrace_span.end,
                ))
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

    pub(super) fn parse_symbol_set_decl(
        &mut self,
        decorators: Vec<Decorator>,
    ) -> Result<Item, ParseError> {
        let kw = self.bump()?; // 'symbol_set'
        let start = kw.span.start;
        let (name, _) = self.parse_path()?;
        self.expect(TokenKind::LBrace, "expected '{' after symbol_set name")?;
        let mut symbols: Vec<SymbolEntry> = Vec::new();
        loop {
            // Comments above this entry, before its (optional) decorators.
            let leading_trivia = self.peek_leading_trivia()?;
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
                            leading_trivia,
                            trailing_comment: None,
                        });
                        // An inline comment after this entry trails it.
                        if let Some(c) = self.take_same_line_comment()?
                            && let Some(last) = symbols.last_mut()
                        {
                            last.trailing_comment = Some(c);
                        }
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
        // Comments on their own lines after the last entry, before `}`.
        let trailing_trivia = rbrace.leading_trivia;
        Ok(Item::SymbolSetDecl(SymbolSetDecl {
            name,
            symbols,
            decorators,
            span: Span::new(start, rbrace.span.end),
            leading_trivia: self.take_item_trivia(),
            trailing_comment: None,
            trailing_trivia,
        }))
    }

    /// Parse `[@decorators] connection NAME : TypeRef -> TypeRef : Path`.
    /// The leading `connection` keyword has not yet been consumed; any
    /// `@decorators` (e.g. `@dynamic`) were parsed by the caller.
    pub(super) fn parse_connection_decl(
        &mut self,
        decorators: Vec<crate::ast::Decorator>,
    ) -> Result<Item, ParseError> {
        let kw = self.bump()?; // `connection`
        let start = decorators.first().map_or(kw.span.start, |d| d.span.start);
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
            decorators,
            span: Span::new(start, end),
            leading_trivia: self.take_item_trivia(),
            trailing_comment: None,
        }))
    }

    /// Parse a connection statement starting at `lhs -> rhs [':' sym]`.
    /// The lhs ident has already been consumed; `lhs_span` covers it.
    pub(super) fn parse_connection_stmt(
        &mut self,
        lhs: String,
        lhs_span: Span,
    ) -> Result<Item, ParseError> {
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
            trailing_comment: None,
        }))
    }

    pub(super) fn parse_field(
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
            trailing_comment: None,
        }))
    }

    /// Parse a `let name = expr` item (top-level or inside a block).
    /// `let` was matched by the dispatcher but not consumed. No
    /// terminator — the value expression ends exactly where a field's
    /// `name = expr` value would, so items stay newline-separated.
    pub(super) fn parse_let_item(&mut self) -> Result<Item, ParseError> {
        let let_tok = self.bump()?; // consume 'let'
        let start = let_tok.span.start;
        let (name, _name_span) = self.bump_ident("expected name after 'let'")?;
        self.expect(TokenKind::Eq, "expected '=' after let name")?;
        let (value, value_span) = self.parse_expr()?;
        let span = Span::new(start, value_span.end);
        Ok(Item::Let(crate::ast::LetItem {
            name,
            value,
            decorators: Vec::new(),
            fn_syntax: false,
            span,
            leading_trivia: self.take_item_trivia(),
            trailing_comment: None,
        }))
    }

    /// Parse a `fn name(params) -> T body` item (top-level or inside a
    /// block). Sugar for `let name = fn(params) -> T body`: resolution,
    /// caching and data-invisibility are exactly the let item's, but the
    /// binding is registered in the symbol index and re-printed in `fn`
    /// form. Decorators (`@doc`) are allowed.
    pub(super) fn parse_fn_item(
        &mut self,
        decorators: Vec<crate::ast::Decorator>,
    ) -> Result<Item, ParseError> {
        let fn_tok = self.bump()?; // consume 'fn'
        let start = decorators
            .first()
            .map(|d| d.span.start)
            .unwrap_or(fn_tok.span.start);
        let (name, _name_span) = self.bump_ident("expected name after 'fn'")?;
        if !matches!(self.peek()?.kind, TokenKind::LParen) {
            let span = self.peek()?.span;
            return Err(self.err(
                "expected '(' after fn name",
                span,
                "a fn item is `fn name(params) -> T body`",
            ));
        }
        let (value, value_span) = self.parse_function_tail(fn_tok.span.start)?;
        let span = Span::new(start, value_span.end);
        Ok(Item::Let(crate::ast::LetItem {
            name,
            value,
            decorators,
            fn_syntax: true,
            span,
            leading_trivia: self.take_item_trivia(),
            trailing_comment: None,
        }))
    }

    pub(super) fn parse_block(
        &mut self,
        kind: String,
        kind_ns: Vec<String>,
        start: usize,
        kind_end: usize,
        decorators: Vec<Decorator>,
    ) -> Result<Item, ParseError> {
        let slot_decl = kind_ns.is_empty()
            && kind == "slot"
            && matches!(self.peek()?.kind, TokenKind::Ident(_))
            && matches!(self.peek2()?.kind, TokenKind::Colon);
        if slot_decl {
            return self.parse_slot_decl(start, decorators);
        }

        let conditional = if matches!(self.peek()?.kind, TokenKind::Question) {
            self.bump()?;
            true
        } else {
            false
        };
        // Labels are value expressions in positional slots. Their types are
        // determined by the schema's `@inline(N)`-decorated fields.
        //
        // The label loop stops on:
        //   * `{` — body coming up (consumed below)
        //   * a token preceded by a newline — block ends here with no body
        //     (the empty-body shorthand: `h1 "Title"`, `hr`, …)
        //   * any non-label token — same as today
        let mut labels: Vec<Expr> = Vec::new();
        let mut end_offset = kind_end;
        loop {
            let p = self.peek()?;
            if matches!(p.kind, TokenKind::LBrace) {
                break;
            }
            // A newline between the previous token (kind or last label) and
            // the next one ends the label list — what follows is a new item,
            // not another label.
            if p.preceded_by_newline {
                break;
            }
            match &p.kind {
                TokenKind::Str(StringLit::Utf8(_))
                | TokenKind::Str(StringLit::Ascii(_))
                | TokenKind::Str(StringLit::Utf16(_))
                | TokenKind::Str(StringLit::Utf32(_))
                // An interpolated `$"…${x}…"` string is a valid label too —
                // e.g. a templated body's `h3 $"${title}"`. It
                // evaluates in the block's scope (see `Block::labels`).
                | TokenKind::Str(StringLit::Interpolated { .. })
                | TokenKind::Number(_)
                | TokenKind::NumberWithUnit(..)
                | TokenKind::Bool(_)
                | TokenKind::Symbol(_)
                | TokenKind::None => {
                    let (expr, span) = self.parse_value_expr()?;
                    end_offset = span.end;
                    labels.push(expr);
                }
                // A bare identifier label may extend across `-`/`/`
                // connectors into one compound identifier (kebab-case
                // class names, path-like page names) — see
                // `parse_label_ident`.
                TokenKind::Ident(_) => {
                    let (expr, span) = self.parse_label_ident()?;
                    end_offset = span.end;
                    labels.push(expr);
                }
                // Any other token (EOF, `}`, decorator `@`, …) ends the
                // label list. The post-loop check below decides whether
                // the block has a body or is empty.
                _ => break,
            }
        }
        // If no `{` follows, this is an empty-body block — `kind` plus any
        // labels collected above. The next item will be parsed by the
        // enclosing `parse_item` loop.
        if !matches!(self.peek()?.kind, TokenKind::LBrace) {
            return Ok(Item::Block(Block {
                kind,
                kind_ns,
                conditional,
                slot_decl: None,
                labels,
                items: Vec::new(),
                decorators,
                span: Span::new(start, end_offset),
                leading_trivia: self.take_item_trivia(),
                trailing_comment: None,
                trailing_trivia: Vec::new(),
            }));
        }
        // Capture this block's leading trivia before parsing the body:
        // every nested `parse_item` overwrites the shared buffer, so
        // draining it after the body would lose the comments/blank lines
        // that precede the block.
        let leading_trivia = self.take_item_trivia();
        self.bump()?; // consume '{'
        self.enter_recursion()?;
        self.block_depth += 1;
        // String-literal field keys are accepted only inside the body of a
        // `@schemaless` block. Reflect the *immediate* enclosing block —
        // saved and restored so a normal block nested in a schemaless one
        // resets the flag, and vice versa.
        let prev_schemaless = self.in_schemaless_block;
        self.in_schemaless_block = decorator_is_schemaless(&decorators);
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
                    _ => {
                        items.push(self.parse_item()?);
                        // An inline comment after this item (carried on
                        // the next token, including the `}`) trails it.
                        self.attach_trailing_to_last(&mut items)?;
                    }
                }
            }
            Ok(items)
        })();
        self.in_schemaless_block = prev_schemaless;
        self.block_depth -= 1;
        self.leave_recursion();
        let items = body_result?;
        let rbrace = self.bump()?;
        // Comments on their own lines after the last item, before `}`.
        let trailing_trivia = rbrace.leading_trivia;
        Ok(Item::Block(Block {
            kind,
            kind_ns,
            conditional,
            slot_decl: None,
            labels,
            items,
            decorators,
            span: Span::new(start, rbrace.span.end),
            leading_trivia,
            trailing_comment: None,
            trailing_trivia,
        }))
    }

    /// Parse the host-neutral slot declaration syntax:
    /// `slot name: Type[? | *] [= default]`.
    ///
    /// It is represented as an ordinary `slot` block so schemas can place
    /// declarations using `@children("slot")`. The type/modifiers live in
    /// side-band AST metadata because a TypeRef is not a value expression;
    /// an inline default is also exposed as the block's `default` field so
    /// existing block APIs and instance-kind derivation can evaluate it.
    fn parse_slot_decl(
        &mut self,
        start: usize,
        decorators: Vec<Decorator>,
    ) -> Result<Item, ParseError> {
        let (name, name_span) = self.bump_ident("expected slot name after 'slot'")?;
        self.expect(TokenKind::Colon, "expected ':' after slot name")?;
        let (ty, ty_span) = self.parse_type_ref()?;
        let mut optional = false;
        let mut repeated = false;
        let mut end = ty_span.end;
        match self.peek()?.kind {
            TokenKind::Question => {
                end = self.bump()?.span.end;
                optional = true;
            }
            TokenKind::Star => {
                end = self.bump()?.span.end;
                repeated = true;
            }
            _ => {}
        }
        let mut items = Vec::new();
        if matches!(self.peek()?.kind, TokenKind::Eq) {
            self.bump()?;
            let (expr, expr_span) = self.parse_expr()?;
            end = expr_span.end;
            items.push(Item::Field(Field {
                name: "default".to_string(),
                expr,
                decorators: Vec::new(),
                span: Span::new(name_span.start, expr_span.end),
                leading_trivia: Vec::new(),
                trailing_comment: None,
            }));
        }
        Ok(Item::Block(Block {
            kind: "slot".to_string(),
            kind_ns: Vec::new(),
            conditional: false,
            slot_decl: Some(crate::ast::SlotDecl {
                ty,
                ty_span,
                optional,
                repeated,
            }),
            labels: vec![Expr::Identifier(name, name_span)],
            items,
            decorators,
            span: Span::new(start, end),
            leading_trivia: self.take_item_trivia(),
            trailing_comment: None,
            trailing_trivia: Vec::new(),
        }))
    }
}
