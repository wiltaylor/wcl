//! The host environment: synthetic schema types + registered built-in functions.
//!
//! Lets a Rust embedder ship "built-in" type declarations and Rust callables
//! that participate in a document's type registry and evaluator. The four
//! schema decorators that the language treats specially (`block`, `decorator`,
//! `inline`, `default`) are pre-registered in every [`Environment::new`].

use std::collections::HashMap;

use crate::ast;
use crate::ast::Span;
use crate::builtins::BuiltinFn;
use crate::value::{BuiltinType, TypeRef, Value};

/// Host-supplied bundle of synthetic types and built-in functions merged
/// into a [`Document`](crate::Document) at open time.
///
/// Use [`Environment::new`] for an environment pre-populated with the four
/// language-built-in decorator schemas and no builtins; use
/// [`Environment::empty`] for a strictly empty one.
#[derive(Clone, Default)]
pub struct Environment {
    types: Vec<ast::TypeDecl>,
    builtins: HashMap<String, BuiltinFn>,
}

impl std::fmt::Debug for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Environment")
            .field("types", &self.types.len())
            .field("builtins", &self.builtins.keys().collect::<Vec<_>>())
            .finish()
    }
}

impl Environment {
    /// Environment pre-populated with the built-in decorator schemas and an
    /// empty builtins map.
    pub fn new() -> Self {
        let mut env = Self::empty();
        env.types.extend(builtin_decorator_schemas());
        crate::collections::register(&mut env);
        crate::math::register(&mut env);
        crate::reflect::register(&mut env);
        env
    }

    /// Strictly-empty environment. No synthetic types, no builtins.
    pub fn empty() -> Self {
        Self {
            types: Vec::new(),
            builtins: HashMap::new(),
        }
    }

    /// Register a programmatically built type declaration.
    pub fn add_type(&mut self, t: BuiltType) -> &mut Self {
        self.types.push(t.inner);
        self
    }

    /// Register a built-in function callable from WCL code by `name`.
    ///
    /// Use [`from_fn`](crate::from_fn) (or build a [`BuiltinFn`] manually)
    /// to construct the second argument.
    pub fn add_builtin(&mut self, name: impl Into<String>, f: BuiltinFn) -> &mut Self {
        self.builtins.insert(name.into(), f);
        self
    }

    pub(crate) fn types(&self) -> &[ast::TypeDecl] {
        &self.types
    }

    pub(crate) fn builtin(&self, name: &str) -> Option<&BuiltinFn> {
        self.builtins.get(name)
    }

    /// Iterate registered built-in callables as `(name, &BuiltinFn)`
    /// pairs. Hosts can read each builtin's arity and (when present)
    /// signature directly off the [`BuiltinFn`] for completion / hover
    /// tooling.
    pub fn builtins(&self) -> impl Iterator<Item = (&str, &BuiltinFn)> {
        self.builtins.iter().map(|(name, f)| (name.as_str(), f))
    }
}

fn synthetic_span() -> Span {
    Span::new(0, 0)
}

fn builtin_decorator_schemas() -> Vec<ast::TypeDecl> {
    vec![
        synth_decorator_schema(
            "Block",
            "block",
            "name",
            TypeRef::Builtin(BuiltinType::Utf8),
        ),
        synth_decorator_schema(
            "Decorator",
            "decorator",
            "name",
            TypeRef::Builtin(BuiltinType::Utf8),
        ),
        synth_decorator_schema(
            "Inline",
            "inline",
            "slot",
            TypeRef::Builtin(BuiltinType::U64),
        ),
        // `default` accepts any value; tighten the inner type when an
        // `any` builtin lands.
        synth_decorator_schema(
            "Default",
            "default",
            "value",
            TypeRef::Builtin(BuiltinType::Utf8),
        ),
        synth_decorator_schema(
            "Child",
            "child",
            "kind",
            TypeRef::Builtin(BuiltinType::Utf8),
        ),
        synth_decorator_schema(
            "Children",
            "children",
            "kind",
            TypeRef::Builtin(BuiltinType::Utf8),
        ),
        synth_decorator_schema(
            "Table",
            "table",
            "name",
            TypeRef::Builtin(BuiltinType::Utf8),
        ),
        synth_decorator_schema(
            "Connections",
            "connections",
            "schema",
            TypeRef::Builtin(BuiltinType::Utf8),
        ),
        // `@document` marks a type as the schema for the document
        // root. The single declared positional arg is ignored at
        // recognition time; only the decorator name matters.
        synth_decorator_schema(
            "Document",
            "document",
            "name",
            TypeRef::Builtin(BuiltinType::Utf8),
        ),
        // `@schemaless` opts a block or field out of strict schema
        // validation. Same shape — only the decorator name is read.
        synth_decorator_schema(
            "Schemaless",
            "schemaless",
            "reason",
            TypeRef::Builtin(BuiltinType::Utf8),
        ),
    ]
}

