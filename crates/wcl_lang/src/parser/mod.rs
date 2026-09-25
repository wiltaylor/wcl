mod decls;
mod expr;
mod pattern;
mod types;

use std::cell::OnceCell;
use std::sync::Arc;

use miette::{NamedSource, SourceSpan};

use crate::ast::{Block, Expr, Item, Source, Span};
use crate::diagnostics::{MAX_SYNTAX_ERRORS, ParseError, SyntaxError};
use crate::lexer::{LexError, Lexer, StringLit, Token, TokenKind};
use crate::symbols::{DuplicateSymbol, SymbolIndex, SymbolKind, SymbolPath, SymbolRecord};

// Re-exports used by the integration-style tests in `tests.rs`
// (included via `mod tests`). Gated to keep lib builds free of unused
// imports.
#[cfg(test)]
use crate::ast::TypeRef;
#[cfg(test)]
use crate::ast::{BinOp, Field, SymbolSetDecl, TypeDecl, UnaryOp, UnionDecl, UseForm, VariantBody};

/// Recursive-descent parser over a token stream, building both the
/// syntax tree and the symbol index in one pass.
pub struct Parser<'a> {
    /// Source name, reported in diagnostics.
    file: String,
    /// The `NamedSource` every diagnostic renders against, built on the
    /// first error. Its text is an `Arc<str>`, so each further error
    /// clones a pointer rather than copying the whole source.
    named_src: OnceCell<NamedSource<Arc<str>>>,
    /// The full source text, retained so `parse_import_decl` can slice
    /// the raw path out of an `import <...>` between the `<` and `>`
    /// token spans (the bracketed path is not a single token).
    src: &'a str,
    /// The token source.
    lexer: Lexer<'a>,
    /// One token of lookahead.
    peeked: Option<Token>,
    /// A second token of lookahead, for the few two-token decisions.
    peeked2: Option<Token>,
    /// Namespace declared so far, prefixed to declared names.
    file_ns: Vec<String>,
    /// Symbol index built as declarations are parsed.
    index: SymbolIndex,
    /// Current nesting depth, bounded to stop runaway recursion.
    block_depth: u32,
    /// Whether the item loop is currently inside the body of a block
    /// carrying a lexical `@schemaless` decorator. While set, a field
    /// key may be written as a plain string literal (`"allowed-tools" =
    /// …`), letting `@schemaless frontmatter { … }` author keys that
    /// aren't valid identifiers. Saved/restored per block body, so it
    /// reflects the *immediate* enclosing block only.
    in_schemaless_block: bool,
    /// Recursive-descent depth across the four self-nesting parse
    /// paths (expressions, type references, patterns, blocks). Capped
    /// at [`MAX_PARSE_DEPTH`] so pathological input (kilobytes of
    /// `(((((…`) raises a spanned diagnostic instead of overflowing
    /// the stack — anything taking untrusted input (`wcl check`, the
    /// LSP, `wdoc serve` mid-edit) can hit this.
    recursion_depth: u32,
    /// Tree depth of the deepest expression finished at the current
    /// nesting level. Threaded bottom-up by the expression parser so a
    /// flat chain (`1 + 1 + …`) is capped at [`MAX_EXPR_DEPTH`] even
    /// though parsing it never recurses.
    expr_depth: u32,
    /// Trivia (comments + blank lines) captured at the start of the
    /// current `parse_item` call. Each sub-parser drains this via
    /// `take_item_trivia()` when it builds the final Item struct, so
    /// the round-trip printer can re-emit comments at their original
    /// positions. Fresh per Item: `parse_item` overwrites it on entry.
    current_item_trivia: Vec<crate::ast::Trivia>,
    /// Syntax errors recorded so far. An item that fails to parse lands
    /// its error here and the item loop resynchronises, so one parse
    /// reports every mistake rather than the first.
    errors: Vec<SyntaxError>,
    /// Set once [`MAX_SYNTAX_ERRORS`] have been recorded: every item loop
    /// then stops, and the parse returns what it has.
    gave_up: bool,
    /// Whether "unexpected end of file inside block" has been reported.
    /// Each unclosed enclosing block meets the same end of file; the
    /// innermost one reports it and the rest stay quiet.
    eof_in_block_reported: bool,
    /// The brackets (`{`, `(`, `[`) consumed and not yet closed, in the
    /// order they opened. Recovery reads it to skip the rest of a broken
    /// item without stopping inside a nested body or list.
    open_delims: Vec<Delim>,
    /// End offset of the last token consumed.
    last_end: usize,
}

