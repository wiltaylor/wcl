//! Type references, as written in a declaration.
//!
//! Parsed by [`parser::types`](crate::parser), printed by
//! [`format::types`](crate::format). These are the *syntactic* types:
//! what a `.wcl` file writes, before the document resolves it. The
//! resolved counterpart is [`ResolvedType`](crate::ResolvedType), and
//! the values a resolved type describes live in [`crate::Value`].

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// A primitive type the language provides, as opposed to one a document
/// declares.
pub enum BuiltinType {
    /// `bool`.
    Bool,

    /// `i8`.
    I8,
    /// `i16`.
    I16,
    /// `i32`.
    I32,
    /// `i64`.
    I64,
    /// `i128`.
    I128,
    /// `isize`.
    Isize,

    /// `u8`.
    U8,
    /// `u16`.
    U16,
    /// `u32`.
    U32,
    /// `u64`.
    U64,
    /// `u128`.
    U128,
    /// `usize`.
    Usize,

    /// `f32`.
    F32,
    /// `f64`.
    F64,

    /// `utf8`.
    Utf8,
    /// `ascii`.
    Ascii,
    /// `utf16`.
    Utf16,
    /// `utf32`.
    Utf32,

    /// `symbol`.
    Symbol,
    /// `identifier` — a name used as data.
    Identifier,
}

impl BuiltinType {
    /// Parse a builtin type name, or `None` when it names none.
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

    /// The type name as WCL spells it. Inverse of
    /// [`BuiltinType::from_name`].
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
/// A type as *written* in a declaration, before it is resolved against
/// the document. The syntactic counterpart of `ResolvedType`.
pub enum TypeRef {
    /// A primitive type.
    Builtin(BuiltinType),
    /// A reference to a declared type by path, optionally carrying type
    /// arguments (`content<SvgBlock>`).
    ///
    /// The arguments are **syntax only**: the parser accepts them, the
    /// printer round-trips them, and consumers may read them as
    /// metadata. Nothing checks their arity and nothing substitutes
    /// them — a named type resolves by `path` alone, exactly as it did
    /// before arguments existed. Full generics are a separate effort.
    Named {
        /// Dotted path naming the declaration.
        path: Vec<String>,
        /// Type arguments, preserved as syntax only — see above.
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        args: Vec<TypeRef>,
    },
    /// `&T` — a reference to a block of the wrapped type, by id.
    Reference(Box<TypeRef>),
    /// `list<T>`.
    List(Box<TypeRef>),
    /// `tensor<T, [dims]>`.
    Tensor {
        /// The element type.
        element: Box<TypeRef>,
        /// The declared extents, outermost first.
        dims: Vec<TensorDim>,
    },
    /// `fn(params) -> return_ty`.
    Function {
        /// Parameter types, in order.
        params: Vec<TypeRef>,
        /// The return type.
        return_ty: Box<TypeRef>,
    },
}

impl TypeRef {
    /// A named type reference with no type arguments — the shape every
    /// construction site outside the parser wants.
    pub fn named(path: Vec<String>) -> Self {
        TypeRef::Named {
            path,
            args: Vec::new(),
        }
    }

    /// The type arguments written on a named reference, empty for every
    /// other shape. Metadata for consumers: nothing here participates in
    /// resolution or checking.
    pub fn type_args(&self) -> &[TypeRef] {
        match self {
            TypeRef::Named { args, .. } => args,
            _ => &[],
        }
    }

    /// Structural equality that ignores type arguments at every level.
    ///
    /// Arguments are metadata: nothing resolves or substitutes them, so
    /// two references differing only in their arguments name the *same*
    /// type. Any check deciding whether two declarations mean the same
    /// type must ask this rather than `==` — plain equality would give
    /// the arguments the meaning syntax-only generics must not carry,
    /// making `S<A>` and `S<B>` distinct types by the back door.
    pub fn same_ignoring_type_args(&self, other: &TypeRef) -> bool {
        match (self, other) {
            (TypeRef::Named { path: a, .. }, TypeRef::Named { path: b, .. }) => a == b,
            (TypeRef::Reference(a), TypeRef::Reference(b))
            | (TypeRef::List(a), TypeRef::List(b)) => a.same_ignoring_type_args(b),
            (
                TypeRef::Tensor {
                    element: a,
                    dims: a_dims,
                },
                TypeRef::Tensor {
                    element: b,
                    dims: b_dims,
                },
            ) => a_dims == b_dims && a.same_ignoring_type_args(b),
            (
                TypeRef::Function {
                    params: a,
                    return_ty: a_ret,
                },
                TypeRef::Function {
                    params: b,
                    return_ty: b_ret,
                },
            ) => {
                a.len() == b.len()
                    && a.iter()
                        .zip(b.iter())
                        .all(|(x, y)| x.same_ignoring_type_args(y))
                    && a_ret.same_ignoring_type_args(b_ret)
            }
            _ => self == other,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
/// One dimension of a declared tensor type.
pub enum TensorDim {
    /// A literal extent.
    Fixed(u64),
    /// A named extent, checked for consistency rather than for a
    /// specific size.
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
            TypeRef::Named { path, args } => {
                write!(f, "{}", path.join("."))?;
                if !args.is_empty() {
                    let parts: Vec<String> = args.iter().map(|t| t.to_string()).collect();
                    write!(f, "<{}>", parts.join(", "))?;
                }
                Ok(())
            }
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