fn synth_decorator_schema(
    type_name: &str,
    decorator_name: &str,
    field_name: &str,
    field_ty: TypeRef,
) -> ast::TypeDecl {
    ast::TypeDecl {
        name: vec![type_name.to_string()],
        extends: Vec::new(),
        fields: vec![ast::TypeField {
            name: field_name.to_string(),
            ty: field_ty,
            ty_span: synthetic_span(),
            optional: false,
            decorators: Vec::new(),
            span: synthetic_span(),
            default_expr: None,
        }],
        decorators: vec![ast::Decorator {
            name: vec!["decorator".to_string()],
            positional: vec![ast::Expr::Utf8(decorator_name.to_string())],
            named: Vec::new(),
            span: synthetic_span(),
        }],
        span: synthetic_span(),
        leading_trivia: Vec::new(),
    }
}

/// Output of [`TypeBuilder::build`] — a finished synthetic type declaration
/// ready to register with an [`Environment`].
pub struct BuiltType {
    pub(crate) inner: ast::TypeDecl,
}

/// Builder for synthetic type declarations.
pub struct TypeBuilder {
    name: Vec<String>,
    fields: Vec<ast::TypeField>,
    decorators: Vec<ast::Decorator>,
}

impl TypeBuilder {
    pub fn new<I, S>(name: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            name: name.into_iter().map(Into::into).collect(),
            fields: Vec::new(),
            decorators: Vec::new(),
        }
    }

    pub fn decorator(mut self, d: DecoratorBuilder) -> Self {
        self.decorators.push(d.build());
        self
    }

    pub fn field(mut self, f: TypeFieldBuilder) -> Self {
        self.fields.push(f.build());
        self
    }

    pub fn build(self) -> BuiltType {
        BuiltType {
            inner: ast::TypeDecl {
                name: self.name,
                extends: Vec::new(),
                fields: self.fields,
                decorators: self.decorators,
                span: synthetic_span(),
                leading_trivia: Vec::new(),
            },
        }
    }
}

/// Builder for synthetic type fields.
pub struct TypeFieldBuilder {
    name: String,
    ty: TypeRef,
    optional: bool,
    decorators: Vec<ast::Decorator>,
}

impl TypeFieldBuilder {
    pub fn new(name: impl Into<String>, ty: TypeRef) -> Self {
        Self {
            name: name.into(),
            ty,
            optional: false,
            decorators: Vec::new(),
        }
    }

    pub fn optional(mut self, optional: bool) -> Self {
        self.optional = optional;
        self
    }

    pub fn decorator(mut self, d: DecoratorBuilder) -> Self {
        self.decorators.push(d.build());
        self
    }

    pub(crate) fn build(self) -> ast::TypeField {
        ast::TypeField {
            name: self.name,
            ty: self.ty,
            ty_span: synthetic_span(),
            optional: self.optional,
            decorators: self.decorators,
            span: synthetic_span(),
            default_expr: None,
        }
    }
}

/// Builder for synthetic decorators.
pub struct DecoratorBuilder {
    name: Vec<String>,
    positional: Vec<ast::Expr>,
    named: Vec<ast::NamedArg>,
}

impl DecoratorBuilder {
    pub fn new<I, S>(name: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            name: name.into_iter().map(Into::into).collect(),
            positional: Vec::new(),
            named: Vec::new(),
        }
    }

    pub fn positional(mut self, value: Value) -> Self {
        self.positional.push(value_to_expr(value));
        self
    }

    pub fn named(mut self, name: impl Into<String>, value: Value) -> Self {
        self.named.push(ast::NamedArg {
            name: name.into(),
            value: value_to_expr(value),
            span: synthetic_span(),
        });
        self
    }

    pub(crate) fn build(self) -> ast::Decorator {
        ast::Decorator {
            name: self.name,
            positional: self.positional,
            named: self.named,
            span: synthetic_span(),
        }
    }
}

