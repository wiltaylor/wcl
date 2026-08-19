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
//! Two families live here. [`Expr`] and its supporting types are the
//! expression language. [`Item`] and its variants are the declarations a
//! file is made of — fields, blocks, type/union/interface declarations,
//! imports and connections — collected into a [`Source`].
//!
//! Nodes are built by the parser, printed by [`crate::format`], and
//! evaluated by the document layer. The evaluator ignores trivia
//! entirely.

/// A half-open byte range `[start, end)` into the source text.
///
/// Every node carries one so a diagnostic can point at the text that
/// produced it. Nodes with no source behind them (schema types the
/// evaluator fabricates) carry an empty span instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Span {
    /// Byte offset of the first character.
    pub start: usize,
    /// Byte offset one past the last character.
    pub end: usize,
}

impl Span {
    /// Build a span covering `[start, end)`.
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    /// The span's width in bytes.
    pub fn len(&self) -> usize {
        self.end - self.start
    }

    /// True when the span covers no text — the shape of a synthetic node.
    pub fn is_empty(&self) -> bool {
        self.start == self.end
    }
}

/// The span every synthesised AST node carries: there is no source text
/// behind it. Shared with the schema derivation in `doc.rs`, which
/// fabricates type declarations the same way.
pub(crate) fn synthetic_span() -> Span {
    Span::new(0, 0)
}

/// A decorator with no named args, spanning nothing — the shape every
/// synthesised `@block("x")` / `@contextual` / `@decorator("y")` takes.
pub(crate) fn synthetic_decorator(name: &str, positional: Vec<Expr>) -> Decorator {
    let positional_spans = vec![synthetic_span(); positional.len()];
    Decorator {
        name: vec![name.to_string()],
        name_span: synthetic_span(),
        positional,
        positional_spans,
        named: Vec::new(),
        span: synthetic_span(),
    }
}

/// A field of a synthesised type: no decorators, no default, no span.
pub(crate) fn synthetic_field(name: &str, ty: crate::value::TypeRef, optional: bool) -> TypeField {
    TypeField {
        name: name.to_string(),
        ty,
        ty_span: synthetic_span(),
        optional,
        decorators: Vec::new(),
        span: synthetic_span(),
        default_expr: None,
        leading_trivia: Vec::new(),
        trailing_comment: None,
    }
}

/// Side-band formatting hints attached to each top-level [`Item`] in
/// `leading_trivia`. The lexer collects these from the source between
/// tokens; the parser hands them to the next Item it builds. The
/// source printer re-emits them so comments and blank-line groupings
/// survive a round-trip. Other formatting (indentation, brace style,
/// number radix, string-delimiter choice) is reformatted canonically
/// — only what's in this enum is preserved.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Trivia {
    /// A line comment, payload-only (the leading `#` or `//` and the
    /// trailing newline are stripped). The printer re-adds the `#`
    /// prefix; original prefix style is not preserved.
    LineComment(String),
    /// One blank line break between items. Multiple consecutive blank
    /// lines collapse to a single marker — canonical output emits at
    /// most one blank line between any two items.
    BlankLine,
}

/// Comment trivia for one element of a comma-separated expression
/// collection whose elements are bare [`Expr`]s (list literals, call
/// arguments) and so have no struct of their own to hang trivia on. The
/// parser builds one entry per element, index-aligned with the element
/// vec; the evaluator ignores these entirely. `leading` holds comments
/// (and blank lines) printed above the element; `trailing` is a same-line
/// comment printed after it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ElemTrivia {
    /// Comments and blank lines printed above the element.
    pub leading: Vec<Trivia>,
    /// A same-line comment printed after the element.
    pub trailing: Option<String>,
}

impl ElemTrivia {
    /// True when this element carries a line comment in either position
    /// (blank lines alone don't count — they don't force a multi-line
    /// layout).
    pub fn has_comment(&self) -> bool {
        self.trailing.is_some()
            || self
                .leading
                .iter()
                .any(|t| matches!(t, Trivia::LineComment(_)))
    }
}

