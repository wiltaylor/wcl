//! WCL Abstract Syntax Tree definitions.
//!
//! Every node carries a `span: Span` (byte-range in the source file) and a
//! `trivia: Trivia` (leading/trailing whitespace and comments) so that the
//! tree can be used for both evaluation and lossless round-trip formatting.
//!
//! The types in this module mirror the EBNF grammar in Section 30 of the WCL
//! specification.  Recursive types use `Box` to keep enum/struct sizes
//! reasonable.

use crate::span::Span;
use crate::trivia::Trivia;

// ===== Top-level Document =====

/// The root node of a parsed WCL file.
#[derive(Debug, Clone)]
pub struct Document {
    pub items: Vec<DocItem>,
    pub trivia: Trivia,
    pub span: Span,
}

/// A single top-level item: an import/export directive or a body item.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone)]
pub enum DocItem {
    Import(Import),
    ExportLet(ExportLet),
    ReExport(ReExport),
    Body(BodyItem),
    FunctionDecl(FunctionDecl),
}

/// Whether an import uses a relative path or a well-known library name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportKind {
    /// `import "path/to/file.wcl"`
    Relative,
    /// `import <name.wcl>`
    Library,
}

/// `import "./path/to/file.wcl"` or `import <name.wcl>`
#[derive(Debug, Clone)]
pub struct Import {
    pub path: StringLit,
    pub kind: ImportKind,
    pub trivia: Trivia,
    pub span: Span,
}

/// `export let name = <expr>`
#[derive(Debug, Clone)]
pub struct ExportLet {
    pub name: Ident,
    pub value: Expr,
    pub trivia: Trivia,
    pub span: Span,
}

/// `export name`  (re-export a previously bound identifier)
#[derive(Debug, Clone)]
pub struct ReExport {
    pub name: Ident,
    pub trivia: Trivia,
    pub span: Span,
}

/// `declare fn_name(param: type, ...) -> return_type`
#[derive(Debug, Clone)]
pub struct FunctionDecl {
    pub name: Ident,
    pub params: Vec<FunctionDeclParam>,
    pub return_type: Option<TypeExpr>,
    pub doc: Option<String>,
    pub trivia: Trivia,
    pub span: Span,
}

/// A single parameter in a function declaration.
#[derive(Debug, Clone)]
pub struct FunctionDeclParam {
    pub name: Ident,
    pub type_expr: TypeExpr,
    pub span: Span,
}

// ===== Body Items =====

/// Any item that can appear inside a block body or at the top level.
#[derive(Debug, Clone)]
pub enum BodyItem {
    Attribute(Attribute),
    Block(Block),
    Table(Table),
    LetBinding(LetBinding),
    MacroDef(MacroDef),
    MacroCall(MacroCall),
    ForLoop(ForLoop),
    Conditional(Conditional),
    Validation(Validation),
    Schema(Schema),
    DecoratorSchema(DecoratorSchema),
}

// ===== Identifiers =====

/// A plain WCL identifier: `[a-zA-Z_][a-zA-Z0-9_]*`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

/// An identifier literal (the `id` type): may contain hyphens.
/// Grammar: `[a-zA-Z_][a-zA-Z0-9_\-]*`
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct IdentifierLit {
    pub value: String,
    pub span: Span,
}

/// A string literal, potentially containing interpolated expressions.
#[derive(Debug, Clone)]
pub struct StringLit {
    pub parts: Vec<StringPart>,
    pub span: Span,
}

/// A segment of a string literal.
#[derive(Debug, Clone)]
pub enum StringPart {
    /// A run of literal (non-interpolated) characters.
    Literal(String),
    /// An interpolated expression: `${expr}`.
    Interpolation(Box<Expr>),
}

// ===== Attributes =====

/// `[@decorator...] name = <expr>`
#[derive(Debug, Clone)]
pub struct Attribute {
    pub decorators: Vec<Decorator>,
    pub name: Ident,
    pub value: Expr,
    pub trivia: Trivia,
    pub span: Span,
}

// ===== Blocks =====

/// `[@decorator...] [partial] kind [IDENTIFIER_LIT] [inline_args...] ( { body } | HEREDOC | STRING )`
#[derive(Debug, Clone)]
pub struct Block {
    pub decorators: Vec<Decorator>,
    pub partial: bool,
    pub kind: Ident,
    pub inline_id: Option<InlineId>,
    pub inline_args: Vec<Expr>,
    pub body: Vec<BodyItem>,
    pub text_content: Option<StringLit>,
    pub trivia: Trivia,
    pub span: Span,
}

