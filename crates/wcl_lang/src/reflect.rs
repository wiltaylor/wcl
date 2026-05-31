//! Reflective builtins: query the decorators attached to anything
//! addressable in the document tree. The `decorator_names` and
//! `decorator_arg` HOF builtins consume a [`Value::DataPath`]
//! (produced whenever a non-leaf identifier or member chain is
//! evaluated) and walk the underlying view's `decorators()` iterator.

use std::collections::{BTreeMap, HashSet};

use crate::builtins::{BuiltinFn, Caller, DataPath, FromValue};
use crate::data::{DataKind, DataRef};
use crate::doc::{ChildKind, DeclName, Decorator, TypeDecl, TypeField};
use crate::environment::Environment;
use crate::value::Value;

pub(crate) fn register(env: &mut Environment) {
    env.add_builtin(
        "decorator_names",
        BuiltinFn::hof(1, decorator_names_hof).with_signature("fn (&T) -> [utf8]"),
    );
    env.add_builtin(
        "decorator_arg",
        BuiltinFn::hof(3, decorator_arg_hof).with_signature("fn (&T, utf8, utf8) -> any"),
    );
    env.add_builtin(
        "type_fields",
        BuiltinFn::hof(1, type_fields_hof).with_signature("fn (&T) -> [record]"),
    );
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

/// `type_fields(&T)` — reflect a type declaration into a list of records
/// describing each of its fields, for documentation generators. Own fields
/// come first (declaration order), then fields inherited transitively via
/// `extends` (those not already declared on the type itself). Each record:
///
///   { name: utf8, type: utf8, is_function: bool, optional: bool,
///     has_default: bool, is_block: bool, repeated: bool, accepts: utf8,
///     decorators: [{ name: utf8, arg: utf8 }] }
///
/// `is_block`/`repeated`/`accepts` describe `@child` / `@children` nested-
/// block slots (structural facts); `decorators` is the escape hatch for any
/// metadata decorator (e.g. a `@doc("…")` description) the caller defines.
fn type_fields_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let path = DataPath::from_value(&args[0])?;
    let dr = resolve_path(caller, "type_fields", &path)?;
    let DataKind::Type(decl) = dr.inner() else {
        return Err(format!(
            "type_fields: '{}' is not a type declaration",
            path.segments.join(".")
        ));
    };
    let records = ordered_fields(decl)
        .into_iter()
        .map(|f| field_record(&f))
        .collect();
    Ok(Value::List(records))
}

/// A type's own fields (declaration order) followed by its inherited fields
/// (from `effective_fields`, which walks `extends`) that it doesn't itself
/// redeclare. Own-first reads better in a docs table than the ancestors-
/// first order `effective_fields` returns on its own.
fn ordered_fields<'a>(decl: &TypeDecl<'a>) -> Vec<TypeField<'a>> {
    let own: Vec<TypeField<'a>> = decl.fields().collect();
    let own_names: HashSet<&str> = own.iter().map(TypeField::name).collect();
    let mut out = own;
    for f in decl.effective_fields() {
        if !own_names.contains(f.name()) {
            out.push(f);
        }
    }
    out
}

/// Build the per-field documentation record.
fn field_record(f: &TypeField<'_>) -> Value {
    let child = f.child_kind_or_union();
    let children = f.children_kind_or_union();
    let accepts = child
        .as_ref()
        .or(children.as_ref())
        .map(child_accepts)
        .unwrap_or_default();

    let decorators: Vec<Value> = f
        .decorators()
        .map(|d| {
            let mut m = BTreeMap::new();
            m.insert("name".to_string(), Value::Utf8(d.name().to_string()));
            m.insert("arg".to_string(), Value::Utf8(decorator_first_arg(&d)));
            Value::Record {
                ty: vec!["Decorator".to_string()],
                fields: m,
            }
        })
        .collect();

    let mut m = BTreeMap::new();
    m.insert("name".to_string(), Value::Utf8(f.name().to_string()));
    m.insert("type".to_string(), Value::Utf8(f.type_ref().to_string()));
    m.insert(
        "is_function".to_string(),
        Value::Bool(matches!(
            f.type_ref(),
            crate::value::TypeRef::Function { .. }
        )),
    );
    m.insert("optional".to_string(), Value::Bool(f.optional()));
    m.insert(
        "has_default".to_string(),
        Value::Bool(f.default_value().is_some()),
    );
    m.insert(
        "is_block".to_string(),
        Value::Bool(child.is_some() || children.is_some()),
    );
    m.insert("repeated".to_string(), Value::Bool(children.is_some()));
    m.insert("accepts".to_string(), Value::Utf8(accepts));
    m.insert("decorators".to_string(), Value::List(decorators));
    Value::Record {
        ty: vec!["TypeField".to_string()],
        fields: m,
    }
}