/// An expression: everything that can appear on the right of an `=`.
///
/// The numeric variants mirror WCL's explicit integer and float widths —
/// a literal's suffix (`8080u32`) picks the variant, and an unsuffixed
/// literal defaults to `I64` / `F64`. Everything else is the usual
/// expression language: literals, operators, calls, blocks, control flow
/// and pattern matching.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// `true` / `false`.
    Bool(bool),

    /// Signed 8-bit integer literal.
    I8(i8),
    /// Signed 16-bit integer literal.
    I16(i16),
    /// Signed 32-bit integer literal.
    I32(i32),
    /// Signed 64-bit integer literal — the default for an unsuffixed
    /// integer.
    I64(i64),
    /// Signed 128-bit integer literal.
    I128(i128),
    /// Pointer-sized signed integer literal.
    Isize(isize),

    /// Unsigned 8-bit integer literal.
    U8(u8),
    /// Unsigned 16-bit integer literal.
    U16(u16),
    /// Unsigned 32-bit integer literal.
    U32(u32),
    /// Unsigned 64-bit integer literal.
    U64(u64),
    /// Unsigned 128-bit integer literal.
    U128(u128),
    /// Pointer-sized unsigned integer literal.
    Usize(usize),

    /// 32-bit float literal.
    F32(f32),
    /// 64-bit float literal — the default for an unsuffixed float.
    F64(f64),

    /// A numeric literal with a **literal unit** suffix (`5MiB`, `3km`).
    /// `value` is the raw magnitude (int → `i64`, float → `f64` by
    /// default); `unit` is the suffix name. The unit is resolved against
    /// the binding's declared type during evaluation — multiplying by the
    /// type's `@unit(name, factor)` decorator — so a `UnitLiteral` only
    /// type-checks where a unit-bearing type is in context.
    UnitLiteral {
        /// The raw magnitude, before the unit factor is applied.
        value: crate::lexer::NumberLit,
        /// The suffix name, resolved against the declared type's
        /// `@unit` decorators.
        unit: String,
        /// Source span of the whole literal.
        span: Span,
    },

    /// A UTF-8 string literal.
    Utf8(String),
    /// An ASCII string literal (`ascii"…"`).
    Ascii(String),
    /// A UTF-16 string literal, stored as code units.
    Utf16(Vec<u16>),
    /// A UTF-32 string literal, stored as scalar values.
    Utf32(Vec<char>),

    /// Opt-in interpolated string literal (`$"…"`, `$ascii"…"`,
    /// `$<<TAG ... TAG`, …). Each part is either a literal body chunk
    /// or a sub-parsed expression; the evaluator concatenates the
    /// stringified results and re-encodes per the declared encoding.
    InterpolatedString {
        /// Target encoding the concatenated result is re-encoded to.
        encoding: crate::lexer::StringEncoding,
        /// Alternating literal chunks and embedded expressions.
        parts: Vec<TemplatePart>,
        /// Source span of the whole literal.
        span: Span,
    },

    /// A bare name, resolved against the enclosing scopes at evaluation.
    Identifier(String, Span),
    /// A symbol literal (`:gold`), matched against a `symbol_set`.
    Symbol(String),
    /// The `none` literal — the absent value.
    None,

    /// An anonymous function literal (`fn(x: u32) -> u32 { … }`).
    Function(FunctionLit),
    /// A call `callee(args…)`.
    Call {
        /// The expression being called — usually an identifier.
        callee: Box<Expr>,
        /// Evaluated left to right.
        args: Vec<Expr>,
        /// Per-argument comment trivia, index-aligned with `args`. Empty
        /// (or all-default) when the call carries no comments.
        arg_trivia: Vec<ElemTrivia>,
        /// Comments/blank lines after the last argument, before `)`.
        trailing_trivia: Vec<Trivia>,
        /// Source span of the whole call.
        span: Span,
    },
    /// A binary operation `lhs op rhs`.
    Binary {
        /// The operator.
        op: BinOp,
        /// Left operand.
        lhs: Box<Expr>,
        /// Right operand.
        rhs: Box<Expr>,
        /// Source span of the whole operation.
        span: Span,
    },
    /// A prefix operation `op operand`.
    Unary {
        /// The operator.
        op: UnaryOp,
        /// The operand.
        operand: Box<Expr>,
        /// Source span of the whole operation.
        span: Span,
    },
    /// A block expression `{ let …; tail }` — bindings followed by the
    /// expression whose value the block takes.
    Block {
        /// Bindings visible to `tail` and to later bindings.
        lets: Vec<LetBinding>,
        /// The expression the block evaluates to.
        tail: Box<Expr>,
        /// Comments/blank lines between the last `let` (or the tail) and
        /// the closing `}` of the block expression.
        trailing_trivia: Vec<Trivia>,
        /// Source span of the whole block.
        span: Span,
    },
    /// A parenthesised expression. Kept in the tree so the printer can
    /// re-emit grouping the author wrote even where precedence makes it
    /// redundant.
    Paren {
        /// The wrapped expression.
        inner: Box<Expr>,
        /// Source span including both parentheses.
        span: Span,
    },

    /// A list literal `[a, b, c]`.
    ListLit {
        /// The elements, in source order.
        elements: Vec<Expr>,
        /// Per-element comment trivia, index-aligned with `elements`.
        elem_trivia: Vec<ElemTrivia>,
        /// Comments/blank lines after the last element, before `]`.
        trailing_trivia: Vec<Trivia>,
        /// Source span including both brackets.
        span: Span,
    },

    /// The `self` keyword — the block currently being evaluated.
    SelfKw(Span),
    /// The `parent` keyword — the enclosing block.
    ParentKw(Span),
    /// Member access `recv.name`.
    Member {
        /// The receiver expression.
        recv: Box<Expr>,
        /// The member being accessed.
        name: String,
        /// Source span of the whole access.
        span: Span,
    },

    /// `if cond { … } else { … }`.
    If {
        /// The condition, which must evaluate to a `bool`.
        cond: Box<Expr>,
        /// Taken when `cond` is true.
        then_block: Box<Expr>,
        /// Absent when the source omits `else`; the untaken branch of an
        /// else-less `if` evaluates to `none`.
        else_block: Option<Box<Expr>>,
        /// Source span of the whole conditional.
        span: Span,
    },
    /// `if let pattern = scrut { … } else { … }` — conditional binding.
    IfLet {
        /// Matched against `scrut`; its bindings are visible in
        /// `then_block`.
        pattern: Pattern,
        /// The value being matched.
        scrut: Box<Expr>,
        /// Taken when the pattern matches.
        then_block: Box<Expr>,
        /// Taken when it does not. Required, unlike plain `if`.
        else_block: Box<Expr>,
        /// Source span of the whole conditional.
        span: Span,
    },
    /// `match scrut { arms… }`.
    Match {
        /// The value being matched.
        scrut: Box<Expr>,
        /// Tried in source order; the first matching arm wins.
        arms: Vec<MatchArm>,
        /// Comments/blank lines after the last arm, before `}`.
        trailing_trivia: Vec<Trivia>,
        /// Source span of the whole match.
        span: Span,
    },
    /// An explicit union variant constructor (`Shape::Circle { r = 1 }`).
    Variant {
        /// Dotted path naming the union type.
        type_path: Vec<String>,
        /// The variant name.
        variant: String,
        /// The payload, whose shape must match the variant's declaration.
        args: VariantArgs,
        /// Source span of the whole constructor.
        span: Span,
    },
    /// `try body catch name => handler` — evaluate `body`; on an
    /// evaluation error bind the error message (a `utf8`) to `name`
    /// and evaluate `handler` instead. Catches every evaluation error
    /// (builtin failures, cycles, propagated field errors).
    Try {
        /// The expression attempted first.
        body: Box<Expr>,
        /// Name bound to the error message inside `handler`.
        binder: String,
        /// Source span of `binder`.
        binder_span: Span,
        /// Evaluated instead when `body` fails.
        handler: Box<Expr>,
        /// Source span of the whole expression.
        span: Span,
    },
    /// A bare record literal `{ name: value, … }`. When the surrounding
    /// context declares a union (or `list<union>`) type, the evaluator
    /// shape-infers the matching variant; otherwise it evaluates to an
    /// anonymous `Value::Record`.
    Record {
        /// The record's fields, in source order.
        fields: Vec<NamedArg>,
        /// Comments/blank lines after the last field, before `}`.
        trailing_trivia: Vec<Trivia>,
        /// Source span including both braces.
        span: Span,
    },
}

