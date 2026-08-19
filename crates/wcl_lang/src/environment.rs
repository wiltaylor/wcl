//! The host environment: synthetic schema declarations + registered built-in
//! functions + the expander for `@contextual` block kinds.
//!
//! Lets a Rust embedder ship "built-in" declarations and Rust callables
//! that participate in a document's type registry and evaluator. Language-defined
//! decorator schemas and their closed position vocabulary are pre-registered in
//! every [`Environment::new`] so they use the same registry as user declarations.

use std::collections::HashMap;
use std::sync::Arc;

use crate::ast;
use crate::ast::{synthetic_decorator, synthetic_field, synthetic_span};
use crate::builtins::BuiltinFn;
use crate::doc::Block;
use crate::value::{BuiltinType, TypeRef, Value};

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
        crate::collections::register(&mut env);
        crate::math::register(&mut env);
        crate::paths::register(&mut env);
        crate::reflect::register(&mut env);
        crate::units::register(&mut env);
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

/// The always-on built-in unit types (`std.ByteSize`, `std.Distance`,
/// `std.Duration`), parsed once from the embedded `lib/units.wcl` and
/// cloned into every [`Environment::new`]. They are plain type aliases
/// carrying `@unit` decorators, injected as synthetic types so a literal
/// unit (`5MiB`) resolves with no `import`.
fn stdlib_unit_types() -> Vec<ast::TypeDecl> {
    static UNITS: std::sync::LazyLock<Vec<ast::TypeDecl>> = std::sync::LazyLock::new(|| {
        const SRC: &str = include_str!("../lib/units.wcl");
        let source =
            crate::parse_for_edit(SRC, "<wcl-units>").expect("built-in lib/units.wcl must parse");
        source
            .items
            .into_iter()
            .filter_map(|item| match item {
                ast::Item::TypeDecl(t) => Some(t),
                _ => None,
            })
            .collect()
    });
    UNITS.clone()
}

/// Every decorator schema the language declares for itself — the
/// `@block`, `@children`, `@unit` and friends a document can use
/// without declaring them.
fn builtin_decorator_schemas() -> Vec<ast::TypeDecl> {
    vec![
        block_schema(),
        decorator_schema(),
        applies_to_schema(),
        unit_schema(),
        synth_decorator_schema(
            "Inline",
            "inline",
            "slot",
            TypeRef::Builtin(BuiltinType::U64),
        ),
        // `default` accepts any value; tighten the inner type when an
        // `any` builtin lands.
        default_schema(),
        synth_decorator_schema(
            "Child",
            "child",
            "kind",
            TypeRef::Builtin(BuiltinType::Identifier),
        ),
        children_schema(),
        synth_decorator_schema(
            "Table",
            "table",
            "name",
            TypeRef::Builtin(BuiltinType::Identifier),
        ),
        synth_decorator_schema(
            "Connections",
            "connections",
            "schema",
            TypeRef::Builtin(BuiltinType::Identifier),
        ),
        synth_decorator_schema(
            "DocDecorator",
            "doc",
            "text",
            TypeRef::Builtin(BuiltinType::Utf8),
        ),
        synth_decorator_schema(
            "MinDecorator",
            "min",
            "value",
            TypeRef::Builtin(BuiltinType::F64),
        ),
        synth_decorator_schema(
            "MaxDecorator",
            "max",
            "value",
            TypeRef::Builtin(BuiltinType::F64),
        ),
        synth_marker_decorator_schema("NonEmptyDecorator", "non_empty"),
        synth_decorator_schema(
            "RefDecorator",
            "ref",
            "kind",
            TypeRef::Builtin(BuiltinType::Identifier),
        ),
        synth_marker_decorator_schema("ByRefDecorator", "by_ref"),
        synth_marker_decorator_schema("DynamicDecorator", "dynamic"),
        // `@document` marks a type as the schema for the document
        // root. The single declared positional arg is ignored at
        // recognition time; only the decorator name matters.
        synth_optional_decorator_schema(
            "Document",
            "document",
            "name",
            TypeRef::Builtin(BuiltinType::Utf8),
        ),
        // The bare form opts a block or field out of strict validation;
        // `annotations = true` exempts only decorators on that node.
        schemaless_decorator_schema(),
        // `@contextual` marks a block kind whose placement is decided by
        // context rather than by kind: it is legal wherever children are
        // allowed at all, its body is not recursed into by the child
        // walk, and its children are generated by the host's
        // [`Expander`]. Same shape — only the decorator name is read.
        synth_optional_decorator_schema(
            "Contextual",
            "contextual",
            "reason",
            TypeRef::Builtin(BuiltinType::Utf8),
        ),
        // `@block_slot` marks a type used by `slot name: Type` as a nested
        // block hole rather than a scalar parameter. Hosts choose the type
        // and its accepted child vocabulary; the language only keeps the
        // declaration form host-neutral.
        synth_marker_decorator_schema("BlockSlot", "block_slot"),
        // `@declares_kind(name = 0, params = "…", body = "…")` marks a
        // block type whose *instances* declare block kinds of their own.
        // Kind lookup falls back to a schema derived from the named
        // param field, so an instance of a declared kind validates like
        // any other block. Three slots, so it gets its own literal
        // rather than the one-field helper.
        declares_kind_schema(),
    ]
}