/// The optional inline ID that follows a block-kind keyword.
///
/// In a regular block it is a plain identifier literal.  Inside a `for` loop
/// the ID may contain interpolated segments, e.g. `svc-worker-${region}`.
#[derive(Debug, Clone)]
pub enum InlineId {
    Literal(IdentifierLit),
    Interpolated(Vec<StringPart>),
}

// ===== Let Bindings =====

/// `[@decorator...] let name = <expr>`
#[derive(Debug, Clone)]
pub struct LetBinding {
    pub decorators: Vec<Decorator>,
    pub name: Ident,
    pub value: Expr,
    pub trivia: Trivia,
    pub span: Span,
}

// ===== Decorators =====

/// `@name` or `@name(args...)`
#[derive(Debug, Clone)]
pub struct Decorator {
    pub name: Ident,
    pub args: Vec<DecoratorArg>,
    pub span: Span,
}

/// A single argument in a decorator invocation.
#[derive(Debug, Clone)]
pub enum DecoratorArg {
    Positional(Expr),
    Named(Ident, Expr),
}

// ===== Tables =====

/// `[@decorator...] [partial] table IDENTIFIER_LIT [: schema_ref] { columns rows }`
/// or `[@decorator...] table IDENTIFIER_LIT [: schema_ref] = import_table(...)`
#[derive(Debug, Clone)]
pub struct Table {
    pub decorators: Vec<Decorator>,
    pub partial: bool,
    pub inline_id: Option<InlineId>,
    pub schema_ref: Option<Ident>,
    pub columns: Vec<ColumnDecl>,
    pub rows: Vec<TableRow>,
    pub import_expr: Option<Box<Expr>>,
    pub trivia: Trivia,
    pub span: Span,
}

/// Arguments to the `import_table(...)` expression.
#[derive(Debug, Clone)]
pub struct ImportTableArgs {
    pub path: StringLit,
    pub separator: Option<StringLit>,
    pub headers: Option<bool>,
    pub columns: Option<Vec<StringLit>>,
}

/// A single column declaration inside a table: `[@decorator...] name : type_expr`
#[derive(Debug, Clone)]
pub struct ColumnDecl {
    pub decorators: Vec<Decorator>,
    pub name: Ident,
    pub type_expr: TypeExpr,
    pub trivia: Trivia,
    pub span: Span,
}

/// A single data row inside a table: `| expr | expr | ... |`
#[derive(Debug, Clone)]
pub struct TableRow {
    pub cells: Vec<Expr>,
    pub span: Span,
}

// ===== Schemas =====

/// `[@decorator...] schema "name" { fields... }`
#[derive(Debug, Clone)]
pub struct Schema {
    pub decorators: Vec<Decorator>,
    pub name: StringLit,
    pub fields: Vec<SchemaField>,
    pub trivia: Trivia,
    pub span: Span,
}

/// A single field inside a schema or decorator schema.
///
/// Decorators can appear both before and after the type expression.
#[derive(Debug, Clone)]
pub struct SchemaField {
    pub decorators_before: Vec<Decorator>,
    pub name: Ident,
    pub type_expr: TypeExpr,
    pub decorators_after: Vec<Decorator>,
    pub trivia: Trivia,
    pub span: Span,
}

// ===== Decorator Schemas =====

/// `[@decorator...] decorator_schema "name" { target = [...] fields... }`
#[derive(Debug, Clone)]
pub struct DecoratorSchema {
    pub decorators: Vec<Decorator>,
    pub name: StringLit,
    pub target: Vec<DecoratorTarget>,
    pub fields: Vec<SchemaField>,
    pub trivia: Trivia,
    pub span: Span,
}

/// The kinds of AST nodes a decorator schema may be applied to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecoratorTarget {
    Block,
    Attribute,
    Table,
    Schema,
}

// ===== Macros =====

/// `[@decorator...] macro name(params) { body }`  or
/// `[@decorator...] macro @name(params) { transform_body }`
#[derive(Debug, Clone)]
pub struct MacroDef {
    pub decorators: Vec<Decorator>,
    pub kind: MacroKind,
    pub name: Ident,
    pub params: Vec<MacroParam>,
    pub body: MacroBody,
    pub trivia: Trivia,
    pub span: Span,
}

/// Whether a macro is a function macro or an attribute macro.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MacroKind {
    /// `macro name(...)  { body_items... }`
    Function,
    /// `macro @name(...) { transform_directives... }`
    Attribute,
}

/// A single formal parameter in a macro definition.
#[derive(Debug, Clone)]
pub struct MacroParam {
    pub name: Ident,
    pub type_constraint: Option<TypeExpr>,
    pub default: Option<Expr>,
    pub span: Span,
}

