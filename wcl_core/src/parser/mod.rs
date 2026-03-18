//! WCL Parser — hand-written recursive descent with Pratt parsing for expressions.
//!
//! Operates on the token stream produced by the lexer.

mod common;
mod expr;
mod types;

use crate::ast::*;
use crate::diagnostic::DiagnosticBag;
use crate::lexer::{Token, TokenKind};
use crate::span::Span;
use crate::trivia::Trivia;

// ── Parser ────────────────────────────────────────────────────────────────────

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    diagnostics: DiagnosticBag,
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser {
            tokens,
            pos: 0,
            diagnostics: DiagnosticBag::new(),
        }
    }

    // ── Token navigation ──────────────────────────────────────────────────

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&self.tokens[self.tokens.len() - 1])
    }

    fn peek_kind(&self) -> &TokenKind {
        &self.peek().kind
    }

    /// Advance and return a clone of the consumed token.
    fn advance(&mut self) -> Token {
        let tok = self.peek().clone();
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    /// Check if the current token's discriminant matches `kind` (ignoring inner values).
    fn at(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(self.peek_kind()) == std::mem::discriminant(kind)
    }

    /// Consume the current token if it matches `kind`; otherwise report an error.
    fn expect(&mut self, kind: &TokenKind) -> Result<Token, ()> {
        if self.at(kind) {
            Ok(self.advance())
        } else {
            self.diagnostics.error_with_code(
                format!("expected {:?}, found {:?}", kind, self.peek_kind()),
                self.current_span(),
                "E002",
            );
            Err(())
        }
    }

    /// Consume an identifier token and return an `Ident`.
    fn expect_ident(&mut self) -> Result<Ident, ()> {
        if let TokenKind::Ident(ref name) = self.peek_kind().clone() {
            let name = name.clone();
            let span = self.current_span();
            self.advance();
            Ok(Ident { name, span })
        } else {
            self.diagnostics.error_with_code(
                format!("expected identifier, found {:?}", self.peek_kind()),
                self.current_span(),
                "E002",
            );
            Err(())
        }
    }

    /// Try to parse an identifier; returns None if not at an Ident token.
    fn try_parse_ident(&mut self) -> Option<Ident> {
        if let TokenKind::Ident(ref name) = self.peek_kind().clone() {
            let name = name.clone();
            let span = self.current_span();
            self.advance();
            Some(Ident { name, span })
        } else {
            None
        }
    }

    fn current_span(&self) -> Span {
        self.peek().span
    }

    /// The span of the token immediately before the current position.
    fn prev_span(&self) -> Span {
        if self.pos > 0 {
            self.tokens[self.pos - 1].span
        } else {
            Span::dummy()
        }
    }

    fn is_at_end(&self) -> bool {
        matches!(self.peek_kind(), TokenKind::Eof)
    }

    // ── Trivia collection ─────────────────────────────────────────────────

    /// Collect comments and count newlines before the next meaningful token.
    fn collect_trivia(&mut self) -> Trivia {
        let mut comments = Vec::new();
        let mut newlines: u32 = 0;

        loop {
            match self.peek_kind() {
                TokenKind::Newline => {
                    newlines += 1;
                    self.advance();
                }
                TokenKind::LineComment(ref text) => {
                    let text = text.clone();
                    let span = self.current_span();
                    self.advance();
                    comments.push(crate::trivia::Comment {
                        text,
                        style: crate::trivia::CommentStyle::Line,
                        placement: crate::trivia::CommentPlacement::Leading,
                        span,
                    });
                }
                TokenKind::BlockComment(ref text) => {
                    let text = text.clone();
                    let span = self.current_span();
                    self.advance();
                    comments.push(crate::trivia::Comment {
                        text,
                        style: crate::trivia::CommentStyle::Block,
                        placement: crate::trivia::CommentPlacement::Leading,
                        span,
                    });
                }
                TokenKind::DocComment(ref text) => {
                    let text = text.clone();
                    let span = self.current_span();
                    self.advance();
                    comments.push(crate::trivia::Comment {
                        text,
                        style: crate::trivia::CommentStyle::Doc,
                        placement: crate::trivia::CommentPlacement::Leading,
                        span,
                    });
                }
                _ => break,
            }
        }

        Trivia {
            comments,
            leading_newlines: newlines,
        }
    }

    /// Skip newlines only (not comments).
    fn skip_newlines(&mut self) {
        while matches!(self.peek_kind(), TokenKind::Newline) {
            self.advance();
        }
    }

    // ── Main entry point ──────────────────────────────────────────────────

    pub fn parse_document(mut self) -> (Document, DiagnosticBag) {
        let start_span = self.current_span();
        let trivia = self.collect_trivia();
        let mut items = Vec::new();

        while !self.is_at_end() {
            if let Some(item) = self.parse_doc_item() {
                items.push(item);
            } else {
                // Skip one token to avoid infinite loop on error
                if !self.is_at_end() {
                    self.advance();
                }
            }
            // Consume any trailing trivia between items
            self.collect_trivia();
        }

        let end_span = self.current_span();
        let span = start_span.merge(end_span);
        let doc = Document {
            items,
            trivia,
            span,
        };
        (doc, self.diagnostics)
    }

    /// Parse a standalone query pipeline (consuming the parser).
    pub fn parse_query_standalone(mut self) -> (Option<QueryPipeline>, DiagnosticBag) {
        let result = self.parse_query_pipeline();
        (result, self.diagnostics)
    }

    // ── Top-level items ───────────────────────────────────────────────────

    fn parse_doc_item(&mut self) -> Option<DocItem> {
        let trivia = self.collect_trivia();

        match self.peek_kind() {
            TokenKind::Import => {
                let imp = self.parse_import(trivia)?;
                Some(DocItem::Import(imp))
            }
            TokenKind::Export => self.parse_export(trivia),
            _ => {
                let body_item = self.parse_body_item_with_trivia(trivia)?;
                Some(DocItem::Body(body_item))
            }
        }
    }

    fn parse_import(&mut self, trivia: Trivia) -> Option<Import> {
        let start_span = self.current_span();
        self.advance(); // consume `import`
        self.skip_newlines();
        let path = self.parse_string_lit()?;
        let span = start_span.merge(path.span);
        Some(Import { path, trivia, span })
    }

    fn parse_export(&mut self, trivia: Trivia) -> Option<DocItem> {
        let start_span = self.current_span();
        self.advance(); // consume `export`
        self.skip_newlines();

        if matches!(self.peek_kind(), TokenKind::Let) {
            // export let name = expr
            self.advance(); // consume `let`
            self.skip_newlines();
            let name = self.expect_ident().ok()?;
            self.skip_newlines();
            if self.expect(&TokenKind::Equals).is_err() {
                return None;
            }
            self.skip_newlines();
            let value = self.parse_expr()?;
            let span = start_span.merge(value.span());
            Some(DocItem::ExportLet(ExportLet {
                name,
                value,
                trivia,
                span,
            }))
        } else {
            // export name (re-export)
            let name = self.expect_ident().ok()?;
            let span = start_span.merge(name.span);
            Some(DocItem::ReExport(ReExport { name, trivia, span }))
        }
    }

    // ── Body items ────────────────────────────────────────────────────────

    fn parse_body_items(&mut self) -> Vec<BodyItem> {
        let mut items = Vec::new();
        let mut seen_attrs: std::collections::HashSet<String> = std::collections::HashSet::new();
        loop {
            let trivia = self.collect_trivia();
            if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                break;
            }
            // E036: export/re-export only allowed at top level
            if matches!(self.peek_kind(), TokenKind::Export) {
                self.diagnostics.error_with_code(
                    "export declarations must be at the top level, not inside blocks",
                    self.current_span(),
                    "E036",
                );
                self.advance(); // skip the export keyword
                // Skip until newline or closing brace
                while !matches!(
                    self.peek_kind(),
                    TokenKind::Newline | TokenKind::RBrace | TokenKind::Eof
                ) {
                    self.advance();
                }
                continue;
            }
            if let Some(item) = self.parse_body_item_with_trivia(trivia) {
                // Duplicate attribute detection (§7.4)
                if let BodyItem::Attribute(ref attr) = item {
                    if !seen_attrs.insert(attr.name.name.clone()) {
                        self.diagnostics.error_with_code(
                            format!("duplicate attribute '{}' in block", attr.name.name),
                            attr.span,
                            "E002",
                        );
                    }
                }
                items.push(item);
            } else {
                // Skip one token to recover
                if !self.is_at_end() && !matches!(self.peek_kind(), TokenKind::RBrace) {
                    self.advance();
                } else {
                    break;
                }
            }
        }
        items
    }

    fn parse_body_item_with_trivia(&mut self, trivia: Trivia) -> Option<BodyItem> {
        // Collect decorators
        let decorators = self.parse_decorators();

        match self.peek_kind().clone() {
            TokenKind::Let => {
                let binding = self.parse_let_binding(decorators, trivia)?;
                Some(BodyItem::LetBinding(binding))
            }
            TokenKind::Partial => {
                // Peek next: table or block
                let mut i = self.pos + 1;
                while i < self.tokens.len()
                    && matches!(self.tokens[i].kind, TokenKind::Newline)
                {
                    i += 1;
                }
                if i < self.tokens.len() && matches!(self.tokens[i].kind, TokenKind::Table) {
                    let table = self.parse_table(decorators, trivia, true)?;
                    Some(BodyItem::Table(table))
                } else {
                    let block = self.parse_block(decorators, trivia, true)?;
                    Some(BodyItem::Block(block))
                }
            }
            TokenKind::Macro => {
                let m = self.parse_macro_def(decorators, trivia)?;
                Some(BodyItem::MacroDef(m))
            }
            TokenKind::For => {
                let f = self.parse_for_loop(trivia)?;
                Some(BodyItem::ForLoop(f))
            }
            TokenKind::If => {
                let c = self.parse_conditional(trivia)?;
                Some(BodyItem::Conditional(c))
            }
            TokenKind::Schema => {
                let s = self.parse_schema(decorators, trivia)?;
                Some(BodyItem::Schema(s))
            }
            TokenKind::DecoratorSchema => {
                let ds = self.parse_decorator_schema(decorators, trivia)?;
                Some(BodyItem::DecoratorSchema(ds))
            }
            TokenKind::Table => {
                let t = self.parse_table(decorators, trivia, false)?;
                Some(BodyItem::Table(t))
            }
            TokenKind::Validation => {
                let v = self.parse_validation(decorators, trivia)?;
                Some(BodyItem::Validation(v))
            }
            TokenKind::Ident(ref name) => {
                let name = name.clone();
                // Disambiguate: attribute (ident =), macro call (ident (), or block (ident ident/string/{)
                let mut i = self.pos + 1;
                // Skip newlines for lookahead
                while i < self.tokens.len()
                    && matches!(self.tokens[i].kind, TokenKind::Newline)
                {
                    i += 1;
                }
                if i < self.tokens.len() {
                    match &self.tokens[i].kind {
                        TokenKind::Equals => {
                            let attr = self.parse_attribute(decorators, trivia)?;
                            Some(BodyItem::Attribute(attr))
                        }
                        TokenKind::LParen => {
                            let mc = self.parse_macro_call(trivia)?;
                            Some(BodyItem::MacroCall(mc))
                        }
                        TokenKind::LBrace
                        | TokenKind::Ident(_)
                        | TokenKind::IdentifierLit(_)
                        | TokenKind::StringLit(_)
                        | TokenKind::InterpStart => {
                            let block = self.parse_block(decorators, trivia, false)?;
                            Some(BodyItem::Block(block))
                        }
                        _ => {
                            // Default to attribute if nothing else matches
                            let attr = self.parse_attribute(decorators, trivia)?;
                            Some(BodyItem::Attribute(attr))
                        }
                    }
                } else {
                    self.diagnostics.error(
                        format!("unexpected end of file after '{}'", name),
                        self.current_span(),
                    );
                    None
                }
            }
            _ => {
                if !decorators.is_empty() {
                    self.diagnostics.error(
                        "decorators must be followed by a declaration",
                        self.current_span(),
                    );
                }
                self.diagnostics.error(
                    format!(
                        "expected body item, found {:?}",
                        self.peek_kind()
                    ),
                    self.current_span(),
                );
                None
            }
        }
    }

    // ── Decorators ────────────────────────────────────────────────────────

    fn parse_decorators(&mut self) -> Vec<Decorator> {
        let mut decorators = Vec::new();
        loop {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::At) {
                if let Some(dec) = self.parse_decorator() {
                    decorators.push(dec);
                }
            } else {
                break;
            }
        }
        decorators
    }

    fn parse_decorator(&mut self) -> Option<Decorator> {
        let start_span = self.current_span();
        self.advance(); // consume @
        let name = self.expect_ident().ok()?;
        let args = if matches!(self.peek_kind(), TokenKind::LParen) {
            self.parse_decorator_args()
        } else {
            Vec::new()
        };
        let end_span = if args.is_empty() {
            name.span
        } else {
            self.prev_span()
        };
        Some(Decorator {
            name,
            args,
            span: start_span.merge(end_span),
        })
    }

    fn parse_decorator_args(&mut self) -> Vec<DecoratorArg> {
        let mut args = Vec::new();
        if self.expect(&TokenKind::LParen).is_err() {
            return args;
        }
        loop {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                break;
            }
            // Try named arg: ident = expr
            if let TokenKind::Ident(ref name) = self.peek_kind().clone() {
                let name = name.clone();
                let mut j = self.pos + 1;
                while j < self.tokens.len()
                    && matches!(self.tokens[j].kind, TokenKind::Newline)
                {
                    j += 1;
                }
                if j < self.tokens.len() && matches!(self.tokens[j].kind, TokenKind::Equals) {
                    let span = self.current_span();
                    self.advance(); // consume ident
                    self.skip_newlines();
                    self.advance(); // consume =
                    self.skip_newlines();
                    if let Some(val) = self.parse_expr() {
                        args.push(DecoratorArg::Named(Ident { name, span }, val));
                    }
                } else {
                    // Positional
                    if let Some(expr) = self.parse_expr() {
                        args.push(DecoratorArg::Positional(expr));
                    }
                }
            } else if let Some(expr) = self.parse_expr() {
                args.push(DecoratorArg::Positional(expr));
            } else {
                break;
            }
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.skip_newlines();
        let _ = self.expect(&TokenKind::RParen);
        args
    }

    // ── Attributes ────────────────────────────────────────────────────────

    fn parse_attribute(
        &mut self,
        decorators: Vec<Decorator>,
        trivia: Trivia,
    ) -> Option<Attribute> {
        let start_span = self.current_span();
        let name = self.expect_ident().ok()?;
        self.skip_newlines();
        if self.expect(&TokenKind::Equals).is_err() {
            return None;
        }
        self.skip_newlines();
        let value = self.parse_expr()?;
        let span = start_span.merge(value.span());
        Some(Attribute {
            decorators,
            name,
            value,
            trivia,
            span,
        })
    }

    // ── Blocks ────────────────────────────────────────────────────────────

    fn parse_block(
        &mut self,
        decorators: Vec<Decorator>,
        trivia: Trivia,
        partial: bool,
    ) -> Option<Block> {
        let start_span = self.current_span();

        if partial {
            self.advance(); // consume `partial`
            self.skip_newlines();
        }

        let kind = self.expect_ident().ok()?;
        self.skip_newlines();

        let inline_id = self.parse_inline_id();
        self.skip_newlines();
        let labels = self.parse_labels();
        self.skip_newlines();

        if self.expect(&TokenKind::LBrace).is_err() {
            return None;
        }

        let body = self.parse_body_items();

        if self.expect(&TokenKind::RBrace).is_err() {
            return None;
        }

        let span = start_span.merge(self.prev_span());
        Some(Block {
            decorators,
            partial,
            kind,
            inline_id,
            labels,
            body,
            trivia,
            span,
        })
    }

    fn parse_inline_id(&mut self) -> Option<InlineId> {
        // Check if the token sequence starting at current position forms an
        // interpolated inline ID (contains `${...}` segments joined by hyphens).
        if let Some(interp) = self.try_parse_interpolated_inline_id() {
            return Some(interp);
        }

        match self.peek_kind().clone() {
            TokenKind::IdentifierLit(ref val) => {
                let val = val.clone();
                let span = self.current_span();
                self.advance();
                Some(InlineId::Literal(IdentifierLit { value: val, span }))
            }
            TokenKind::Ident(ref name) => {
                // Only consume as inline ID if not followed by `=` (which would mean attribute)
                // and not a keyword that starts a new item
                let name_clone = name.clone();
                if self.is_keyword_token(&TokenKind::Ident(name_clone.clone())) {
                    return None;
                }
                // Look ahead: if followed by `{`, `"string"`, or another ident, this is an inline ID
                let mut i = self.pos + 1;
                while i < self.tokens.len()
                    && matches!(self.tokens[i].kind, TokenKind::Newline)
                {
                    i += 1;
                }
                if i < self.tokens.len() {
                    match &self.tokens[i].kind {
                        TokenKind::LBrace | TokenKind::StringLit(_) => {
                            let span = self.current_span();
                            self.advance();
                            Some(InlineId::Literal(IdentifierLit {
                                value: name_clone,
                                span,
                            }))
                        }
                        _ => None,
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// Try to parse an interpolated inline ID like `svc-${name}` or `${name}-api`.
    ///
    /// The lexer splits these into multiple tokens, e.g.:
    ///   `svc-${name}` → Ident("svc"), Minus, InterpStart, Ident("name"), RBrace
    ///   `svc-api-${name}` → IdentifierLit("svc-api"), Minus, InterpStart, Ident("name"), RBrace
    ///
    /// This method scans ahead to detect the pattern, and if the interpolated ID
    /// is followed by `{` or a string literal (indicating it's truly an inline ID,
    /// not an expression), it consumes the tokens and returns `InlineId::Interpolated`.
    fn try_parse_interpolated_inline_id(&mut self) -> Option<InlineId> {
        // Scan ahead (without consuming) to see if this forms an interpolated ID.
        // An interpolated ID is a sequence of:
        //   (Ident|IdentifierLit)? ( Minus? InterpStart ... RBrace (Minus (Ident|IdentifierLit))? )*
        // that contains at least one InterpStart.

        let start = self.pos;
        let mut i = start;
        let mut has_interp = false;

        // Optionally start with an identifier
        if i < self.tokens.len() {
            match &self.tokens[i].kind {
                TokenKind::Ident(name) => {
                    if self.is_keyword_token(&TokenKind::Ident(name.clone())) {
                        return None;
                    }
                    i += 1;
                }
                TokenKind::IdentifierLit(_) => {
                    i += 1;
                }
                TokenKind::InterpStart => {
                    // starts directly with interpolation, handled below
                }
                _ => return None,
            }
        }

        // Now scan the rest: alternating Minus and (InterpStart..RBrace | Ident/IdentifierLit)
        loop {
            if i >= self.tokens.len() {
                break;
            }

            // Check for Minus followed by InterpStart or ident
            if matches!(self.tokens[i].kind, TokenKind::Minus) {
                let after_minus = i + 1;
                if after_minus < self.tokens.len() {
                    match &self.tokens[after_minus].kind {
                        TokenKind::InterpStart => {
                            i = after_minus; // will be handled below
                        }
                        TokenKind::Ident(_) | TokenKind::IdentifierLit(_) => {
                            // literal suffix after hyphen, e.g., `-suffix`
                            i = after_minus + 1;
                            continue;
                        }
                        _ => break,
                    }
                } else {
                    break;
                }
            }

            // Check for InterpStart
            if i < self.tokens.len() && matches!(self.tokens[i].kind, TokenKind::InterpStart) {
                has_interp = true;
                i += 1; // past InterpStart
                // Scan for matching RBrace (with nesting)
                let mut depth = 1;
                while i < self.tokens.len() && depth > 0 {
                    match &self.tokens[i].kind {
                        TokenKind::LBrace => depth += 1,
                        TokenKind::RBrace => depth -= 1,
                        TokenKind::Eof => break,
                        _ => {}
                    }
                    i += 1;
                }
                if depth != 0 {
                    return None; // unmatched braces
                }
                continue;
            }

            break;
        }

        if !has_interp {
            return None;
        }

        // `i` now points past the last token of the interpolated ID.
        // Check that what follows (skipping newlines) is LBrace or StringLit.
        let mut check = i;
        while check < self.tokens.len()
            && matches!(self.tokens[check].kind, TokenKind::Newline)
        {
            check += 1;
        }
        if check >= self.tokens.len() {
            return None;
        }
        match &self.tokens[check].kind {
            TokenKind::LBrace | TokenKind::StringLit(_) => {}
            _ => return None,
        }

        // Now actually consume and build the parts.
        let end_pos = i;
        let mut parts: Vec<StringPart> = Vec::new();

        while self.pos < end_pos {
            match self.peek_kind().clone() {
                TokenKind::Ident(ref name) => {
                    parts.push(StringPart::Literal(name.clone()));
                    self.advance();
                }
                TokenKind::IdentifierLit(ref val) => {
                    parts.push(StringPart::Literal(val.clone()));
                    self.advance();
                }
                TokenKind::Minus => {
                    // Hyphen between parts — append to last literal or create new one
                    if let Some(StringPart::Literal(ref mut s)) = parts.last_mut() {
                        s.push('-');
                    } else {
                        parts.push(StringPart::Literal("-".to_string()));
                    }
                    self.advance();
                }
                TokenKind::InterpStart => {
                    self.advance(); // consume `${`
                    // Parse expression inside the interpolation
                    if let Some(expr) = self.parse_expr() {
                        parts.push(StringPart::Interpolation(Box::new(expr)));
                    }
                    // Consume the closing `}`
                    let _ = self.expect(&TokenKind::RBrace);
                }
                _ => {
                    // Unexpected token — skip it to avoid infinite loop
                    self.advance();
                }
            }
        }

        // Merge adjacent literals
        let mut merged: Vec<StringPart> = Vec::new();
        for part in parts {
            match part {
                StringPart::Literal(s) => {
                    if let Some(StringPart::Literal(ref mut last)) = merged.last_mut() {
                        last.push_str(&s);
                    } else {
                        merged.push(StringPart::Literal(s));
                    }
                }
                other => merged.push(other),
            }
        }

        Some(InlineId::Interpolated(merged))
    }

    fn is_keyword_token(&self, _kind: &TokenKind) -> bool {
        // The lexer already distinguishes keywords from idents,
        // so any Ident token is NOT a keyword.
        false
    }

    fn parse_labels(&mut self) -> Vec<StringLit> {
        let mut labels = Vec::new();
        loop {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::StringLit(_)) {
                // Check that this is followed by another string or `{` (i.e., it's a label, not a value)
                if let Some(s) = self.parse_string_lit() {
                    labels.push(s);
                } else {
                    break;
                }
            } else {
                break;
            }
        }
        labels
    }

    // ── Let bindings ──────────────────────────────────────────────────────

    pub(crate) fn parse_let_binding(&mut self, decorators: Vec<Decorator>, trivia: Trivia) -> Option<LetBinding> {
        let start_span = self.current_span();
        self.advance(); // consume `let`
        self.skip_newlines();
        let name = self.expect_ident().ok()?;
        self.skip_newlines();
        if self.expect(&TokenKind::Equals).is_err() {
            return None;
        }
        self.skip_newlines();
        let value = self.parse_expr()?;
        let span = start_span.merge(value.span());
        Some(LetBinding {
            decorators,
            name,
            value,
            trivia,
            span,
        })
    }

    // ── Tables ────────────────────────────────────────────────────────────

    fn parse_table(
        &mut self,
        decorators: Vec<Decorator>,
        trivia: Trivia,
        partial: bool,
    ) -> Option<Table> {
        let start_span = self.current_span();

        if partial {
            self.advance(); // consume `partial`
            self.skip_newlines();
        }

        self.advance(); // consume `table`
        self.skip_newlines();

        let inline_id = self.parse_inline_id();
        self.skip_newlines();
        let labels = self.parse_labels();
        self.skip_newlines();

        if self.expect(&TokenKind::LBrace).is_err() {
            return None;
        }

        let mut columns = Vec::new();
        let mut rows = Vec::new();

        // Parse column declarations and rows
        loop {
            let _trivia = self.collect_trivia();
            if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                break;
            }

            if matches!(self.peek_kind(), TokenKind::Pipe) {
                // Table row
                if let Some(row) = self.parse_table_row() {
                    rows.push(row);
                } else {
                    break;
                }
            } else {
                // Column declaration
                if let Some(col) = self.parse_column_decl() {
                    columns.push(col);
                } else {
                    break;
                }
            }
        }

        if self.expect(&TokenKind::RBrace).is_err() {
            return None;
        }

        let span = start_span.merge(self.prev_span());
        Some(Table {
            decorators,
            partial,
            inline_id,
            labels,
            columns,
            rows,
            trivia,
            span,
        })
    }

    fn parse_column_decl(&mut self) -> Option<ColumnDecl> {
        let trivia = self.collect_trivia();
        let decorators = self.parse_decorators();
        let start_span = self.current_span();
        let name = self.expect_ident().ok()?;
        self.skip_newlines();
        if self.expect(&TokenKind::Colon).is_err() {
            return None;
        }
        self.skip_newlines();
        let type_expr = self.parse_type_expr()?;
        let span = start_span.merge(type_expr.span());
        Some(ColumnDecl {
            decorators,
            name,
            type_expr,
            trivia,
            span,
        })
    }

    fn parse_table_row(&mut self) -> Option<TableRow> {
        let start_span = self.current_span();
        self.advance(); // consume leading |
        let mut cells = Vec::new();
        loop {
            self.skip_newlines();
            if matches!(
                self.peek_kind(),
                TokenKind::Pipe | TokenKind::Newline | TokenKind::RBrace | TokenKind::Eof
            ) {
                // End of row — check if pipe is trailing
                if matches!(self.peek_kind(), TokenKind::Pipe) {
                    // Look ahead: if next after pipe is newline/rbrace/eof, it's trailing
                    let mut j = self.pos + 1;
                    while j < self.tokens.len()
                        && matches!(self.tokens[j].kind, TokenKind::Newline)
                    {
                        j += 1;
                    }
                    if j >= self.tokens.len()
                        || matches!(
                            self.tokens[j].kind,
                            TokenKind::RBrace | TokenKind::Eof | TokenKind::Pipe
                        )
                    {
                        // Trailing pipe — end of row
                        self.advance(); // consume trailing |
                        break;
                    }
                    // Not trailing — it's a cell separator
                    self.advance(); // consume |
                    continue;
                }
                break;
            }
            if let Some(expr) = self.parse_expr() {
                cells.push(expr);
            } else {
                break;
            }
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::Pipe) {
                self.advance(); // consume cell separator |
                // Check if this pipe is followed by newline/rbrace/eof — then it's the trailing pipe
                let mut j = self.pos;
                while j < self.tokens.len()
                    && matches!(self.tokens[j].kind, TokenKind::Newline)
                {
                    j += 1;
                }
                if j >= self.tokens.len()
                    || matches!(
                        self.tokens[j].kind,
                        TokenKind::RBrace | TokenKind::Eof | TokenKind::Pipe
                    )
                {
                    break; // trailing pipe, row is done
                }
            } else {
                break;
            }
        }
        let span = start_span.merge(self.prev_span());
        Some(TableRow { cells, span })
    }

    // ── Schemas ───────────────────────────────────────────────────────────

    fn parse_schema(
        &mut self,
        decorators: Vec<Decorator>,
        trivia: Trivia,
    ) -> Option<Schema> {
        let start_span = self.current_span();
        self.advance(); // consume `schema`
        self.skip_newlines();
        let name = self.parse_string_lit()?;
        self.skip_newlines();
        if self.expect(&TokenKind::LBrace).is_err() {
            return None;
        }

        let mut fields = Vec::new();
        loop {
            let _trivia = self.collect_trivia();
            if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                break;
            }
            if let Some(field) = self.parse_schema_field() {
                fields.push(field);
            } else {
                break;
            }
        }

        if self.expect(&TokenKind::RBrace).is_err() {
            return None;
        }

        let span = start_span.merge(self.prev_span());
        Some(Schema {
            decorators,
            name,
            fields,
            trivia,
            span,
        })
    }

    fn parse_schema_field(&mut self) -> Option<SchemaField> {
        let trivia = self.collect_trivia();
        let decorators_before = self.parse_decorators();
        let start_span = self.current_span();
        let name = self.expect_ident().ok()?;
        self.skip_newlines();
        if self.expect(&TokenKind::Colon).is_err() {
            return None;
        }
        self.skip_newlines();
        let type_expr = self.parse_type_expr()?;
        // Optional decorators after the type
        let decorators_after = self.parse_decorators();
        let span = start_span.merge(self.prev_span());
        Some(SchemaField {
            decorators_before,
            name,
            type_expr,
            decorators_after,
            trivia,
            span,
        })
    }

    fn parse_decorator_schema(
        &mut self,
        decorators: Vec<Decorator>,
        trivia: Trivia,
    ) -> Option<DecoratorSchema> {
        let start_span = self.current_span();
        self.advance(); // consume `decorator_schema`
        self.skip_newlines();
        let name = self.parse_string_lit()?;
        self.skip_newlines();
        if self.expect(&TokenKind::LBrace).is_err() {
            return None;
        }

        // Parse target = [...]
        let mut target = Vec::new();
        let mut fields = Vec::new();

        loop {
            let _trivia = self.collect_trivia();
            if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                break;
            }
            // Check for target = [...]
            if let TokenKind::Ident(ref n) = self.peek_kind().clone() {
                if n == "target" {
                    // Lookahead for =
                    let mut j = self.pos + 1;
                    while j < self.tokens.len()
                        && matches!(self.tokens[j].kind, TokenKind::Newline)
                    {
                        j += 1;
                    }
                    if j < self.tokens.len()
                        && matches!(self.tokens[j].kind, TokenKind::Equals)
                    {
                        self.advance(); // consume `target`
                        self.skip_newlines();
                        self.advance(); // consume `=`
                        self.skip_newlines();
                        if self.expect(&TokenKind::LBracket).is_err() {
                            return None;
                        }
                        loop {
                            self.skip_newlines();
                            if matches!(
                                self.peek_kind(),
                                TokenKind::RBracket | TokenKind::Eof
                            ) {
                                break;
                            }
                            if let Some(t) = self.parse_decorator_target() {
                                target.push(t);
                            }
                            self.skip_newlines();
                            if matches!(self.peek_kind(), TokenKind::Comma) {
                                self.advance();
                            } else {
                                break;
                            }
                        }
                        self.skip_newlines();
                        let _ = self.expect(&TokenKind::RBracket);
                        continue;
                    }
                }
            }
            // Otherwise, schema field
            if let Some(field) = self.parse_schema_field() {
                fields.push(field);
            } else {
                break;
            }
        }

        if self.expect(&TokenKind::RBrace).is_err() {
            return None;
        }

        let span = start_span.merge(self.prev_span());
        Some(DecoratorSchema {
            decorators,
            name,
            target,
            fields,
            trivia,
            span,
        })
    }

    fn parse_decorator_target(&mut self) -> Option<DecoratorTarget> {
        if let TokenKind::Ident(ref name) = self.peek_kind().clone() {
            let result = match name.as_str() {
                "block" => Some(DecoratorTarget::Block),
                "attribute" => Some(DecoratorTarget::Attribute),
                "table" => Some(DecoratorTarget::Table),
                "schema" => Some(DecoratorTarget::Schema),
                other => {
                    self.diagnostics.error(
                        format!("unknown decorator target: {}", other),
                        self.current_span(),
                    );
                    None
                }
            };
            self.advance();
            result
        } else {
            self.diagnostics.error(
                "expected decorator target (block, attribute, table, schema)",
                self.current_span(),
            );
            None
        }
    }

    // ── Macros ────────────────────────────────────────────────────────────

    fn parse_macro_def(
        &mut self,
        decorators: Vec<Decorator>,
        trivia: Trivia,
    ) -> Option<MacroDef> {
        let start_span = self.current_span();
        self.advance(); // consume `macro`
        self.skip_newlines();

        // Determine kind: function macro or attribute macro (@name)
        let (kind, name) = if matches!(self.peek_kind(), TokenKind::At) {
            self.advance(); // consume @
            let name = self.expect_ident().ok()?;
            (MacroKind::Attribute, name)
        } else {
            let name = self.expect_ident().ok()?;
            (MacroKind::Function, name)
        };

        self.skip_newlines();
        let params = self.parse_macro_params();
        self.skip_newlines();

        if self.expect(&TokenKind::LBrace).is_err() {
            return None;
        }

        let body = match kind {
            MacroKind::Function => MacroBody::Function(self.parse_body_items()),
            MacroKind::Attribute => MacroBody::Attribute(self.parse_transform_body()),
        };

        if self.expect(&TokenKind::RBrace).is_err() {
            return None;
        }

        let span = start_span.merge(self.prev_span());
        Some(MacroDef {
            decorators,
            kind,
            name,
            params,
            body,
            trivia,
            span,
        })
    }

    fn parse_macro_params(&mut self) -> Vec<MacroParam> {
        let mut params = Vec::new();
        if self.expect(&TokenKind::LParen).is_err() {
            return params;
        }
        loop {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                break;
            }
            let start_span = self.current_span();
            let name = match self.expect_ident() {
                Ok(n) => n,
                Err(()) => break,
            };

            // Optional type constraint: `: type`
            let type_constraint = if matches!(self.peek_kind(), TokenKind::Colon) {
                self.advance();
                self.skip_newlines();
                self.parse_type_expr()
            } else {
                None
            };

            // Optional default: `= expr`
            let default = if matches!(self.peek_kind(), TokenKind::Equals) {
                self.advance();
                self.skip_newlines();
                self.parse_expr()
            } else {
                None
            };

            let span = start_span.merge(self.prev_span());
            params.push(MacroParam {
                name,
                type_constraint,
                default,
                span,
            });

            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.skip_newlines();
        let _ = self.expect(&TokenKind::RParen);
        params
    }

    fn parse_transform_body(&mut self) -> Vec<TransformDirective> {
        let mut directives = Vec::new();
        loop {
            let _trivia = self.collect_trivia();
            if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                break;
            }
            if let Some(d) = self.parse_transform_directive() {
                directives.push(d);
            } else if !self.is_at_end() && !matches!(self.peek_kind(), TokenKind::RBrace) {
                self.advance();
            } else {
                break;
            }
        }
        directives
    }

    fn parse_transform_directive(&mut self) -> Option<TransformDirective> {
        self.skip_newlines();
        match self.peek_kind().clone() {
            TokenKind::Inject => {
                let start_span = self.current_span();
                self.advance(); // consume `inject`
                self.skip_newlines();
                if self.expect(&TokenKind::LBrace).is_err() {
                    return None;
                }
                let body = self.parse_body_items();
                if self.expect(&TokenKind::RBrace).is_err() {
                    return None;
                }
                let span = start_span.merge(self.prev_span());
                Some(TransformDirective::Inject(InjectBlock { body, span }))
            }
            TokenKind::Set => {
                let start_span = self.current_span();
                self.advance(); // consume `set`
                self.skip_newlines();
                if self.expect(&TokenKind::LBrace).is_err() {
                    return None;
                }
                let mut attrs = Vec::new();
                loop {
                    let trivia = self.collect_trivia();
                    if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                        break;
                    }
                    let decorators = self.parse_decorators();
                    if let Some(attr) = self.parse_attribute(decorators, trivia) {
                        attrs.push(attr);
                    } else {
                        break;
                    }
                }
                if self.expect(&TokenKind::RBrace).is_err() {
                    return None;
                }
                let span = start_span.merge(self.prev_span());
                Some(TransformDirective::Set(SetBlock { attrs, span }))
            }
            TokenKind::Remove => {
                let start_span = self.current_span();
                self.advance(); // consume `remove`
                self.skip_newlines();
                if self.expect(&TokenKind::LBracket).is_err() {
                    return None;
                }
                let mut names = Vec::new();
                loop {
                    self.skip_newlines();
                    if matches!(self.peek_kind(), TokenKind::RBracket | TokenKind::Eof)
                    {
                        break;
                    }
                    if let Ok(ident) = self.expect_ident() {
                        names.push(ident);
                    } else {
                        break;
                    }
                    self.skip_newlines();
                    if matches!(self.peek_kind(), TokenKind::Comma) {
                        self.advance();
                    } else {
                        break;
                    }
                }
                self.skip_newlines();
                let _ = self.expect(&TokenKind::RBracket);
                let span = start_span.merge(self.prev_span());
                Some(TransformDirective::Remove(RemoveBlock { names, span }))
            }
            TokenKind::When => {
                let start_span = self.current_span();
                self.advance(); // consume `when`
                self.skip_newlines();
                let condition = self.parse_expr()?;
                self.skip_newlines();
                if self.expect(&TokenKind::LBrace).is_err() {
                    return None;
                }
                let directives = self.parse_transform_body();
                if self.expect(&TokenKind::RBrace).is_err() {
                    return None;
                }
                let span = start_span.merge(self.prev_span());
                Some(TransformDirective::When(WhenBlock {
                    condition,
                    directives,
                    span,
                }))
            }
            _ => {
                self.diagnostics.error(
                    format!(
                        "expected transform directive (inject/set/remove/when), found {:?}",
                        self.peek_kind()
                    ),
                    self.current_span(),
                );
                None
            }
        }
    }

    // ── Macro calls ───────────────────────────────────────────────────────

    fn parse_macro_call(&mut self, trivia: Trivia) -> Option<MacroCall> {
        let start_span = self.current_span();
        let name = self.expect_ident().ok()?;
        self.skip_newlines();

        // Parse args like call_args but for MacroCallArg
        let mut args = Vec::new();
        if self.expect(&TokenKind::LParen).is_err() {
            return None;
        }
        loop {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                break;
            }
            // Try named arg: ident = expr
            if let TokenKind::Ident(ref arg_name) = self.peek_kind().clone() {
                let arg_name = arg_name.clone();
                let mut j = self.pos + 1;
                while j < self.tokens.len()
                    && matches!(self.tokens[j].kind, TokenKind::Newline)
                {
                    j += 1;
                }
                if j < self.tokens.len() && matches!(self.tokens[j].kind, TokenKind::Equals) {
                    let span = self.current_span();
                    self.advance(); // consume ident
                    self.skip_newlines();
                    self.advance(); // consume =
                    self.skip_newlines();
                    if let Some(val) = self.parse_expr() {
                        args.push(MacroCallArg::Named(
                            Ident {
                                name: arg_name,
                                span,
                            },
                            val,
                        ));
                    }
                } else if let Some(expr) = self.parse_expr() {
                    args.push(MacroCallArg::Positional(expr));
                }
            } else if let Some(expr) = self.parse_expr() {
                args.push(MacroCallArg::Positional(expr));
            } else {
                break;
            }
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::Comma) {
                self.advance();
            } else {
                break;
            }
        }
        self.skip_newlines();
        let _ = self.expect(&TokenKind::RParen);
        let span = start_span.merge(self.prev_span());
        Some(MacroCall {
            name,
            args,
            trivia,
            span,
        })
    }

    // ── Control flow ──────────────────────────────────────────────────────

    fn parse_for_loop(&mut self, trivia: Trivia) -> Option<ForLoop> {
        let start_span = self.current_span();
        self.advance(); // consume `for`
        self.skip_newlines();
        let iterator = self.expect_ident().ok()?;

        // Optional index: `, index`
        let index = if matches!(self.peek_kind(), TokenKind::Comma) {
            self.advance();
            self.skip_newlines();
            Some(self.expect_ident().ok()?)
        } else {
            None
        };

        self.skip_newlines();
        if self.expect(&TokenKind::In).is_err() {
            return None;
        }
        self.skip_newlines();
        let iterable = self.parse_expr()?;
        self.skip_newlines();
        if self.expect(&TokenKind::LBrace).is_err() {
            return None;
        }
        let body = self.parse_body_items();
        if self.expect(&TokenKind::RBrace).is_err() {
            return None;
        }
        let span = start_span.merge(self.prev_span());
        Some(ForLoop {
            iterator,
            index,
            iterable,
            body,
            trivia,
            span,
        })
    }

    fn parse_conditional(&mut self, trivia: Trivia) -> Option<Conditional> {
        let start_span = self.current_span();
        self.advance(); // consume `if`
        self.skip_newlines();
        let condition = self.parse_expr()?;
        self.skip_newlines();
        if self.expect(&TokenKind::LBrace).is_err() {
            return None;
        }
        let then_body = self.parse_body_items();
        if self.expect(&TokenKind::RBrace).is_err() {
            return None;
        }
        self.skip_newlines();

        let else_branch = if matches!(self.peek_kind(), TokenKind::Else) {
            self.advance(); // consume `else`
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::If) {
                // else if
                let trivia_inner = self.collect_trivia();
                let nested = self.parse_conditional(trivia_inner)?;
                Some(ElseBranch::ElseIf(Box::new(nested)))
            } else {
                // else { ... }
                let else_trivia = self.collect_trivia();
                let else_start = self.current_span();
                if self.expect(&TokenKind::LBrace).is_err() {
                    return None;
                }
                let else_body = self.parse_body_items();
                if self.expect(&TokenKind::RBrace).is_err() {
                    return None;
                }
                let else_span = else_start.merge(self.prev_span());
                Some(ElseBranch::Else(else_body, else_trivia, else_span))
            }
        } else {
            None
        };

        let span = start_span.merge(self.prev_span());
        Some(Conditional {
            condition,
            then_body,
            else_branch,
            trivia,
            span,
        })
    }

    // ── Validation ────────────────────────────────────────────────────────

    fn parse_validation(
        &mut self,
        decorators: Vec<Decorator>,
        trivia: Trivia,
    ) -> Option<Validation> {
        let start_span = self.current_span();
        self.advance(); // consume `validation`
        self.skip_newlines();
        let name = self.parse_string_lit()?;
        self.skip_newlines();
        if self.expect(&TokenKind::LBrace).is_err() {
            return None;
        }

        let mut lets = Vec::new();
        let mut check = None;
        let mut message = None;

        loop {
            let inner_trivia = self.collect_trivia();
            if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                break;
            }
            if matches!(self.peek_kind(), TokenKind::Let) {
                if let Some(binding) = self.parse_let_binding(vec![], inner_trivia) {
                    lets.push(binding);
                }
            } else if let TokenKind::Ident(ref n) = self.peek_kind().clone() {
                let n = n.clone();
                match n.as_str() {
                    "check" => {
                        self.advance();
                        self.skip_newlines();
                        if self.expect(&TokenKind::Equals).is_err() {
                            break;
                        }
                        self.skip_newlines();
                        check = self.parse_expr();
                    }
                    "message" => {
                        self.advance();
                        self.skip_newlines();
                        if self.expect(&TokenKind::Equals).is_err() {
                            break;
                        }
                        self.skip_newlines();
                        message = self.parse_expr();
                    }
                    _ => {
                        self.diagnostics.error(
                            format!(
                                "unexpected identifier '{}' in validation block",
                                n
                            ),
                            self.current_span(),
                        );
                        self.advance();
                    }
                }
            } else {
                self.diagnostics.error(
                    "expected 'let', 'check', or 'message' in validation block",
                    self.current_span(),
                );
                self.advance();
            }
        }

        if self.expect(&TokenKind::RBrace).is_err() {
            return None;
        }

        let check = match check {
            Some(c) => c,
            None => {
                self.diagnostics.error(
                    "validation block missing 'check' field",
                    start_span,
                );
                return None;
            }
        };
        let message = match message {
            Some(m) => m,
            None => {
                self.diagnostics.error(
                    "validation block missing 'message' field",
                    start_span,
                );
                return None;
            }
        };

        let span = start_span.merge(self.prev_span());
        Some(Validation {
            decorators,
            name,
            lets,
            check,
            message,
            trivia,
            span,
        })
    }

    // ── Strings ───────────────────────────────────────────────────────────

    /// Parse a string literal token, handling interpolation.
    pub(crate) fn parse_string_lit(&mut self) -> Option<StringLit> {
        match self.peek_kind().clone() {
            TokenKind::StringLit(ref content) => {
                let content = content.clone();
                let span = self.current_span();
                self.advance();
                let parts = Self::parse_string_interpolation(&content, span);
                Some(StringLit { parts, span })
            }
            TokenKind::Heredoc {
                ref content,
                indented: _,
                raw,
            } => {
                let content = content.clone();
                let span = self.current_span();
                self.advance();
                let parts = if raw {
                    vec![StringPart::Literal(content)]
                } else {
                    Self::parse_string_interpolation(&content, span)
                };
                Some(StringLit { parts, span })
            }
            _ => {
                self.diagnostics.error(
                    format!(
                        "expected string literal, found {:?}",
                        self.peek_kind()
                    ),
                    self.current_span(),
                );
                None
            }
        }
    }

    /// Parse `${...}` interpolation sequences within a string.
    fn parse_string_interpolation(raw: &str, _span: Span) -> Vec<StringPart> {
        let mut parts = Vec::new();
        let mut current = String::new();
        let mut chars = raw.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '$' && chars.peek() == Some(&'{') {
                chars.next(); // consume {
                // Save any accumulated literal text
                if !current.is_empty() {
                    parts.push(StringPart::Literal(std::mem::take(&mut current)));
                }
                // Collect expression text between ${ and matching }
                let mut depth = 1;
                let mut expr_text = String::new();
                for ec in chars.by_ref() {
                    if ec == '{' {
                        depth += 1;
                        expr_text.push(ec);
                    } else if ec == '}' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        expr_text.push(ec);
                    } else {
                        expr_text.push(ec);
                    }
                }
                // Lex and parse the expression text
                let file_id = _span.file;
                match crate::lexer::lex(&expr_text, file_id) {
                    Ok(tokens) => {
                        let mut sub_parser = Parser::new(tokens);
                        if let Some(expr) = sub_parser.parse_expr() {
                            parts.push(StringPart::Interpolation(Box::new(expr)));
                        } else {
                            // Fallback: put it as literal
                            parts.push(StringPart::Literal(format!("${{{}}}", expr_text)));
                        }
                    }
                    Err(_) => {
                        parts.push(StringPart::Literal(format!("${{{}}}", expr_text)));
                    }
                }
            } else {
                current.push(c);
            }
        }

        if !current.is_empty() {
            parts.push(StringPart::Literal(current));
        }

        if parts.is_empty() {
            parts.push(StringPart::Literal(String::new()));
        }

        parts
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostic::DiagnosticBag;
    use crate::lexer;
    use crate::span::FileId;

    fn parse(input: &str) -> (Document, DiagnosticBag) {
        let file_id = FileId(0);
        let tokens = lexer::lex(input, file_id).unwrap_or_else(|_diags| {
            // If lexing fails, return just an EOF token
            vec![Token {
                kind: TokenKind::Eof,
                span: Span::new(file_id, 0, 0),
            }]
        });
        let parser = Parser::new(tokens);
        parser.parse_document()
    }

    #[test]
    fn test_empty_document() {
        let (doc, diags) = parse("");
        assert!(doc.items.is_empty());
        assert!(!diags.has_errors());
    }

    #[test]
    fn test_simple_attribute() {
        let (doc, diags) = parse("port = 8080");
        assert!(!diags.has_errors(), "diagnostics: {:?}", diags.diagnostics());
        assert_eq!(doc.items.len(), 1);
        match &doc.items[0] {
            DocItem::Body(BodyItem::Attribute(attr)) => {
                assert_eq!(attr.name.name, "port");
                match &attr.value {
                    Expr::IntLit(val, _) => assert_eq!(*val, 8080),
                    other => panic!("expected IntLit, got {:?}", other),
                }
            }
            other => panic!("expected Attribute, got {:?}", other),
        }
    }

    #[test]
    fn test_simple_block() {
        let (doc, diags) = parse("config {\n  port = 8080\n}");
        assert!(!diags.has_errors(), "diagnostics: {:?}", diags.diagnostics());
        assert_eq!(doc.items.len(), 1);
        match &doc.items[0] {
            DocItem::Body(BodyItem::Block(block)) => {
                assert_eq!(block.kind.name, "config");
                assert!(block.inline_id.is_none());
                assert_eq!(block.body.len(), 1);
            }
            other => panic!("expected Block, got {:?}", other),
        }
    }

    #[test]
    fn test_block_with_id() {
        let (doc, diags) = parse("service svc-api {\n  port = 8080\n}");
        assert!(!diags.has_errors(), "diagnostics: {:?}", diags.diagnostics());
        assert_eq!(doc.items.len(), 1);
        match &doc.items[0] {
            DocItem::Body(BodyItem::Block(block)) => {
                assert_eq!(block.kind.name, "service");
                match &block.inline_id {
                    Some(InlineId::Literal(id)) => assert_eq!(id.value, "svc-api"),
                    other => panic!("expected InlineId::Literal, got {:?}", other),
                }
                assert_eq!(block.body.len(), 1);
            }
            other => panic!("expected Block, got {:?}", other),
        }
    }

    #[test]
    fn test_let_binding() {
        let (doc, diags) = parse("let x = 42");
        assert!(!diags.has_errors(), "diagnostics: {:?}", diags.diagnostics());
        assert_eq!(doc.items.len(), 1);
        match &doc.items[0] {
            DocItem::Body(BodyItem::LetBinding(binding)) => {
                assert_eq!(binding.name.name, "x");
                match &binding.value {
                    Expr::IntLit(val, _) => assert_eq!(*val, 42),
                    other => panic!("expected IntLit, got {:?}", other),
                }
            }
            other => panic!("expected LetBinding, got {:?}", other),
        }
    }

    #[test]
    fn test_arithmetic_expr() {
        // a + b * c should parse as a + (b * c) due to precedence
        let (doc, diags) = parse("result = a + b * c");
        assert!(!diags.has_errors(), "diagnostics: {:?}", diags.diagnostics());
        assert_eq!(doc.items.len(), 1);
        match &doc.items[0] {
            DocItem::Body(BodyItem::Attribute(attr)) => {
                assert_eq!(attr.name.name, "result");
                match &attr.value {
                    Expr::BinaryOp(lhs, BinOp::Add, rhs, _) => {
                        // lhs should be `a`
                        match lhs.as_ref() {
                            Expr::Ident(id) => assert_eq!(id.name, "a"),
                            other => panic!("expected Ident(a), got {:?}", other),
                        }
                        // rhs should be `b * c`
                        match rhs.as_ref() {
                            Expr::BinaryOp(b, BinOp::Mul, c, _) => {
                                match b.as_ref() {
                                    Expr::Ident(id) => assert_eq!(id.name, "b"),
                                    other => panic!("expected Ident(b), got {:?}", other),
                                }
                                match c.as_ref() {
                                    Expr::Ident(id) => assert_eq!(id.name, "c"),
                                    other => panic!("expected Ident(c), got {:?}", other),
                                }
                            }
                            other => panic!("expected BinaryOp(Mul), got {:?}", other),
                        }
                    }
                    other => panic!("expected BinaryOp(Add), got {:?}", other),
                }
            }
            other => panic!("expected Attribute, got {:?}", other),
        }
    }

    #[test]
    fn test_list_literal() {
        let (doc, diags) = parse("items = [1, 2, 3]");
        assert!(!diags.has_errors(), "diagnostics: {:?}", diags.diagnostics());
        assert_eq!(doc.items.len(), 1);
        match &doc.items[0] {
            DocItem::Body(BodyItem::Attribute(attr)) => {
                assert_eq!(attr.name.name, "items");
                match &attr.value {
                    Expr::List(items, _) => {
                        assert_eq!(items.len(), 3);
                    }
                    other => panic!("expected List, got {:?}", other),
                }
            }
            other => panic!("expected Attribute, got {:?}", other),
        }
    }

    #[test]
    fn test_nested_blocks() {
        let src = r#"
server {
    listener {
        port = 8080
    }
    logging {
        level = "info"
    }
}
"#;
        let (doc, diags) = parse(src);
        assert!(!diags.has_errors(), "diagnostics: {:?}", diags.diagnostics());
        assert_eq!(doc.items.len(), 1);
        match &doc.items[0] {
            DocItem::Body(BodyItem::Block(block)) => {
                assert_eq!(block.kind.name, "server");
                assert_eq!(block.body.len(), 2);
                match &block.body[0] {
                    BodyItem::Block(inner) => assert_eq!(inner.kind.name, "listener"),
                    other => panic!("expected inner Block, got {:?}", other),
                }
                match &block.body[1] {
                    BodyItem::Block(inner) => assert_eq!(inner.kind.name, "logging"),
                    other => panic!("expected inner Block, got {:?}", other),
                }
            }
            other => panic!("expected Block, got {:?}", other),
        }
    }

    fn parse_query(input: &str) -> (Option<QueryPipeline>, DiagnosticBag) {
        let file_id = FileId(0);
        let tokens = lexer::lex(input, file_id).unwrap_or_else(|_diags| {
            vec![Token {
                kind: TokenKind::Eof,
                span: Span::new(file_id, 0, 0),
            }]
        });
        let parser = Parser::new(tokens);
        parser.parse_query_standalone()
    }

    #[test]
    fn parse_query_table_id_selector() {
        let (pipeline, diags) = parse_query("table#my-table");
        assert!(!diags.has_errors(), "diagnostics: {:?}", diags.diagnostics());
        let pipeline = pipeline.expect("expected a query pipeline");
        match &pipeline.selector {
            QuerySelector::TableId(id_lit) => {
                assert_eq!(id_lit.value, "my-table");
            }
            other => panic!("expected TableId, got {:?}", other),
        }
    }

    #[test]
    fn parse_query_kind_id_selector_not_table() {
        let (pipeline, diags) = parse_query("service#my-svc");
        assert!(!diags.has_errors(), "diagnostics: {:?}", diags.diagnostics());
        let pipeline = pipeline.expect("expected a query pipeline");
        match &pipeline.selector {
            QuerySelector::KindId(ident, id_lit) => {
                assert_eq!(ident.name, "service");
                assert_eq!(id_lit.value, "my-svc");
            }
            other => panic!("expected KindId, got {:?}", other),
        }
    }

    #[test]
    fn e036_export_inside_block() {
        let (doc, diags) = parse("config {\n  export let x = 1\n}");
        assert!(diags.has_errors(), "expected E036 error");
        let e036_errors: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E036"))
            .collect();
        assert_eq!(e036_errors.len(), 1);
        assert!(e036_errors[0]
            .message
            .contains("export declarations must be at the top level"));
        // The block itself should still parse
        assert_eq!(doc.items.len(), 1);
        match &doc.items[0] {
            DocItem::Body(BodyItem::Block(block)) => {
                assert_eq!(block.kind.name, "config");
            }
            other => panic!("expected Block, got {:?}", other),
        }
    }

    #[test]
    fn e036_re_export_inside_block() {
        let (doc, diags) = parse("config {\n  export myvar\n}");
        assert!(diags.has_errors(), "expected E036 error");
        let e036_errors: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E036"))
            .collect();
        assert_eq!(e036_errors.len(), 1);
        // The block should still parse
        assert_eq!(doc.items.len(), 1);
    }

    #[test]
    fn e036_no_error_for_top_level_export() {
        let (_doc, diags) = parse("export let x = 42");
        let e036_errors: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E036"))
            .collect();
        assert_eq!(e036_errors.len(), 0);
    }

    // ── Interpolated inline ID tests ─────────────────────────────────────

    #[test]
    fn interpolated_inline_id_basic() {
        // svc-${name} should parse as InlineId::Interpolated
        let (doc, diags) = parse("service svc-${name} { }");
        assert!(!diags.has_errors(), "diagnostics: {:?}", diags.diagnostics());
        assert_eq!(doc.items.len(), 1);
        match &doc.items[0] {
            DocItem::Body(BodyItem::Block(block)) => {
                assert_eq!(block.kind.name, "service");
                match &block.inline_id {
                    Some(InlineId::Interpolated(parts)) => {
                        assert_eq!(parts.len(), 2);
                        match &parts[0] {
                            StringPart::Literal(s) => assert_eq!(s, "svc-"),
                            other => panic!("expected Literal, got {:?}", other),
                        }
                        match &parts[1] {
                            StringPart::Interpolation(expr) => {
                                match expr.as_ref() {
                                    Expr::Ident(id) => assert_eq!(id.name, "name"),
                                    other => panic!("expected Ident expr, got {:?}", other),
                                }
                            }
                            other => panic!("expected Interpolation, got {:?}", other),
                        }
                    }
                    other => panic!("expected InlineId::Interpolated, got {:?}", other),
                }
            }
            other => panic!("expected Block, got {:?}", other),
        }
    }

    #[test]
    fn interpolated_inline_id_with_hyphenated_prefix() {
        // svc-api-${name} should produce ["svc-api-", ${name}]
        let (doc, diags) = parse("service svc-api-${name} { }");
        assert!(!diags.has_errors(), "diagnostics: {:?}", diags.diagnostics());
        match &doc.items[0] {
            DocItem::Body(BodyItem::Block(block)) => {
                match &block.inline_id {
                    Some(InlineId::Interpolated(parts)) => {
                        assert_eq!(parts.len(), 2);
                        match &parts[0] {
                            StringPart::Literal(s) => assert_eq!(s, "svc-api-"),
                            other => panic!("expected Literal 'svc-api-', got {:?}", other),
                        }
                        assert!(matches!(&parts[1], StringPart::Interpolation(_)));
                    }
                    other => panic!("expected InlineId::Interpolated, got {:?}", other),
                }
            }
            other => panic!("expected Block, got {:?}", other),
        }
    }

    #[test]
    fn interpolated_inline_id_prefix_and_suffix() {
        // svc-${name}-api should produce ["svc-", ${name}, "-api"]
        let (doc, diags) = parse("service svc-${name}-api { }");
        assert!(!diags.has_errors(), "diagnostics: {:?}", diags.diagnostics());
        match &doc.items[0] {
            DocItem::Body(BodyItem::Block(block)) => {
                match &block.inline_id {
                    Some(InlineId::Interpolated(parts)) => {
                        assert_eq!(parts.len(), 3);
                        match &parts[0] {
                            StringPart::Literal(s) => assert_eq!(s, "svc-"),
                            other => panic!("expected Literal 'svc-', got {:?}", other),
                        }
                        assert!(matches!(&parts[1], StringPart::Interpolation(_)));
                        match &parts[2] {
                            StringPart::Literal(s) => assert_eq!(s, "-api"),
                            other => panic!("expected Literal '-api', got {:?}", other),
                        }
                    }
                    other => panic!("expected InlineId::Interpolated, got {:?}", other),
                }
            }
            other => panic!("expected Block, got {:?}", other),
        }
    }

    #[test]
    fn interpolated_inline_id_only_interp() {
        // ${name} as inline ID
        let (doc, diags) = parse("service ${name} { }");
        assert!(!diags.has_errors(), "diagnostics: {:?}", diags.diagnostics());
        match &doc.items[0] {
            DocItem::Body(BodyItem::Block(block)) => {
                match &block.inline_id {
                    Some(InlineId::Interpolated(parts)) => {
                        assert_eq!(parts.len(), 1);
                        assert!(matches!(&parts[0], StringPart::Interpolation(_)));
                    }
                    other => panic!("expected InlineId::Interpolated, got {:?}", other),
                }
            }
            other => panic!("expected Block, got {:?}", other),
        }
    }

    #[test]
    fn interpolated_inline_id_in_for_loop() {
        let src = r#"for name in ["a", "b"] {
            service svc-${name} {
                port = 8080
            }
        }"#;
        let (doc, diags) = parse(src);
        assert!(!diags.has_errors(), "diagnostics: {:?}", diags.diagnostics());
        assert_eq!(doc.items.len(), 1);
        // The for loop body should contain a block with interpolated inline ID
        match &doc.items[0] {
            DocItem::Body(BodyItem::ForLoop(for_loop)) => {
                assert_eq!(for_loop.body.len(), 1);
                match &for_loop.body[0] {
                    BodyItem::Block(block) => {
                        assert_eq!(block.kind.name, "service");
                        assert!(matches!(&block.inline_id, Some(InlineId::Interpolated(_))));
                    }
                    other => panic!("expected Block in for body, got {:?}", other),
                }
            }
            other => panic!("expected ForLoop, got {:?}", other),
        }
    }

    #[test]
    fn literal_inline_id_still_works() {
        // Ensure non-interpolated IDs still parse as Literal
        let (doc, diags) = parse("service my-svc { }");
        assert!(!diags.has_errors(), "diagnostics: {:?}", diags.diagnostics());
        match &doc.items[0] {
            DocItem::Body(BodyItem::Block(block)) => {
                match &block.inline_id {
                    Some(InlineId::Literal(id)) => assert_eq!(id.value, "my-svc"),
                    other => panic!("expected InlineId::Literal, got {:?}", other),
                }
            }
            other => panic!("expected Block, got {:?}", other),
        }
    }

    #[test]
    fn plain_ident_inline_id_still_works() {
        // Ensure plain ident (no hyphens) still works
        let (doc, diags) = parse("service api { }");
        assert!(!diags.has_errors(), "diagnostics: {:?}", diags.diagnostics());
        match &doc.items[0] {
            DocItem::Body(BodyItem::Block(block)) => {
                match &block.inline_id {
                    Some(InlineId::Literal(id)) => assert_eq!(id.value, "api"),
                    other => panic!("expected InlineId::Literal, got {:?}", other),
                }
            }
            other => panic!("expected Block, got {:?}", other),
        }
    }
}
