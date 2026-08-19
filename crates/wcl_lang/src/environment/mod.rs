//! The host environment: synthetic schema declarations + registered built-in
//! functions + the expander for `@contextual` block kinds.
//!
//! Lets a Rust embedder ship "built-in" declarations and Rust callables
//! that participate in a document's type registry and evaluator. The
//! registration surface is here; the two things registered *through* it
//! split by who supplies them:
//!
//! - [`stdlib`] — the declarations the **language** itself ships: the
//!   `@block` / `@children` / `@decorator` / `@unit` … decorator schemas,
//!   the built-in unit types, and the closed vocabulary of decorator
//!   positions. They are pre-registered in every [`Environment::new`] so
//!   they land in the same registry a user declaration does, and there is
//!   one lookup rule rather than two.
//! - [`builder`] — the fluent API a **host** builds its own declarations
//!   with, and hands to [`Environment::add_type`].

mod builder;
mod stdlib;

use std::collections::HashMap;
use std::sync::Arc;

use crate::ast;
use crate::doc::Block;
use crate::functions::BuiltinFn;

pub use builder::{BuiltType, DecoratorBuilder, TypeBuilder, TypeFieldBuilder};
use stdlib::{builtin_decorator_schemas, decorator_position_set, stdlib_unit_types};

/// Host callback that expands a `@contextual` block into the blocks it
/// generates.
///
/// A decorator can declare *that* a block expands; it cannot carry *how*
/// ("iterate `each`, bind each element to the symbol named by `as`";
/// "bind parameter names to the instance's fields, falling back to each
/// parameter's default"). That is behaviour, so it lives here — with the
/// host that defines the vocabulary — and the language consults it when
/// it projects children.
///
/// Build the returned blocks with [`Block::expand_bodies`], which carries
/// the per-expansion bindings and gives each expansion its own evaluation
/// cache. A kind this expander does not generate from returns an empty
/// list.
pub trait Expander: Send + Sync {
    /// Produce the children of a `@contextual` block. An empty vector
    /// means the block expands to nothing.
    fn expand<'a>(&self, block: &Block<'a>) -> Vec<Block<'a>>;
}

/// Host-supplied bundle of synthetic declarations and built-in functions merged
/// into a [`Document`](crate::Document) at open time.
///
/// Use [`Environment::new`] for an environment pre-populated with the
/// language-built-in decorator schemas and functions; use
/// [`Environment::empty`] for a strictly empty one.
#[derive(Clone, Default)]
pub struct Environment {
    /// Type declarations the host supplies, on top of the language's own.
    types: Vec<ast::TypeDecl>,
    /// Symbol sets the host supplies.
    symbol_sets: Vec<ast::SymbolSetDecl>,
    /// Host functions callable from document expressions, by name.
    builtins: HashMap<String, BuiltinFn>,
    /// Expander for `@contextual` blocks, if the host registered one.
    expander: Option<Arc<dyn Expander>>,
}

impl std::fmt::Debug for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Environment")
            .field("types", &self.types.len())
            .field("symbol_sets", &self.symbol_sets.len())
            .field("builtins", &self.builtins.keys().collect::<Vec<_>>())
            .field("expander", &self.expander.is_some())
            .finish()
    }
}

impl Environment {
    /// Environment pre-populated with the built-in decorator schemas and an
    /// empty builtins map.
    pub fn new() -> Self {
        let mut env = Self::empty();
        env.types.extend(builtin_decorator_schemas());
        env.symbol_sets.push(decorator_position_set());
        env.types.extend(stdlib_unit_types());
        crate::functions::register(&mut env);
        env
    }

    /// Strictly-empty environment. No synthetic types, no builtins.
    pub fn empty() -> Self {
        Self {
            types: Vec::new(),
            symbol_sets: Vec::new(),
            builtins: HashMap::new(),
            expander: None,
        }
    }

    /// Register a programmatically built type declaration.
    pub fn add_type(&mut self, t: BuiltType) -> &mut Self {
        self.types.push(t.inner);
        self
    }

    /// Register a built-in function callable from WCL code by `name`.
    ///
    /// Use [`from_fn`](crate::from_fn) (or build a [`BuiltinFn`] manually)
    /// to construct the second argument.
    pub fn add_builtin(&mut self, name: impl Into<String>, f: BuiltinFn) -> &mut Self {
        self.builtins.insert(name.into(), f);
        self
    }

    /// Register the [`Expander`] consulted when a `@contextual` block's
    /// generated children are demanded. Without one, demanding them is a
    /// hard error ([`EvalError::MissingExpander`](crate::EvalError)) —
    /// the language never guesses at a host's expansion semantics.
    pub fn set_expander(&mut self, expander: Arc<dyn Expander>) -> &mut Self {
        self.expander = Some(expander);
        self
    }

    /// The registered `@contextual` expander, if any.
    pub(crate) fn expander(&self) -> Option<&dyn Expander> {
        self.expander.as_deref()
    }

    /// The host-supplied type declarations.
    pub(crate) fn types(&self) -> &[ast::TypeDecl] {
        &self.types
    }

    /// The host-supplied symbol sets.
    pub(crate) fn symbol_sets(&self) -> &[ast::SymbolSetDecl] {
        &self.symbol_sets
    }

    /// The host builtin registered under `name`, if any.
    pub(crate) fn builtin(&self, name: &str) -> Option<&BuiltinFn> {
        self.builtins.get(name)
    }

    /// Iterate registered built-in callables as `(name, &BuiltinFn)`
    /// pairs. Hosts can read each builtin's arity and (when present)
    /// signature directly off the [`BuiltinFn`] for completion / hover
    /// tooling.
    pub fn builtins(&self) -> impl Iterator<Item = (&str, &BuiltinFn)> {
        self.builtins.iter().map(|(name, f)| (name.as_str(), f))
    }
}

#[cfg(test)]
mod tests;