/// The body of a macro definition.
#[derive(Debug, Clone)]
pub enum MacroBody {
    /// Function macro: a sequence of body items to splice in at the call site.
    Function(Vec<BodyItem>),
    /// Attribute macro: a sequence of transform directives.
    Attribute(Vec<TransformDirective>),
}

/// A directive inside an attribute macro body.
#[derive(Debug, Clone)]
pub enum TransformDirective {
    Inject(InjectBlock),
    Set(SetBlock),
    Remove(RemoveBlock),
    When(WhenBlock),
    Update(UpdateBlock),
}

/// Selector for update directive.
#[derive(Debug, Clone)]
pub enum TargetSelector {
    /// All children of kind
    BlockKind(Ident),
    /// `kind#id`
    BlockKindId(Ident, IdentifierLit),
    /// `kind[n]`
    BlockIndex(Ident, usize, Span),
    /// `table#id`
    TableId(IdentifierLit),
    /// `table[n]`
    TableIndex(usize, Span),
}

/// Row-level operation inside `update table#... { }`.
#[derive(Debug, Clone)]
pub enum TableDirective {
    /// `inject_rows { rows }`
    InjectRows(Vec<TableRow>, Span),
    /// `remove_rows where <expr>`
    RemoveRows { condition: Expr, span: Span },
    /// `update_rows where <expr> { set { k = v, ... } }`
    UpdateRows {
        condition: Expr,
        attrs: Vec<(Ident, Expr)>,
        span: Span,
    },
    /// `clear_rows`
    ClearRows(Span),
}

/// `update <selector> { directives... }`
#[derive(Debug, Clone)]
pub struct UpdateBlock {
    pub selector: TargetSelector,
    pub block_directives: Vec<TransformDirective>,
    pub table_directives: Vec<TableDirective>,
    pub span: Span,
}

/// `inject { body_items... }`
#[derive(Debug, Clone)]
pub struct InjectBlock {
    pub body: Vec<BodyItem>,
    pub span: Span,
}

/// `set { attributes... }`
#[derive(Debug, Clone)]
pub struct SetBlock {
    pub attrs: Vec<Attribute>,
    pub span: Span,
}

/// `remove [ target, ... ]`
#[derive(Debug, Clone)]
pub struct RemoveBlock {
    pub targets: Vec<RemoveTarget>,
    pub span: Span,
}

/// Target in a remove list.
#[derive(Debug, Clone)]
pub enum RemoveTarget {
    /// Bare ident → attribute
    Attr(Ident),
    /// `kind#id` → child block by kind and ID
    Block(Ident, IdentifierLit),
    /// `kind#*` → all child blocks of kind
    BlockAll(Ident),
    /// `kind[n]` → child block by kind and index (0-based)
    BlockIndex(Ident, usize, Span),
    /// `table#id` → table by ID
    Table(IdentifierLit),
    /// `table#*` → all tables
    AllTables(Span),
    /// `table[n]` → table by index (0-based)
    TableIndex(usize, Span),
}

/// `when <expr> { transform_directives... }`
#[derive(Debug, Clone)]
pub struct WhenBlock {
    pub condition: Expr,
    pub directives: Vec<TransformDirective>,
    pub span: Span,
}

// ===== Macro Calls =====

/// `name(args...)`  — call site for a function macro.
#[derive(Debug, Clone)]
pub struct MacroCall {
    pub name: Ident,
    pub args: Vec<MacroCallArg>,
    pub trivia: Trivia,
    pub span: Span,
}

/// A single argument at a macro call site.
#[derive(Debug, Clone)]
pub enum MacroCallArg {
    Positional(Expr),
    Named(Ident, Expr),
}

// ===== Control Flow =====

/// `for iterator [, index] in <iterable_expr> { body }`
#[derive(Debug, Clone)]
pub struct ForLoop {
    pub iterator: Ident,
    pub index: Option<Ident>,
    pub iterable: Expr,
    pub body: Vec<BodyItem>,
    pub trivia: Trivia,
    pub span: Span,
}

/// `if <expr> { then_body } [else_branch]`
#[derive(Debug, Clone)]
pub struct Conditional {
    pub condition: Expr,
    pub then_body: Vec<BodyItem>,
    pub else_branch: Option<ElseBranch>,
    pub trivia: Trivia,
    pub span: Span,
}

