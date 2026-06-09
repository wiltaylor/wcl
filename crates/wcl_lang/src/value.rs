/// Runtime values produced by evaluating WCL expressions.
///
/// **Serialization is one-way.** `Value` implements [`serde::Serialize`]
/// with a custom impl that emits idiomatic JSON (scalars as primitives,
/// lists as arrays, records as objects). It deliberately does **not**
/// implement [`serde::Deserialize`] — round-tripping arbitrary JSON
/// back into a `Value` loses the original numeric variant (i32 vs i64
/// vs u32 ...), which downstream evaluator paths assume is preserved.
/// Hosts that need a round-trippable representation should serialize
/// the [`TypeRef`] / declaration shape instead.
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

    /// Function values carry an opaque AST body. They serialize as
    /// JSON `null` so containing structures survive — the function
    /// itself doesn't round-trip.
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

impl serde::Serialize for Value {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::{SerializeMap, SerializeSeq};
        match self {
            Value::Bool(b) => s.serialize_bool(*b),
            Value::I8(n) => s.serialize_i8(*n),
            Value::I16(n) => s.serialize_i16(*n),
            Value::I32(n) => s.serialize_i32(*n),
            Value::I64(n) => s.serialize_i64(*n),
            Value::I128(n) => s.serialize_i128(*n),
            Value::Isize(n) => s.serialize_i64(*n as i64),
            Value::U8(n) => s.serialize_u8(*n),
            Value::U16(n) => s.serialize_u16(*n),
            Value::U32(n) => s.serialize_u32(*n),
            Value::U64(n) => s.serialize_u64(*n),
            Value::U128(n) => s.serialize_u128(*n),
            Value::Usize(n) => s.serialize_u64(*n as u64),
            Value::F32(n) => s.serialize_f32(*n),
            Value::F64(n) => s.serialize_f64(*n),
            Value::Utf8(t) | Value::Ascii(t) => s.serialize_str(t),
            Value::Utf16(units) => s.serialize_str(&String::from_utf16_lossy(units)),
            Value::Utf32(chars) => s.serialize_str(&chars.iter().collect::<String>()),
            Value::Identifier(n) | Value::Symbol(n) => s.serialize_str(n),
            Value::None | Value::Function(_) => s.serialize_unit(),
            Value::List(items) => {
                let mut seq = s.serialize_seq(Some(items.len()))?;
                for v in items {
                    seq.serialize_element(v)?;
                }
                seq.end()
            }
            Value::Tensor { shape, data } => {
                let mut map = s.serialize_map(Some(2))?;
                map.serialize_entry("shape", shape)?;
                map.serialize_entry("data", data)?;
                map.end()
            }
            Value::Variant {
                variant, payload, ..
            } => match payload {
                VariantPayload::Unit => s.serialize_str(variant),
                VariantPayload::Positional(v) => {
                    let mut map = s.serialize_map(Some(1))?;
                    map.serialize_entry(variant, v.as_ref())?;
                    map.end()
                }
                VariantPayload::Record(fields) => {
                    let mut map = s.serialize_map(Some(1))?;
                    map.serialize_entry(variant, fields)?;
                    map.end()
                }
            },
            Value::Record { fields, .. } => {
                let mut map = s.serialize_map(Some(fields.len()))?;
                for (k, v) in fields {
                    map.serialize_entry(k, v)?;
                }
                map.end()
            }
            Value::DataPath { kind, segments } => {
                let mut map = s.serialize_map(Some(2))?;
                map.serialize_entry("kind", kind)?;
                map.serialize_entry("path", segments)?;
                map.end()
            }
        }
    }
}

