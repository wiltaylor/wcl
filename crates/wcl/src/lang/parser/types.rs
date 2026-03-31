// parser/types.rs — Type expression parser

use crate::lang::ast::*;
use crate::lang::lexer::TokenKind;

use super::Parser;

impl Parser {
    /// Parse a type expression.
    ///
    /// Handles: string, i8, u8, ..., i128, u128, f32, f64, date, duration,
    /// bool, null, identifier, any, symbol,
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
                    "i8" => {
                        self.advance();
                        Some(TypeExpr::I8(start_span))
                    }
                    "u8" => {
                        self.advance();
                        Some(TypeExpr::U8(start_span))
                    }
                    "i16" => {
                        self.advance();
                        Some(TypeExpr::I16(start_span))
                    }
                    "u16" => {
                        self.advance();
                        Some(TypeExpr::U16(start_span))
                    }
                    "i32" => {
                        self.advance();
                        Some(TypeExpr::I32(start_span))
                    }
                    "u32" => {
                        self.advance();
                        Some(TypeExpr::U32(start_span))
                    }
                    "i64" => {
                        self.advance();
                        Some(TypeExpr::I64(start_span))
                    }
                    "u64" => {
                        self.advance();
                        Some(TypeExpr::U64(start_span))
                    }
                    "i128" => {
                        self.advance();
                        Some(TypeExpr::I128(start_span))
                    }
                    "u128" => {
                        self.advance();
                        Some(TypeExpr::U128(start_span))
                    }
                    "f32" => {
                        self.advance();
                        Some(TypeExpr::F32(start_span))
                    }
                    "f64" => {
                        self.advance();
                        Some(TypeExpr::F64(start_span))
                    }
                    "date" => {
                        self.advance();
                        Some(TypeExpr::Date(start_span))
                    }
                    "duration" => {
                        self.advance();
                        Some(TypeExpr::Duration(start_span))
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
                    "symbol" => {
                        self.advance();
                        Some(TypeExpr::Symbol(start_span))
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
                    "pattern" => {
                        self.advance();
                        Some(TypeExpr::Pattern(start_span))
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
                        // Treat unknown identifiers as struct type references
                        let ident = Ident {
                            name: name.clone(),
                            span: start_span,
                        };
                        self.advance();
                        Some(TypeExpr::StructType(ident, start_span))
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