/// Schema for `@schemaless`.
fn schemaless_decorator_schema() -> ast::TypeDecl {
    let mut reason = synthetic_field("reason", TypeRef::Builtin(BuiltinType::Utf8), true);
    reason
        .decorators
        .push(synthetic_decorator("inline", vec![ast::Expr::U64(0)]));
    ast::TypeDecl {
        name: vec!["Schemaless".to_string()],
        extends: Vec::new(),
        alias: None,
        fields: vec![
            reason,
            synthetic_field("annotations", TypeRef::Builtin(BuiltinType::Bool), true),
        ],
        decorators: vec![synthetic_decorator(
            "decorator",
            vec![ast::Expr::Utf8("schemaless".to_string())],
        )],
        span: synthetic_span(),
        leading_trivia: Vec::new(),
        trailing_comment: None,
        trailing_trivia: Vec::new(),
    }
}

/// Schema for `@block`.
fn block_schema() -> ast::TypeDecl {
    let mut name = synthetic_field("name", TypeRef::Builtin(BuiltinType::Identifier), false);
    name.decorators
        .push(synthetic_decorator("inline", vec![ast::Expr::U64(0)]));
    ast::TypeDecl {
        name: vec!["Block".to_string()],
        extends: Vec::new(),
        alias: None,
        fields: vec![
            name,
            synthetic_field(
                "required_children",
                TypeRef::List(Box::new(TypeRef::Builtin(BuiltinType::Utf8))),
                true,
            ),
            synthetic_field(
                "required_fields",
                TypeRef::List(Box::new(TypeRef::Builtin(BuiltinType::Utf8))),
                true,
            ),
            synthetic_field("max_children", TypeRef::Builtin(BuiltinType::U64), true),
        ],
        decorators: vec![synthetic_decorator(
            "decorator",
            vec![ast::Expr::Utf8("block".to_string())],
        )],
        span: synthetic_span(),
        leading_trivia: Vec::new(),
        trailing_comment: None,
        trailing_trivia: Vec::new(),
    }
}

/// Schema for `@children`.
fn children_schema() -> ast::TypeDecl {
    let mut kind = synthetic_field("kind", TypeRef::Builtin(BuiltinType::Identifier), false);
    kind.decorators
        .push(synthetic_decorator("inline", vec![ast::Expr::U64(0)]));
    ast::TypeDecl {
        name: vec!["Children".to_string()],
        extends: Vec::new(),
        alias: None,
        fields: vec![
            kind,
            synthetic_field("min", TypeRef::Builtin(BuiltinType::U64), true),
            synthetic_field("max", TypeRef::Builtin(BuiltinType::U64), true),
        ],
        decorators: vec![synthetic_decorator(
            "decorator",
            vec![ast::Expr::Utf8("children".to_string())],
        )],
        span: synthetic_span(),
        leading_trivia: Vec::new(),
        trailing_comment: None,
        trailing_trivia: Vec::new(),
    }
}

