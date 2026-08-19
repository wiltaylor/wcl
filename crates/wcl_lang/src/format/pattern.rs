//! Printing patterns. The counterpart of
//! [`parser::pattern`](crate::parser).

use crate::ast::*;
use crate::lexer::StringEncoding;

use super::expr::number_lit_to_expr;
use super::{Printer, join_path};

impl Printer {
    /// Print a pattern, in any of its forms.
    pub(super) fn print_pattern(&mut self, p: &Pattern) {
        match p {
            Pattern::Wildcard(_) => self.push("_"),
            Pattern::Binding { name, .. } => self.push(name),
            Pattern::At { name, inner, .. } => {
                self.push(name);
                self.push(" @ ");
                self.print_pattern(inner);
            }
            Pattern::LiteralBool(b, _) => self.push(if *b { "true" } else { "false" }),
            Pattern::LiteralNumber { lit, .. } => {
                // NumberLit's Debug renders the value with its
                // typed-variant suffix, but that's not the source
                // form. Map to the same logic as Expr literals.
                let synthesized = number_lit_to_expr(lit);
                self.print_expr(&synthesized, 0);
            }
            Pattern::LiteralUtf8(s, _) => self.print_string_lit(s, StringEncoding::Utf8),
            Pattern::LiteralAscii(s, _) => self.print_string_lit(s, StringEncoding::Ascii),
            Pattern::LiteralSymbol(s, _) => {
                self.push(":");
                self.push(s);
            }
            Pattern::LiteralNone(_) => self.push("none"),
            Pattern::Variant {
                type_path,
                variant,
                args,
                ..
            } => {
                if !type_path.is_empty() {
                    self.push(&join_path(type_path));
                    self.push("::");
                }
                self.push(variant);
                match args {
                    VariantPatArgs::Unit => {}
                    VariantPatArgs::Positional(inner) => {
                        self.push("(");
                        self.print_pattern(inner);
                        self.push(")");
                    }
                    VariantPatArgs::Record { fields, rest } => {
                        self.push(" { ");
                        for (i, (name, pat)) in fields.iter().enumerate() {
                            if i > 0 {
                                self.push(", ");
                            }
                            self.push(name);
                            self.push(": ");
                            self.print_pattern(pat);
                        }
                        if *rest {
                            if !fields.is_empty() {
                                self.push(", ");
                            }
                            self.push("..");
                        }
                        self.push(" }");
                    }
                }
            }
        }
    }
}
