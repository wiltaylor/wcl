#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

impl Span {
    pub fn new(start: usize, end: usize) -> Self {
        Self { start, end }
    }

    pub fn len(&self) -> usize {
        self.end - self.start
    }

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
    pub leading: Vec<Trivia>,
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

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Bool(bool),

    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    I128(i128),
    Isize(isize),

    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    U128(u128),
    Usize(usize),

    F32(f32),
    F64(f64),

    /// A numeric literal with a **literal unit** suffix (`5MiB`, `3km`).
    /// `value` is the raw magnitude (int → `i64`, float → `f64` by
    /// default); `unit` is the suffix name. The unit is resolved against
    /// the binding's declared type during evaluation — multiplying by the
    /// type's `@unit(name, factor)` decorator — so a `UnitLiteral` only
    /// type-checks where a unit-bearing type is in context.
    UnitLiteral {
        value: crate::lexer::NumberLit,
        unit: String,
        span: Span,
    },

    Utf8(String),
    Ascii(String),
    Utf16(Vec<u16>),
    Utf32(Vec<char>),

    /// Opt-in interpolated string literal (`$"…"`, `$ascii"…"`,
    /// `$<<TAG ... TAG`, …). Each part is either a literal body chunk
    /// or a sub-parsed expression; the evaluator concatenates the
    /// stringified results and re-encodes per the declared encoding.
    InterpolatedString {
        encoding: crate::lexer::StringEncoding,
        parts: Vec<TemplatePart>,
        span: Span,
    },

    Identifier(String, Span),
    Symbol(String),
    None,

