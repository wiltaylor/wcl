//! WCL Eval — Evaluation, imports, macros, merge, query, built-in functions

pub mod control_flow;
pub mod evaluator;
pub mod functions;
pub mod imports;
pub mod macros;
pub mod merge;
pub mod query;
pub mod scope;
pub mod value;

pub use value::{Value, BlockRef, DecoratorValue, FunctionValue, FunctionBody, ScopeId};
pub use scope::{ScopeArena, Scope, ScopeEntry, ScopeEntryKind, ScopeKind};
pub use imports::{ImportResolver, FileSystem, RealFileSystem, InMemoryFs};
pub use macros::{MacroRegistry, MacroExpander};
pub use control_flow::ControlFlowExpander;
pub use merge::{PartialMerger, ConflictMode};
pub use evaluator::Evaluator;
pub use query::QueryEngine;
