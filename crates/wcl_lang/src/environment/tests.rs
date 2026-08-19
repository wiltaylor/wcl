use super::*;
use crate::ast::{BuiltinType, TypeRef};
use crate::doc::{DeclName, Document, ResolvedType};
use crate::value::Value;

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
            .decorator(DecoratorBuilder::new(["block"]).positional(Value::Utf8("service".into())))
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
    use crate::functions::from_fn;
    let mut env = Environment::empty();
    env.add_builtin("upper", from_fn(|s: String| s.to_uppercase()));
    assert!(env.builtin("upper").is_some());
    assert!(env.builtin("missing").is_none());
}