    Function(FunctionLit),
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        /// Per-argument comment trivia, index-aligned with `args`. Empty
        /// (or all-default) when the call carries no comments.
        arg_trivia: Vec<ElemTrivia>,
        /// Comments/blank lines after the last argument, before `)`.
        trailing_trivia: Vec<Trivia>,
        span: Span,
    },
    Binary {
        op: BinOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
        span: Span,
    },
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
        span: Span,
    },
    Block {
        lets: Vec<LetBinding>,
        tail: Box<Expr>,
        /// Comments/blank lines between the last `let` (or the tail) and
        /// the closing `}` of the block expression.
        trailing_trivia: Vec<Trivia>,
        span: Span,
    },
    Paren {
        inner: Box<Expr>,
        span: Span,
    },

    ListLit {
        elements: Vec<Expr>,
        /// Per-element comment trivia, index-aligned with `elements`.
        elem_trivia: Vec<ElemTrivia>,
        /// Comments/blank lines after the last element, before `]`.
        trailing_trivia: Vec<Trivia>,
        span: Span,
    },

    SelfKw(Span),
    ParentKw(Span),
    Member {
        recv: Box<Expr>,
        name: String,
        span: Span,
    },

    If {
        cond: Box<Expr>,
        then_block: Box<Expr>,
        /// Absent when the source omits `else`; the untaken branch of an
        /// else-less `if` evaluates to `none`.
        else_block: Option<Box<Expr>>,
        span: Span,
    },
    IfLet {
        pattern: Pattern,
        scrut: Box<Expr>,
        then_block: Box<Expr>,
        else_block: Box<Expr>,
        span: Span,
    },
    Match {
        scrut: Box<Expr>,
        arms: Vec<MatchArm>,
        /// Comments/blank lines after the last arm, before `}`.
        trailing_trivia: Vec<Trivia>,
        span: Span,
    },
    Variant {
        type_path: Vec<String>,
        variant: String,
        args: VariantArgs,
        span: Span,
    },
    /// `try body catch name => handler` — evaluate `body`; on an
    /// evaluation error bind the error message (a `utf8`) to `name`
    /// and evaluate `handler` instead. Catches every evaluation error
    /// (builtin failures, cycles, propagated field errors).
    Try {
        body: Box<Expr>,
        /// Name bound to the error message inside `handler`.
        binder: String,
        binder_span: Span,
        handler: Box<Expr>,
        span: Span,
    },
    /// A bare record literal `{ name: value, … }`. When the surrounding
    /// context declares a union (or `list<union>`) type, the evaluator
    /// shape-infers the matching variant; otherwise it evaluates to an
    /// anonymous `Value::Record`.
    Record {
        fields: Vec<NamedArg>,
        /// Comments/blank lines after the last field, before `}`.
        trailing_trivia: Vec<Trivia>,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum TemplatePart {
    Literal(String),
    Expr(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    /// One or more alternative patterns. The arm fires when any matches.
    pub patterns: Vec<Pattern>,
    /// Optional `if` guard, evaluated in the matched pattern's scope.
    pub guard: Option<Expr>,
    pub body: Expr,
    pub span: Span,
    /// Comments/blank lines printed above this arm.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this arm.
    pub trailing_comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VariantArgs {
    Unit,
    Positional(Box<Expr>),
    Record {
        fields: Vec<NamedArg>,
        /// Comments/blank lines after the last field, before `}`.
        trailing_trivia: Vec<Trivia>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Wildcard(Span),
    Binding {
        name: String,
        span: Span,
    },
    /// `name @ inner` — binds `name` to the full value while matching
    /// `inner` against it.
    At {
        name: String,
        inner: Box<Pattern>,
        span: Span,
    },
    LiteralBool(bool, Span),
    LiteralNumber {
        lit: crate::lexer::NumberLit,
        span: Span,
    },
    LiteralUtf8(String, Span),
    LiteralAscii(String, Span),
    LiteralSymbol(String, Span),
    LiteralNone(Span),
    Variant {
        type_path: Vec<String>,
        variant: String,
        args: VariantPatArgs,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum VariantPatArgs {
    Unit,
    Positional(Box<Pattern>),
    Record {
        fields: Vec<(String, Pattern)>,
        rest: bool,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionLit {
    pub params: Vec<Parameter>,
    pub return_ty: crate::value::TypeRef,
    pub return_ty_span: Span,
    /// `Arc`, not `Box`: every evaluation of the literal builds an
    /// `FnValue` sharing this body, and resolving a named function
    /// clones that `FnValue` per call — a deep AST clone on either
    /// path dominated closure-heavy documents.
    pub body: std::sync::Arc<Expr>,
    pub span: Span,
    /// Comments/blank lines after the last parameter, before `)`.
    pub trailing_trivia: Vec<Trivia>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Parameter {
    pub name: String,
    pub ty: crate::value::TypeRef,
    pub ty_span: Span,
    pub span: Span,
    /// Comments/blank lines printed above this parameter.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this parameter.
    pub trailing_comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LetBinding {
    pub name: String,
    pub value: Expr,
    pub span: Span,
    /// Comments/blank lines printed above this `let` binding.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this `let` binding.
    pub trailing_comment: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    And,
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
pub const CALL_BP: u8 = 16;
pub const MEMBER_BP: u8 = 17;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Decorator {
    pub name: Vec<String>,
    /// Span of the dotted name only, excluding the leading `@` and any
    /// arguments. Decorator-level diagnostics point here.
    pub name_span: Span,
    pub positional: Vec<Expr>,
    /// Source spans index-aligned with [`Self::positional`]. Synthetic
    /// decorators carry empty spans for their synthetic arguments.
    pub positional_spans: Vec<Span>,
    pub named: Vec<NamedArg>,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SchemalessMode {
    Full,
    AnnotationsOnly,
}

impl Decorator {
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

#[derive(Debug, Clone, PartialEq)]
pub struct NamedArg {
    pub name: String,
    pub value: Expr,
    pub span: Span,
    /// Comments/blank lines printed above this field (record literals).
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this field.
    pub trailing_comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub expr: Expr,
    pub decorators: Vec<Decorator>,
    pub span: Span,
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
    pub name: String,
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
    pub span: Span,
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this `let` item.
    pub trailing_comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
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
    pub labels: Vec<Expr>,
    pub items: Vec<Item>,
    pub decorators: Vec<Decorator>,
    pub span: Span,
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after the block's `}` (or after the
    /// kind/labels line for the empty-body shorthand).
    pub trailing_comment: Option<String>,
    /// Comments/blank lines after the last item, before `}`.
    pub trailing_trivia: Vec<Trivia>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SlotDecl {
    pub ty: crate::value::TypeRef,
    pub ty_span: Span,
    pub optional: bool,
    pub repeated: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NamespaceDecl {
    pub path: Vec<String>,
    pub span: Span,
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this declaration.
    pub trailing_comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UseDecl {
    pub path: Vec<String>,
    pub form: UseForm,
    pub span: Span,
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this declaration.
    pub trailing_comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UseForm {
    Bare(Option<String>),
    List(Vec<UseItem>),
}

#[derive(Debug, Clone, PartialEq)]
pub struct UseItem {
    pub name: String,
    pub alias: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeDecl {
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
    pub fields: Vec<TypeField>,
    pub decorators: Vec<Decorator>,
    pub span: Span,
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after the closing `}`.
    pub trailing_comment: Option<String>,
    /// Comments/blank lines after the last field, before `}`.
    pub trailing_trivia: Vec<Trivia>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InterfaceDecl {
    pub name: Vec<String>,
    /// Parent types/interfaces — same shape as `TypeDecl::extends`.
    pub extends: Vec<Vec<String>>,
    pub fields: Vec<TypeField>,
    pub decorators: Vec<Decorator>,
    pub span: Span,
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after the closing `}`.
    pub trailing_comment: Option<String>,
    /// Comments/blank lines after the last field, before `}`.
    pub trailing_trivia: Vec<Trivia>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TypeField {
    pub name: String,
    pub ty: crate::value::TypeRef,
    pub ty_span: Span,
    pub optional: bool,
    pub decorators: Vec<Decorator>,
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

#[derive(Debug, Clone, PartialEq)]
pub struct UnionDecl {
    pub name: Vec<String>,
    /// Parent unions whose variants are inherited. Empty when this
    /// union is declared without an `extends` clause. Variants are
    /// resolved through `Document::union_decl` then composed by
    /// `UnionDecl::effective_variants`.
    pub extends: Vec<Vec<String>>,
    pub variants: Vec<UnionVariant>,
    pub decorators: Vec<Decorator>,
    pub span: Span,
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after the closing `}`.
    pub trailing_comment: Option<String>,
    /// Comments/blank lines after the last variant, before `}`.
    pub trailing_trivia: Vec<Trivia>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnionVariant {
    pub name: String,
    pub body: VariantBody,
    pub decorators: Vec<Decorator>,
    pub span: Span,
    /// Comments/blank lines printed above this variant.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this variant.
    pub trailing_comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum VariantBody {
    Record {
        fields: Vec<TypeField>,
        /// Comments/blank lines after the last field, before `}`.
        trailing_trivia: Vec<Trivia>,
    },
    TypeRef {
        ty: crate::value::TypeRef,
        ty_span: Span,
    },
    /// `Drawn &Drawable` — variant payload is any value whose runtime
    /// type structurally implements the named interface.
    InterfaceRef {
        iface: Vec<String>,
        iface_span: Span,
    },
    Unit,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SymbolSetDecl {
    pub name: Vec<String>,
    pub symbols: Vec<SymbolEntry>,
    pub decorators: Vec<Decorator>,
    pub span: Span,
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after the closing `}`.
    pub trailing_comment: Option<String>,
    /// Comments/blank lines after the last symbol, before `}`.
    pub trailing_trivia: Vec<Trivia>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SymbolEntry {
    pub name: String,
    pub decorators: Vec<Decorator>,
    pub span: Span,
    /// Comments/blank lines printed above this symbol.
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this symbol.
    pub trailing_comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportDecl {
    pub path: String,
    pub path_span: Span,
    /// `true` for an angle-bracket system import (`import <wdoc/core.wcl>`,
    /// resolved through a registry); `false` for a quoted disk import
    /// (`import "./foo.wcl"`).
    pub system: bool,
    pub span: Span,
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this import.
    pub trailing_comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub values: Vec<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TableItem {
    pub field_name: String,
    pub rows: Vec<Row>,
    pub span: Span,
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after the table.
    pub trailing_comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConnectionDecl {
    pub name: Vec<String>,
    pub source: crate::value::TypeRef,
    pub source_span: Span,
    pub destination: crate::value::TypeRef,
    pub destination_span: Span,
    pub kind_set: Vec<String>,
    pub kind_set_span: Span,
    pub decorators: Vec<Decorator>,
    pub span: Span,
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this declaration.
    pub trailing_comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ConnectionStmt {
    pub lhs: String,
    pub lhs_span: Span,
    pub rhs: String,
    pub rhs_span: Span,
    pub kind: Option<String>,
    pub kind_span: Option<Span>,
    pub span: Span,
    pub leading_trivia: Vec<Trivia>,
    /// A same-line comment printed after this statement.
    pub trailing_comment: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Item {
    Field(Field),
    Let(LetItem),
    Block(Block),
    TypeDecl(TypeDecl),
    InterfaceDecl(InterfaceDecl),
    UnionDecl(UnionDecl),
    NamespaceDecl(NamespaceDecl),
    UseDecl(UseDecl),
    SymbolSetDecl(SymbolSetDecl),
    Import(ImportDecl),
    Table(TableItem),
    ConnectionDecl(ConnectionDecl),
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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Source {
    pub items: Vec<Item>,
    /// Comments/blank lines after the last top-level item, before EOF.
    pub trailing_trivia: Vec<Trivia>,
}