/// Schema for `@decorator` — the declaration that lets a document
/// declare decorators of its own.
fn decorator_schema() -> ast::TypeDecl {
    let mut name = synthetic_field("name", TypeRef::Builtin(BuiltinType::Utf8), false);
    name.decorators
        .push(synthetic_decorator("inline", vec![ast::Expr::U64(0)]));
    let mut repeatable = synthetic_field("repeatable", TypeRef::Builtin(BuiltinType::Bool), true);
    repeatable.default_expr = Some(ast::Expr::Bool(false));
    ast::TypeDecl {
        name: vec!["Decorator".to_string()],
        extends: Vec::new(),
        alias: None,
        fields: vec![name, repeatable],
        decorators: vec![synthetic_decorator(
            "decorator",
            vec![ast::Expr::Utf8("decorator".to_string())],
        )],
        span: synthetic_span(),
        leading_trivia: Vec::new(),
        trailing_comment: None,
        trailing_trivia: Vec::new(),
    }
}

/// Schema for `@default`.
fn default_schema() -> ast::TypeDecl {
    let mut schema = synth_decorator_schema(
        "Default",
        "default",
        "value",
        TypeRef::Builtin(BuiltinType::Utf8),
    );
    // Defaults may have any declared field type. Until WCL has an `any`
    // type, use the existing field-level escape hatch to keep this slot
    // permissive while retaining its positional shape for reflection.
    schema.fields[0]
        .decorators
        .push(synthetic_decorator("schemaless", Vec::new()));
    schema
}

/// Schema for `@applies_to`.
fn applies_to_schema() -> ast::TypeDecl {
    ast::TypeDecl {
        name: vec!["AppliesTo".to_string()],
        extends: Vec::new(),
        alias: None,
        fields: vec![
            synthetic_field(
                "on",
                TypeRef::List(Box::new(TypeRef::named(vec![
                    "DecoratorPosition".to_string(),
                ]))),
                false,
            ),
            synthetic_field(
                "kinds",
                TypeRef::List(Box::new(TypeRef::Builtin(BuiltinType::Utf8))),
                true,
            ),
        ],
        decorators: vec![synthetic_decorator(
            "decorator",
            vec![ast::Expr::Utf8("applies_to".to_string())],
        )],
        span: synthetic_span(),
        leading_trivia: Vec::new(),
        trailing_comment: None,
        trailing_trivia: Vec::new(),
    }
}

/// Schema for `@unit`.
fn unit_schema() -> ast::TypeDecl {
    let mut declaration =
        synthetic_decorator("decorator", vec![ast::Expr::Utf8("unit".to_string())]);
    declaration.named.push(ast::NamedArg {
        name: "repeatable".to_string(),
        value: ast::Expr::Bool(true),
        span: synthetic_span(),
        leading_trivia: Vec::new(),
        trailing_comment: None,
    });
    let mut name = synthetic_field("name", TypeRef::Builtin(BuiltinType::Utf8), false);
    name.decorators
        .push(synthetic_decorator("inline", vec![ast::Expr::U64(0)]));
    let mut factor = synthetic_field("factor", TypeRef::Builtin(BuiltinType::F64), false);
    factor
        .decorators
        .push(synthetic_decorator("inline", vec![ast::Expr::U64(1)]));
    ast::TypeDecl {
        name: vec!["Unit".to_string()],
        extends: Vec::new(),
        alias: None,
        fields: vec![name, factor],
        decorators: vec![declaration],
        span: synthetic_span(),
        leading_trivia: Vec::new(),
        trailing_comment: None,
        trailing_trivia: Vec::new(),
    }
}

/// The `DecoratorPosition` symbol set that `@applies_to` draws its
/// values from.
fn decorator_position_set() -> ast::SymbolSetDecl {
    const POSITIONS: [&str; 11] = [
        "field",
        "fn",
        "block",
        "type",
        "interface",
        "type_field",
        "union",
        "variant",
        "symbol_set",
        "symbol",
        "connection",
    ];
    ast::SymbolSetDecl {
        name: vec!["DecoratorPosition".to_string()],
        symbols: POSITIONS
            .into_iter()
            .map(|name| ast::SymbolEntry {
                name: name.to_string(),
                decorators: Vec::new(),
                span: synthetic_span(),
                leading_trivia: Vec::new(),
                trailing_comment: None,
            })
            .collect(),
        decorators: Vec::new(),
        span: synthetic_span(),
        leading_trivia: Vec::new(),
        trailing_comment: None,
        trailing_trivia: Vec::new(),
    }
}

