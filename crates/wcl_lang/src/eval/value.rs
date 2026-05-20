use crate::lang::Span;
use indexmap::IndexMap;
use std::fmt;
use std::ops::Deref;
use std::sync::{Arc, Mutex};

/// Runtime value in WCL
#[derive(Debug, Clone)]
pub enum Value {
    String(String),
    Int(i64),
    /// Large integer for values exceeding i64 range (u64 large values, i128, u128)
    BigInt(i128),
    Float(f64),
    Bool(bool),
    Null,
    /// Identifier literal value (the id type)
    Identifier(String),
    /// Ordered list
    List(Vec<Value>),
    /// Ordered map (preserves insertion order)
    Map(IndexMap<String, Value>),
    /// Named runtime object with map-like fields.
    Object(ObjectValue),
    /// Raw bytes.
    Bytes(Vec<u8>),
    /// MessagePack extension value.
    MsgPackExt {
        type_id: i8,
        data: Vec<u8>,
    },
    /// MessagePack timestamp extension value.
    MsgPackTimestamp {
        seconds: i64,
        nanoseconds: u32,
    },
    /// Set (ordered, unique values)
    Set(Vec<Value>),
    /// Reference to a block (block type, inline id, attributes map, child blocks, decorators)
    BlockRef(BlockRef),
    /// Symbol value (e.g. `:GET`, `:relational`)
    Symbol(String),
    /// Lambda/function value
    Function(FunctionValue),
    /// One-shot lazy value.
    Lazy(LazyValue),
    /// Cursor-style lazy stream.
    Stream(StreamValue),
    /// Host-backed stream.
    NativeStream(NativeStreamValue),
    /// Private state handle injected while evaluating lazy/stream bodies.
    StateHandle(StateHandleValue),
    /// ISO 8601 date value (e.g. `d"2024-03-15"`)
    Date(String),
    /// RFC 3339 offset date-time value (e.g. `odt"1979-05-27T07:32:00Z"`)
    OffsetDateTime(String),
    /// TOML local date-time value (e.g. `ldt"1979-05-27T07:32:00"`)
    LocalDateTime(String),
    /// TOML local time value (e.g. `lt"07:32:00"`)
    LocalTime(String),
    /// ISO 8601 duration value (e.g. `dur"P1Y2M3D"`)
    Duration(String),
    /// Regex pattern value (e.g. /^[a-z]+$/)
    Pattern(String),
}

#[derive(Debug, Clone)]
pub struct BlockRef(Arc<BlockRefData>);

impl BlockRef {
    pub fn new(data: BlockRefData) -> Self {
        Self(Arc::new(data))
    }

    pub fn into_data(self) -> BlockRefData {
        Arc::unwrap_or_clone(self.0)
    }

    pub fn to_data(&self) -> BlockRefData {
        (*self.0).clone()
    }
}

impl Deref for BlockRef {
    type Target = BlockRefData;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Debug, Clone)]
pub struct BlockRefData {
    pub kind: String,
    pub id: Option<String>,
    /// Fully qualified dotted ID path (e.g. `"alpha.http.health"`).
    /// Built from ancestor inline IDs joined by `.`.
    pub qualified_id: Option<String>,
    pub attributes: IndexMap<String, Value>,
    pub attribute_decorators: IndexMap<String, Vec<DecoratorValue>>,
    pub children: Vec<BlockRef>,
    pub decorators: Vec<DecoratorValue>,
    pub span: Span,
}

impl BlockRef {
    /// Check if this block has a decorator with the given name
    pub fn has_decorator(&self, name: &str) -> bool {
        self.decorators.iter().any(|d| d.name == name)
    }

    /// Get a decorator by name
    pub fn decorator(&self, name: &str) -> Option<&DecoratorValue> {
        self.decorators.iter().find(|d| d.name == name)
    }

    /// Get an attribute value by name
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.attributes.get(key)
    }
}

#[derive(Debug, Clone)]
pub struct DecoratorValue {
    pub name: String,
    pub args: IndexMap<String, Value>,
}

#[derive(Debug, Clone)]
pub struct ObjectValue {
    pub type_name: String,
    pub fields: IndexMap<String, Value>,
}

#[derive(Debug, Clone)]
pub struct FunctionValue {
    pub params: Vec<String>,
    pub body: FunctionBody,
    pub closure_scope: Option<ScopeId>,
    /// Decorators applied to the let/export-let binding that produced this function.
    pub decorators: Vec<DecoratorValue>,
    /// Attributes from decorators on the let binding (e.g. @stateful, @accumulator)
    pub lambda_attrs: LambdaAttrs,
    /// Type annotations for parameters (if provided)
    pub param_types: Vec<Option<String>>,
    /// Return type annotation (if provided)
    pub return_type: Option<String>,
}

