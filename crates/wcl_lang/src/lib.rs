//! WCL language library.
//!
//! Two entry points, mutually exclusive by design:
//!
//! - **Evaluating path** — [`Document::open`] / [`Document::open_with`] parse a
//!   source string and return a lazy, evaluation-only view. Fields evaluate
//!   on first access and cache; the document is otherwise immutable.
//! - **Editing path** — [`parse_for_edit`] returns an owned [`ast::Source`]
//!   with public fields. Hosts inspect or mutate the AST. They then print it
//!   back to a `.wcl` file with [`format::to_source`]. To evaluate after an
//!   edit, reopen the file as a `Document`.
//!
//! There is no AST escape hatch on `Document`; mixing edit + evaluate inside
//! one process state would silently invalidate the document's cell caches,
//! so the API forces the host to pick one mode per parse.

pub mod ast;
mod data;
/// The document model: opening, evaluating and querying a file.
mod doc;
pub mod edit;
mod environment;
/// Parse and evaluation error types, and their diagnostics.
mod error;
pub mod format;
mod functions;
mod lexer;
mod math;
mod numeric;
/// The recursive-descent parser: tokens in, syntax tree out.
mod parser;
mod paths;
mod profile;
mod reflect;
/// The name index built during parsing.
mod symbols;
mod units;
/// Runtime values and the type references that describe them.
mod value;

pub use ast::Span;
pub use data::{DataKind, DataRef};
pub use doc::{
    Block, ChildKind, Connection, ConnectionDecl, DeclName, DeclaresKind, Decorator, Document,
    Field, FieldShape, FileLoader, InterfaceDecl, NamedArg, Registry, ResolvedType, RowView,
    SYSTEM_IMPORT_ROOT, SymbolEntry, SymbolHit, SymbolSetDecl, TableView, TypeDecl, TypeField,
    UnionDecl, UnionVariant, UseDeclView, UseFormView, UseItem, VariantBodyView, disk_loader,
    overlay_loader, system_import_key,
};
pub use environment::{
    BuiltType, DecoratorBuilder, Environment, Expander, TypeBuilder, TypeFieldBuilder,
};
pub use error::{ArithmeticFault, EvalError, ParseError, SchemaViolationKind, SyntaxError};
pub use functions::{
    BuiltinFn, Caller, DataPath, FromValue, IntoBuiltin, IntoValue, IntoValueResult, from_fn,
};
pub use lexer::{
    LexError, Lexer, NumberLit, StringEncoding, StringLit, StringPart, Token, TokenKind,
    is_identifier,
};
pub use profile::{Profile, ProfileKey, ProfileNode};
pub use symbols::{SymbolIndex, SymbolKind, SymbolPath, SymbolRecord};
pub use value::{BuiltinType, FnParam, FnValue, TensorDim, TypeRef, Value, VariantPayload};

/// Parse a WCL source string into an owned [`ast::Source`] for inspection
/// or mutation. The returned AST has fully `pub` fields. Hosts walk it,
/// edit it, and print it back to a `.wcl` file with
/// [`format::to_source`].
///
/// This is the **edit-path** entry point. It performs *no* evaluation,
/// schema checks, or import resolution — those happen only when a
/// [`Document`] is opened from the (post-edit) file. The two paths are
/// deliberately disjoint so AST mutations can't invalidate a
/// Document's cached fields silently.
///
/// `name` is used for diagnostics only (it becomes the
/// `NamedSource` label on any [`ParseError`]).
pub fn parse_for_edit(source: &str, name: impl Into<String>) -> Result<ast::Source, ParseError> {
    parser::Parser::new(source, name)
        .parse_source()
        .map(|(src, _idx)| src)
}

/// Parse a single WCL expression from a standalone string. Returns the
/// parsed [`ast::Expr`] ready to drop into a host-mutated AST
/// (e.g. `field.expr = parse_expr(...)?`).
///
/// Fails if the input is empty, has trailing tokens after the
/// expression, or contains a lex/parse error. `name` is used only for
/// diagnostics — typically `"<cli>"` or `"<set value>"` when there's
/// no real source location.
///
/// Useful for CLI flows like `wcl set file path <value>`, where
/// `<value>` is a literal expression supplied on the command line.
pub fn parse_expr(source: &str, name: impl Into<String>) -> Result<ast::Expr, ParseError> {
    parser::Parser::new(source, name).parse_expr_only()
}