/// One segment of an interpolated string literal.
#[derive(Debug, Clone, PartialEq)]
pub enum TemplatePart {
    /// A verbatim chunk of the string body.
    Literal(String),
    /// An embedded `${…}` expression, stringified at evaluation.
    Expr(Box<Expr>),
}

/// One arm of a [`Expr::Match`].
#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    /// One or more alternative patterns. The arm fires when any matches.
    pub patterns: Vec<Pattern>,
    /// Optional `if` guard, evaluated in the matched pattern's scope.
    pub guard: Option<Expr>,
    /// Evaluated when the arm fires.
    pub body: Expr,
    /// Source span of the whole arm.
    pub span: Span,
    /// Comments/blank lines printed above this arm.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this arm.
    pub trailing_comment: Option<String>,
}

/// The payload of a union variant *constructor* — the expression form.
/// Mirrors [`VariantBody`], which is the declaration form.
#[derive(Debug, Clone, PartialEq)]
pub enum VariantArgs {
    /// No payload (`Shape::Empty`).
    Unit,
    /// A single unnamed payload (`Shape::Radius(2)`).
    Positional(Box<Expr>),
    /// Named fields (`Shape::Circle { r = 2 }`).
    Record {
        /// The fields, in source order.
        fields: Vec<NamedArg>,
        /// Comments/blank lines after the last field, before `}`.
        trailing_trivia: Vec<Trivia>,
    },
}

/// A pattern, as written in a `match` arm or an `if let`.
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    /// `_` — matches anything, binds nothing.
    Wildcard(Span),
    /// A bare name — matches anything and binds it.
    Binding {
        /// The bound name.
        name: String,
        /// Source span of the name.
        span: Span,
    },
    /// `name @ inner` — binds `name` to the full value while matching
    /// `inner` against it.
    At {
        /// The name bound to the whole value.
        name: String,
        /// The pattern the value must also match.
        inner: Box<Pattern>,
        /// Source span of the whole pattern.
        span: Span,
    },
    /// Matches a specific `true` / `false`.
    LiteralBool(bool, Span),
    /// Matches a specific numeric literal.
    LiteralNumber {
        /// The literal to compare against.
        lit: crate::lexer::NumberLit,
        /// Source span of the literal.
        span: Span,
    },
    /// Matches a specific UTF-8 string.
    LiteralUtf8(String, Span),
    /// Matches a specific ASCII string.
    LiteralAscii(String, Span),
    /// Matches a specific symbol (`:gold`).
    LiteralSymbol(String, Span),
    /// Matches the `none` value.
    LiteralNone(Span),
    /// Matches one variant of a union, destructuring its payload.
    Variant {
        /// Dotted path naming the union type. May be empty when the
        /// type is inferred from context.
        type_path: Vec<String>,
        /// The variant name.
        variant: String,
        /// How the payload is destructured.
        args: VariantPatArgs,
        /// Source span of the whole pattern.
        span: Span,
    },
}

