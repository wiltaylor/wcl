//! Printing type references. The counterpart of
//! [`parser::types`](crate::parser).

use std::fmt::Write as _;

use crate::value::{BuiltinType, TensorDim, TypeRef};

use super::Printer;

impl Printer {
    /// Print a type as written.
    pub(super) fn print_type_ref(&mut self, t: &TypeRef) {
        match t {
            TypeRef::Builtin(b) => self.push(builtin_name(*b)),
            TypeRef::Named { path, args } => {
                self.push(&path.join("."));
                if !args.is_empty() {
                    self.push("<");
                    for (i, a) in args.iter().enumerate() {
                        if i > 0 {
                            self.push(", ");
                        }
                        self.print_type_ref(a);
                    }
                    self.push(">");
                }
            }
            TypeRef::Reference(inner) => {
                self.push("&");
                self.print_type_ref(inner);
            }
            TypeRef::List(inner) => {
                self.push("list<");
                self.print_type_ref(inner);
                self.push(">");
            }
            TypeRef::Tensor { element, dims } => {
                self.push("tensor<");
                self.print_type_ref(element);
                self.push(", [");
                for (i, d) in dims.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    match d {
                        TensorDim::Fixed(n) => {
                            let _ = write!(self.buf, "{n}");
                        }
                        TensorDim::Symbolic(s) => self.push(s),
                    }
                }
                self.push("]>");
            }
            TypeRef::Function { params, return_ty } => {
                self.push("fn(");
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        self.push(", ");
                    }
                    self.print_type_ref(p);
                }
                self.push(") -> ");
                self.print_type_ref(return_ty);
            }
        }
    }
}

/// The source spelling of a builtin type.
pub(super) fn builtin_name(b: BuiltinType) -> &'static str {
    b.name()
}
