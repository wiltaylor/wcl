#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
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

    /// Function values carry an opaque AST body that doesn't round-trip
    /// through serde — `Document::to_json` skips top-level fields that
    /// resolve to functions, and direct serialization errors at this
    /// variant.
    #[serde(skip)]
    Function(FnValue),
    List(Vec<Value>),
    Tensor {
        shape: Vec<u64>,
        data: Vec<Value>,
    },
    Variant {
        /// FQN of the declaring union, e.g. `["company", "Shape"]`.
        union: Vec<String>,
        variant: String,
        payload: VariantPayload,
    },
    /// Anonymous-shape record value produced by built-in deconstruction
    /// (e.g. connection statements projected through `@connections`).
    /// `ty` carries the FQN of the declaration that produced the
    /// record so consumers can dispatch by type; `fields` holds the
    /// named slots in deterministic order.
    Record {
        ty: Vec<String>,
        fields: std::collections::BTreeMap<String, Value>,
    },
    /// First-class handle into the document tree that survived
    /// evaluation without auto-dereffing to a leaf value. Produced
    /// whenever an identifier or member chain resolves to a non-leaf
    /// `DataRef` (type, union, variant, type field, block, etc.).
    /// `kind` is the underlying `DataKind` tag for diagnostics;
    /// `segments` is the dotted FQN that re-resolves the same target
    /// from document root via `Caller::resolve`.
    DataPath {
        kind: String,
        segments: Vec<String>,
    },
}

/// Runtime payload of a [`Value::Variant`], matching the shape of the
/// variant body declared on its [`UnionDecl`](crate::ast::UnionDecl).
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum VariantPayload {
    /// `Empty none` — variant with no payload.
    Unit,
    /// `Polygon P` — single positional value typed as the variant's
    /// declared TypeRef.
    Positional(Box<Value>),
    /// `Circle { center: P, radius: f64 }` — named fields. Stored in a
    /// `BTreeMap` so `PartialEq` and `Debug` order deterministically.
    Record(std::collections::BTreeMap<String, Value>),
}

/// A function value: a parameter list, a return type, and an opaque body.
///
/// The body is preserved as an AST expression internally so equality of
/// function values takes the body into account, but it is not part of the
/// public surface (no evaluator yet — there is nothing the consumer can do
/// with it).
#[derive(Debug, Clone, PartialEq)]
pub struct FnValue {
    params: Vec<FnParam>,
    return_ty: TypeRef,
    pub(crate) body: Box<crate::ast::Expr>,
    /// Snapshot of the evaluator's local bindings at the moment this
    /// function literal was constructed. On invocation, captured pairs
    /// are pushed onto the call's locals stack *before* the parameter
    /// binds — so captures participate in identifier lookup but are
    /// shadowed by parameters that reuse the same name.
    ///
    /// Document-scope identifiers (fields, blocks, types, unions)
    /// are *not* captured: the document is immutable after open, so
    /// they resolve correctly via scope walks at call time.
    pub(crate) captured: Vec<(String, Value)>,
}

impl FnValue {
    pub(crate) fn new(
        params: Vec<FnParam>,
        return_ty: TypeRef,
        body: Box<crate::ast::Expr>,
    ) -> Self {
        Self {
            params,
            return_ty,
            body,
            captured: Vec::new(),
        }
    }

    pub(crate) fn with_captures(mut self, captured: Vec<(String, Value)>) -> Self {
        self.captured = captured;
        self
    }

    pub fn params(&self) -> &[FnParam] {
        &self.params
    }

    pub fn return_ty(&self) -> &TypeRef {
        &self.return_ty
    }
}

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FnParam {
    name: String,
    ty: TypeRef,
}

impl FnParam {
    pub(crate) fn new(name: String, ty: TypeRef) -> Self {
        Self { name, ty }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn ty(&self) -> &TypeRef {
        &self.ty
    }
}

impl Value {
    /// Convert a numeric `Value` to `u64` for use as a decorator count or
    /// slot index. Returns `None` for non-numeric values, negative signed
    /// values, or magnitudes that don't fit in `u64`.
    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Value::I8(n) if *n >= 0 => Some(*n as u64),
            Value::I16(n) if *n >= 0 => Some(*n as u64),
            Value::I32(n) if *n >= 0 => Some(*n as u64),
            Value::I64(n) if *n >= 0 => Some(*n as u64),
            Value::I128(n) if *n >= 0 => u64::try_from(*n).ok(),
            Value::Isize(n) if *n >= 0 => Some(*n as u64),
            Value::U8(n) => Some(*n as u64),
            Value::U16(n) => Some(*n as u64),
            Value::U32(n) => Some(*n as u64),
            Value::U64(n) => Some(*n),
            Value::U128(n) => u64::try_from(*n).ok(),
            Value::Usize(n) => u64::try_from(*n).ok(),
            _ => None,
        }
    }

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
            Value::Function(_) => "fn",
            Value::List(_) => "list",
            Value::Tensor { .. } => "tensor",
            Value::Variant { .. } => "variant",
            Value::Record { .. } => "record",
            Value::DataPath { .. } => "data_path",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum TypeRef {
    Builtin(BuiltinType),
    Named(Vec<String>),
    Reference(Box<TypeRef>),
    List(Box<TypeRef>),
    Tensor {
        element: Box<TypeRef>,
        dims: Vec<TensorDim>,
    },
    Function {
        params: Vec<TypeRef>,
        return_ty: Box<TypeRef>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TensorDim {
    Fixed(u64),
    Symbolic(String),
}

impl std::fmt::Display for TensorDim {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TensorDim::Fixed(n) => write!(f, "{n}"),
            TensorDim::Symbolic(s) => write!(f, "{s}"),
        }
    }
}