pub(crate) type SharedLazyState = Arc<Mutex<LazyState>>;
pub(crate) type SharedStreamState = Arc<Mutex<StreamState>>;
pub(crate) type SharedNativeStreamState = Arc<Mutex<NativeStreamState>>;
pub(crate) type SharedStateStore = Arc<Mutex<IndexMap<String, Value>>>;

#[derive(Debug, Clone)]
pub struct LazyValue {
    pub(crate) inner: SharedLazyState,
}

#[derive(Debug, Clone)]
pub struct StreamValue {
    pub(crate) inner: SharedStreamState,
}

#[derive(Clone)]
pub struct NativeStreamValue {
    pub(crate) inner: SharedNativeStreamState,
}

impl fmt::Debug for NativeStreamValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeStreamValue").finish_non_exhaustive()
    }
}

#[derive(Debug, Clone)]
pub struct StateHandleValue {
    pub(crate) store: SharedStateStore,
}

#[derive(Debug, Clone)]
pub(crate) struct LazyState {
    pub lets: Vec<crate::lang::ast::LetBinding>,
    pub final_expr: Box<crate::lang::ast::Expr>,
    pub closure_scope: ScopeId,
    pub store: SharedStateStore,
    pub status: LazyStatus,
}

#[derive(Debug, Clone)]
pub(crate) enum LazyStatus {
    Pending,
    Evaluating,
    Ready(Value),
}

#[derive(Debug, Clone)]
pub(crate) struct StreamState {
    pub lets: Vec<crate::lang::ast::LetBinding>,
    pub final_expr: Box<crate::lang::ast::Expr>,
    pub closure_scope: ScopeId,
    pub store: SharedStateStore,
    pub exhausted: bool,
}

pub(crate) struct NativeStreamState {
    pub next: Box<dyn FnMut() -> Result<Option<Value>, String> + Send>,
    pub exhausted: bool,
}

impl fmt::Debug for NativeStreamState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeStreamState")
            .field("exhausted", &self.exhausted)
            .finish()
    }
}

/// Attributes derived from decorators on a let/export-let binding containing a lambda.
#[derive(Debug, Clone, Default)]
pub struct LambdaAttrs {
    pub stateful: Option<StatefulAttr>,
    pub accumulator: bool,
    pub instrumented: bool,
}

/// Configuration for the @stateful decorator.
#[derive(Debug, Clone)]
pub struct StatefulAttr {
    /// Optional named scope: `@stateful(scope = "per_sensor")`
    pub scope: Option<String>,
}

#[derive(Debug, Clone)]
pub enum FunctionBody {
    /// Built-in function (identified by name)
    Builtin(String),
    /// User-defined lambda (index into AST expressions, we'll store the Expr here)
    UserDefined(Box<crate::lang::ast::Expr>),
    /// Block expression body (lets + final expr)
    BlockExpr(
        Vec<(String, Box<crate::lang::ast::Expr>)>,
        Box<crate::lang::ast::Expr>,
    ),
}

/// Scope identifier
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScopeId(pub u32);

