//! The syntax tree the parser builds and the formatter prints.
//!
//! This is a **lossless-enough** tree: it carries everything needed to
//! re-emit source that round-trips through `parse → print → parse`, which
//! is what `wcl fmt` and `wcl set` depend on. Concretely, that means every
//! node carries a [`Span`] into the original text, and most carry
//! [`Trivia`] — the comments and blank-line groupings that would otherwise
//! be lost. What is *not* preserved is normalized on purpose: indentation,
//! brace style, number radix and string-delimiter choice all come back in
//! canonical form.
//!
//! Nodes are built by the parser, printed by [`crate::format`], and
//! evaluated by the document layer. The evaluator ignores trivia
//! entirely.
//!
//! Every type is re-exported here, so `ast::Expr` is its path no matter
//! which file declares it. The tree is split three ways, and the first
//! three modules sit at the same relative path as the code that reads
//! and writes them — `ast::expr` is what `parser::expr` parses and
//! `format::expr` prints:
//!
//! - `expr` / `pattern` / `types` — the expression language, the
//!   patterns that destructure it, and the types that describe it.
//! - `items` / `decls` / `decorators` — what a file is made of: the
//!   data items, the declarations that give them a schema, and the
//!   annotations attached to either.
//! - `span` / `trivia` / `synthetic` — what every node carries in
//!   addition to its own content, and how a node with no source behind
//!   it is built.

mod decls;
mod decorators;
mod expr;
mod items;
mod pattern;
mod span;
mod synthetic;
mod trivia;
mod types;

pub use decls::{
    ConnectionDecl, ImportDecl, InterfaceDecl, NamespaceDecl, SymbolEntry, SymbolSetDecl, TypeDecl,
    TypeField, UnionDecl, UnionVariant, UseDecl, UseForm, UseItem, VariantBody,
};
pub use decorators::Decorator;
pub(crate) use decorators::SchemalessMode;
pub use expr::{
    BinOp, CALL_BP, Expr, FunctionLit, LetBinding, MEMBER_BP, MatchArm, NamedArg, Parameter,
    TemplatePart, UNARY_BP, UnaryOp, VariantArgs,
};
pub use items::{Block, ConnectionStmt, Field, Item, LetItem, Row, SlotDecl, Source, TableItem};
pub use pattern::{Pattern, VariantPatArgs};
pub use span::Span;
pub(crate) use synthetic::{synthetic_decorator, synthetic_field, synthetic_span};
pub use trivia::{ElemTrivia, Trivia};
pub use types::{BuiltinType, TensorDim, TypeRef};