/// The optional `else` / `else if` branch of a conditional.
#[derive(Debug, Clone)]
pub enum ElseBranch {
    /// `else if <expr> { ... } [else_branch]`
    ElseIf(Box<Conditional>),
    /// `else { body }`
    Else(Vec<BodyItem>, Trivia, Span),
}

// ===== Validation =====

/// `[@decorator...] validation "name" { [let bindings...] check = <expr>  message = <expr> }`
#[derive(Debug, Clone)]
pub struct Validation {
    pub decorators: Vec<Decorator>,
    pub name: StringLit,
    pub lets: Vec<LetBinding>,
    pub check: Expr,
    pub message: Expr,
    pub trivia: Trivia,
    pub span: Span,
}

// ===== Type Expressions =====

/// A WCL type annotation.
///
/// The `Span` inside each variant covers the entire textual representation of
/// the type, including any nested type arguments.
#[derive(Debug, Clone)]
pub enum TypeExpr {
    /// `string`
    String(Span),
    /// `int`
    Int(Span),
    /// `float`
    Float(Span),
    /// `bool`
    Bool(Span),
    /// `null`
    Null(Span),
    /// `identifier`
    Identifier(Span),
    /// `any`
    Any(Span),
    /// `list(inner)`
    List(Box<TypeExpr>, Span),
    /// `map(key, value)`
    Map(Box<TypeExpr>, Box<TypeExpr>, Span),
    /// `set(inner)`
    Set(Box<TypeExpr>, Span),
    /// `ref("schema_name")`
    Ref(StringLit, Span),
    /// `union(t1, t2, ...)`
    Union(Vec<TypeExpr>, Span),
}

impl TypeExpr {
    /// Returns the source span of this type expression.
    pub fn span(&self) -> Span {
        match self {
            TypeExpr::String(s)
            | TypeExpr::Int(s)
            | TypeExpr::Float(s)
            | TypeExpr::Bool(s)
            | TypeExpr::Null(s)
            | TypeExpr::Identifier(s)
            | TypeExpr::Any(s)
            | TypeExpr::List(_, s)
            | TypeExpr::Map(_, _, s)
            | TypeExpr::Set(_, s)
            | TypeExpr::Ref(_, s)
            | TypeExpr::Union(_, s) => *s,
        }
    }
}

// ===== Expressions =====

/// A WCL expression.  All variants carry their source `Span` either inline or
/// via a nested node that exposes `.span`.
#[derive(Debug, Clone)]
pub enum Expr {
    /// Integer literal: `42`, `0xFF`, `0o77`, `0b1010`
    IntLit(i64, Span),
    /// Floating-point literal: `3.14`, `1.0e-5`
    FloatLit(f64, Span),
    /// String literal (possibly with interpolation)
    StringLit(StringLit),
    /// Boolean literal: `true` | `false`
    BoolLit(bool, Span),
    /// `null`
    NullLit(Span),
    /// Identifier reference (variable or attribute name)
    Ident(Ident),
    /// Identifier literal (`id` type value), e.g. `svc-auth`
    IdentifierLit(IdentifierLit),
    /// List literal: `[a, b, c]`
    List(Vec<Expr>, Span),
    /// Map literal: `{ key = val, ... }`
    Map(Vec<(MapKey, Expr)>, Span),
    /// Binary operation: `lhs op rhs`
    BinaryOp(Box<Expr>, BinOp, Box<Expr>, Span),
    /// Unary operation: `op expr`
    UnaryOp(UnaryOp, Box<Expr>, Span),
    /// Ternary conditional: `cond ? then : else`
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>, Span),
    /// Member access: `expr.field`
    MemberAccess(Box<Expr>, Ident, Span),
    /// Index access: `expr[index]`
    IndexAccess(Box<Expr>, Box<Expr>, Span),
    /// Function / method call: `callee(args...)`
    FnCall(Box<Expr>, Vec<CallArg>, Span),
    /// Lambda expression: `params => body`
    Lambda(Vec<Ident>, Box<Expr>, Span),
    /// Block expression: `{ let bindings... final_expr }`
    BlockExpr(Vec<LetBinding>, Box<Expr>, Span),
    /// Query expression: `query(pipeline)`
    Query(QueryPipeline, Span),
    /// `ref(identifier_lit)` — a cross-reference to another block by ID
    Ref(IdentifierLit, Span),
    /// `import_raw("path")` — import file contents as a raw string
    ImportRaw(StringLit, Span),
    /// `import_table("path", ...)` — import a CSV/TSV file as a table
    ImportTable(ImportTableArgs, Span),
    /// Parenthesized expression — preserved for formatting fidelity
    Paren(Box<Expr>, Span),
}