impl Value {
    pub fn type_name(&self) -> &'static str {
        match self {
            Value::String(_) => "string",
            Value::Int(_) => "int",
            Value::BigInt(_) => "bigint",
            Value::Float(_) => "float",
            Value::Bool(_) => "bool",
            Value::Null => "null",
            Value::Identifier(_) => "identifier",
            Value::List(_) => "list",
            Value::Map(_) => "map",
            Value::Object(_) => "object",
            Value::Bytes(_) => "bytes",
            Value::MsgPackExt { .. } => "msgpack_ext",
            Value::MsgPackTimestamp { .. } => "msgpack_timestamp",
            Value::Set(_) => "set",
            Value::Symbol(_) => "symbol",
            Value::BlockRef(_) => "block_ref",
            Value::Function(_) => "function",
            Value::Lazy(_) => "lazy",
            Value::Stream(_) => "stream",
            Value::NativeStream(_) => "stream",
            Value::StateHandle(_) => "state",
            Value::Date(_) => "date",
            Value::OffsetDateTime(_) => "offset_datetime",
            Value::LocalDateTime(_) => "local_datetime",
            Value::LocalTime(_) => "local_time",
            Value::Duration(_) => "duration",
            Value::Pattern(_) => "pattern",
        }
    }

    pub fn is_truthy(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_string(&self) -> Option<&str> {
        match self {
            Value::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_int(&self) -> Option<i64> {
        match self {
            Value::Int(i) => Some(*i),
            _ => None,
        }
    }

    pub fn as_float(&self) -> Option<f64> {
        match self {
            Value::Float(f) => Some(*f),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Value::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn as_list(&self) -> Option<&[Value]> {
        match self {
            Value::List(l) => Some(l),
            _ => None,
        }
    }

    pub fn as_map(&self) -> Option<&IndexMap<String, Value>> {
        match self {
            Value::Map(m) => Some(m),
            Value::Object(o) => Some(&o.fields),
            _ => None,
        }
    }

    pub fn as_identifier(&self) -> Option<&str> {
        match self {
            Value::Identifier(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_symbol(&self) -> Option<&str> {
        match self {
            Value::Symbol(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_block_ref(&self) -> Option<&BlockRef> {
        match self {
            Value::BlockRef(b) => Some(b),
            _ => None,
        }
    }

    pub fn is_null(&self) -> bool {
        matches!(self, Value::Null)
    }

    pub fn as_bigint(&self) -> Option<i128> {
        match self {
            Value::BigInt(i) => Some(*i),
            Value::Int(i) => Some(*i as i128),
            _ => None,
        }
    }

    pub fn as_date(&self) -> Option<&str> {
        match self {
            Value::Date(s) => Some(s),
            Value::OffsetDateTime(s) => Some(s),
            Value::LocalDateTime(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_local_time(&self) -> Option<&str> {
        match self {
            Value::LocalTime(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_duration(&self) -> Option<&str> {
        match self {
            Value::Duration(s) => Some(s),
            _ => None,
        }
    }

    pub fn to_interp_string(&self) -> Result<String, String> {
        match self {
            Value::String(s) => Ok(s.clone()),
            Value::Int(i) => Ok(i.to_string()),
            Value::BigInt(i) => Ok(i.to_string()),
            Value::Float(f) => Ok(f.to_string()),
            Value::Bool(b) => Ok(b.to_string()),
            Value::Null => Ok("null".to_string()),
            Value::Identifier(s) => Ok(s.clone()),
            Value::Symbol(s) => Ok(format!(":{}", s)),
            Value::Object(object) => Ok(object.type_name.clone()),
            Value::Date(s) => Ok(s.clone()),
            Value::OffsetDateTime(s) => Ok(s.clone()),
            Value::LocalDateTime(s) => Ok(s.clone()),
            Value::LocalTime(s) => Ok(s.clone()),
            Value::Duration(s) => Ok(s.clone()),
            Value::Pattern(s) => Ok(format!("/{}/", s)),
            _ => Err(format!(
                "cannot interpolate {} into string",
                self.type_name()
            )),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Value::String(a), Value::String(b)) => a == b,
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::BigInt(a), Value::BigInt(b)) => a == b,
            (Value::Int(a), Value::BigInt(b)) => (*a as i128) == *b,
            (Value::BigInt(a), Value::Int(b)) => *a == (*b as i128),
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Null, Value::Null) => true,
            (Value::Identifier(a), Value::Identifier(b)) => a == b,
            (Value::Symbol(a), Value::Symbol(b)) => a == b,
            (Value::List(a), Value::List(b)) => a == b,
            (Value::Map(a), Value::Map(b)) => a == b,
            (Value::Object(a), Value::Object(b)) => {
                a.type_name == b.type_name && a.fields == b.fields
            }
            (Value::Bytes(a), Value::Bytes(b)) => a == b,
            (
                Value::MsgPackExt {
                    type_id: a_type_id,
                    data: a_data,
                },
                Value::MsgPackExt {
                    type_id: b_type_id,
                    data: b_data,
                },
            ) => a_type_id == b_type_id && a_data == b_data,
            (
                Value::MsgPackTimestamp {
                    seconds: a_seconds,
                    nanoseconds: a_nanoseconds,
                },
                Value::MsgPackTimestamp {
                    seconds: b_seconds,
                    nanoseconds: b_nanoseconds,
                },
            ) => a_seconds == b_seconds && a_nanoseconds == b_nanoseconds,
            (Value::Set(a), Value::Set(b)) => a == b,
            (Value::Lazy(a), Value::Lazy(b)) => Arc::ptr_eq(&a.inner, &b.inner),
            (Value::Stream(a), Value::Stream(b)) => Arc::ptr_eq(&a.inner, &b.inner),
            (Value::NativeStream(a), Value::NativeStream(b)) => Arc::ptr_eq(&a.inner, &b.inner),
            (Value::StateHandle(a), Value::StateHandle(b)) => Arc::ptr_eq(&a.store, &b.store),
            (Value::Date(a), Value::Date(b)) => a == b,
            (Value::OffsetDateTime(a), Value::OffsetDateTime(b)) => a == b,
            (Value::LocalDateTime(a), Value::LocalDateTime(b)) => a == b,
            (Value::LocalTime(a), Value::LocalTime(b)) => a == b,
            (Value::Duration(a), Value::Duration(b)) => a == b,
            (Value::Pattern(a), Value::Pattern(b)) => a == b,
            _ => false,
        }
    }
}

pub(crate) fn values_equal_for_expr(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::BlockRef(left), Value::BlockRef(right)) => {
            left.kind == right.kind && left.id == right.id
        }
        (Value::BlockRef(block), Value::Identifier(id))
        | (Value::BlockRef(block), Value::String(id))
        | (Value::Identifier(id), Value::BlockRef(block))
        | (Value::String(id), Value::BlockRef(block)) => block.id.as_deref() == Some(id),
        _ => a == b,
    }
}

pub(crate) fn values_equal_for_id_expr(a: &Value, b: &Value) -> bool {
    match (id_text(a), id_text(b)) {
        (Some(left), Some(right)) => left == right,
        _ => values_equal_for_expr(a, b),
    }
}

fn id_text(value: &Value) -> Option<&str> {
    match value {
        Value::String(s) | Value::Identifier(s) => Some(s),
        Value::BlockRef(block) => block.id.as_deref(),
        _ => None,
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::String(s) => write!(f, "{}", s),
            Value::Int(i) => write!(f, "{}", i),
            Value::BigInt(i) => write!(f, "{}", i),
            Value::Float(v) => write!(f, "{}", v),
            Value::Bool(b) => write!(f, "{}", b),
            Value::Null => write!(f, "null"),
            Value::Identifier(s) => write!(f, "{}", s),
            Value::Symbol(s) => write!(f, ":{}", s),
            Value::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "]")
            }
            Value::Map(map) => {
                write!(f, "{{")?;
                for (i, (k, v)) in map.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{} = {}", k, v)?;
                }
                write!(f, "}}")
            }
            Value::Object(object) => {
                write!(f, "{} {{", object.type_name)?;
                for (i, (k, v)) in object.fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, " {} = {}", k, v)?;
                }
                write!(f, " }}")
            }
            Value::Bytes(bytes) => {
                write!(f, "{{__type = bytes, bytes = [")?;
                for (i, byte) in bytes.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", byte)?;
                }
                write!(f, "]}}")
            }
            Value::MsgPackExt { type_id, data } => {
                write!(f, "{{__type = msgpack_ext, type_id = {}, data = [", type_id)?;
                for (i, byte) in data.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", byte)?;
                }
                write!(f, "]}}")
            }
            Value::MsgPackTimestamp {
                seconds,
                nanoseconds,
            } => write!(
                f,
                "{{__type = msgpack_timestamp, seconds = {}, nanoseconds = {}}}",
                seconds, nanoseconds
            ),
            Value::Set(items) => {
                write!(f, "set(")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, ")")
            }
            Value::BlockRef(br) => {
                write!(f, "{}", br.kind)?;
                if let Some(id) = &br.id {
                    write!(f, " {}", id)?;
                }
                write!(f, " {{")?;
                for (i, (k, v)) in br.attributes.iter().enumerate() {
                    if i > 0 {
                        write!(f, ",")?;
                    }
                    write!(f, " {} = {}", k, v)?;
                }
                for child in &br.children {
                    write!(f, " {}", Value::BlockRef(child.clone()))?;
                }
                write!(f, " }}")
            }
            Value::Function(_) => write!(f, "<function>"),
            Value::Lazy(_) => write!(f, "<lazy>"),
            Value::Stream(_) => write!(f, "<stream>"),
            Value::NativeStream(_) => write!(f, "<stream>"),
            Value::StateHandle(_) => write!(f, "<state>"),
            Value::Date(s) => write!(f, "d\"{}\"", s),
            Value::OffsetDateTime(s) => write!(f, "odt\"{}\"", s),
            Value::LocalDateTime(s) => write!(f, "ldt\"{}\"", s),
            Value::LocalTime(s) => write!(f, "lt\"{}\"", s),
            Value::Duration(s) => write!(f, "dur\"{}\"", s),
            Value::Pattern(s) => write!(f, "/{}/", s),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Value::type_name() ────────────────────────────────────────────────────

    #[test]
    fn type_name_primitives() {
        assert_eq!(Value::String("hi".into()).type_name(), "string");
        assert_eq!(Value::Int(1).type_name(), "int");
        assert_eq!(Value::Float(1.0).type_name(), "float");
        assert_eq!(Value::Bool(true).type_name(), "bool");
        assert_eq!(Value::Null.type_name(), "null");
        assert_eq!(
            Value::Identifier("svc-auth".into()).type_name(),
            "identifier"
        );
        assert_eq!(Value::List(vec![]).type_name(), "list");
        assert_eq!(Value::Map(IndexMap::new()).type_name(), "map");
        assert_eq!(Value::Set(vec![]).type_name(), "set");
    }

    #[test]
    fn type_name_function() {
        let f = Value::Function(FunctionValue {
            params: vec![],
            body: FunctionBody::Builtin("len".into()),
            closure_scope: None,
            decorators: Vec::new(),
            lambda_attrs: LambdaAttrs::default(),
            param_types: vec![],
            return_type: None,
        });
        assert_eq!(f.type_name(), "function");
    }

    // ── Value equality ────────────────────────────────────────────────────────

    #[test]
    fn equality_same_variant() {
        assert_eq!(Value::Int(42), Value::Int(42));
        assert_eq!(Value::String("hello".into()), Value::String("hello".into()));
        assert_eq!(Value::Bool(false), Value::Bool(false));
        assert_eq!(Value::Null, Value::Null);
        assert_eq!(
            Value::Identifier("svc-api".into()),
            Value::Identifier("svc-api".into())
        );
    }

    #[test]
    fn equality_different_value() {
        assert_ne!(Value::Int(1), Value::Int(2));
        assert_ne!(Value::String("a".into()), Value::String("b".into()));
        assert_ne!(Value::Bool(true), Value::Bool(false));
    }

    #[test]
    fn equality_cross_variant_false() {
        // Int and Float with the same magnitude are NOT equal
        assert_ne!(Value::Int(1), Value::Float(1.0));
        // String and Identifier are NOT equal even if text matches
        assert_ne!(Value::String("foo".into()), Value::Identifier("foo".into()));
        // Function values are never equal (no PartialEq impl for FunctionValue)
        // — they simply don't match any other branch, so the default is false.
    }

    #[test]
    fn equality_list_and_map() {
        let a = Value::List(vec![Value::Int(1), Value::Int(2)]);
        let b = Value::List(vec![Value::Int(1), Value::Int(2)]);
        let c = Value::List(vec![Value::Int(1)]);
        assert_eq!(a, b);
        assert_ne!(a, c);

        let mut m1 = IndexMap::new();
        m1.insert("k".into(), Value::Bool(true));
        let mut m2 = IndexMap::new();
        m2.insert("k".into(), Value::Bool(true));
        assert_eq!(Value::Map(m1), Value::Map(m2));
    }

    // ── Value::to_interp_string() ─────────────────────────────────────────────

    #[test]
    fn interp_string_scalars() {
        assert_eq!(
            Value::String("hello".into()).to_interp_string(),
            Ok("hello".into())
        );
        assert_eq!(Value::Int(42).to_interp_string(), Ok("42".into()));
        assert_eq!(Value::Bool(true).to_interp_string(), Ok("true".into()));
        assert_eq!(Value::Null.to_interp_string(), Ok("null".into()));
        assert_eq!(
            Value::Identifier("svc-auth".into()).to_interp_string(),
            Ok("svc-auth".into())
        );
    }

    #[test]
    fn interp_string_non_scalar_errors() {
        assert!(Value::List(vec![]).to_interp_string().is_err());
        assert!(Value::Map(IndexMap::new()).to_interp_string().is_err());
        assert!(Value::Set(vec![]).to_interp_string().is_err());
    }

    // ── Display ───────────────────────────────────────────────────────────────

    #[test]
    fn display_list() {
        let v = Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)]);
        assert_eq!(v.to_string(), "[1, 2, 3]");
    }

    #[test]
    fn display_null() {
        assert_eq!(Value::Null.to_string(), "null");
    }

    #[test]
    fn display_set() {
        let v = Value::Set(vec![Value::String("a".into()), Value::String("b".into())]);
        assert_eq!(v.to_string(), "set(a, b)");
    }
}