fn value_to_expr(v: Value) -> ast::Expr {
    match v {
        Value::Bool(b) => ast::Expr::Bool(b),
        Value::I8(n) => ast::Expr::I8(n),
        Value::I16(n) => ast::Expr::I16(n),
        Value::I32(n) => ast::Expr::I32(n),
        Value::I64(n) => ast::Expr::I64(n),
        Value::I128(n) => ast::Expr::I128(n),
        Value::Isize(n) => ast::Expr::Isize(n),
        Value::U8(n) => ast::Expr::U8(n),
        Value::U16(n) => ast::Expr::U16(n),
        Value::U32(n) => ast::Expr::U32(n),
        Value::U64(n) => ast::Expr::U64(n),
        Value::U128(n) => ast::Expr::U128(n),
        Value::Usize(n) => ast::Expr::Usize(n),
        Value::F32(n) => ast::Expr::F32(n),
        Value::F64(n) => ast::Expr::F64(n),
        Value::Utf8(s) => ast::Expr::Utf8(s),
        Value::Ascii(s) => ast::Expr::Ascii(s),
        Value::Utf16(v) => ast::Expr::Utf16(v),
        Value::Utf32(v) => ast::Expr::Utf32(v),
        Value::Identifier(s) => ast::Expr::Identifier(s, ast::Span::new(0, 0)),
        Value::Symbol(s) => ast::Expr::Symbol(s),
        Value::None => ast::Expr::None,
        Value::Function(_) => {
            unreachable!("function values are not constructible via the schema builder API")
        }
        Value::List(items) => ast::Expr::ListLit {
            elements: items.into_iter().map(value_to_expr).collect(),
            span: synthetic_span(),
        },
        Value::Tensor { .. } => {
            unreachable!("tensor values are not constructible via the schema builder API")
        }
        Value::Variant { .. } => {
            unreachable!("variant values are not constructible via the schema builder API")
        }
        Value::Record { .. } => {
            unreachable!("record values are not constructible via the schema builder API")
        }
        Value::DataPath { .. } => {
            unreachable!("data path values are not constructible via the schema builder API")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{DeclName, Document, ResolvedType};

    #[test]
    fn empty_environment_has_no_types() {
        let r = Environment::empty();
        assert!(r.types().is_empty());
        assert!(r.builtin("anything").is_none());
    }

    #[test]
    fn new_environment_includes_builtin_decorator_schemas() {
        let r = Environment::new();
        let names: Vec<_> = r.types().iter().map(|t| t.name.join(".")).collect();
        assert!(names.contains(&"Block".to_string()));
        assert!(names.contains(&"Decorator".to_string()));
        assert!(names.contains(&"Inline".to_string()));
        assert!(names.contains(&"Default".to_string()));
    }

    #[test]
    fn register_type_with_builder() {
        let mut r = Environment::empty();
        r.add_type(
            TypeBuilder::new(["Service"])
                .decorator(
                    DecoratorBuilder::new(["block"]).positional(Value::Utf8("service".into())),
                )
                .field(
                    TypeFieldBuilder::new("id", TypeRef::Builtin(BuiltinType::Identifier))
                        .decorator(DecoratorBuilder::new(["inline"]).positional(Value::I64(0))),
                )
                .field(
                    TypeFieldBuilder::new("port", TypeRef::Builtin(BuiltinType::U32))
                        .optional(true)
                        .decorator(DecoratorBuilder::new(["default"]).positional(Value::I64(8080))),
                )
                .build(),
        );
        let doc = Document::open_with("", "test", &r).expect("open");
        let s = doc.block_schema("service").expect("registered schema");
        assert_eq!(s.name(), "Service");
        assert_eq!(s.field("id").unwrap().inline_slot(), Some(0));
        assert_eq!(
            s.field("port").unwrap().default_value(),
            Some(Value::I64(8080))
        );
    }

    #[test]
    fn synthetic_type_used_as_field_type() {
        let mut r = Environment::empty();
        r.add_type(TypeBuilder::new(["Service"]).build());
        let doc = Document::open_with("type Cfg { s: Service }", "test", &r).expect("open");
        let cfg = doc.type_decl("Cfg").unwrap();
        let s = cfg.field("s").unwrap();
        match doc.resolve(s.type_ref()) {
            ResolvedType::Named(d) => assert_eq!(d.name(), "Service"),
            _ => panic!("expected Named"),
        }
    }

    #[test]
    fn source_redeclares_synthetic_errors() {
        let mut r = Environment::empty();
        r.add_type(TypeBuilder::new(["Service"]).build());
        let err = Document::open_with("type Service {}", "test", &r).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("duplicate declaration"), "{msg}");
    }

    #[test]
    fn registers_builtin_callable_by_name() {
        use crate::builtins::from_fn;
        let mut env = Environment::empty();
        env.add_builtin("upper", from_fn(|s: String| s.to_uppercase()));
        assert!(env.builtin("upper").is_some());
        assert!(env.builtin("missing").is_none());
    }
}