/// How a [`Pattern::Variant`] destructures its payload. The pattern-side
/// counterpart of [`VariantArgs`].
#[derive(Debug, Clone, PartialEq)]
pub enum VariantPatArgs {
    /// No payload to destructure.
    Unit,
    /// A single unnamed payload, matched against one sub-pattern.
    Positional(Box<Pattern>),
    /// Named fields, each matched against a sub-pattern.
    Record {
        /// The `(field name, sub-pattern)` pairs written in the source.
        fields: Vec<(String, Pattern)>,
        /// `true` when the pattern ends in `..`, allowing fields the
        /// pattern does not name. Without it the pattern must be
        /// exhaustive over the variant's fields.
        rest: bool,
    },
}

/// An anonymous function literal.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionLit {
    /// Declared parameters, in order.
    pub params: Vec<Parameter>,
    /// The declared return type.
    pub return_ty: crate::value::TypeRef,
    /// Source span of the return type annotation.
    pub return_ty_span: Span,
    /// `Arc`, not `Box`: every evaluation of the literal builds an
    /// `FnValue` sharing this body, and resolving a named function
    /// clones that `FnValue` per call — a deep AST clone on either
    /// path dominated closure-heavy documents.
    pub body: std::sync::Arc<Expr>,
    /// Source span of the whole literal.
    pub span: Span,
    /// Comments/blank lines after the last parameter, before `)`.
    pub trailing_trivia: Vec<Trivia>,
}

/// One declared parameter of a [`FunctionLit`].
#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    /// The parameter name, bound in the function body.
    pub name: String,
    /// The declared type.
    pub ty: crate::value::TypeRef,
    /// Source span of the type annotation.
    pub ty_span: Span,
    /// Source span of the whole parameter.
    pub span: Span,
    /// Comments/blank lines printed above this parameter.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this parameter.
    pub trailing_comment: Option<String>,
}

/// A `let name = value;` binding inside an [`Expr::Block`]. Distinct from
/// the item-level [`LetItem`], which is declared at file or block scope.
#[derive(Debug, Clone, PartialEq)]
pub struct LetBinding {
    /// The bound name.
    pub name: String,
    /// The bound expression.
    pub value: Expr,
    /// Source span of the whole binding.
    pub span: Span,
    /// Comments/blank lines printed above this `let` binding.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this `let` binding.
    pub trailing_comment: Option<String>,
}

/// A binary operator. Precedence lives in [`BinOp::binding_power`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    /// `a + b`.
    Add,
    /// `a - b`.
    Sub,
    /// `a * b`.
    Mul,
    /// `a / b`.
    Div,
    /// `a % b`.
    Mod,
    /// `a == b`.
    Eq,
    /// `a != b`.
    Ne,
    /// `a < b`.
    Lt,
    /// `a <= b`.
    Le,
    /// `a > b`.
    Gt,
    /// `a >= b`.
    Ge,
    /// `a && b`, short-circuiting.
    And,
    /// `a || b`, short-circuiting.
    Or,
    /// `a ?? b` — the left value unless it is `none`.
    Coalesce,
}

impl BinOp {
    /// Pratt binding powers `(left, right)` — the single source of truth
    /// shared by the parser and the formatter, so a parse → print round
    /// trip preserves precedence and associativity by construction.
    pub fn binding_power(self) -> (u8, u8) {
        match self {
            BinOp::Coalesce => (1, 2),
            BinOp::Or => (3, 4),
            BinOp::And => (5, 6),
            BinOp::Eq | BinOp::Ne => (7, 8),
            BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => (9, 10),
            BinOp::Add | BinOp::Sub => (11, 12),
            BinOp::Mul | BinOp::Div | BinOp::Mod => (13, 14),
        }
    }

    /// The operator's source spelling.
    pub fn as_str(self) -> &'static str {
        match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::Eq => "==",
            BinOp::Ne => "!=",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
            BinOp::Coalesce => "??",
        }
    }
}

/// Binding powers of the prefix / postfix forms, all tighter than any
/// binary operator in [`BinOp::binding_power`]. Shared by the parser
/// (parse-time) and the formatter (parenthesisation) so they can't drift.
pub const UNARY_BP: u8 = 15;
/// Binding power of a call's argument list, tighter than [`UNARY_BP`].
pub const CALL_BP: u8 = 16;
/// Binding power of member access, the tightest form in the grammar.
pub const MEMBER_BP: u8 = 17;