impl std::fmt::Display for TypeRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeRef::Builtin(b) => write!(f, "{}", b.name()),
            TypeRef::Named(path) => write!(f, "{}", path.join(".")),
            TypeRef::Reference(inner) => write!(f, "&{inner}"),
            TypeRef::List(inner) => write!(f, "list<{inner}>"),
            TypeRef::Tensor { element, dims } => {
                let dims_str = dims
                    .iter()
                    .map(|d| d.to_string())
                    .collect::<Vec<_>>()
                    .join(", ");
                write!(f, "tensor<{element}, [{dims_str}]>")
            }
            TypeRef::Function { params, return_ty } => {
                let parts: Vec<String> = params.iter().map(|t| t.to_string()).collect();
                write!(f, "fn({}) -> {return_ty}", parts.join(", "))
            }
        }
    }
}

/// Round-trippable rendering of a [`Value`]: integers / floats carry their
/// Rust-style suffix, strings are quoted, lists / tensors / variants /
/// records mirror their source syntax. This is the "primary" display
/// form — emitted by the CLI's `parse` and `eval` commands.
///
/// For the compact, interpolation-friendly form used by the `format(...)`
/// builtin (unsuffixed numbers, unquoted strings), see
/// [`crate::collections::format_value`].
impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Bool(b) => write!(f, "{b}"),

            // Default-typed integers and floats render without a suffix;
            // every other numeric form keeps its suffix so the dump
            // round-trips through the parser.
            Value::I64(n) => write!(f, "{n}"),
            Value::F64(n) => f.write_str(&format_float_lit(*n)),

            Value::I8(n) => write!(f, "{n}i8"),
            Value::I16(n) => write!(f, "{n}i16"),
            Value::I32(n) => write!(f, "{n}i32"),
            Value::I128(n) => write!(f, "{n}i128"),
            Value::Isize(n) => write!(f, "{n}isize"),

            Value::U8(n) => write!(f, "{n}u8"),
            Value::U16(n) => write!(f, "{n}u16"),
            Value::U32(n) => write!(f, "{n}u32"),
            Value::U64(n) => write!(f, "{n}u64"),
            Value::U128(n) => write!(f, "{n}u128"),
            Value::Usize(n) => write!(f, "{n}usize"),

            Value::F32(n) => write!(f, "{}f32", format_float_lit(*n as f64)),

            Value::Utf8(s) => write!(f, "\"{}\"", EscapeString(s)),
            Value::Ascii(s) => write!(f, "ascii\"{}\"", EscapeString(s)),
            Value::Utf16(units) => {
                let s = String::from_utf16_lossy(units);
                write!(f, "utf16\"{}\"", EscapeString(&s))
            }
            Value::Utf32(chars) => {
                let s: String = chars.iter().collect();
                write!(f, "utf32\"{}\"", EscapeString(&s))
            }

            Value::Identifier(s) => f.write_str(s),
            Value::Symbol(s) => write!(f, ":{s}"),
            Value::None => f.write_str("none"),
            Value::Function(fv) => {
                f.write_str("fn(")?;
                for (i, p) in fv.params().iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{}: {}", p.name(), p.ty())?;
                }
                write!(f, ") -> {} {{ ... }}", fv.return_ty())
            }
            Value::List(items) => {
                f.write_str("[")?;
                for (i, v) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{v}")?;
                }
                f.write_str("]")
            }
            Value::Tensor { shape, data } => {
                let dims: Vec<String> = shape.iter().map(u64::to_string).collect();
                f.write_str("tensor[")?;
                f.write_str(&dims.join("x"))?;
                f.write_str("](")?;
                for (i, v) in data.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{v}")?;
                }
                f.write_str(")")
            }
            Value::Variant {
                union,
                variant,
                payload,
            } => {
                write!(f, "{}::{variant}", union.join("."))?;
                match payload {
                    VariantPayload::Unit => Ok(()),
                    VariantPayload::Positional(v) => write!(f, "({v})"),
                    VariantPayload::Record(map) => {
                        f.write_str(" { ")?;
                        for (i, (k, v)) in map.iter().enumerate() {
                            if i > 0 {
                                f.write_str(", ")?;
                            }
                            write!(f, "{k}: {v}")?;
                        }
                        f.write_str(" }")
                    }
                }
            }
            Value::Record { ty, fields } => {
                write!(f, "{} {{ ", ty.join("."))?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                f.write_str(" }")
            }
            Value::DataPath { kind, segments } => {
                write!(f, "&{}<{kind}>", segments.join("."))
            }
        }
    }
}

fn format_float_lit(n: f64) -> String {
    let s = format!("{n}");
    if s.contains('.') || s.contains('e') || s.contains('E') || !n.is_finite() {
        s
    } else {
        format!("{s}.0")
    }
}

struct EscapeString<'a>(&'a str);

impl std::fmt::Display for EscapeString<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for c in self.0.chars() {
            match c {
                '"' => f.write_str("\\\"")?,
                '\\' => f.write_str("\\\\")?,
                '\n' => f.write_str("\\n")?,
                '\t' => f.write_str("\\t")?,
                '\r' => f.write_str("\\r")?,
                other => f.write_str(other.encode_utf8(&mut [0u8; 4]))?,
            }
        }
        Ok(())
    }
}
