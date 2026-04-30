// parser/expr.rs — Expression parser using Pratt/precedence climbing

use crate::lang::ast::*;
use crate::lang::lexer::TokenKind;

use super::Parser;

impl Parser {
    /// Entry point for expression parsing.
    pub(crate) fn parse_expr(&mut self) -> Option<Expr> {
        // Bare query pipeline: if the tokens ahead form a query pipeline that
        // is *unambiguous* (cannot also be parsed as a normal expression),
        // produce an `Expr::Query` directly. Ambiguous case: a selector that
        // is a bare identifier with no filters and no `#id` — that still
        // parses as an `Expr::Ident`, so users add `| ...` filters or use
        // `..kind` / `kind#id` / `table#id` when they want a query.
        let saved_pos = self.pos;
        let saved_diag_len = self.diagnostics.len();
        if let Some(pipeline) = self.parse_query_pipeline() {
            // Ambiguous selectors (those that also parse as normal
            // expressions) require at least one `| filter` before we commit
            // to the query interpretation. A bare `Kind(Ident)` is also an
            // identifier reference, and a dotted `Path([a, b, …])` is also
            // member access.
            let ambiguous_without_filters = matches!(
                pipeline.selector,
                QuerySelector::Kind(_) | QuerySelector::Path(_)
            );
            let is_unambiguous = !pipeline.filters.is_empty() || !ambiguous_without_filters;
            if is_unambiguous {
                let span = pipeline.span;
                return Some(Expr::Query(pipeline, span));
            }
        }
        self.pos = saved_pos;
        self.diagnostics.truncate(saved_diag_len);

        self.parse_ternary()
    }

    /// Parse a ternary expression: `expr ? expr : expr`
    fn parse_ternary(&mut self) -> Option<Expr> {
        let cond = self.parse_binary(2)?; // start at lowest binary precedence
        if matches!(self.peek_kind(), TokenKind::Question) {
            self.advance(); // consume ?
            let then_expr = self.parse_expr()?;
            if self.expect(&TokenKind::Colon).is_err() {
                return None;
            }
            let else_expr = self.parse_expr()?;
            let span = cond.span().merge(else_expr.span());
            Some(Expr::Ternary(
                Box::new(cond),
                Box::new(then_expr),
                Box::new(else_expr),
                span,
            ))
        } else {
            Some(cond)
        }
    }

    /// Parse binary expressions using precedence climbing.
    fn parse_binary(&mut self, min_prec: u8) -> Option<Expr> {
        let mut lhs = self.parse_unary()?;

        loop {
            let op = match self.token_to_binop() {
                Some(op) if op.precedence() >= min_prec => op,
                _ => break,
            };

            self.advance(); // consume operator token
            let next_prec = op.precedence() + 1; // left-associative
            let rhs = self.parse_binary(next_prec)?;
            let span = lhs.span().merge(rhs.span());
            lhs = Expr::BinaryOp(Box::new(lhs), op, Box::new(rhs), span);
        }

        Some(lhs)
    }

    /// Convert current token to a BinOp if applicable.
    fn token_to_binop(&self) -> Option<BinOp> {
        match self.peek_kind() {
            TokenKind::Or => Some(BinOp::Or),
            TokenKind::And => Some(BinOp::And),
            TokenKind::EqEq => Some(BinOp::Eq),
            TokenKind::Neq => Some(BinOp::Neq),
            TokenKind::Lt => Some(BinOp::Lt),
            TokenKind::Gt => Some(BinOp::Gt),
            TokenKind::Lte => Some(BinOp::Lte),
            TokenKind::Gte => Some(BinOp::Gte),
            TokenKind::Match => Some(BinOp::Match),
            TokenKind::Plus => Some(BinOp::Add),
            TokenKind::Minus => Some(BinOp::Sub),
            TokenKind::Star => Some(BinOp::Mul),
            TokenKind::Slash => Some(BinOp::Div),
            TokenKind::Percent => Some(BinOp::Mod),
            _ => None,
        }
    }

    /// Parse unary prefix expressions: `!expr`, `-expr`
    fn parse_unary(&mut self) -> Option<Expr> {
        match self.peek_kind() {
            TokenKind::Not => {
                let start = self.current_span();
                self.advance();
                let expr = self.parse_unary()?;
                let span = start.merge(expr.span());
                Some(Expr::UnaryOp(UnaryOp::Not, Box::new(expr), span))
            }
            TokenKind::Minus => {
                let start = self.current_span();
                self.advance();
                let expr = self.parse_unary()?;
                let span = start.merge(expr.span());
                Some(Expr::UnaryOp(UnaryOp::Neg, Box::new(expr), span))
            }
            _ => {
                let primary = self.parse_primary()?;
                Some(self.parse_postfix(primary))
            }
        }
    }

