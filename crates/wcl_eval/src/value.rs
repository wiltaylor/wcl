use indexmap::IndexMap;
use std::fmt;
use wcl_core::Span;

/// Runtime value in WCL
#[derive(Debug, Clone)]
pub enum Value {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
    /// Identifier literal value (the id type)
    Identifier(String),
    /// Ordered list
    List(Vec<Value>),
    /// Ordered map (preserves insertion order)
    Map(IndexMap<String, Value>),
    /// Set (ordered, unique values)
    Set(Vec<Value>),
    /// Reference to a block (block type, inline id, attributes map, child blocks, decorators)
    BlockRef(BlockRef),
    /// Symbol value (e.g. `:GET`, `:relational`)
    Symbol(String),
    /// Lambda/function value
    Function(FunctionValue),
}

#[derive(Debug, Clone)]
pub struct BlockRef {
    pub kind: String,
    pub id: Option<String>,
    pub attributes: IndexMap<String, Value>,
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
pub struct FunctionValue {
    pub params: Vec<String>,
    pub body: FunctionBody,
    pub closure_scope: Option<ScopeId>,
}

#[derive(Debug, Clone)]
pub enum FunctionBody {
    /// Built-in function (identified by name)
    Builtin(String),
    /// User-defined lambda (index into AST expressions, we'll store the Expr here)
    UserDefined(Box<wcl_core::ast::Expr>),
    /// Block expression body (lets + final expr)
    BlockExpr(
        Vec<(String, Box<wcl_core::ast::Expr>)>,
        Box<wcl_core::ast::Expr>,
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
            Value::Float(_) => "float",
            Value::Bool(_) => "bool",
            Value::Null => "null",
            Value::Identifier(_) => "identifier",
            Value::List(_) => "list",
            Value::Map(_) => "map",
            Value::Set(_) => "set",
            Value::Symbol(_) => "symbol",
            Value::BlockRef(_) => "block_ref",
            Value::Function(_) => "function",
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

    pub fn to_interp_string(&self) -> Result<String, String> {
        match self {
            Value::String(s) => Ok(s.clone()),
            Value::Int(i) => Ok(i.to_string()),
            Value::Float(f) => Ok(f.to_string()),
            Value::Bool(b) => Ok(b.to_string()),
            Value::Null => Ok("null".to_string()),
            Value::Identifier(s) => Ok(s.clone()),
            Value::Symbol(s) => Ok(format!(":{}", s)),
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
            (Value::Float(a), Value::Float(b)) => a == b,
            (Value::Bool(a), Value::Bool(b)) => a == b,
            (Value::Null, Value::Null) => true,
            (Value::Identifier(a), Value::Identifier(b)) => a == b,
            (Value::Symbol(a), Value::Symbol(b)) => a == b,
            (Value::List(a), Value::List(b)) => a == b,
            (Value::Map(a), Value::Map(b)) => a == b,
            (Value::Set(a), Value::Set(b)) => a == b,
            _ => false,
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::String(s) => write!(f, "{}", s),
            Value::Int(i) => write!(f, "{}", i),
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
