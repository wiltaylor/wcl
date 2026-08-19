//! Expressions: everything that can appear on the right of an `=`.
//!
//! Parsed by [`parser::expr`](crate::parser), printed by
//! [`format::expr`](crate::format). The operator binding powers live
//! here rather than in either of them, so the two cannot disagree about
//! precedence.

use super::{ElemTrivia, Pattern, Span, Trivia, TypeRef};

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
/// Mirrors [`VariantBody`](super::VariantBody), which is the declaration form.
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

/// An anonymous function literal.
#[derive(Debug, Clone, PartialEq)]
pub struct FunctionLit {
    /// Declared parameters, in order.
    pub params: Vec<Parameter>,
    /// The declared return type.
    pub return_ty: TypeRef,
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
    pub ty: TypeRef,
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
/// the item-level [`LetItem`](super::LetItem), which is declared at file or block scope.
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