/// Build the schema for a decorator that takes no arguments at all.
fn synth_marker_decorator_schema(type_name: &str, decorator_name: &str) -> ast::TypeDecl {
    ast::TypeDecl {
        name: vec![type_name.to_string()],
        extends: Vec::new(),
        alias: None,
        fields: Vec::new(),
        decorators: vec![synthetic_decorator(
            "decorator",
            vec![ast::Expr::Utf8(decorator_name.to_string())],
        )],
        span: synthetic_span(),
        leading_trivia: Vec::new(),
        trailing_comment: None,
        trailing_trivia: Vec::new(),
    }
}

/// Schema for `@declares_kind(name = 0, params = "slots", body = "body")`.
/// `name` is the declarer's `@inline(N)` label slot carrying the declared
/// kind's name; `params` and `body` name declarer fields.
fn declares_kind_schema() -> ast::TypeDecl {
    ast::TypeDecl {
        name: vec!["DeclaresKind".to_string()],
        extends: Vec::new(),
        alias: None,
        fields: vec![
            synthetic_field("name", TypeRef::Builtin(BuiltinType::U64), true),
            synthetic_field("params", TypeRef::Builtin(BuiltinType::Utf8), false),
            synthetic_field("body", TypeRef::Builtin(BuiltinType::Utf8), true),
        ],
        decorators: vec![synthetic_decorator(
            "decorator",
            vec![ast::Expr::Utf8("declares_kind".to_string())],
        )],
        span: synthetic_span(),
        leading_trivia: Vec::new(),
        trailing_comment: None,
        trailing_trivia: Vec::new(),
    }
}

/// Build the schema for a decorator taking one required argument.
fn synth_decorator_schema(
    type_name: &str,
    decorator_name: &str,
    field_name: &str,
    field_ty: TypeRef,
) -> ast::TypeDecl {
    let mut field = synthetic_field(field_name, field_ty, false);
    field
        .decorators
        .push(synthetic_decorator("inline", vec![ast::Expr::U64(0)]));
    ast::TypeDecl {
        name: vec![type_name.to_string()],
        extends: Vec::new(),
        alias: None,
        fields: vec![field],
        decorators: vec![synthetic_decorator(
            "decorator",
            vec![ast::Expr::Utf8(decorator_name.to_string())],
        )],
        span: synthetic_span(),
        leading_trivia: Vec::new(),
        trailing_comment: None,
        trailing_trivia: Vec::new(),
    }
}

/// Build the schema for a decorator taking one optional argument.
fn synth_optional_decorator_schema(
    type_name: &str,
    decorator_name: &str,
    field_name: &str,
    field_ty: TypeRef,
) -> ast::TypeDecl {
    let mut schema = synth_decorator_schema(type_name, decorator_name, field_name, field_ty);
    schema.fields[0].optional = true;
    schema
}

/// Output of [`TypeBuilder::build`] — a finished synthetic type declaration
/// ready to register with an [`Environment`].
pub struct BuiltType {
    /// The finished declaration.
    pub(crate) inner: ast::TypeDecl,
}

/// Builder for synthetic type declarations.
pub struct TypeBuilder {
    /// Dotted name of the type being built.
    name: Vec<String>,
    /// Fields accumulated so far.
    fields: Vec<ast::TypeField>,
    /// Decorators accumulated so far.
    decorators: Vec<ast::Decorator>,
}

impl TypeBuilder {
    /// Start building a type declaration with the given dotted name.
    pub fn new<I, S>(name: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            name: name.into_iter().map(Into::into).collect(),
            fields: Vec::new(),
            decorators: Vec::new(),
        }
    }

    /// Attach a decorator to the type being built.
    pub fn decorator(mut self, d: DecoratorBuilder) -> Self {
        self.decorators.push(d.build());
        self
    }

    /// Append a field to the type being built.
    pub fn field(mut self, f: TypeFieldBuilder) -> Self {
        self.fields.push(f.build());
        self
    }

    /// Finish the type declaration.
    pub fn build(self) -> BuiltType {
        BuiltType {
            inner: ast::TypeDecl {
                name: self.name,
                extends: Vec::new(),
                alias: None,
                fields: self.fields,
                decorators: self.decorators,
                span: synthetic_span(),
                leading_trivia: Vec::new(),
                trailing_comment: None,
                trailing_trivia: Vec::new(),
            },
        }
    }
}