    /// Parse postfix operations: member access `.`, index `[]`, call `()`
    fn parse_postfix(&mut self, mut lhs: Expr) -> Expr {
        loop {
            match self.peek_kind() {
                TokenKind::Dot => {
                    self.advance(); // consume .
                    if let Some(ident) = self.try_parse_ident() {
                        let span = lhs.span().merge(ident.span);
                        lhs = Expr::MemberAccess(Box::new(lhs), ident, span);
                    } else {
                        self.diagnostics
                            .error("expected identifier after '.'", self.current_span());
                        break;
                    }
                }
                TokenKind::LBracket => {
                    self.advance(); // consume [
                    let index = match self.parse_expr() {
                        Some(e) => e,
                        None => break,
                    };
                    if self.expect(&TokenKind::RBracket).is_err() {
                        break;
                    }
                    let span = lhs.span().merge(self.prev_span());
                    lhs = Expr::IndexAccess(Box::new(lhs), Box::new(index), span);
                }
                TokenKind::LParen => {
                    let args = self.parse_call_args();
                    let span = lhs.span().merge(self.prev_span());
                    lhs = Expr::FnCall(Box::new(lhs), args, span);
                }
                _ => break,
            }
        }
        lhs
    }

    /// Parse a primary (atom) expression.
    pub(super) fn parse_primary(&mut self) -> Option<Expr> {
        self.skip_newlines();
        match self.peek_kind().clone() {
            TokenKind::IntLit(n) => {
                let span = self.current_span();
                self.advance();
                Some(Expr::IntLit(n, span))
            }
            TokenKind::FloatLit(n) => {
                let span = self.current_span();
                self.advance();
                Some(Expr::FloatLit(n, span))
            }
            TokenKind::DateLit(s) => {
                let span = self.current_span();
                let s = s.clone();
                self.advance();
                Some(Expr::DateLit(s, span))
            }
            TokenKind::DurationLit(s) => {
                let span = self.current_span();
                let s = s.clone();
                self.advance();
                Some(Expr::DurationLit(s, span))
            }
            TokenKind::BoolLit(b) => {
                let span = self.current_span();
                self.advance();
                Some(Expr::BoolLit(b, span))
            }
            TokenKind::NullLit => {
                let span = self.current_span();
                self.advance();
                Some(Expr::NullLit(span))
            }
            TokenKind::StringLit(_) | TokenKind::Heredoc { .. } => {
                let s = self.parse_string_lit()?;
                Some(Expr::StringLit(s))
            }
            TokenKind::SymbolLit(ref name) => {
                let name = name.clone();
                let span = self.current_span();
                self.advance();
                Some(Expr::SymbolLit(name, span))
            }
            TokenKind::IdentifierLit(ref val) => {
                let val = val.clone();
                let span = self.current_span();
                self.advance();
                Some(Expr::IdentifierLit(IdentifierLit { value: val, span }))
            }
            TokenKind::Ident(ref name) => {
                let name = name.clone();
                // Check for special keyword-like functions
                match name.as_str() {
                    "import_raw" => self.parse_import_raw_expr(),
                    "import_table" => self.parse_import_table_expr(),
                    _ => self.parse_ident_or_lambda(),
                }
            }
            TokenKind::Ref => self.parse_ref_expr(),
            TokenKind::Hash => self.parse_ref_shorthand(),
            TokenKind::SelfKw => {
                let span = self.current_span();
                self.advance();
                Some(Expr::Ident(Ident {
                    name: "self".to_string(),
                    span,
                }))
            }
            // `in` is a keyword in for-loops but also a valid identifier in
            // expressions (e.g. `in.name` in transform map blocks)
            TokenKind::In => {
                let span = self.current_span();
                self.advance();
                Some(Expr::Ident(Ident {
                    name: "in".to_string(),
                    span,
                }))
            }
            TokenKind::LBracket => self.parse_list_literal(),
            TokenKind::LBrace => {
                // Could be map literal or block expression.
                // Heuristic: if we see `{ ident =` or `{ string =`, it's a map.
                // If we see `{ let`, it's a block expr. Otherwise, try map.
                self.parse_map_or_block_expr()
            }
            TokenKind::LParen => {
                // Could be parenthesized expr or lambda params
                self.parse_paren_or_lambda()
            }
            _ => {
                self.diagnostics.error(
                    format!("expected expression, found {:?}", self.peek_kind()),
                    self.current_span(),
                );
                None
            }
        }
    }

