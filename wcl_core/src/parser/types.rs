// parser/types.rs — Type expression parser

use crate::ast::*;
use crate::lexer::TokenKind;

use super::Parser;

impl Parser {
    /// Parse a type expression.
    ///
    /// Handles: string, int, float, bool, null, identifier, any,
    /// list(T), map(K,V), set(T), ref("name"), union(T1, T2, ...)
    pub(crate) fn parse_type_expr(&mut self) -> Option<TypeExpr> {
        self.skip_newlines();
        let start_span = self.current_span();

        match self.peek_kind().clone() {
            TokenKind::Ident(ref name) => {
                let name = name.clone();
                match name.as_str() {
                    "string" => {
                        self.advance();
                        Some(TypeExpr::String(start_span))
                    }
                    "int" => {
                        self.advance();
                        Some(TypeExpr::Int(start_span))
                    }
                    "float" => {
                        self.advance();
                        Some(TypeExpr::Float(start_span))
                    }
                    "bool" => {
                        self.advance();
                        Some(TypeExpr::Bool(start_span))
                    }
                    "null" => {
                        self.advance();
                        Some(TypeExpr::Null(start_span))
                    }
                    "identifier" => {
                        self.advance();
                        Some(TypeExpr::Identifier(start_span))
                    }
                    "any" => {
                        self.advance();
                        Some(TypeExpr::Any(start_span))
                    }
                    "list" => {
                        self.advance();
                        if self.expect(&TokenKind::LParen).is_err() {
                            return None;
                        }
                        let inner = self.parse_type_expr()?;
                        if self.expect(&TokenKind::RParen).is_err() {
                            return None;
                        }
                        let end_span = self.prev_span();
                        Some(TypeExpr::List(Box::new(inner), start_span.merge(end_span)))
                    }
                    "map" => {
                        self.advance();
                        if self.expect(&TokenKind::LParen).is_err() {
                            return None;
                        }
                        let key = self.parse_type_expr()?;
                        if self.expect(&TokenKind::Comma).is_err() {
                            return None;
                        }
                        let value = self.parse_type_expr()?;
                        if self.expect(&TokenKind::RParen).is_err() {
                            return None;
                        }
                        let end_span = self.prev_span();
                        Some(TypeExpr::Map(
                            Box::new(key),
                            Box::new(value),
                            start_span.merge(end_span),
                        ))
                    }
                    "set" => {
                        self.advance();
                        if self.expect(&TokenKind::LParen).is_err() {
                            return None;
                        }
                        let inner = self.parse_type_expr()?;
                        if self.expect(&TokenKind::RParen).is_err() {
                            return None;
                        }
                        let end_span = self.prev_span();
                        Some(TypeExpr::Set(Box::new(inner), start_span.merge(end_span)))
                    }
                    "union" => {
                        self.advance();
                        if self.expect(&TokenKind::LParen).is_err() {
                            return None;
                        }
                        let mut types = Vec::new();
                        loop {
                            self.skip_newlines();
                            if matches!(self.peek_kind(), TokenKind::RParen) {
                                break;
                            }
                            if let Some(t) = self.parse_type_expr() {
                                types.push(t);
                            } else {
                                break;
                            }
                            if !matches!(self.peek_kind(), TokenKind::Comma) {
                                break;
                            }
                            self.advance(); // consume comma
                        }
                        if self.expect(&TokenKind::RParen).is_err() {
                            return None;
                        }
                        let end_span = self.prev_span();
                        Some(TypeExpr::Union(types, start_span.merge(end_span)))
                    }
                    "function" => {
                        self.diagnostics.error(
                            "the `function` type cannot be used in schema field declarations",
                            start_span,
                        );
                        self.advance();
                        None
                    }
                    _ => {
                        // Unknown type name — treat as error
                        self.diagnostics.error(
                            format!("unknown type: {}", name),
                            start_span,
                        );
                        self.advance();
                        None
                    }
                }
            }
            TokenKind::Ref => {
                // ref("schema_name")
                self.advance();
                if self.expect(&TokenKind::LParen).is_err() {
                    return None;
                }
                let s = self.parse_string_lit()?;
                if self.expect(&TokenKind::RParen).is_err() {
                    return None;
                }
                let end_span = self.prev_span();
                Some(TypeExpr::Ref(s, start_span.merge(end_span)))
            }
            TokenKind::NullLit => {
                self.advance();
                Some(TypeExpr::Null(start_span))
            }
            _ => {
                self.diagnostics.error(
                    format!("expected type expression, found {:?}", self.peek_kind()),
                    start_span,
                );
                None
            }
        }
    }
}