/// Builder for synthetic type fields.
pub struct TypeFieldBuilder {
    /// Field name.
    name: String,
    /// Declared type.
    ty: TypeRef,
    /// Whether the field is optional.
    optional: bool,
    /// Decorators accumulated so far.
    decorators: Vec<ast::Decorator>,
}

impl TypeFieldBuilder {
    /// Start a required field of the given name and type.
    pub fn new(name: impl Into<String>, ty: TypeRef) -> Self {
        Self {
            name: name.into(),
            ty,
            optional: false,
            decorators: Vec::new(),
        }
    }

    /// Mark the field optional (or not).
    pub fn optional(mut self, optional: bool) -> Self {
        self.optional = optional;
        self
    }

    /// Attach a decorator to the field being built.
    pub fn decorator(mut self, d: DecoratorBuilder) -> Self {
        self.decorators.push(d.build());
        self
    }

    /// Finish the field declaration.
    pub(crate) fn build(self) -> ast::TypeField {
        ast::TypeField {
            name: self.name,
            ty: self.ty,
            ty_span: synthetic_span(),
            optional: self.optional,
            decorators: self.decorators,
            span: synthetic_span(),
            default_expr: None,
            leading_trivia: Vec::new(),
            trailing_comment: None,
        }
    }
}

/// Builder for synthetic decorators.
pub struct DecoratorBuilder {
    /// Dotted decorator name.
    name: Vec<String>,
    /// Positional arguments accumulated so far.
    positional: Vec<ast::Expr>,
    /// Named arguments accumulated so far.
    named: Vec<ast::NamedArg>,
}

impl DecoratorBuilder {
    /// Start building a decorator with the given dotted name.
    pub fn new<I, S>(name: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            name: name.into_iter().map(Into::into).collect(),
            positional: Vec::new(),
            named: Vec::new(),
        }
    }

    /// Append a positional argument.
    pub fn positional(mut self, value: Value) -> Self {
        self.positional.push(value_to_expr(value));
        self
    }

    /// Set a named argument.
    pub fn named(mut self, name: impl Into<String>, value: Value) -> Self {
        self.named.push(ast::NamedArg {
            name: name.into(),
            value: value_to_expr(value),
            span: synthetic_span(),
            leading_trivia: Vec::new(),
            trailing_comment: None,
        });
        self
    }

    /// Finish the decorator.
    pub(crate) fn build(self) -> ast::Decorator {
        let positional_spans = vec![synthetic_span(); self.positional.len()];
        ast::Decorator {
            name: self.name,
            name_span: synthetic_span(),
            positional: self.positional,
            positional_spans,
            named: self.named,
            span: synthetic_span(),
        }
    }
}