    /// Parse an identifier, possibly a lambda: `ident => expr`
    fn parse_ident_or_lambda(&mut self) -> Option<Expr> {
        let ident = self.try_parse_ident()?;

        // Check for single-param lambda: `ident => expr`
        if matches!(self.peek_kind(), TokenKind::FatArrow) {
            self.advance(); // consume =>
            let body = self.parse_expr()?;
            let span = ident.span.merge(body.span());
            return Some(Expr::Lambda(vec![ident], Box::new(body), span));
        }

        // Check for qualified name: `namespace::name` or `a::b::c`
        if matches!(self.peek_kind(), TokenKind::ColonColon) {
            let mut name = ident.name.clone();
            let mut end_span = ident.span;
            while matches!(self.peek_kind(), TokenKind::ColonColon) {
                self.advance(); // consume ::
                let member = self.expect_ident().ok()?;
                name = format!("{}::{}", name, member.name);
                end_span = member.span;
            }
            let span = ident.span.merge(end_span);
            let qualified = Ident { name, span };
            return Some(Expr::Ident(qualified));
        }

        Some(Expr::Ident(ident))
    }

    /// Parse `( ... )` — either parenthesized expression or lambda `(a, b) => expr`
    fn parse_paren_or_lambda(&mut self) -> Option<Expr> {
        let start_span = self.current_span();

        // Try to detect lambda pattern by looking ahead
        if self.is_lambda_params() {
            return self.parse_lambda();
        }

        // Regular parenthesized expression
        self.advance(); // consume (
        self.skip_newlines();
        let inner = self.parse_expr()?;
        self.skip_newlines();
        if self.expect(&TokenKind::RParen).is_err() {
            return None;
        }
        let span = start_span.merge(self.prev_span());
        Some(Expr::Paren(Box::new(inner), span))
    }

    /// Detect if we're looking at lambda parameters: `(ident, ...) =>`
    fn is_lambda_params(&self) -> bool {
        if !matches!(self.peek_kind(), TokenKind::LParen) {
            return false;
        }
        let mut i = self.pos + 1;
        loop {
            // Skip newlines
            while i < self.tokens.len() && matches!(self.tokens[i].kind, TokenKind::Newline) {
                i += 1;
            }
            if i >= self.tokens.len() {
                return false;
            }
            match &self.tokens[i].kind {
                TokenKind::Ident(_) => {
                    i += 1;
                    // Skip newlines
                    while i < self.tokens.len() && matches!(self.tokens[i].kind, TokenKind::Newline)
                    {
                        i += 1;
                    }
                    if i >= self.tokens.len() {
                        return false;
                    }
                    match &self.tokens[i].kind {
                        TokenKind::Comma => {
                            i += 1;
                            continue;
                        }
                        TokenKind::RParen => {
                            i += 1;
                            // Skip newlines after )
                            while i < self.tokens.len()
                                && matches!(self.tokens[i].kind, TokenKind::Newline)
                            {
                                i += 1;
                            }
                            return i < self.tokens.len()
                                && matches!(self.tokens[i].kind, TokenKind::FatArrow);
                        }
                        _ => return false,
                    }
                }
                TokenKind::RParen => {
                    i += 1;
                    // Skip newlines after )
                    while i < self.tokens.len() && matches!(self.tokens[i].kind, TokenKind::Newline)
                    {
                        i += 1;
                    }
                    return i < self.tokens.len()
                        && matches!(self.tokens[i].kind, TokenKind::FatArrow);
                }
                _ => return false,
            }
        }
    }