impl Expr {
    /// Returns the source span of this expression.
    pub fn span(&self) -> Span {
        match self {
            Expr::IntLit(_, s)
            | Expr::FloatLit(_, s)
            | Expr::BoolLit(_, s)
            | Expr::NullLit(s)
            | Expr::List(_, s)
            | Expr::Map(_, s)
            | Expr::BinaryOp(_, _, _, s)
            | Expr::UnaryOp(_, _, s)
            | Expr::Ternary(_, _, _, s)
            | Expr::MemberAccess(_, _, s)
            | Expr::IndexAccess(_, _, s)
            | Expr::FnCall(_, _, s)
            | Expr::Lambda(_, _, s)
            | Expr::BlockExpr(_, _, s)
            | Expr::Query(_, s)
            | Expr::Ref(_, s)
            | Expr::ImportRaw(_, s)
            | Expr::ImportTable(_, s)
            | Expr::Paren(_, s) => *s,
            Expr::StringLit(s) => s.span,
            Expr::Ident(i) => i.span,
            Expr::IdentifierLit(i) => i.span,
        }
    }
}

/// The key of a map literal entry.
#[derive(Debug, Clone)]
pub enum MapKey {
    Ident(Ident),
    String(StringLit),
}

/// A single argument in a function/method call expression.
#[derive(Debug, Clone)]
pub enum CallArg {
    Positional(Expr),
    Named(Ident, Expr),
}

// ===== Binary and Unary Operators =====

/// Binary infix operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinOp {
    Add,   // +
    Sub,   // -
    Mul,   // *
    Div,   // /
    Mod,   // %
    Eq,    // ==
    Neq,   // !=
    Lt,    // <
    Gt,    // >
    Lte,   // <=
    Gte,   // >=
    Match, // =~
    And,   // &&
    Or,    // ||
}

impl BinOp {
    /// Operator precedence (higher = tighter binding).
    ///
    /// Matches the grammar hierarchy in Section 30:
    /// `||` (2) < `&&` (3) < `== !=` (4) < `< > <= >= =~` (5) < `+ -` (6) < `* / %` (7)
    pub fn precedence(self) -> u8 {
        match self {
            BinOp::Or => 2,
            BinOp::And => 3,
            BinOp::Eq | BinOp::Neq => 4,
            BinOp::Lt | BinOp::Gt | BinOp::Lte | BinOp::Gte | BinOp::Match => 5,
            BinOp::Add | BinOp::Sub => 6,
            BinOp::Mul | BinOp::Div | BinOp::Mod => 7,
        }
    }
}

/// Unary prefix operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not, // !
    Neg, // -
}

// ===== Query Pipeline =====

/// A query pipeline: `selector [ | filter ]*`
#[derive(Debug, Clone)]
pub struct QueryPipeline {
    pub selector: QuerySelector,
    pub filters: Vec<QueryFilter>,
    pub span: Span,
}

/// The initial selector of a query pipeline.
///
/// Corresponds to the `selector` production in Section 30.
#[derive(Debug, Clone)]
pub enum QuerySelector {
    /// `kind`  — match all blocks of this kind
    Kind(Ident),
    /// `kind#id`  — match a specific block by kind and inline ID
    KindId(Ident, IdentifierLit),
    /// `seg1.seg2.seg3`  — dot-separated path through nested blocks/attributes
    Path(Vec<PathSegment>),
    /// `..kind`  — recursive descent matching all blocks of kind
    Recursive(Ident),
    /// `..kind#id`  — recursive descent with inline ID filter
    RecursiveId(Ident, IdentifierLit),
    /// `.`  — the root document node
    Root,
    /// `*`  — all direct children
    Wildcard,
    /// `table#id`  — select a table by inline ID
    TableId(IdentifierLit),
}

/// A segment in a dot-separated query path.
#[derive(Debug, Clone)]
pub enum PathSegment {
    Ident(Ident),
}

/// A filter step in a query pipeline (after `|`).
///
/// Corresponds to the `filter` production in Section 30.
#[derive(Debug, Clone)]
pub enum QueryFilter {
    /// `.attr op expr`  — compare an attribute value
    AttrComparison(Ident, BinOp, Expr),
    /// `.attr`  — project (select) the value of an attribute
    Projection(Ident),
    /// `has(.attr)`  — test whether an attribute is present
    HasAttr(Ident),
    /// `has(@decorator)`  — test whether a decorator is applied
    HasDecorator(Ident),
    /// `@decorator.param op expr`  — compare a decorator argument value
    DecoratorArgFilter(Ident, Ident, BinOp, Expr),
}