/// A prefix operator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    /// `-x`, arithmetic negation.
    Neg,
    /// `!x`, logical negation.
    Not,
}

/// A `@name(args…)` annotation attached to an item, field or type.
///
/// The language does not interpret most decorators itself: it validates
/// them against their `@decorator` declarations and exposes them to the
/// host, which gives them meaning.
#[derive(Debug, Clone, PartialEq)]
pub struct Decorator {
    /// Dotted decorator name, split on `.`.
    pub name: Vec<String>,
    /// Span of the dotted name only, excluding the leading `@` and any
    /// arguments. Decorator-level diagnostics point here.
    pub name_span: Span,
    /// Positional arguments, in source order.
    pub positional: Vec<Expr>,
    /// Source spans index-aligned with [`Self::positional`]. Synthetic
    /// decorators carry empty spans for their synthetic arguments.
    pub positional_spans: Vec<Span>,
    /// Named `key = value` arguments.
    pub named: Vec<NamedArg>,
    /// Source span of the whole decorator, including the `@`.
    pub span: Span,
}

/// How much of a type's schema `@schemaless` waives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SchemalessMode {
    /// Waive membership checking entirely: any field is accepted.
    Full,
    /// Waive it for annotations only, still checking declared fields.
    AnnotationsOnly,
}

impl Decorator {
    /// The `@schemaless` mode this decorator requests, or `None` when it
    /// is not a `@schemaless` decorator at all.
    pub(crate) fn schemaless_mode(&self) -> Option<SchemalessMode> {
        if self.name.len() != 1 || self.name[0] != "schemaless" {
            return None;
        }
        if self
            .named
            .iter()
            .any(|arg| arg.name == "annotations" && matches!(arg.value, Expr::Bool(true)))
        {
            Some(SchemalessMode::AnnotationsOnly)
        } else {
            Some(SchemalessMode::Full)
        }
    }
}

/// A `name = value` pair, as written in a record literal, a variant
/// constructor or a decorator's named arguments.
#[derive(Debug, Clone, PartialEq)]
pub struct NamedArg {
    /// The field or argument name.
    pub name: String,
    /// The bound expression.
    pub value: Expr,
    /// Source span of the whole pair.
    pub span: Span,
    /// Comments/blank lines printed above this field (record literals).
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this field.
    pub trailing_comment: Option<String>,
}

/// A `name = expr` item: one piece of document data.
///
/// This is the item form. Unlike a [`LetItem`], a field is part of the
/// document model — queryable, iterable and schema-validated.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    /// The field name.
    pub name: String,
    /// The field's value, evaluated lazily and cached once forced.
    pub expr: Expr,
    /// Decorators attached to this field.
    pub decorators: Vec<Decorator>,
    /// Source span of the whole field.
    pub span: Span,
    /// Comments/blank lines printed above this field.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this field.
    pub trailing_comment: Option<String>,
}

/// A `let name = expr` item declared at the file (global) scope or
/// inside a block. Unlike a [`Field`], a let binding is a composition
/// helper: it is resolvable by name from sibling/descendant
/// expressions but never appears as document data (not queryable, not
/// iterated, not schema-validated). Distinct from the expression-level
/// [`LetBinding`] (which lives inside `Expr::Block` and uses `;`).
#[derive(Debug, Clone, PartialEq)]
pub struct LetItem {
    /// The bound name.
    pub name: String,
    /// The bound expression.
    pub value: Expr,
    /// Decorators on the `fn` item form (e.g. `@doc`). Always empty for
    /// `let` syntax, which rejects decorators.
    pub decorators: Vec<Decorator>,
    /// `true` when this binding was written as a `fn name(…) -> T body`
    /// item. Sugar for `let name = fn(…) -> T body`, with two visible
    /// differences: the binding is registered in the symbol index
    /// (outline / hover / go-to-def) and the formatter re-prints the
    /// `fn` form.
    pub fn_syntax: bool,
    /// Source span of the whole item.
    pub span: Span,
    /// Comments/blank lines printed above this item.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this `let` item.
    pub trailing_comment: Option<String>,
}

/// A block instance — `kind "label" { items… }`. Blocks are how a
/// document nests: each carries its own items and is validated against
/// the type its `@block` decorator names.
#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    /// The block kind, unqualified.
    pub kind: String,
    /// Namespace qualifier written before the kind with `::`, e.g.
    /// `wdoc::process` parses to `kind_ns = ["wdoc"]`, `kind = "process"`.
    /// Empty for a bare, unqualified kind. Multi-segment namespaces are
    /// dot-separated on the left of `::` (`foo.bar::process`).
    pub kind_ns: Vec<String>,
    /// A bare-name content fill may be conditional (`aside? { ... }`).
    /// The host that owns the surrounding slot contract interprets this;
    /// the language only preserves the syntax.
    pub conditional: bool,
    /// Present for the declaration form `slot name: Type[? | *]`.
    /// Slots remain ordinary blocks so host schemas can place them with
    /// `@children("slot")`, while retaining their type syntax losslessly.
    pub slot_decl: Option<SlotDecl>,
    /// Labels written after the kind. The first is conventionally the
    /// block's id, via an `@inline(0)` field on its type.
    pub labels: Vec<Expr>,
    /// The block's body.
    pub items: Vec<Item>,
    /// Decorators attached to this block.
    pub decorators: Vec<Decorator>,
    /// Source span of the whole block.
    pub span: Span,
    /// Comments/blank lines printed above this block.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after the block's `}` (or after the
    /// kind/labels line for the empty-body shorthand).
    pub trailing_comment: Option<String>,
    /// Comments/blank lines after the last item, before `}`.
    pub trailing_trivia: Vec<Trivia>,
}