    /// Parse a lambda expression: `(a, b) => expr` or `a => expr`
    fn parse_lambda(&mut self) -> Option<Expr> {
        let start_span = self.current_span();
        self.advance(); // consume (
        let mut params = Vec::new();
        loop {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::RParen) {
                break;
            }
            let ident = self.expect_ident().ok()?;
            params.push(ident);
            self.skip_newlines();
            if !matches!(self.peek_kind(), TokenKind::Comma) {
                break;
            }
            self.advance(); // consume comma
        }
        self.skip_newlines();
        if self.expect(&TokenKind::RParen).is_err() {
            return None;
        }
        if self.expect(&TokenKind::FatArrow).is_err() {
            return None;
        }
        let body = self.parse_expr()?;
        let span = start_span.merge(body.span());
        Some(Expr::Lambda(params, Box::new(body), span))
    }

    /// Parse list literal: `[a, b, c]`
    fn parse_list_literal(&mut self) -> Option<Expr> {
        let start_span = self.current_span();
        self.advance(); // consume [
        let mut items = Vec::new();
        loop {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::RBracket | TokenKind::Eof) {
                break;
            }
            if let Some(expr) = self.parse_expr() {
                items.push(expr);
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
        if self.expect(&TokenKind::RBracket).is_err() {
            return None;
        }
        let span = start_span.merge(self.prev_span());
        Some(Expr::List(items, span))
    }

    /// Disambiguate between map literal and block expression.
    fn parse_map_or_block_expr(&mut self) -> Option<Expr> {
        // Look ahead to decide: if `{ let ...` it's a block expr,
        // if `{ ident = ...` or `{ "string" = ...` or `{ }` it's a map.
        let mut i = self.pos + 1;
        // Skip newlines
        while i < self.tokens.len() && matches!(self.tokens[i].kind, TokenKind::Newline) {
            i += 1;
        }
        if i < self.tokens.len() {
            match &self.tokens[i].kind {
                TokenKind::Let => return self.parse_block_expr(),
                TokenKind::RBrace => return self.parse_map_literal(), // empty map {}
                _ => {}
            }
            // Check if it's `ident =` or `"str" =`
            if matches!(
                self.tokens[i].kind,
                TokenKind::Ident(_) | TokenKind::StringLit(_)
            ) {
                let mut j = i + 1;
                while j < self.tokens.len() && matches!(self.tokens[j].kind, TokenKind::Newline) {
                    j += 1;
                }
                if j < self.tokens.len() && matches!(self.tokens[j].kind, TokenKind::Equals) {
                    return self.parse_map_literal();
                }
            }
        }
        // Default: try map literal
        self.parse_map_literal()
    }

    /// Parse map literal: `{ key = val, key2 = val2, ... }`
    fn parse_map_literal(&mut self) -> Option<Expr> {
        let start_span = self.current_span();
        self.advance(); // consume {
        let mut entries = Vec::new();
        loop {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::RBrace | TokenKind::Eof) {
                break;
            }
            // Parse key
            let key = match self.peek_kind().clone() {
                TokenKind::Ident(ref name) => {
                    let name = name.clone();
                    let span = self.current_span();
                    self.advance();
                    MapKey::Ident(Ident { name, span })
                }
                TokenKind::StringLit(_) => {
                    let s = self.parse_string_lit()?;
                    MapKey::String(s)
                }
                _ => {
                    self.diagnostics.error(
                        "expected map key (identifier or string)",
                        self.current_span(),
                    );
                    break;
                }
            };
            self.skip_newlines();
            if self.expect(&TokenKind::Equals).is_err() {
                break;
            }
            self.skip_newlines();
            let value = match self.parse_expr() {
                Some(v) => v,
                None => break,
            };
            entries.push((key, value));
            self.skip_newlines();
            // Allow comma or newline as separator
            if matches!(self.peek_kind(), TokenKind::Comma) {
                self.advance();
            }
        }
        self.skip_newlines();
        if self.expect(&TokenKind::RBrace).is_err() {
            return None;
        }
        let span = start_span.merge(self.prev_span());
        Some(Expr::Map(entries, span))
    }

    /// Parse block expression: `{ let x = 1; let y = 2; x + y }`
    fn parse_block_expr(&mut self) -> Option<Expr> {
        let start_span = self.current_span();
        self.advance(); // consume {
        let mut lets = Vec::new();
        loop {
            self.skip_newlines();
            if !matches!(self.peek_kind(), TokenKind::Let) {
                break;
            }
            let trivia = crate::lang::trivia::Trivia::empty();
            if let Some(binding) = self.parse_let_binding(vec![], trivia) {
                lets.push(binding);
            } else {
                break;
            }
        }
        self.skip_newlines();
        let final_expr = self.parse_expr()?;
        self.skip_newlines();
        if self.expect(&TokenKind::RBrace).is_err() {
            return None;
        }
        let span = start_span.merge(self.prev_span());
        Some(Expr::BlockExpr(lets, Box::new(final_expr), span))
    }

    /// Parse query pipeline: `selector [| filter]*`
    pub(crate) fn parse_query_pipeline(&mut self) -> Option<QueryPipeline> {
        self.skip_newlines();
        let start_span = self.current_span();
        let selector = self.parse_query_selector()?;
        let mut filters = Vec::new();

        loop {
            self.skip_newlines();
            if !matches!(self.peek_kind(), TokenKind::Pipe) {
                break;
            }
            self.advance(); // consume |
            self.skip_newlines();
            if let Some(filter) = self.parse_query_filter() {
                filters.push(filter);
            } else {
                break;
            }
        }

        let end_span = if let Some(last) = filters.last() {
            match last {
                QueryFilter::AttrComparison(_, _, e) => e.span(),
                QueryFilter::Projection(id) => id.span,
                QueryFilter::HasAttr(id) | QueryFilter::HasDecorator(id) => id.span,
                QueryFilter::DecoratorArgFilter(_, _, _, e) => e.span(),
            }
        } else {
            self.prev_span()
        };

        Some(QueryPipeline {
            selector,
            filters,
            span: start_span.merge(end_span),
        })
    }

    /// Parse a query selector.
    pub(crate) fn parse_query_selector(&mut self) -> Option<QuerySelector> {
        self.skip_newlines();
        match self.peek_kind().clone() {
            TokenKind::Dot => {
                self.advance();
                // Could be root `.` or path starting with `.`
                if matches!(self.peek_kind(), TokenKind::Dot) {
                    // `..` recursive descent
                    self.advance();
                    let kind_ident = self.expect_ident().ok()?;
                    // Check for #id
                    if matches!(self.peek_kind(), TokenKind::Hash) {
                        self.advance();
                        if let TokenKind::IdentifierLit(ref val) = self.peek_kind().clone() {
                            let val = val.clone();
                            let span = self.current_span();
                            self.advance();
                            Some(QuerySelector::RecursiveId(
                                kind_ident,
                                IdentifierLit { value: val, span },
                            ))
                        } else if let TokenKind::Ident(ref val) = self.peek_kind().clone() {
                            let val = val.clone();
                            let span = self.current_span();
                            self.advance();
                            Some(QuerySelector::RecursiveId(
                                kind_ident,
                                IdentifierLit { value: val, span },
                            ))
                        } else {
                            self.diagnostics
                                .error("expected identifier after '#'", self.current_span());
                            None
                        }
                    } else {
                        Some(QuerySelector::Recursive(kind_ident))
                    }
                } else {
                    // Root selector
                    Some(QuerySelector::Root)
                }
            }
            TokenKind::DotDot => {
                self.advance();
                let kind_ident = self.expect_ident().ok()?;
                if matches!(self.peek_kind(), TokenKind::Hash) {
                    self.advance();
                    if let TokenKind::IdentifierLit(ref val) = self.peek_kind().clone() {
                        let val = val.clone();
                        let span = self.current_span();
                        self.advance();
                        Some(QuerySelector::RecursiveId(
                            kind_ident,
                            IdentifierLit { value: val, span },
                        ))
                    } else if let TokenKind::Ident(ref val) = self.peek_kind().clone() {
                        let val = val.clone();
                        let span = self.current_span();
                        self.advance();
                        Some(QuerySelector::RecursiveId(
                            kind_ident,
                            IdentifierLit { value: val, span },
                        ))
                    } else {
                        self.diagnostics
                            .error("expected identifier after '#'", self.current_span());
                        None
                    }
                } else {
                    Some(QuerySelector::Recursive(kind_ident))
                }
            }
            TokenKind::Star => {
                self.advance();
                Some(QuerySelector::Wildcard)
            }
            TokenKind::Table => {
                let span = self.current_span();
                self.advance();
                // table#id → TableId, table."label" → TableLabel
                match self.peek_kind().clone() {
                    TokenKind::Hash => {
                        self.advance();
                        if let TokenKind::IdentifierLit(ref val) = self.peek_kind().clone() {
                            let val = val.clone();
                            let span = self.current_span();
                            self.advance();
                            Some(QuerySelector::TableId(IdentifierLit { value: val, span }))
                        } else if let TokenKind::Ident(ref val) = self.peek_kind().clone() {
                            let val = val.clone();
                            let span = self.current_span();
                            self.advance();
                            Some(QuerySelector::TableId(IdentifierLit { value: val, span }))
                        } else {
                            self.diagnostics
                                .error("expected identifier after '#'", self.current_span());
                            None
                        }
                    }
                    TokenKind::Dot => {
                        self.advance(); // consume .
                                        // table.something — treat as path
                        let table_ident = Ident {
                            name: "table".into(),
                            span,
                        };
                        let mut segments = vec![PathSegment::Ident(table_ident)];
                        if let TokenKind::Ident(ref name) = self.peek_kind().clone() {
                            let name = name.clone();
                            let span = self.current_span();
                            self.advance();
                            segments.push(PathSegment::Ident(Ident { name, span }));
                        }
                        while matches!(self.peek_kind(), TokenKind::Dot) {
                            self.advance();
                            if let TokenKind::Ident(ref name) = self.peek_kind().clone() {
                                let name = name.clone();
                                let span = self.current_span();
                                self.advance();
                                segments.push(PathSegment::Ident(Ident { name, span }));
                            } else {
                                break;
                            }
                        }
                        Some(QuerySelector::Path(segments))
                    }
                    _ => {
                        // bare `table` — treat as Kind
                        let ident = Ident {
                            name: "table".into(),
                            span,
                        };
                        Some(QuerySelector::Kind(ident))
                    }
                }
            }
            TokenKind::Ident(_) => {
                let ident = self.expect_ident().ok()?;
                // Check for #id, ."label", or .path
                match self.peek_kind().clone() {
                    TokenKind::Hash => {
                        self.advance();
                        if let TokenKind::IdentifierLit(ref val) = self.peek_kind().clone() {
                            let val = val.clone();
                            let span = self.current_span();
                            self.advance();
                            Some(QuerySelector::KindId(
                                ident,
                                IdentifierLit { value: val, span },
                            ))
                        } else if let TokenKind::Ident(ref val) = self.peek_kind().clone() {
                            let val = val.clone();
                            let span = self.current_span();
                            self.advance();
                            Some(QuerySelector::KindId(
                                ident,
                                IdentifierLit { value: val, span },
                            ))
                        } else {
                            self.diagnostics
                                .error("expected identifier after '#'", self.current_span());
                            None
                        }
                    }
                    TokenKind::Dot => {
                        // Path selector
                        let mut segments = vec![PathSegment::Ident(ident)];
                        while matches!(self.peek_kind(), TokenKind::Dot) {
                            self.advance(); // consume .
                            if let TokenKind::Ident(ref name) = self.peek_kind().clone() {
                                let name = name.clone();
                                let span = self.current_span();
                                self.advance();
                                segments.push(PathSegment::Ident(Ident { name, span }));
                            } else {
                                break;
                            }
                        }
                        Some(QuerySelector::Path(segments))
                    }
                    _ => Some(QuerySelector::Kind(ident)),
                }
            }
            _ => {
                self.diagnostics.error(
                    format!("expected query selector, found {:?}", self.peek_kind()),
                    self.current_span(),
                );
                None
            }
        }
    }

    /// Parse a query filter (after `|`).
    pub(crate) fn parse_query_filter(&mut self) -> Option<QueryFilter> {
        self.skip_newlines();
        match self.peek_kind().clone() {
            TokenKind::Dot => {
                self.advance(); // consume .
                let attr_ident = self.expect_ident().ok()?;
                // Check for comparison operator
                if let Some(op) = self.token_to_binop() {
                    self.advance(); // consume op
                    let expr = self.parse_expr()?;
                    Some(QueryFilter::AttrComparison(attr_ident, op, expr))
                } else if matches!(self.peek_kind(), TokenKind::Equals) {
                    self.diagnostics.error(
                        "use `==` for equality comparison in query filters, not `=`",
                        self.current_span(),
                    );
                    None
                } else {
                    Some(QueryFilter::Projection(attr_ident))
                }
            }
            TokenKind::Ident(ref name) if name == "has" => {
                self.advance(); // consume `has`
                if self.expect(&TokenKind::LParen).is_err() {
                    return None;
                }
                self.skip_newlines();
                if matches!(self.peek_kind(), TokenKind::At) {
                    self.advance(); // consume @
                    let name_ident = self.expect_ident().ok()?;
                    if self.expect(&TokenKind::RParen).is_err() {
                        return None;
                    }
                    Some(QueryFilter::HasDecorator(name_ident))
                } else if matches!(self.peek_kind(), TokenKind::Dot) {
                    self.advance(); // consume .
                    let attr_ident = self.expect_ident().ok()?;
                    if self.expect(&TokenKind::RParen).is_err() {
                        return None;
                    }
                    Some(QueryFilter::HasAttr(attr_ident))
                } else {
                    self.diagnostics.error(
                        "expected '.attr' or '@decorator' inside has()",
                        self.current_span(),
                    );
                    None
                }
            }
            TokenKind::At => {
                self.advance(); // consume @
                let dec_name = self.expect_ident().ok()?;
                if self.expect(&TokenKind::Dot).is_err() {
                    return None;
                }
                let param_name = self.expect_ident().ok()?;
                let op = match self.token_to_binop() {
                    Some(op) => {
                        self.advance();
                        op
                    }
                    None => {
                        self.diagnostics.error(
                            "expected comparison operator in decorator filter",
                            self.current_span(),
                        );
                        return None;
                    }
                };
                let expr = self.parse_expr()?;
                Some(QueryFilter::DecoratorArgFilter(
                    dec_name, param_name, op, expr,
                ))
            }
            _ => {
                self.diagnostics.error(
                    format!("expected query filter, found {:?}", self.peek_kind()),
                    self.current_span(),
                );
                None
            }
        }
    }

    /// Parse `ref(target)` expression.
    ///
    /// Accepts bare identifiers (`ref(alpha)`) or string paths (`ref("alpha.http")`, `ref("../beta")`).
    pub(crate) fn parse_ref_expr(&mut self) -> Option<Expr> {
        let start_span = self.current_span();
        self.advance(); // consume `ref`
        if self.expect(&TokenKind::LParen).is_err() {
            return None;
        }
        let target = match self.peek_kind().clone() {
            TokenKind::IdentifierLit(ref val) => {
                let val = val.clone();
                let span = self.current_span();
                self.advance();
                RefTarget::Bare(IdentifierLit { value: val, span })
            }
            TokenKind::Ident(ref val) => {
                let val = val.clone();
                let span = self.current_span();
                self.advance();
                RefTarget::Bare(IdentifierLit { value: val, span })
            }
            TokenKind::StringLit(ref val) => {
                let val = val.clone();
                let span = self.current_span();
                self.advance();
                RefTarget::Path(StringLit {
                    parts: vec![StringPart::Literal(val)],
                    heredoc: None,
                    span,
                })
            }
            _ => {
                self.diagnostics.error(
                    "expected identifier or string path in ref()",
                    self.current_span(),
                );
                return None;
            }
        };
        if self.expect(&TokenKind::RParen).is_err() {
            return None;
        }
        let span = start_span.merge(self.prev_span());
        Some(Expr::Ref(target, RefStyle::Long, span))
    }

    /// Parse `#name` shorthand for `ref(name)`.
    ///
    /// Only accepts a bare identifier operand. String operands such as
    /// `#"alpha.http"` or `#"../beta"` are rejected with a hint to use the
    /// long `ref("...")` form.
    pub(crate) fn parse_ref_shorthand(&mut self) -> Option<Expr> {
        let start_span = self.current_span();
        self.advance(); // consume `#`
        let target = match self.peek_kind().clone() {
            TokenKind::IdentifierLit(ref val) => {
                let val = val.clone();
                let span = self.current_span();
                self.advance();
                RefTarget::Bare(IdentifierLit { value: val, span })
            }
            TokenKind::Ident(ref val) => {
                let val = val.clone();
                let span = self.current_span();
                self.advance();
                RefTarget::Bare(IdentifierLit { value: val, span })
            }
            TokenKind::StringLit(_) | TokenKind::Heredoc { .. } => {
                self.diagnostics.error(
                    "string operand not allowed after '#'; use ref(\"...\") for qualified or relative paths",
                    self.current_span(),
                );
                return None;
            }
            _ => {
                self.diagnostics
                    .error("expected identifier after '#'", self.current_span());
                return None;
            }
        };
        let span = start_span.merge(self.prev_span());
        Some(Expr::Ref(target, RefStyle::Short, span))
    }

    /// Parse `import_raw("path")` expression.
    fn parse_import_raw_expr(&mut self) -> Option<Expr> {
        let start_span = self.current_span();
        self.advance(); // consume `import_raw`
        if self.expect(&TokenKind::LParen).is_err() {
            return None;
        }
        let path = self.parse_string_lit()?;
        if self.expect(&TokenKind::RParen).is_err() {
            return None;
        }
        let span = start_span.merge(self.prev_span());
        Some(Expr::ImportRaw(path, span))
    }

    /// Parse `import_table("path" [, ...])` expression.
    ///
    /// Supports:
    /// - `import_table("path")` — defaults
    /// - `import_table("path", "\t")` — legacy positional separator
    /// - `import_table("path", headers=false, columns=["a", "b"], separator="\t")`
    fn parse_import_table_expr(&mut self) -> Option<Expr> {
        let start_span = self.current_span();
        self.advance(); // consume `import_table`
        if self.expect(&TokenKind::LParen).is_err() {
            return None;
        }
        let path = self.parse_string_lit()?;

        let mut separator = None;
        let mut headers = None;
        let mut columns = None;

        while matches!(self.peek_kind(), TokenKind::Comma) {
            self.advance(); // consume `,`
            self.skip_newlines();

            if matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                break; // trailing comma
            }

            // Check for named arg: ident = expr
            if let TokenKind::Ident(ref name) = self.peek_kind().clone() {
                let name_clone = name.clone();
                let mut j = self.pos + 1;
                while j < self.tokens.len() && matches!(self.tokens[j].kind, TokenKind::Newline) {
                    j += 1;
                }
                if j < self.tokens.len() && matches!(self.tokens[j].kind, TokenKind::Equals) {
                    // Named argument
                    self.advance(); // consume ident
                    self.skip_newlines();
                    self.advance(); // consume =
                    self.skip_newlines();
                    match name_clone.as_str() {
                        "separator" => {
                            separator = Some(self.parse_string_lit()?);
                        }
                        "headers" => match self.peek_kind().clone() {
                            TokenKind::BoolLit(b) => {
                                headers = Some(b);
                                self.advance();
                            }
                            _ => {
                                self.diagnostics.error(
                                    "expected bool for 'headers' parameter",
                                    self.current_span(),
                                );
                                return None;
                            }
                        },
                        "columns" => {
                            // Parse list of string literals: ["a", "b"]
                            if self.expect(&TokenKind::LBracket).is_err() {
                                return None;
                            }
                            let mut col_names = Vec::new();
                            loop {
                                self.skip_newlines();
                                if matches!(self.peek_kind(), TokenKind::RBracket | TokenKind::Eof)
                                {
                                    break;
                                }
                                col_names.push(self.parse_string_lit()?);
                                self.skip_newlines();
                                if matches!(self.peek_kind(), TokenKind::Comma) {
                                    self.advance();
                                } else {
                                    break;
                                }
                            }
                            self.skip_newlines();
                            let _ = self.expect(&TokenKind::RBracket);
                            columns = Some(col_names);
                        }
                        other => {
                            self.diagnostics.error(
                                format!("unknown import_table parameter '{}'", other),
                                self.current_span(),
                            );
                            return None;
                        }
                    }
                    continue;
                }
            }

            // Legacy positional: second arg is a string separator
            if matches!(
                self.peek_kind(),
                TokenKind::StringLit(_) | TokenKind::Heredoc { .. }
            ) {
                separator = Some(self.parse_string_lit()?);
            } else {
                self.diagnostics.error(
                    "expected string literal or named argument in import_table()",
                    self.current_span(),
                );
                return None;
            }
        }

        if self.expect(&TokenKind::RParen).is_err() {
            return None;
        }
        let span = start_span.merge(self.prev_span());
        Some(Expr::ImportTable(
            ImportTableArgs {
                path,
                separator,
                headers,
                columns,
            },
            span,
        ))
    }

    /// Parse call arguments: `(arg1, arg2, name = val, ...)`
    /// Returns the args; expects that the opening `(` has NOT been consumed yet.
    pub(crate) fn parse_call_args(&mut self) -> Vec<CallArg> {
        let mut args = Vec::new();
        if self.expect(&TokenKind::LParen).is_err() {
            return args;
        }
        loop {
            self.skip_newlines();
            if matches!(self.peek_kind(), TokenKind::RParen | TokenKind::Eof) {
                break;
            }
            // Try named argument: ident = expr
            if let TokenKind::Ident(ref name) = self.peek_kind().clone() {
                let name = name.clone();
                // Look ahead for `=`
                let mut j = self.pos + 1;
                while j < self.tokens.len() && matches!(self.tokens[j].kind, TokenKind::Newline) {
                    j += 1;
                }
                if j < self.tokens.len() && matches!(self.tokens[j].kind, TokenKind::Equals) {
                    // Named argument
                    let span = self.current_span();
                    self.advance(); // consume ident
                    self.skip_newlines();
                    self.advance(); // consume =
                    self.skip_newlines();
                    if let Some(val) = self.parse_expr() {
                        args.push(CallArg::Named(Ident { name, span }, val));
                    }
                } else {
                    // Positional
                    if let Some(expr) = self.parse_expr() {
                        args.push(CallArg::Positional(expr));
                    }
                }
            } else if let Some(expr) = self.parse_expr() {
                args.push(CallArg::Positional(expr));
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
}
