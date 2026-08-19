//! The machine-readable categories a diagnostic carries.
//!
//! Both of these are structured rather than pre-formatted for the same
//! reason: a tool should be able to branch on *what kind* of failure it
//! is without parsing the rendered message. They sit beside
//! [`EvalError`](super::EvalError) rather than inside it because they
//! outlive any one error value — [`ArithmeticFault`] is raised by the
//! `sum` builtin as well as by the arithmetic operators, and one fault
//! reads the same wherever it surfaces.

/// Why an arithmetic operator had no answer for operands it *did* accept.
/// Distinct from a type mismatch: the operands were compatible, the result
/// simply isn't representable (or, for `/` and `%`, isn't defined at all).
///
/// Structured rather than pre-formatted so tools can act on it without
/// parsing the message — the same reason [`SchemaViolationKind`] exists.
/// `Display` is the phrasing that follows "cannot", and is the one copy of
/// that wording: `EvalError::Arithmetic` and the `sum` builtin both render
/// through it, so one fault reads the same wherever it surfaces.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArithmeticFault {
    /// `/` or `%` with a zero divisor — undefined, not merely
    /// unrepresentable, so no wider type would rescue it.
    DivideByZero,
    /// The result doesn't fit the numeric variant the operands share,
    /// carried in `ty` as WCL spells it (`i8`, `usize`).
    /// The result does not fit the numeric type shared by the
    /// operands.
    Overflow {
        /// The type involved, as WCL spells it.
        ty: String,
    },
}

impl ArithmeticFault {
    /// Build an [`ArithmeticFault::Overflow`] for the named type.
    pub fn overflow(ty: impl Into<String>) -> Self {
        Self::Overflow { ty: ty.into() }
    }
}

impl std::fmt::Display for ArithmeticFault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DivideByZero => f.write_str("divide by zero"),
            Self::Overflow { ty } => write!(f, "represent the result in {ty} (overflow)"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// The machine-readable category of an [`EvalError::SchemaViolation`](super::EvalError::SchemaViolation).
///
/// Structured rather than pre-formatted so a tool can branch on the kind
/// of violation without parsing the rendered message.
pub enum SchemaViolationKind {
    /// A decorator name that resolves to no type carrying a matching
    /// `@decorator("name")` declaration.
    UndeclaredDecorator,
    /// A declared decorator was written in a syntactic position excluded
    /// by its decorator schema's `@applies_to` declaration.
    DecoratorNotApplicable,
    /// An `@applies_to` declaration is internally inconsistent or attached
    /// to a type that declares no decorator schema.
    InvalidDecoratorApplicability,
    /// A non-repeatable decorator occurred more than once on one syntax
    /// node. The violation points at the repeated occurrence.
    DecoratorCardinality,
    /// A nested block whose kind the parent's schema does not accept
    /// in any `@child` / `@children` position.
    DisallowedChild,
    /// A field the schema declares without `?` and without a default
    /// was not supplied.
    MissingRequired,
    /// Fewer children than the `@children(min = n)` floor.
    ChildrenTooFew,
    /// More children than the `@children(max = n)` ceiling.
    ChildrenTooMany,
    /// More nested blocks than the parent's `@block(max_children = n)`
    /// ceiling allows, counted across every child kind.
    BlockChildrenOverflow,
    /// A child block that no gather field on the parent claims, in a
    /// schema that accepts children but not this one.
    UnexpectedExtraChild,
    /// Block whose `kind` has no corresponding `@block`/`@table` type
    /// declaration anywhere in the document.
    UnregisteredKind,
    /// Field whose `name` isn't declared by the parent schema (or by
    /// the document schema, for top-level fields).
    UnknownField,
    /// Top-level value (field or block) but no `@document`-decorated
    /// type exists.
    NoDocumentSchema,
    /// More than one `@document`-decorated type declared in the
    /// document.
    MultipleDocumentSchemas,
    /// Two `@block`/`@table`/`@decorator` declarations carry the same
    /// kind string within the same namespace, so a reference to that
    /// kind is ambiguous. Declarations in *different* namespaces are
    /// fine — they're disambiguated by a `::` qualifier.
    DuplicateBlockKind,
    /// Two sibling blocks of the same kind share an identity label —
    /// the kind's schema declares an `@inline(0) identifier` field, so
    /// the label is an id and a duplicate makes every reference to it
    /// ambiguous. Kinds whose label isn't identifier-typed (`code wcl`,
    /// `li`, …) repeat freely.
    DuplicateBlockId,
    /// A kind declared by a `@declares_kind` instance collides with a
    /// kind declared by a `@block`/`@table` type. The dispatch paths
    /// disagree on the winner (expansion prefers the declarer, schema
    /// lookup the declared type), so instances behave incoherently —
    /// the collision itself is the error.
    DeclaredKindCollision,
    /// A `&Interface` reference field's target doesn't implement
    /// the interface (missing or differently-typed field), or a
    /// `&T` reference field's target isn't `T` and isn't a
    /// descendant of `T` via the `extends` chain.
    InterfaceNotImplemented,
    /// A field declared `: SomeUnion` was assigned a variant whose
    /// constructing union FQN differs.
    VariantUnionMismatch,
    /// A field's evaluated value doesn't match its declared
    /// `TypeRef` under the conservative `value_matches_type_ref` check
    /// (scalar / string / list-element / variant FQN).
    FieldTypeMismatch,
    /// A field's value violates a constraint decorator (`@min` /
    /// `@max` / `@non_empty`) declared on the field or on a type alias
    /// its declared type goes through.
    ConstraintViolation,
    /// Two variants in a union's effective list share a name (across
    /// the `extends` chain).
    DuplicateVariant,
    /// Two variants in the same effective list have identical bodies,
    /// making structural dispatch ambiguous.
    VariantShapeCollision,
    /// A block / decorator / table-row didn't match any variant of
    /// the declared union via structural dispatch.
    VariantNoMatch,
    /// Defensive: structural dispatch matched more than one variant.
    /// Should be unreachable after `VariantShapeCollision` declaration
    /// checks land.
    VariantAmbiguous,
    /// A connection statement's lhs or rhs identifier doesn't resolve
    /// to a block in scope.
    UnknownConnectionOperand,
    /// No declared `connection` schema accepts the resolved operand
    /// types of a connection statement.
    UnknownConnection,
    /// More than one declared `connection` schema matches the operand
    /// types of a connection statement.
    AmbiguousConnection,
    /// A connection statement's `:kind` symbol isn't a member of the
    /// matched schema's `kind_set`.
    UnknownConnectionKind,
    /// A field declared with a `symbol_set` type was assigned a symbol
    /// that isn't one of the set's members.
    SymbolNotInSet,
    /// A field carrying `@ref("kind")` holds an id that doesn't name any
    /// existing block of that kind — a dangling reference.
    DanglingReference,
    /// Two `@document` schemas that co-govern a namespace declare the
    /// same field name and at least one side is a gather slot
    /// (`@child`/`@children`). The merged document schema resolves the
    /// name to only one declaration, so the other schema's gathered
    /// blocks silently vanish from templates iterating the field.
    /// Reported by [`Document::schema_warnings`], never by
    /// `schema_errors` — merging is a designed feature and existing
    /// documents must keep building.
    DocumentFieldShadow,
}