/// The type syntax of a `slot name: Type[? | *]` declaration, preserved
/// losslessly on the [`Block`] that carries it.
#[derive(Debug, Clone, PartialEq)]
pub struct SlotDecl {
    /// The declared slot type.
    pub ty: crate::value::TypeRef,
    /// Source span of the type annotation.
    pub ty_span: Span,
    /// `true` for the `?` suffix — the slot may be left unfilled.
    pub optional: bool,
    /// `true` for the `*` suffix — the slot accepts many fills.
    pub repeated: bool,
}

/// A `namespace a.b.c` declaration, scoping the names that follow it.
#[derive(Debug, Clone, PartialEq)]
pub struct NamespaceDecl {
    /// The dotted namespace path.
    pub path: Vec<String>,
    /// Source span of the whole declaration.
    pub span: Span,
    /// Comments/blank lines printed above this declaration.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this declaration.
    pub trailing_comment: Option<String>,
}

/// A `use a.b.c` declaration, bringing names into the current scope.
#[derive(Debug, Clone, PartialEq)]
pub struct UseDecl {
    /// The dotted path being imported from.
    pub path: Vec<String>,
    /// Which names are taken, and under what local spelling.
    pub form: UseForm,
    /// Source span of the whole declaration.
    pub span: Span,
    /// Comments/blank lines printed above this declaration.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this declaration.
    pub trailing_comment: Option<String>,
}

/// What a [`UseDecl`] brings into scope.
#[derive(Debug, Clone, PartialEq)]
pub enum UseForm {
    /// `use a.b.c` or `use a.b.c as d` — the path's last segment,
    /// optionally renamed.
    Bare(Option<String>),
    /// `use a.b.{x, y as z}` — an explicit list of names.
    List(Vec<UseItem>),
}

/// One name in a [`UseForm::List`].
#[derive(Debug, Clone, PartialEq)]
pub struct UseItem {
    /// The name as declared at the source path.
    pub name: String,
    /// The local spelling, when written as `name as alias`.
    pub alias: Option<String>,
    /// Source span of the entry.
    pub span: Span,
}

/// A `type Name { fields… }` declaration, or the alias form
/// `type Name = TypeRef`.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeDecl {
    /// The dotted type name.
    pub name: Vec<String>,
    /// Names of parent types/interfaces this declaration inherits
    /// from, in source order. Empty when no `extends` clause was
    /// written.
    pub extends: Vec<Vec<String>>,
    /// `Some` for the alias form `type Name = TypeRef` — a transparent
    /// name for the target type. `fields` and `extends` are then empty;
    /// constraint decorators (`@min` / `@max` / `@non_empty`) on the
    /// alias apply to every field declared with it.
    pub alias: Option<crate::value::TypeRef>,
    /// The declared fields, in source order.
    pub fields: Vec<TypeField>,
    /// Decorators attached to this declaration.
    pub decorators: Vec<Decorator>,
    /// Source span of the whole declaration.
    pub span: Span,
    /// Comments/blank lines printed above this declaration.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after the closing `}`.
    pub trailing_comment: Option<String>,
    /// Comments/blank lines after the last field, before `}`.
    pub trailing_trivia: Vec<Trivia>,
}

/// An `interface Name { fields… }` declaration. Unlike a [`TypeDecl`] an
/// interface is never instantiated: it constrains what a type must
/// structurally provide.
#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceDecl {
    /// The dotted interface name.
    pub name: Vec<String>,
    /// Parent types/interfaces — same shape as `TypeDecl::extends`.
    pub extends: Vec<Vec<String>>,
    /// The required fields, in source order.
    pub fields: Vec<TypeField>,
    /// Decorators attached to this declaration.
    pub decorators: Vec<Decorator>,
    /// Source span of the whole declaration.
    pub span: Span,
    /// Comments/blank lines printed above this declaration.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after the closing `}`.
    pub trailing_comment: Option<String>,
    /// Comments/blank lines after the last field, before `}`.
    pub trailing_trivia: Vec<Trivia>,
}