/// Lift an already-evaluated value back into the literal expression
/// that produces it, so synthesised declarations can carry values.
fn value_to_expr(v: Value) -> ast::Expr {
    match v {
        Value::Bool(b) => ast::Expr::Bool(b),
        Value::I8(n) => ast::Expr::I8(n),
        Value::I16(n) => ast::Expr::I16(n),
        Value::I32(n) => ast::Expr::I32(n),
        Value::I64(n) => ast::Expr::I64(n),
        Value::I128(n) => ast::Expr::I128(n),
        Value::Isize(n) => ast::Expr::Isize(n),
        Value::U8(n) => ast::Expr::U8(n),
        Value::U16(n) => ast::Expr::U16(n),
        Value::U32(n) => ast::Expr::U32(n),
        Value::U64(n) => ast::Expr::U64(n),
        Value::U128(n) => ast::Expr::U128(n),
        Value::Usize(n) => ast::Expr::Usize(n),
        Value::F32(n) => ast::Expr::F32(n),
        Value::F64(n) => ast::Expr::F64(n),
        Value::Utf8(s) => ast::Expr::Utf8(s),
        Value::Ascii(s) => ast::Expr::Ascii(s),
        Value::Utf16(v) => ast::Expr::Utf16(v),
        Value::Utf32(v) => ast::Expr::Utf32(v),
        Value::Identifier(s) => ast::Expr::Identifier(s, ast::Span::new(0, 0)),
        Value::Symbol(s) => ast::Expr::Symbol(s),
        Value::None => ast::Expr::None,
        Value::Function(_) => {
            unreachable!("function values are not constructible via the schema builder API")
        }
        Value::List(items) => ast::Expr::ListLit {
            elements: std::sync::Arc::unwrap_or_clone(items)
                .into_iter()
                .map(value_to_expr)
                .collect(),
            elem_trivia: Vec::new(),
            trailing_trivia: Vec::new(),
            span: synthetic_span(),
        },
        Value::Tensor { .. } => {
            unreachable!("tensor values are not constructible via the schema builder API")
        }
        Value::Variant { .. } => {
            unreachable!("variant values are not constructible via the schema builder API")
        }
        Value::Record { .. } => {
            unreachable!("record values are not constructible via the schema builder API")
        }
        Value::DataPath { .. } => {
            unreachable!("data path values are not constructible via the schema builder API")
        }
        Value::PendingUnit { .. } => {
            unreachable!(
                "unresolved unit literals are not constructible via the schema builder API"
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{DeclName, Document, ResolvedType};

    #[test]
    fn empty_environment_has_no_types() {
        let r = Environment::empty();
        assert!(r.types().is_empty());
        assert!(r.builtin("anything").is_none());
    }

    #[test]
    fn new_environment_includes_builtin_decorator_schemas() {
        let r = Environment::new();
        let names: Vec<_> = r.types().iter().map(|t| t.name.join(".")).collect();
        assert!(names.contains(&"Block".to_string()));
        assert!(names.contains(&"Decorator".to_string()));
        assert!(names.contains(&"Inline".to_string()));
        assert!(names.contains(&"Default".to_string()));
    }

    #[test]
    fn register_type_with_builder() {
        let mut r = Environment::empty();
        r.add_type(
            TypeBuilder::new(["Service"])
                .decorator(
                    DecoratorBuilder::new(["block"]).positional(Value::Utf8("service".into())),
                )
                .field(
                    TypeFieldBuilder::new("id", TypeRef::Builtin(BuiltinType::Identifier))
                        .decorator(DecoratorBuilder::new(["inline"]).positional(Value::I64(0))),
                )
                .field(
                    TypeFieldBuilder::new("port", TypeRef::Builtin(BuiltinType::U32))
                        .optional(true)
                        .decorator(DecoratorBuilder::new(["default"]).positional(Value::I64(8080))),
                )
                .build(),
        );
        let doc = Document::open_with("", "test", &r).expect("open");
        let s = doc.block_schema("service").expect("registered schema");
        assert_eq!(s.name(), "Service");
        assert_eq!(s.field("id").unwrap().inline_slot(), Some(0));
        assert_eq!(
            s.field("port").unwrap().default_value(),
            Some(Value::I64(8080))
        );
    }

    #[test]
    fn synthetic_type_used_as_field_type() {
        let mut r = Environment::empty();
        r.add_type(TypeBuilder::new(["Service"]).build());
        let doc = Document::open_with("type Cfg { s: Service }", "test", &r).expect("open");
        let cfg = doc.type_decl("Cfg").unwrap();
        let s = cfg.field("s").unwrap();
        match doc.resolve(s.type_ref()) {
            ResolvedType::Named(d) => assert_eq!(d.name(), "Service"),
            _ => panic!("expected Named"),
        }
    }

    #[test]
    fn source_redeclares_synthetic_errors() {
        let mut r = Environment::empty();
        r.add_type(TypeBuilder::new(["Service"]).build());
        let err = Document::open_with("type Service {}", "test", &r).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("duplicate declaration"), "{msg}");
    }

    #[test]
    fn registers_builtin_callable_by_name() {
        use crate::builtins::from_fn;
        let mut env = Environment::empty();
        env.add_builtin("upper", from_fn(|s: String| s.to_uppercase()));
        assert!(env.builtin("upper").is_some());
        assert!(env.builtin("missing").is_none());
    }
}