/// The kind / union / interface name a `@child` / `@children` slot accepts.
fn child_accepts(ck: &ChildKind<'_>) -> String {
    if let Some(k) = ck.as_kind() {
        k.to_string()
    } else if let Some(u) = ck.as_union() {
        u.name().to_string()
    } else if let Some(i) = ck.as_interface() {
        i.name().to_string()
    } else {
        String::new()
    }
}

/// A decorator's first positional argument as a string (`""` when it has
/// none or can't be evaluated). A string arg yields its text; other values
/// fall back to their `Display` form.
fn decorator_first_arg(d: &Decorator<'_>) -> String {
    match d.positional() {
        Ok(args) => match args.into_iter().next() {
            Some(Value::Utf8(s)) | Some(Value::Ascii(s)) => s,
            Some(other) => other.to_string(),
            None => String::new(),
        },
        Err(_) => String::new(),
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

    fn record_field<'v>(rec: &'v Value, key: &str) -> &'v Value {
        match rec {
            Value::Record { fields, .. } => fields.get(key).expect("record has key"),
            other => panic!("expected record, got {other:?}"),
        }
    }

    #[test]
    fn type_fields_reports_own_then_inherited_with_metadata() {
        let v = eval_field(
            r#"
            interface Base { id: identifier? }
            @block("svc") @schemaless
            type Svc extends Base {
              @doc("the service name") @inline(0) name: utf8
              port: u32?
              @hidden secret: utf8
              @children("route") routes: list<Route>
            }
            @block("route") type Route { path: utf8 }
            @schemaless out = type_fields(Svc)
            "#,
            "out",
        );
        let Value::List(fields) = v else {
            panic!("expected a list, got {v:?}");
        };
        // Own fields (name, port, secret, routes) first, then inherited `id`.
        let names: Vec<String> = fields
            .iter()
            .map(|f| match record_field(f, "name") {
                Value::Utf8(s) => s.clone(),
                other => panic!("name not a string: {other:?}"),
            })
            .collect();
        assert_eq!(names, ["name", "port", "secret", "routes", "id"]);

        // `name`: required, has a @doc decorator, not a block.
        let name = &fields[0];
        assert_eq!(record_field(name, "type"), &Value::Utf8("utf8".into()));
        assert_eq!(record_field(name, "optional"), &Value::Bool(false));
        assert_eq!(record_field(name, "is_block"), &Value::Bool(false));
        let Value::List(decs) = record_field(name, "decorators") else {
            panic!("decorators not a list");
        };
        // @doc("…") then @inline(0).
        assert_eq!(record_field(&decs[0], "name"), &Value::Utf8("doc".into()));
        assert_eq!(
            record_field(&decs[0], "arg"),
            &Value::Utf8("the service name".into())
        );

        // `port`: optional.
        assert_eq!(record_field(&fields[1], "optional"), &Value::Bool(true));

        // `secret`: carries a @hidden decorator.
        let Value::List(secret_decs) = record_field(&fields[2], "decorators") else {
            panic!("decorators not a list");
        };
        assert_eq!(
            record_field(&secret_decs[0], "name"),
            &Value::Utf8("hidden".into())
        );

        // `routes`: a @children block slot accepting `route`, repeated.
        let routes = &fields[3];
        assert_eq!(record_field(routes, "is_block"), &Value::Bool(true));
        assert_eq!(record_field(routes, "repeated"), &Value::Bool(true));
        assert_eq!(
            record_field(routes, "accepts"),
            &Value::Utf8("route".into())
        );

        // Inherited `id`: optional, last.
        assert_eq!(record_field(&fields[4], "optional"), &Value::Bool(true));
    }

    #[test]
    fn type_fields_rejects_non_type() {
        let doc =
            Document::open(r#"@schemaless out = type_fields("nope")"#, "test").expect("opens");
        let err = doc
            .get("out")
            .expect("field present")
            .value()
            .expect_err("non-path arg surfaces a builtin error");
        assert!(format!("{err:?}").contains("data path"));
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