/// One field of a [`TypeDecl`], [`InterfaceDecl`] or record variant.
#[derive(Debug, Clone, PartialEq)]
pub struct TypeField {
    /// The field name.
    pub name: String,
    /// The declared (or, for the `name = expr` form, inferred) type.
    pub ty: crate::value::TypeRef,
    /// Source span of the type annotation.
    pub ty_span: Span,
    /// `true` for the `?` suffix — the field may be absent.
    pub optional: bool,
    /// Decorators attached to this field.
    pub decorators: Vec<Decorator>,
    /// Source span of the whole field.
    pub span: Span,
    /// Comments/blank lines printed above this field.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this field.
    pub trailing_comment: Option<String>,
    /// Inline default expression, set when the field is declared as
    /// `name = expr` (no explicit type). The type in `ty` is then
    /// inferred from the expression. `None` for the classical
    /// `name: type [?]` form.
    pub default_expr: Option<Expr>,
}

/// A `union Name { variants… }` declaration.
#[derive(Debug, Clone, PartialEq)]
pub struct UnionDecl {
    /// The dotted union name.
    pub name: Vec<String>,
    /// Parent unions whose variants are inherited. Empty when this
    /// union is declared without an `extends` clause. Variants are
    /// resolved through `Document::union_decl` then composed by
    /// `UnionDecl::effective_variants`.
    pub extends: Vec<Vec<String>>,
    /// The variants declared directly on this union.
    pub variants: Vec<UnionVariant>,
    /// Decorators attached to this declaration.
    pub decorators: Vec<Decorator>,
    /// Source span of the whole declaration.
    pub span: Span,
    /// Comments/blank lines printed above this declaration.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after the closing `}`.
    pub trailing_comment: Option<String>,
    /// Comments/blank lines after the last variant, before `}`.
    pub trailing_trivia: Vec<Trivia>,
}

/// One variant of a [`UnionDecl`].
#[derive(Debug, Clone, PartialEq)]
pub struct UnionVariant {
    /// The variant name.
    pub name: String,
    /// The variant's payload shape.
    pub body: VariantBody,
    /// Decorators attached to this variant.
    pub decorators: Vec<Decorator>,
    /// Source span of the whole variant.
    pub span: Span,
    /// Comments/blank lines printed above this variant.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this variant.
    pub trailing_comment: Option<String>,
}

/// The payload shape a union variant *declares*. Mirrors [`VariantArgs`],
/// which is the constructor form.
#[derive(Debug, Clone, PartialEq)]
pub enum VariantBody {
    /// Named fields, declared inline on the variant.
    Record {
        /// The declared fields.
        fields: Vec<TypeField>,
        /// Comments/blank lines after the last field, before `}`.
        trailing_trivia: Vec<Trivia>,
    },
    /// A single unnamed payload of the named type.
    TypeRef {
        /// The payload type.
        ty: crate::value::TypeRef,
        /// Source span of the type annotation.
        ty_span: Span,
    },
    /// `Drawn &Drawable` — variant payload is any value whose runtime
    /// type structurally implements the named interface.
    InterfaceRef {
        /// Dotted name of the required interface.
        iface: Vec<String>,
        /// Source span of the interface name.
        iface_span: Span,
    },
    /// No payload.
    Unit,
}

/// A `symbol_set Name { symbols… }` declaration — the closed set of
/// symbols a field of that type may take.
#[derive(Debug, Clone, PartialEq)]
pub struct SymbolSetDecl {
    /// The dotted symbol-set name.
    pub name: Vec<String>,
    /// The permitted symbols, in source order.
    pub symbols: Vec<SymbolEntry>,
    /// Decorators attached to this declaration.
    pub decorators: Vec<Decorator>,
    /// Source span of the whole declaration.
    pub span: Span,
    /// Comments/blank lines printed above this declaration.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after the closing `}`.
    pub trailing_comment: Option<String>,
    /// Comments/blank lines after the last symbol, before `}`.
    pub trailing_trivia: Vec<Trivia>,
}

/// One symbol in a [`SymbolSetDecl`].
#[derive(Debug, Clone, PartialEq)]
pub struct SymbolEntry {
    /// The symbol name, written without the leading `:`.
    pub name: String,
    /// Decorators attached to this symbol.
    pub decorators: Vec<Decorator>,
    /// Source span of the entry.
    pub span: Span,
    /// Comments/blank lines printed above this symbol.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this symbol.
    pub trailing_comment: Option<String>,
}

/// An `import` declaration, pulling another document's items into this
/// one.
#[derive(Debug, Clone, PartialEq)]
pub struct ImportDecl {
    /// The import path, as written.
    pub path: String,
    /// Source span of the path literal.
    pub path_span: Span,
    /// `true` for an angle-bracket system import (`import <wdoc/core.wcl>`,
    /// resolved through a registry); `false` for a quoted disk import
    /// (`import "./foo.wcl"`).
    pub system: bool,
    /// Source span of the whole declaration.
    pub span: Span,
    /// Comments/blank lines printed above this import.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this import.
    pub trailing_comment: Option<String>,
}

