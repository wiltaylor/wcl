#[derive(Debug, Clone, PartialEq)]
pub enum Value {
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
}

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Bool(_) => "bool",
            Value::I8(_) => "i8",
            Value::I16(_) => "i16",
            Value::I32(_) => "i32",
            Value::I64(_) => "i64",
            Value::I128(_) => "i128",
            Value::Isize(_) => "isize",
            Value::U8(_) => "u8",
            Value::U16(_) => "u16",
            Value::U32(_) => "u32",
            Value::U64(_) => "u64",
            Value::U128(_) => "u128",
            Value::Usize(_) => "usize",
            Value::F32(_) => "f32",
            Value::F64(_) => "f64",
            Value::Utf8(_) => "utf8",
            Value::Ascii(_) => "ascii",
            Value::Utf16(_) => "utf16",
            Value::Utf32(_) => "utf32",
            Value::Identifier(_) => "identifier",
            Value::Symbol(_) => "symbol",
            Value::None => "none",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuiltinType {
    Bool,

    I8,
    I16,
    I32,
    I64,
    I128,
    Isize,

    U8,
    U16,
    U32,
    U64,
    U128,
    Usize,

    F32,
    F64,

    Utf8,
    Ascii,
    Utf16,
    Utf32,

    Symbol,
    Identifier,
}

impl BuiltinType {
    pub fn from_name(s: &str) -> Option<Self> {
        Some(match s {
            "bool" => Self::Bool,
            "i8" => Self::I8,
            "i16" => Self::I16,
            "i32" => Self::I32,
            "i64" => Self::I64,
            "i128" => Self::I128,
            "isize" => Self::Isize,
            "u8" => Self::U8,
            "u16" => Self::U16,
            "u32" => Self::U32,
            "u64" => Self::U64,
            "u128" => Self::U128,
            "usize" => Self::Usize,
            "f32" => Self::F32,
            "f64" => Self::F64,
            "utf8" => Self::Utf8,
            "ascii" => Self::Ascii,
            "utf16" => Self::Utf16,
            "utf32" => Self::Utf32,
            "symbol" => Self::Symbol,
            "identifier" => Self::Identifier,
            _ => return None,
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Bool => "bool",
            Self::I8 => "i8",
            Self::I16 => "i16",
            Self::I32 => "i32",
            Self::I64 => "i64",
            Self::I128 => "i128",
            Self::Isize => "isize",
            Self::U8 => "u8",
            Self::U16 => "u16",
            Self::U32 => "u32",
            Self::U64 => "u64",
            Self::U128 => "u128",
            Self::Usize => "usize",
            Self::F32 => "f32",
            Self::F64 => "f64",
            Self::Utf8 => "utf8",
            Self::Ascii => "ascii",
            Self::Utf16 => "utf16",
            Self::Utf32 => "utf32",
            Self::Symbol => "symbol",
            Self::Identifier => "identifier",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum TypeRef {
    Builtin(BuiltinType),
    Named(Vec<String>),
    Reference(Box<TypeRef>),
    List(Box<TypeRef>),
    Tensor {
        element: Box<TypeRef>,
        dims: Vec<TensorDim>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TensorDim {
    Fixed(u64),
    Symbolic(String),
}
