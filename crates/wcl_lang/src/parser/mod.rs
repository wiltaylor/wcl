mod decls;
mod expr;
mod pattern;
mod types;

use miette::{NamedSource, SourceSpan};

use crate::ast::{Block, Expr, Item, Source, Span};
use crate::error::ParseError;
use crate::lexer::{LexError, Lexer, StringLit, Token, TokenKind};
use crate::symbols::{DuplicateSymbol, SymbolIndex, SymbolKind, SymbolPath, SymbolRecord};

// Re-exports used by the integration-style tests in `parser_tests.rs`
// (included via `mod tests`). Gated to keep lib builds free of unused
// imports.
#[cfg(test)]
use crate::ast::{BinOp, Field, SymbolSetDecl, TypeDecl, UnaryOp, UnionDecl, UseForm, VariantBody};
#[cfg(test)]
use crate::value::TypeRef;

pub struct Parser<'a> {
    file: String,
    /// Pre-built `NamedSource` used for every diagnostic. Cloning a
    /// `NamedSource` is cheap (its body is shared), so this is reused
    /// across all of this parse's error sites instead of reallocating
    /// the source string per call.
    named_src: std::sync::Arc<NamedSource<String>>,
    /// The full source text, retained so `parse_import_decl` can slice
    /// the raw path out of an `import <...>` between the `<` and `>`
    /// token spans (the bracketed path is not a single token).
    src: &'a str,
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
            src,
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

    /// Clone the leading trivia (comments + blank lines) sitting on the
    /// next token, without consuming it. Used to seed a member/element's
    /// `leading_trivia` before parsing it. The lexer already collected
    /// these from the source between the previous token and this one.
    pub(super) fn peek_leading_trivia(&mut self) -> Result<Vec<crate::ast::Trivia>, ParseError> {
        Ok(self.peek()?.leading_trivia.clone())
    }

    /// Take the same-line trailing comment that the lexer diverted onto
    /// the next token (a `#` comment that followed the previous token on
    /// the same line). Consumes it from the peeked token so it cannot
    /// also surface as leading trivia. Returns `None` when there isn't one.
    pub(super) fn take_same_line_comment(&mut self) -> Result<Option<String>, ParseError> {
        self.peek()?;
        Ok(self
            .peeked
            .as_mut()
            .and_then(|t| t.same_line_comment.take()))
    }

    /// Attach the next token's same-line comment (if any) to the most
    /// recently parsed top-level item, so an inline comment stays with
    /// the node that ended its line. Call after pushing each item and at
    /// the loop terminator (the `Eof`/`RBrace` token still carries the
    /// last line's trailing comment).
    pub(super) fn attach_trailing_to_last(&mut self, items: &mut [Item]) -> Result<(), ParseError> {
        if let Some(c) = self.take_same_line_comment()?
            && let Some(last) = items.last_mut()
        {
            last.set_trailing_comment(c);
        }
        Ok(())
    }

    pub fn parse_source(&mut self) -> Result<(Source, SymbolIndex), ParseError> {
        let mut items = Vec::new();
        while !matches!(self.peek()?.kind, TokenKind::Eof) {
            let item_idx = items.len();
            let item = self.parse_item()?;
            self.register_item(&item, item_idx)?;
            items.push(item);
            // The next token's same-line comment (incl. the Eof token's,
            // on the final pass) trails the item we just pushed.
            self.attach_trailing_to_last(&mut items)?;
        }
        // Comments + blank lines after the last item, before EOF.
        let trailing_trivia = self.peek()?.leading_trivia.clone();
        Ok((
            Source {
                items,
                trailing_trivia,
            },
            std::mem::take(&mut self.index),
        ))
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
            // Let bindings are intentionally NOT registered in the symbol
            // index: they must stay out of `Document::field`/`block`/`get`
            // and any query that walks named symbols. Resolution happens by
            // scanning items directly (see `find_let` / `root_let`).
            Item::Let(_) => {}
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
            && matches!(self.peek2()?.kind, TokenKind::Str(_) | TokenKind::Lt)
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
                "let" => {
                    if !decorators.is_empty() {
                        let span = decorators[0].span;
                        return Err(self.err(
                            "decorators are not allowed on let bindings",
                            span,
                            "remove decorator",
                        ));
                    }
                    return self.parse_let_item();
                }
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
            // A namespace-qualified block kind: `wdoc::process` or
            // `foo.bar::process`. The namespace path sits on the left of
            // `::`; a leading dotted/`::` form is otherwise not a valid
            // item, so claiming it here is unambiguous.
            TokenKind::Dot | TokenKind::ColonColon => {
                self.parse_qualified_block(name, span_start, decorators)
            }
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
            | TokenKind::None => {
                self.parse_block(name, Vec::new(), span_start, tok.span.end, decorators)
            }
            _ if next.preceded_by_newline => {
                // Empty-body, label-less block: the kind sits alone on
                // its line. The next token belongs to the next item.
                Ok(Item::Block(Block {
                    kind: name,
                    kind_ns: Vec::new(),
                    labels: Vec::new(),
                    items: Vec::new(),
                    decorators,
                    span: Span::new(span_start, tok.span.end),
                    leading_trivia: self.take_item_trivia(),
                    trailing_comment: None,
                    trailing_trivia: Vec::new(),
                }))
            }
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

    /// Parse a namespace-qualified block kind whose first namespace
    /// segment (`first_seg`) has already been consumed. Accepts
    /// `ns::kind` and `ns.sub::kind` — the segments before `::` form the
    /// namespace path; the ident after `::` is the kind. Delegates to
    /// `parse_block` for labels/body.
    fn parse_qualified_block(
        &mut self,
        first_seg: String,
        start: usize,
        decorators: Vec<crate::ast::Decorator>,
    ) -> Result<Item, ParseError> {
        let mut ns = vec![first_seg];
        loop {
            let p = self.peek()?;
            match &p.kind {
                TokenKind::Dot => {
                    self.bump()?; // '.'
                    let (seg, _) =
                        self.bump_ident("expected identifier after '.' in qualified kind")?;
                    ns.push(seg);
                }
                TokenKind::ColonColon => break,
                other => {
                    let msg = format!(
                        "expected '.' or '::' in qualified block kind, found {}",
                        describe(other)
                    );
                    let span = p.span;
                    return Err(self.err(msg, span, "expected '::'"));
                }
            }
        }
        self.bump()?; // '::'
        let (kind, kind_span) = self.bump_ident("expected kind name after '::'")?;
        self.parse_block(kind, ns, start, kind_span.end, decorators)
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

#[cfg(test)]
#[path = "../parser_tests.rs"]
mod tests;