/// One `| a | b | c |` row of a [`TableItem`].
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// The cell expressions, in column order.
    pub values: Vec<Expr>,
    /// Source span of the row.
    pub span: Span,
}

/// A table item — a `field:` name followed by pipe-delimited rows, which
/// is sugar for a list of records.
#[derive(Debug, Clone, PartialEq)]
pub struct TableItem {
    /// The field the rows are collected into.
    pub field_name: String,
    /// The rows, in source order.
    pub rows: Vec<Row>,
    /// Source span of the whole item.
    pub span: Span,
    /// Comments/blank lines printed above the table.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after the table.
    pub trailing_comment: Option<String>,
}

/// A `connection Name: Source -> Destination` declaration, defining what
/// a [`ConnectionStmt`] may link and under which kinds.
#[derive(Debug, Clone, PartialEq)]
pub struct ConnectionDecl {
    /// The dotted connection name.
    pub name: Vec<String>,
    /// The type permitted on the left of a statement.
    pub source: crate::value::TypeRef,
    /// Source span of the source type.
    pub source_span: Span,
    /// The type permitted on the right of a statement.
    pub destination: crate::value::TypeRef,
    /// Source span of the destination type.
    pub destination_span: Span,
    /// The symbol set naming the permitted connection kinds.
    pub kind_set: Vec<String>,
    /// Source span of the kind set.
    pub kind_set_span: Span,
    /// Decorators attached to this declaration.
    pub decorators: Vec<Decorator>,
    /// Source span of the whole declaration.
    pub span: Span,
    /// Comments/blank lines printed above this declaration.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this declaration.
    pub trailing_comment: Option<String>,
}

/// A connection statement — `lhs -> rhs`, optionally tagged with a kind,
/// linking two blocks by id under a [`ConnectionDecl`].
#[derive(Debug, Clone, PartialEq)]
pub struct ConnectionStmt {
    /// Id of the block on the left.
    pub lhs: String,
    /// Source span of `lhs`.
    pub lhs_span: Span,
    /// Id of the block on the right.
    pub rhs: String,
    /// Source span of `rhs`.
    pub rhs_span: Span,
    /// The connection kind, when the statement names one.
    pub kind: Option<String>,
    /// Source span of `kind`, when present.
    pub kind_span: Option<Span>,
    /// Source span of the whole statement.
    pub span: Span,
    /// Comments/blank lines printed above this statement.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this statement.
    pub trailing_comment: Option<String>,
}

/// One top-level (or in-block) declaration. A [`Source`] is a list of
/// these, and a [`Block`]'s body is too.
#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    /// `name = expr` — document data.
    Field(Field),
    /// `let name = expr` or `fn name(…) …` — a composition helper.
    Let(LetItem),
    /// A nested block instance.
    Block(Block),
    /// A `type` declaration.
    TypeDecl(TypeDecl),
    /// An `interface` declaration.
    InterfaceDecl(InterfaceDecl),
    /// A `union` declaration.
    UnionDecl(UnionDecl),
    /// A `namespace` declaration.
    NamespaceDecl(NamespaceDecl),
    /// A `use` declaration.
    UseDecl(UseDecl),
    /// A `symbol_set` declaration.
    SymbolSetDecl(SymbolSetDecl),
    /// An `import` declaration.
    Import(ImportDecl),
    /// A table item.
    Table(TableItem),
    /// A `connection` declaration.
    ConnectionDecl(ConnectionDecl),
    /// A connection statement.
    Connection(ConnectionStmt),
}

impl Item {
    /// Attach a same-line trailing comment to this item, whatever its
    /// variant. Used by the parser to re-attach a comment that the lexer
    /// diverted as the next token's `same_line_comment` onto the item
    /// that ended the line.
    pub(crate) fn set_trailing_comment(&mut self, comment: String) {
        match self {
            Item::Field(x) => x.trailing_comment = Some(comment),
            Item::Let(x) => x.trailing_comment = Some(comment),
            Item::Block(x) => x.trailing_comment = Some(comment),
            Item::TypeDecl(x) => x.trailing_comment = Some(comment),
            Item::InterfaceDecl(x) => x.trailing_comment = Some(comment),
            Item::UnionDecl(x) => x.trailing_comment = Some(comment),
            Item::NamespaceDecl(x) => x.trailing_comment = Some(comment),
            Item::UseDecl(x) => x.trailing_comment = Some(comment),
            Item::SymbolSetDecl(x) => x.trailing_comment = Some(comment),
            Item::Import(x) => x.trailing_comment = Some(comment),
            Item::Table(x) => x.trailing_comment = Some(comment),
            Item::ConnectionDecl(x) => x.trailing_comment = Some(comment),
            Item::Connection(x) => x.trailing_comment = Some(comment),
        }
    }
}

/// One parsed file: its items plus the trivia that follows the last one.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Source {
    /// The file's top-level items, in source order.
    pub items: Vec<Item>,
    /// Comments/blank lines after the last top-level item, before EOF.
    pub trailing_trivia: Vec<Trivia>,
}
