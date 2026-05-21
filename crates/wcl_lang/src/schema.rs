//! Programmatic schema registration.
//!
//! Lets a Rust embedder ship "built-in" type declarations that participate
//! in the document's type registry and schema lookups, without writing
//! source files. The four schema decorators that the language treats
//! specially (`block`, `decorator`, `inline`, `default`) are pre-registered
//! in every `SchemaRegistry::new()`.

use crate::ast;
use crate::ast::Span;
use crate::value::{BuiltinType, TypeRef, Value};

/// A bag of synthetic type declarations merged into a `Document` at open
/// time. Use [`SchemaRegistry::new`] for a registry pre-populated with the
/// four language-built-in decorator schemas; use [`SchemaRegistry::empty`]
/// for a strictly empty one.
pub struct SchemaRegistry {
    types: Vec<ast::TypeDecl>,
}

impl Default for SchemaRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl SchemaRegistry {
    /// Registry pre-populated with the built-in decorator schemas.
    pub fn new() -> Self {
        let mut reg = Self { types: Vec::new() };
        reg.types.extend(builtin_decorator_schemas());
        reg
    }

    /// Registry with no built-ins. Mostly useful for tests that want to
    /// assert exact contents.
    pub fn empty() -> Self {
        Self { types: Vec::new() }
    }

    /// Register a programmatically built type declaration.
    pub fn add_type(&mut self, t: BuiltType) -> &mut Self {
        self.types.push(t.inner);
        self
    }

    pub(crate) fn types(&self) -> &[ast::TypeDecl] {
        &self.types
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
        // `default` accepts any value; we tighten the inner type when an
        // `any` builtin lands.
        synth_decorator_schema(
            "Default",
            "default",
            "value",
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
        fields: vec![ast::TypeField {
            name: field_name.to_string(),
            ty: field_ty,
            ty_span: synthetic_span(),
            optional: false,
            decorators: Vec::new(),
            span: synthetic_span(),
        }],
        decorators: vec![ast::Decorator {
            name: vec!["decorator".to_string()],
            positional: vec![ast::Expr::Utf8(decorator_name.to_string())],
            named: Vec::new(),
            span: synthetic_span(),
        }],
        span: synthetic_span(),
    }
}

/// Output of [`TypeBuilder::build`] — a finished synthetic type declaration
/// ready to register with a [`SchemaRegistry`].
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
                fields: self.fields,
                decorators: self.decorators,
                span: synthetic_span(),
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
        Value::Identifier(s) => ast::Expr::Identifier(s),
        Value::Symbol(s) => ast::Expr::Symbol(s),
        Value::None => ast::Expr::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc::{Document, ResolvedType};

    #[test]
    fn empty_registry_has_no_types() {
        let r = SchemaRegistry::empty();
        assert!(r.types().is_empty());
    }

    #[test]
    fn new_registry_includes_builtin_decorator_schemas() {
        let r = SchemaRegistry::new();
        let names: Vec<_> = r.types().iter().map(|t| t.name.join(".")).collect();
        assert!(names.contains(&"Block".to_string()));
        assert!(names.contains(&"Decorator".to_string()));
        assert!(names.contains(&"Inline".to_string()));
        assert!(names.contains(&"Default".to_string()));
    }

    #[test]
    fn register_type_with_builder() {
        let mut r = SchemaRegistry::empty();
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
        let mut r = SchemaRegistry::empty();
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
        let mut r = SchemaRegistry::empty();
        r.add_type(TypeBuilder::new(["Service"]).build());
        let err = Document::open_with("type Service {}", "test", &r).unwrap_err();
        let msg = format!("{err:?}");
        assert!(msg.contains("duplicate declaration"), "{msg}");
    }
}
