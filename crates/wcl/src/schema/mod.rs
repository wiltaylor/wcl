//! WCL Schema — type checking, schema validation, decorator validation,
//! document validation, and ID uniqueness.

pub mod decorator;
pub mod document;
pub mod id;
#[allow(clippy::module_inception)]
pub mod schema;
pub mod table;
pub mod types;

pub use decorator::{Constraint, DecoratorParam, DecoratorSchemaRegistry, ResolvedDecoratorSchema};
pub use id::IdRegistry;
pub use schema::{
    ChildConstraint, ResolvedField, ResolvedSchema, ResolvedVariant, SchemaRegistry, SymbolSetInfo,
    SymbolSetRegistry, ValidateConstraints,
};
pub use types::type_name;
