#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Expr {
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

    Utf8(String),
    Ascii(String),
    Utf16(Vec<u16>),
    Utf32(Vec<char>),

    Identifier(String),
    Symbol(String),
    None,

    Function(FunctionLit),
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
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
        span: Span,
    },
    Paren {
        inner: Box<Expr>,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct FunctionLit {
    pub params: Vec<Parameter>,
    pub return_ty: crate::value::TypeRef,
    pub return_ty_span: Span,
    pub body: Box<Expr>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Parameter {
    pub name: String,
    pub ty: crate::value::TypeRef,
    pub ty_span: Span,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LetBinding {
    pub name: String,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum BinOp {
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum UnaryOp {
    Neg,
    Not,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Decorator {
    pub name: Vec<String>,
    pub positional: Vec<Expr>,
    pub named: Vec<NamedArg>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NamedArg {
    pub name: String,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Field {
    pub name: String,
    pub expr: Expr,
    pub decorators: Vec<Decorator>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Block {
    pub kind: String,
    pub labels: Vec<Expr>,
    pub items: Vec<Item>,
    pub decorators: Vec<Decorator>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NamespaceDecl {
    pub path: Vec<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct UseDecl {
    pub path: Vec<String>,
    pub form: UseForm,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum UseForm {
    Bare(Option<String>),
    List(Vec<UseItem>),
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct UseItem {
    pub name: String,
    pub alias: Option<String>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TypeDecl {
    pub name: Vec<String>,
    pub fields: Vec<TypeField>,
    pub decorators: Vec<Decorator>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TypeField {
    pub name: String,
    pub ty: crate::value::TypeRef,
    pub ty_span: Span,
    pub optional: bool,
    pub decorators: Vec<Decorator>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct UnionDecl {
    pub name: Vec<String>,
    pub variants: Vec<UnionVariant>,
    pub decorators: Vec<Decorator>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct UnionVariant {
    pub name: String,
    pub body: VariantBody,
    pub decorators: Vec<Decorator>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum VariantBody {
    Record(Vec<TypeField>),
    TypeRef {
        ty: crate::value::TypeRef,
        ty_span: Span,
    },
    Unit,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SymbolSetDecl {
    pub name: Vec<String>,
    pub symbols: Vec<SymbolEntry>,
    pub decorators: Vec<Decorator>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct SymbolEntry {
    pub name: String,
    pub decorators: Vec<Decorator>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ImportDecl {
    pub path: String,
    pub path_span: Span,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Item {
    Field(Field),
    Block(Block),
    TypeDecl(TypeDecl),
    UnionDecl(UnionDecl),
    NamespaceDecl(NamespaceDecl),
    UseDecl(UseDecl),
    SymbolSetDecl(SymbolSetDecl),
    Import(ImportDecl),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Source {
    pub items: Vec<Item>,
}
