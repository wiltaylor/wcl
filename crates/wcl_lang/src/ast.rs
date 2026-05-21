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

    Reference(String),
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Field {
    pub name: String,
    pub expr: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Block {
    pub kind: String,
    pub labels: Vec<String>,
    pub items: Vec<Item>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TypeDecl {
    pub name: String,
    pub fields: Vec<TypeField>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct TypeField {
    pub name: String,
    pub ty: crate::value::TypeRef,
    pub ty_span: Span,
    pub optional: bool,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Item {
    Field(Field),
    Block(Block),
    TypeDecl(TypeDecl),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct Source {
    pub items: Vec<Item>,
}