/// Runtime payload of a [`Value::Variant`], matching the shape of the
/// variant body declared on its [`UnionDecl`](crate::ast::UnionDecl).
/// Inherits `Value`'s one-way serialization story — emits idiomatic
/// JSON but doesn't deserialize.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
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

    /// Pretty-print this function value back to its `fn(params) -> ret body`
    /// source form. Reconstructs an [`ast::FunctionLit`] from the stored
    /// params / return type / body (spans are irrelevant to the printer) and
    /// renders it via the formatter. Used by the `ast_string` builtin.
    pub fn to_source(&self) -> String {
        use crate::ast;
        let zero = ast::Span::new(0, 0);
        let params = self
            .params
            .iter()
            .map(|p| ast::Parameter {
                name: p.name().to_string(),
                ty: p.ty().clone(),
                ty_span: zero,
                span: zero,
                leading_trivia: Vec::new(),
                trailing_comment: None,
            })
            .collect();
        let lit = ast::FunctionLit {
            params,
            return_ty: self.return_ty.clone(),
            return_ty_span: zero,
            body: self.body.clone(),
            span: zero,
            trailing_trivia: Vec::new(),
        };
        crate::format::to_source_expr(&ast::Expr::Function(lit))
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
        crate::numeric::numeric_as_u64!(self, Value)
    }

    /// Lossily widen any numeric `Value` to `f64`. Returns `None` for
    /// non-numeric values. Used by the implicit-coercion path in
    /// arithmetic and comparison.
    pub fn as_f64(&self) -> Option<f64> {
        match *self {
            Value::I8(n) => Some(n as f64),
            Value::I16(n) => Some(n as f64),
            Value::I32(n) => Some(n as f64),
            Value::I64(n) => Some(n as f64),
            Value::I128(n) => Some(n as f64),
            Value::Isize(n) => Some(n as f64),
            Value::U8(n) => Some(n as f64),
            Value::U16(n) => Some(n as f64),
            Value::U32(n) => Some(n as f64),
            Value::U64(n) => Some(n as f64),
            Value::U128(n) => Some(n as f64),
            Value::Usize(n) => Some(n as f64),
            Value::F32(n) => Some(n as f64),
            Value::F64(n) => Some(n),
            _ => None,
        }
    }

    /// Widen any *integer* `Value` to `i128`. Returns `None` for
    /// floats and non-numeric values. `u128` is excluded — its top
    /// half doesn't fit in `i128`.
    pub fn as_i128(&self) -> Option<i128> {
        match *self {
            Value::I8(n) => Some(n as i128),
            Value::I16(n) => Some(n as i128),
            Value::I32(n) => Some(n as i128),
            Value::I64(n) => Some(n as i128),
            Value::I128(n) => Some(n),
            Value::Isize(n) => Some(n as i128),
            Value::U8(n) => Some(n as i128),
            Value::U16(n) => Some(n as i128),
            Value::U32(n) => Some(n as i128),
            Value::U64(n) => Some(n as i128),
            Value::U128(n) => i128::try_from(n).ok(),
            Value::Usize(n) => Some(n as i128),
            _ => None,
        }
    }

    /// `true` for any numeric variant (signed / unsigned / float).
    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            Value::I8(_)
                | Value::I16(_)
                | Value::I32(_)
                | Value::I64(_)
                | Value::I128(_)
                | Value::Isize(_)
                | Value::U8(_)
                | Value::U16(_)
                | Value::U32(_)
                | Value::U64(_)
                | Value::U128(_)
                | Value::Usize(_)
                | Value::F32(_)
                | Value::F64(_)
        )
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

    /// `true` for the integer and floating-point types — the set the
    /// evaluator promotes between in arithmetic and field coercion.
    pub fn is_numeric(self) -> bool {
        matches!(
            self,
            Self::I8
                | Self::I16
                | Self::I32
                | Self::I64
                | Self::I128
                | Self::Isize
                | Self::U8
                | Self::U16
                | Self::U32
                | Self::U64
                | Self::U128
                | Self::Usize
                | Self::F32
                | Self::F64
        )
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

/// Wraps a string so `Display` writes it with WCL inline-string escapes
/// (`\`, `"`, `\n`, `\t`, `\r`). Shared by this module's `Value` `Display`
/// and the formatter's string-literal printing.
pub(crate) struct EscapeString<'a>(pub(crate) &'a str);

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