/// An opening bracket, as tracked by `Parser::open_delims`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Delim {
    /// `{`
    Brace,
    /// `(`
    Paren,
    /// `[`
    Bracket,
}

/// How an item loop resumed after a failed item.
enum Resync {
    /// At the start of the next item, the `}` closing the current body,
    /// or end of file. The loop carries on and sees which.
    Resumed,
    /// The failed item consumed the `}` that closes the current body, so
    /// the body has ended.
    BodyClosed,
}

/// Hard cap on recursive-descent nesting — generous for real
/// documents, far below the stack limit. Sized for the smallest stack
/// the parser runs on (2 MiB test / worker threads, with ASan frame
/// inflation under fuzzing): each paren level costs a few KiB of
/// frames across parse_expr_bp → parse_prefix → the paren arm.
pub(crate) const MAX_PARSE_DEPTH: u32 = 128;

/// Hard cap on the depth of one expression tree. Evaluating, printing
/// and dropping an expression all recurse once per level, so a long
/// flat chain (`1 + 1 + …`, `a.b.c…`, `f()()…`) must stop here rather
/// than abort the process on a small (2 MiB) stack.
pub(crate) const MAX_EXPR_DEPTH: u32 = 256;

impl<'a> Parser<'a> {
    /// Enter one level of self-nesting parse recursion, erroring past
    /// [`MAX_PARSE_DEPTH`]. Pair with `leave_recursion` on success
    /// paths. An `Err` abandons the item being parsed and the item loop
    /// restores the depth it had before the item, so unwinding the
    /// counter on the error path is unnecessary.
    pub(super) fn enter_recursion(&mut self) -> Result<(), ParseError> {
        self.recursion_depth += 1;
        // The stack can run out before the count does — an unoptimised
        // build's frames are several times larger — and that reports as
        // the same limit rather than aborting.
        if self.recursion_depth > MAX_PARSE_DEPTH || crate::stack::is_low() {
            let span = self.peek().map(|t| t.span).unwrap_or(Span::new(0, 0));
            return Err(self.err(
                format!("nesting too deep (more than {MAX_PARSE_DEPTH} levels)"),
                span,
                "nesting limit reached here",
            ));
        }
        Ok(())
    }

    /// Leave one level of nesting, undoing `enter_recursion`.
    pub(super) fn leave_recursion(&mut self) {
        self.recursion_depth -= 1;
    }

    /// Start parsing `src`. `file` names it in diagnostics.
    pub fn new(src: &'a str, file: impl Into<String>) -> Self {
        Self::with_lexer(src, file.into(), Lexer::new(src))
    }

    /// A parser for the `${…}` slot whose text starts at byte `start` of
    /// `src`. The lexer begins there, so spans are offsets into `src`
    /// and diagnostics point into the outer file with no rewriting.
    fn for_slot(src: &'a str, start: usize, file: String) -> Self {
        Self::with_lexer(src, file, Lexer::starting_at(src, start))
    }

