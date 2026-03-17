// parser/common.rs — Shared parser helpers

use crate::lexer::{Token, TokenKind};

use super::Parser;

impl Parser {
    /// Check current token kind with a predicate (ignoring inner values).
    pub(crate) fn at_kind(&self, f: impl Fn(&TokenKind) -> bool) -> bool {
        f(self.peek_kind())
    }

    /// Consume and return the current token if it matches the predicate.
    pub(crate) fn eat_if(&mut self, f: impl Fn(&TokenKind) -> bool) -> Option<Token> {
        if f(self.peek_kind()) {
            Some(self.advance())
        } else {
            None
        }
    }
}
