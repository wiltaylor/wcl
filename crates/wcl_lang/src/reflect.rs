//! Reflective builtins: query the decorators attached to anything
//! addressable in the document tree. The `decorator_names` and
//! `decorator_arg` HOF builtins consume a [`Value::DataPath`]
//! (produced whenever a non-leaf identifier or member chain is
//! evaluated) and walk the underlying view's `decorators()` iterator.

use crate::builtins::{BuiltinFn, Caller, DataPath, FromValue};
use crate::data::{DataKind, DataRef};
use crate::doc::Decorator;
use crate::environment::Environment;
use crate::value::Value;

pub(crate) fn register(env: &mut Environment) {
    env.add_builtin("decorator_names", BuiltinFn::hof(1, decorator_names_hof));
    env.add_builtin("decorator_arg", BuiltinFn::hof(3, decorator_arg_hof));
}

/// Collect the decorators attached to this navigator. `None` for
/// navigators whose underlying AST node has no decorator slot
/// (`Document`, `BlockList`, `Table`, `VariantValue*`).
fn collect_decorators<'a>(dr: &DataRef<'a>) -> Option<Vec<Decorator<'a>>> {
    match dr.inner() {
        DataKind::Type(t) => Some(t.decorators().collect()),
        DataKind::TypeField(f) => Some(f.decorators().collect()),
        DataKind::Union(u) => Some(u.decorators().collect()),
        DataKind::Variant(v) => Some(v.decorators().collect()),
        DataKind::Symbols(s) => Some(s.decorators().collect()),
        DataKind::Symbol(s) => Some(s.decorators().collect()),
        DataKind::Block(b) => Some(b.decorators().collect()),
        DataKind::Field(f) => Some(f.decorators().collect()),
        DataKind::Document(_)
        | DataKind::BlockList(_)
        | DataKind::Table(_)
        | DataKind::VariantValue(_)
        | DataKind::VariantValueList(_) => None,
    }
}

fn resolve_path<'r>(
    caller: &'r dyn Caller,
    builtin: &str,
    path: &DataPath,
) -> Result<DataRef<'r>, String> {
    caller.resolve(&path.segments).ok_or_else(|| {
        format!(
            "{builtin}: path '{}' does not resolve",
            path.segments.join(".")
        )
    })
}

fn decorator_names_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let path = DataPath::from_value(&args[0])?;
    let dr = resolve_path(caller, "decorator_names", &path)?;
    let decs = collect_decorators(&dr).ok_or_else(|| {
        format!(
            "decorator_names: {} targets do not carry decorators",
            path.kind
        )
    })?;
    let names: Vec<Value> = decs.iter().map(|d| Value::Utf8(d.full_name())).collect();
    Ok(Value::List(names))
}

fn decorator_arg_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let path = DataPath::from_value(&args[0])?;
    let dec_name = String::from_value(&args[1])?;
    let slot = String::from_value(&args[2])?;
    let dr = resolve_path(caller, "decorator_arg", &path)?;
    let decs = collect_decorators(&dr).ok_or_else(|| {
        format!(
            "decorator_arg: {} targets do not carry decorators",
            path.kind
        )
    })?;
    let Some(dec) = decs
        .iter()
        .find(|d| d.full_name() == dec_name || d.name() == dec_name)
    else {
        return Ok(Value::None);
    };
    // Try schema-aware resolution first; fall back to a raw named-arg
    // lookup so un-schema'd decorators still work.
    if let Some(res) = dec.resolved_arg_value(&slot) {
        return res.map_err(|e| format!("decorator_arg: {e}"));
    }
    match dec.named_arg(&slot) {
        Some(Ok(v)) => Ok(v),
        Some(Err(e)) => Err(format!("decorator_arg: {e}")),
        None => Ok(Value::None),
    }
}

#[cfg(test)]
mod tests {
    use crate::doc::Document;
    use crate::value::Value;

    fn eval_field(src: &str, field: &str) -> Value {
        let doc = Document::open(src, "test").expect("opens");
        doc.get(field)
            .expect("field present")
            .value()
            .expect("evaluates")
    }

    #[test]
    fn decorator_names_lists_type_decorators_in_source_order() {
        let v = eval_field(
            r#"
            @block("svc") @schemaless
            type Svc { id: utf8 }
            @schemaless out = decorator_names(Svc)
            "#,
            "out",
        );
        assert_eq!(
            v,
            Value::List(vec![
                Value::Utf8("block".into()),
                Value::Utf8("schemaless".into()),
            ])
        );
    }

    #[test]
    fn decorator_arg_reads_positional_via_schema_slot() {
        let v = eval_field(
            r#"
            @block("svc") @schemaless
            type Svc { id: utf8 }
            @schemaless out = decorator_arg(Svc, "block", "name")
            "#,
            "out",
        );
        assert_eq!(v, Value::Utf8("svc".into()));
    }

    #[test]
    fn decorator_arg_on_type_field() {
        let v = eval_field(
            r#"
            @block("svc") @schemaless
            type Svc { @inline(2) id: utf8 }
            @schemaless out = decorator_arg(Svc.id, "inline", "slot")
            "#,
            "out",
        );
        assert_eq!(v, Value::I64(2));
    }

    #[test]
    fn decorator_arg_missing_decorator_returns_none() {
        let v = eval_field(
            r#"
            @block("svc") @schemaless
            type Svc { id: utf8 }
            @schemaless out = decorator_arg(Svc, "nope", "x")
            "#,
            "out",
        );
        assert_eq!(v, Value::None);
    }

    #[test]
    fn decorator_arg_missing_slot_returns_none() {
        let v = eval_field(
            r#"
            @block("svc") @schemaless
            type Svc { id: utf8 }
            @schemaless out = decorator_arg(Svc, "block", "no_such_slot")
            "#,
            "out",
        );
        assert_eq!(v, Value::None);
    }

    #[test]
    fn decorator_names_on_union_variant() {
        // Union variants carry their own decorators; member access
        // walks Union -> Variant.
        let v = eval_field(
            r#"
            union Color {
                @schemaless Red none
                Blue none
            }
            @schemaless out = decorator_names(Color.Red)
            "#,
            "out",
        );
        assert_eq!(v, Value::List(vec![Value::Utf8("schemaless".into())]));
    }

    #[test]
    fn decorator_arg_rejects_non_path_argument() {
        let doc = Document::open(
            r#"@schemaless out = decorator_arg("not a path", "a", "b")"#,
            "test",
        )
        .expect("opens");
        let err = doc
            .get("out")
            .expect("field present")
            .value()
            .expect_err("non-path arg surfaces a builtin error");
        let msg = format!("{err:?}");
        assert!(msg.contains("data path"), "{msg}");
    }
}