    /// Shared constructor behind [`Self::new`] and [`Self::for_slot`].
    fn with_lexer(src: &'a str, file: String, lexer: Lexer<'a>) -> Self {
        Self {
            file,
            named_src: OnceCell::new(),
            src,
            lexer,
            peeked: None,
            peeked2: None,
            file_ns: Vec::new(),
            index: SymbolIndex::default(),
            block_depth: 0,
            in_schemaless_block: false,
            recursion_depth: 0,
            expr_depth: 0,
            current_item_trivia: Vec::new(),
            errors: Vec::new(),
            gave_up: false,
            eof_in_block_reported: false,
            open_delims: Vec::new(),
            last_end: 0,
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

    /// Parse a whole file, returning its items and the symbol index
    /// built alongside them. All or nothing: any syntax error fails the
    /// parse, and the error carries every one the file has (see
    /// [`ParseError::syntax_errors`]).
    pub fn parse_source(&mut self) -> Result<(Source, SymbolIndex), ParseError> {
        let (source, index, errors) = self.parse_source_recovering();
        match ParseError::from_syntax_errors(errors) {
            Some(err) => Err(err),
            None => Ok((source, index)),
        }
    }

    /// Parse a whole file, recovering from syntax errors. Returns the
    /// items that parsed, the symbol index built from them, and every
    /// syntax error found (at most [`MAX_SYNTAX_ERRORS`]) in source
    /// order. With no errors the tree is the one [`Self::parse_source`]
    /// returns.
    ///
    /// A failed item is dropped and the parser skips to the next item
    /// boundary (see [`Self::resync`]). Inside a block body this happens
    /// per item, so one bad field keeps its siblings.
    pub fn parse_source_recovering(&mut self) -> (Source, SymbolIndex, Vec<SyntaxError>) {
        let mut items = Vec::new();
        while !self.gave_up {
            match self.peek() {
                Ok(tok) if matches!(tok.kind, TokenKind::Eof) => break,
                Ok(_) => {}
                Err(e) => {
                    self.record(e);
                    continue;
                }
            }
            let (Some(item), _) = self.parse_item_or_resync() else {
                continue;
            };
            let item_idx = items.len();
            if let Err(e) = self.register_item(&item, item_idx) {
                self.record(e);
            }
            items.push(item);
            // The next token's same-line comment (incl. the Eof token's,
            // on the final pass) trails the item we just pushed.
            if let Err(e) = self.attach_trailing_to_last(&mut items) {
                self.record(e);
            }
        }
        // Comments + blank lines after the last item, before EOF.
        let trailing_trivia = match self.peeked.as_ref() {
            Some(tok) if !self.gave_up => tok.leading_trivia.clone(),
            _ => Vec::new(),
        };
        (
            Source {
                items,
                trailing_trivia,
            },
            std::mem::take(&mut self.index),
            std::mem::take(&mut self.errors),
        )
    }

    /// Parse the items of a block body, whose `{` has been consumed, up
    /// to its closing `}`, recovering from a failed item as the top
    /// level does. Returns the items and the consumed `}`, or `None`
    /// when the body ended without one: at end of file (reported here),
    /// when a failed item consumed it, or when the parse gave up.
    fn parse_body_items(&mut self) -> (Vec<Item>, Option<Token>) {
        let mut items = Vec::new();
        while !self.gave_up {
            match self.peek() {
                Ok(tok) if matches!(tok.kind, TokenKind::RBrace) => {
                    return (items, self.bump().ok());
                }
                Ok(tok) if matches!(tok.kind, TokenKind::Eof) => {
                    let span = tok.span;
                    self.report_eof_in_block(span);
                    return (items, None);
                }
                Ok(_) => {}
                Err(e) => {
                    self.record(e);
                    continue;
                }
            }
            match self.parse_item_or_resync() {
                (Some(item), _) => {
                    items.push(item);
                    // An inline comment after this item (carried on the
                    // next token, including the `}`) trails it.
                    if let Err(e) = self.attach_trailing_to_last(&mut items) {
                        self.record(e);
                    }
                }
                (None, Resync::BodyClosed) => return (items, None),
                (None, Resync::Resumed) => {}
            }
        }
        (items, None)
    }

    /// Parse one item of an item loop (the top level or a block body)
    /// whose first token has been peeked. On failure the error is
    /// recorded, the depth counters the failed item left raised are
    /// restored, and the stream is skipped to the next item boundary;
    /// the item is then `None`.
    fn parse_item_or_resync(&mut self) -> (Option<Item>, Resync) {
        let depth = self.open_delims.len();
        let start = self.peeked.as_ref().map_or(self.last_end, |t| t.span.start);
        let recursion_depth = self.recursion_depth;
        let expr_depth = self.expr_depth;
        match self.parse_item() {
            Ok(item) => (Some(item), Resync::Resumed),
            Err(e) => {
                self.record(e);
                self.recursion_depth = recursion_depth;
                self.expr_depth = expr_depth;
                (None, self.resync(depth, start))
            }
        }
    }

    /// Skip the rest of an item that failed to parse. `depth` is how
    /// many brackets were open when the item started, and `start` is the
    /// offset of its first token.
    ///
    /// Stops, without consuming it, at the first token that is:
    ///
    /// - end of file;
    /// - the `}` closing the enclosing body: one met with no `{` opened
    ///   since the item started still open. At the top level there is
    ///   no enclosing body, so a stray `}` is skipped;
    /// - an identifier or `@` starting a line, with nothing opened since
    ///   the item started still open;
    /// - an identifier or `@` at column 0, with no `{` opened since the
    ///   item started still open. Top-level items sit at column 0, so an
    ///   unclosed `(` or `[` does not swallow the rest of the file.
    ///
    /// Lines inside an open list, call or body are never taken for a new
    /// item. That is what keeps one mistake to one error rather than an
    /// error per line that follows it. Lex errors met while skipping are
    /// recorded, since each is a mistake of its own.
    fn resync(&mut self, depth: usize, start: usize) -> Resync {
        loop {
            if self.gave_up {
                return Resync::Resumed;
            }
            if self.open_delims.len() < depth {
                return Resync::BodyClosed;
            }
            if let Err(e) = self.peek() {
                self.record(e);
                continue;
            }
            let tok = self.peeked.as_ref().expect("just peeked");
            let opened = &self.open_delims[depth..];
            let brace_open = opened.contains(&Delim::Brace);
            let item_start = matches!(tok.kind, TokenKind::Ident(_) | TokenKind::At);
            let column_zero =
                tok.span.start == 0 || self.src.as_bytes().get(tok.span.start - 1) == Some(&b'\n');
            let at_item = tok.span.start > start
                && item_start
                && (tok.preceded_by_newline || column_zero)
                && (opened.is_empty() || (column_zero && !brace_open));
            match tok.kind {
                TokenKind::Eof => return Resync::Resumed,
                TokenKind::RBrace if !brace_open && depth > 0 => return Resync::Resumed,
                _ if at_item => return Resync::Resumed,
                _ => {}
            }
            if let Err(e) = self.bump() {
                self.record(e);
            }
        }
    }

    /// Record a syntax error for the parse to report. Once
    /// [`MAX_SYNTAX_ERRORS`] are held the parse gives up, and errors met
    /// while the item loops unwind are dropped.
    fn record(&mut self, err: ParseError) {
        if self.gave_up {
            return;
        }
        if let ParseError::Syntax(syntax) = err {
            self.errors.push(*syntax);
        }
        if self.errors.len() >= MAX_SYNTAX_ERRORS {
            self.gave_up = true;
        }
    }

    /// Report that a block body met end of file before its `}`, once
    /// per parse: every unclosed enclosing block meets the same end of
    /// file, and one error says it.
    fn report_eof_in_block(&mut self, span: Span) {
        if !self.eof_in_block_reported {
            self.eof_in_block_reported = true;
            let err = self.err("unexpected end of file inside block", span, "expected '}'");
            self.record(err);
        }
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
            // Plain let bindings are intentionally NOT registered in the
            // symbol index: they must stay out of `Document::field`/`block`/
            // `get` and any query that walks named symbols. Resolution
            // happens by scanning items directly (see `find_let` /
            // `root_let`). The `fn name(…)` item form opts in to the index
            // so the LSP can offer outline / hover / go-to-definition — its
            // value resolution is still the let path.
            Item::Let(l) => {
                if l.fn_syntax {
                    let fqn = self.join_fqn(std::slice::from_ref(&l.name));
                    self.try_insert(SymbolRecord {
                        fqn,
                        kind: SymbolKind::FnDecl,
                        span: l.span,
                        path: SymbolPath {
                            item_index,
                            member_index: None,
                        },
                    })?;
                }
            }
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

    /// Index a declaration, turning a name collision into a parse error.
    fn try_insert(&mut self, rec: SymbolRecord) -> Result<(), ParseError> {
        let msg = format!("duplicate declaration '{}'", rec.fqn);
        self.try_insert_with_msg(rec, msg)
    }

    /// Index a declaration, using `msg` to word the collision error.
    fn try_insert_with_msg(&mut self, rec: SymbolRecord, msg: String) -> Result<(), ParseError> {
        match self.index.insert(rec) {
            Ok(()) => Ok(()),
            Err(DuplicateSymbol {
                first_span,
                second_span,
                ..
            }) => Err(ParseError::syntax_with_related(
                msg,
                self.named_src(),
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

    /// Prefix a declared name with the current file namespace.
    fn join_fqn(&self, name: &[String]) -> String {
        if self.file_ns.is_empty() {
            name.join(".")
        } else {
            let mut parts = self.file_ns.clone();
            parts.extend(name.iter().cloned());
            parts.join(".")
        }
    }

    /// Parse one item, dispatching on the leading token.
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
                // `fn name(…)` — a fn item. The identifier lookahead keeps
                // `fn = expr` (a field named `fn`) parsing as before.
                "fn" if matches!(self.peek2()?.kind, TokenKind::Ident(_)) => {
                    return self.parse_fn_item(decorators);
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
            return self.parse_connection_decl(decorators);
        }

        // String-literal field key, e.g. `"allowed-tools" = [...]`. Only
        // legal inside a `@schemaless` block, where it lets a frontmatter
        // field use a key that isn't a valid identifier (hyphens, …).
        // Elsewhere a string at item-start stays an error (below), so this
        // is purely additive with no grammar ambiguity.
        if self.in_schemaless_block
            && matches!(self.peek()?.kind, TokenKind::Str(_))
            && matches!(self.peek2()?.kind, TokenKind::Eq)
        {
            let tok = self.bump()?;
            let span_start = tok.span.start;
            let name = match tok.kind {
                TokenKind::Str(StringLit::Utf8(s) | StringLit::Ascii(s)) => s,
                _ => {
                    return Err(self.err(
                        "a schemaless field key must be a plain string, e.g. \"allowed-tools\"",
                        tok.span,
                        "use a plain double-quoted string",
                    ));
                }
            };
            return self.parse_field(name, span_start, decorators);
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
            | TokenKind::Question
            | TokenKind::Number(_)
            | TokenKind::NumberWithUnit(..)
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
                    conditional: false,
                    slot_decl: None,
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

    /// Consume the next token, requiring it to be `kind`.
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

    /// The next token, without consuming it.
    pub(super) fn peek(&mut self) -> Result<&Token, ParseError> {
        if self.peeked.is_none() {
            self.peeked = Some(self.next_lex()?);
        }
        Ok(self.peeked.as_ref().expect("just set"))
    }

    /// The token after next, without consuming either.
    pub(super) fn peek2(&mut self) -> Result<&Token, ParseError> {
        self.peek()?;
        if self.peeked2.is_none() {
            self.peeked2 = Some(self.next_lex()?);
        }
        Ok(self.peeked2.as_ref().expect("just set"))
    }

    /// Consume and return the next token.
    pub(super) fn bump(&mut self) -> Result<Token, ParseError> {
        let tok = if let Some(t) = self.peeked.take() {
            self.peeked = self.peeked2.take();
            t
        } else {
            self.next_lex()?
        };
        self.track_delim(&tok.kind);
        self.last_end = tok.span.end;
        Ok(tok)
    }

    /// Keep `open_delims` in step with a consumed token. A closer that
    /// matches the innermost opener closes it. A `}` that does not closes
    /// the innermost `{` and every bracket left open inside it, since a
    /// brace ends a body and is the likelier to be meant. A stray `)` or
    /// `]` changes nothing.
    fn track_delim(&mut self, kind: &TokenKind) {
        let closes = match kind {
            TokenKind::LBrace => return self.open_delims.push(Delim::Brace),
            TokenKind::LParen => return self.open_delims.push(Delim::Paren),
            TokenKind::LBracket => return self.open_delims.push(Delim::Bracket),
            TokenKind::RBrace => Delim::Brace,
            TokenKind::RParen => Delim::Paren,
            TokenKind::RBracket => Delim::Bracket,
            _ => return,
        };
        if self.open_delims.last() == Some(&closes) {
            self.open_delims.pop();
        } else if closes == Delim::Brace
            && let Some(i) = self.open_delims.iter().rposition(|d| *d == Delim::Brace)
        {
            self.open_delims.truncate(i);
        }
    }

    /// Pull one token from the lexer, converting a lex error. After an
    /// error the lexer is moved past the rejected text, so the next pull
    /// makes progress.
    fn next_lex(&mut self) -> Result<Token, ParseError> {
        self.lexer.next_token().map_err(|e| {
            self.lexer.recover(&e);
            self.lex_to_parse(e)
        })
    }

    /// Wrap a lexer error as a parse error against this source.
    fn lex_to_parse(&self, e: LexError) -> ParseError {
        let label = e.message.clone();
        self.err(e.message, e.span, label)
    }

    /// Build an `Expr` from a `StringLit` token. Plain forms map
    /// one-to-one to the existing string-typed `Expr` variants; the
    /// interpolated form sub-parses each `${expr}` slot using a fresh
    /// `Parser` whose lexer starts at the slot inside the outer source,
    /// so span offsets stay aligned with the outer file.
    fn string_lit_to_expr(&mut self, lit: StringLit, _span: Span) -> Result<Expr, ParseError> {
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

    /// Sub-parse one `${...}` slot. The lexer captured the slot text
    /// verbatim from this source, so a fresh parser lexes it in place:
    /// it starts just past the `${` and stops where the text ends, and
    /// its spans are already in the outer source's coordinates.
    ///
    /// Any error from the sub-parser is re-issued against this parser's
    /// `NamedSource` (the sub-parser only sees the text up to the slot's
    /// end) and prefixed with `"in interpolation slot:"` so the user can
    /// tell the diagnostic came from inside a `${…}` rather than the
    /// surrounding text.
    fn sub_parse_slot(&mut self, text: &str, slot_span: Span) -> Result<Expr, ParseError> {
        let start = slot_span.start + 2;
        let end = start + text.len();
        debug_assert_eq!(self.src.get(start..end), Some(text));
        let mut sub = Parser::for_slot(&self.src[..end], start, self.file.clone());
        // Slots nest (`$"${ $"${…}" }"`), so the sub-parser continues
        // this parser's recursion count rather than starting afresh.
        sub.recursion_depth = self.recursion_depth;
        let (expr, _) = sub.parse_expr().map_err(|e| self.wrap_slot_error(e))?;
        self.expr_depth = self.expr_depth.max(sub.expr_depth);
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

    /// Convert a sub-parser's `ParseError` (whose source stops at the
    /// slot's end) into one rooted in the outer document's
    /// `NamedSource`, prefixing the message with the interpolation
    /// context.
    fn wrap_slot_error(&self, e: ParseError) -> ParseError {
        match e {
            ParseError::Syntax(inner) => ParseError::syntax(
                format!("in interpolation slot: {}", inner.message),
                self.named_src(),
                inner.span,
                inner.label,
            ),
            other => other,
        }
    }

    /// The source every diagnostic from this parse renders against. The
    /// text is copied once, on the first error; later calls share it.
    fn named_src(&self) -> NamedSource<Arc<str>> {
        self.named_src
            .get_or_init(|| NamedSource::new(self.file.clone(), Arc::from(self.src)))
            .clone()
    }

    /// Build a parse error pointing at the given span.
    pub(super) fn err(
        &self,
        message: impl Into<String>,
        span: Span,
        label: impl Into<String>,
    ) -> ParseError {
        let len = span.len().max(1);
        ParseError::syntax(
            message.into(),
            self.named_src(),
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
            | TokenKind::NumberWithUnit(..)
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

/// Name a token as diagnostics spell it.
pub(super) fn describe(t: &TokenKind) -> String {
    match t {
        TokenKind::Ident(s) => format!("identifier '{s}'"),
        TokenKind::Str(_) => "string".to_string(),
        TokenKind::Number(_) => "number".to_string(),
        TokenKind::NumberWithUnit(..) => "number with unit".to_string(),
        TokenKind::Bool(_) => "boolean".to_string(),
        TokenKind::Symbol(_) => "symbol literal".to_string(),
        TokenKind::None => "'none'".to_string(),
        TokenKind::Eq => "'='".to_string(),
        TokenKind::EqEq => "'=='".to_string(),
        TokenKind::BangEq => "'!='".to_string(),
        TokenKind::Bang => "'!'".to_string(),
        TokenKind::Colon => "':'".to_string(),
        TokenKind::Question => "'?'".to_string(),
        TokenKind::QuestionQuestion => "'??'".to_string(),
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
mod tests;
