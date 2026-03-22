//! WCL Eval — Evaluation, imports, macros, merge, query, built-in functions
#![allow(clippy::result_large_err)]

pub mod control_flow;
pub mod evaluator;
pub mod functions;
pub mod imports;
pub mod macros;
pub mod merge;
pub mod query;
pub mod scope;
pub mod value;

pub use control_flow::ControlFlowExpander;
pub use evaluator::Evaluator;
pub use functions::{builtin_signatures, BuiltinFn, FunctionRegistry, FunctionSignature};
pub use imports::{
    library_search_paths, resolve_library_import, FileSystem, ImportResolver, InMemoryFs,
    LibraryConfig, RealFileSystem,
};
pub use macros::{MacroExpander, MacroRegistry};
pub use merge::{ConflictMode, PartialMerger};
pub use query::QueryEngine;
pub use scope::{Scope, ScopeArena, ScopeEntry, ScopeEntryKind, ScopeKind};
pub use value::{BlockRef, DecoratorValue, FunctionBody, FunctionValue, ScopeId, Value};
