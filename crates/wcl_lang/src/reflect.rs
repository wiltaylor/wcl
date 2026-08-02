//! Reflective builtins: query the decorators attached to anything
//! addressable in the document tree. The `decorator_names` and
//! `decorator_arg` HOF builtins consume a [`Value::DataPath`]
//! (produced whenever a non-leaf identifier or member chain is
//! evaluated) and walk the underlying view's `decorators()` iterator.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::builtins::{BuiltinFn, BuiltinSignature, Caller, DataPath, FromValue, from_fn};
use crate::data::{DataKind, DataRef};
use crate::doc::{ChildKind, DeclName, Decorator, TypeField};
use crate::environment::Environment;
use crate::value::{FnValue, Value};

pub(crate) fn register(env: &mut Environment) {
    env.add_builtin(
        "decorator_names",
        BuiltinFn::hof(1, decorator_names_hof)
            .doc("List the names of the decorators attached to a referenced declaration.")
            .param(
                "target",
                "&T",
                "A reference to a type, field, block, or variant.",
            )
            .returns("[utf8]", "The decorator names, in source order."),
    );
    env.add_builtin(
        "decorator_arg",
        BuiltinFn::hof(3, decorator_arg_hof)
            .doc("Read one named argument of a decorator on a referenced declaration (`none` if absent).")
            .param("target", "&T", "A reference to a type, field, block, or variant.")
            .param("decorator", "utf8", "The decorator name, e.g. `\"doc\"`.")
            .param("slot", "utf8", "The argument (slot) name to read.")
            .returns("any", "The argument's value, or `none` if absent."),
    );
    env.add_builtin(
        "type_fields",
        BuiltinFn::hof(1, type_fields_hof)
            .doc("Reflect a type or interface into a list of field-description records (own fields first, then inherited via `extends`).")
            .param("target", "&T", "A reference to a type or interface declaration.")
            .returns(
                "[record]",
                "One record per field: `{ name, type, is_function, optional, has_default, is_block, repeated, accepts, decorators }`.",
            ),
    );
    env.add_builtin(
        "child_types",
        BuiltinFn::hof(1, child_types_hof)
            .doc("Reflect a type into references to the element types of its `@child` / `@children` block slots (own slots first, then inherited via `extends`). Pair with `type_table` / `type_fields` to auto-document the blocks a `@document` declares.")
            .param("target", "&T", "A reference to a type or interface declaration.")
            .returns(
                "[&T]",
                "One type reference per block slot. Slots that accept a union or interface resolve to that type's name; scalar (non-block) fields are skipped.",
            ),
    );
    env.add_builtin(
        "namespace_decls",
        BuiltinFn::hof(1, namespace_decls_hof)
            .doc("List references to every top-level declaration (`type` / `interface` / `union` / `symbol_set`) in a namespace, for schema-documentation generators. Pair with `decl_info`, `doc_comment`, `type_fields`, and `ast_string` to render each. Imported (library) declarations are included — filter on `decl_info(d).is_imported` to drop them.")
            .param(
                "ns",
                "utf8",
                "The namespace, dotted (e.g. `\"wdoc\"`); `\"\"` for the root namespace.",
            )
            .returns(
                "[&T]",
                "One reference per declaration: types first, then interfaces, unions, symbol sets, in source order.",
            ),
    );
    env.add_builtin(
        "decl_info",
        BuiltinFn::hof(1, decl_info_hof)
            .doc("Describe a top-level declaration: its name, kind, doc comment, and schema classification (block / table / decorator / document).")
            .param(
                "target",
                "&T",
                "A reference to a type, interface, union, or symbol_set declaration.",
            )
            .returns(
                "record",
                "`{ name, full_name, kind, doc, is_imported, is_document, block_kind, table_kind, decorator_name, extends }`. The classification fields are `none` when the decorator is absent.",
            ),
    );
    env.add_builtin(
        "doc_comment",
        BuiltinFn::hof(1, doc_comment_hof)
            .doc("The doc comment — the contiguous run of `#` / `//` lines immediately above a declaration — attached to a reference, or `\"\"` when there is none. Complements `decorator_arg(x, \"doc\", …)` for `@doc(\"…\")` metadata.")
            .param(
                "target",
                "&T",
                "A reference to a type, interface, union, variant, symbol_set, or field.",
            )
            .returns("utf8", "The joined comment text, or `\"\"` when absent."),
    );
    env.add_builtin(
        "ast_string",
        BuiltinFn::hof(1, ast_string_hof)
            .doc("Pretty-print the canonical source behind a reference (type/interface/union/symbol_set/block/field) or a function value.")
            .param("target", "&T", "A dataref to a declaration, or a function value.")
            .returns("utf8", "The canonical (pretty-printed) source text."),
    );
    env.add_builtin(
        "fn_signature",
        BuiltinFn::hof(1, fn_signature_hof)
            .doc("Describe a function's parameters and return type. Pass a function value, or a built-in's name as a string.")
            .param("f", "any", "A function value, or the name of a built-in as a utf8 string.")
            .returns(
                "record",
                "A record `{ doc, params: [{name, type, doc}], return_type, return_doc, signature, is_builtin }`.",
            ),
    );
    env.add_builtin(
        "builtin_names",
        BuiltinFn::hof(0, builtin_names_hof)
            .doc("The names of every registered built-in function, sorted. Pair with `fn_signature` to introspect each one.")
            .returns("[utf8]", "Every built-in's name, sorted alphabetically."),
    );
    // `eval` needs the live evaluator scope, so it's intercepted in
    // `eval_call_builtin` (like `error`/`panic`/`assert`); this registration
    // only makes the name resolvable and arity-checked. The body never runs.
    env.add_builtin(
        "eval",
        from_fn(|_: String| -> Result<Value, String> {
            Err("eval is evaluated in the caller's scope".to_string())
        })
        .doc("Parse a string as a WCL expression and evaluate it in the current scope.")
        .param(
            "src",
            "utf8",
            "WCL expression source to parse and evaluate.",
        )
        .returns("any", "The value the expression evaluates to."),
    );
}

/// `fn_signature(f)` — describe a function's parameters + return type.
/// Accepts a user function value (full structured params from the
/// `FnValue`) or a built-in's name string (structured doc metadata from
/// the registry). Returns a record
/// `{ doc, params: [{name, type, doc}], return_type, signature, is_builtin }`.
fn fn_signature_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    match &args[0] {
        Value::Function(fv) => Ok(user_fn_record(fv)),
        Value::Utf8(name) | Value::Ascii(name) => {
            let info = caller
                .builtin_info(name)
                .ok_or_else(|| format!("fn_signature: '{name}' is not a built-in function"))?;
            Ok(builtin_record(&info))
        }
        other => Err(format!(
            "fn_signature: expected a function value or a built-in name, found {}",
            other.type_name()
        )),
    }
}

/// `builtin_names()` — the sorted names of every registered built-in.
fn builtin_names_hof(caller: &mut dyn Caller, _args: &[Value]) -> Result<Value, String> {
    Ok(Value::list(
        caller
            .builtin_names()
            .into_iter()
            .map(Value::Utf8)
            .collect(),
    ))
}

/// Build a `FnParam` record from name / type / doc strings.
fn fn_param_record(name: &str, ty: &str, doc: &str) -> Value {
    let mut m = BTreeMap::new();
    m.insert("name".to_string(), Value::Utf8(name.to_string()));
    m.insert("type".to_string(), Value::Utf8(ty.to_string()));
    m.insert("doc".to_string(), Value::Utf8(doc.to_string()));
    Value::Record {
        ty: vec!["FnParam".to_string()],
        fields: std::sync::Arc::new(m),
    }
}

/// Assemble the `FnSignature` result record.
fn fn_signature_record(
    doc: &str,
    params: Vec<Value>,
    return_type: &str,
    return_doc: &str,
    signature: &str,
    is_builtin: bool,
) -> Value {
    let mut m = BTreeMap::new();
    m.insert("doc".to_string(), Value::Utf8(doc.to_string()));
    m.insert(
        "params".to_string(),
        Value::List(std::sync::Arc::new(params)),
    );
    m.insert(
        "return_type".to_string(),
        Value::Utf8(return_type.to_string()),
    );
    m.insert(
        "return_doc".to_string(),
        Value::Utf8(return_doc.to_string()),
    );
    m.insert("signature".to_string(), Value::Utf8(signature.to_string()));
    m.insert("is_builtin".to_string(), Value::Bool(is_builtin));
    Value::Record {
        ty: vec!["FnSignature".to_string()],
        fields: std::sync::Arc::new(m),
    }
}

/// `fn_signature` record for a user function value. Param docs are empty
/// (a bare function value carries no help text).
fn user_fn_record(fv: &FnValue) -> Value {
    let params: Vec<Value> = fv
        .params()
        .iter()
        .map(|p| fn_param_record(p.name(), &p.ty().to_string(), ""))
        .collect();
    let return_type = fv.return_ty().to_string();
    let param_sig = fv
        .params()
        .iter()
        .map(|p| format!("{}: {}", p.name(), p.ty()))
        .collect::<Vec<_>>()
        .join(", ");
    let signature = format!("fn({param_sig}) -> {return_type}");
    fn_signature_record("", params, &return_type, "", &signature, false)
}

/// `fn_signature` record for a built-in, from its registered doc metadata.
fn builtin_record(info: &BuiltinSignature) -> Value {
    let params: Vec<Value> = info
        .params
        .iter()
        .map(|p| fn_param_record(&p.name, &p.ty, &p.doc))
        .collect();
    fn_signature_record(
        &info.doc,
        params,
        &info.return_type,
        &info.return_doc,
        &info.signature,
        true,
    )
}

/// `ast_string(x)` — pretty-print the source code behind `x`. Accepts a
/// dataref (a `Value::DataPath` referencing a declaration) and renders that
/// declaration's canonical source, or a function value and renders its
/// `fn(params) -> ret body` source. Other values are an error.
fn ast_string_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    if let Value::Function(fv) = &args[0] {
        return Ok(Value::Utf8(fv.to_source()));
    }
    let path = DataPath::from_value(&args[0])?;
    let dr = resolve_path(caller, "ast_string", &path)?;
    let src = dr
        .to_source()
        .ok_or_else(|| format!("ast_string: {} has no source form", dr.kind()))?;
    Ok(Value::Utf8(src))
}

/// Collect the decorators attached to this navigator. `None` for
/// navigators whose underlying AST node has no decorator slot
/// (`Document`, `BlockList`, `Table`, `VariantValue*`).
fn collect_decorators<'a>(dr: &DataRef<'a>) -> Option<Vec<Decorator<'a>>> {
    match dr.inner() {
        DataKind::Type(t) => Some(t.decorators().collect()),
        DataKind::Interface(i) => Some(i.decorators().collect()),
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
        | DataKind::VariantValueList(_)
        | DataKind::Error(_) => None,
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
    Ok(Value::List(std::sync::Arc::new(names)))
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

/// `type_fields(&T)` — reflect a type **or interface** declaration into a
/// list of records describing each of its fields, for documentation
/// generators. Own fields come first (declaration order), then fields
/// inherited transitively via `extends` (those not redeclared). Each record:
///
///   { name: utf8, type: utf8, is_function: bool, optional: bool,
///     has_default: bool, is_block: bool, repeated: bool, accepts: utf8,
///     decorators: [{ name: utf8, arg: utf8 }] }
///
/// `is_block`/`repeated`/`accepts` describe `@child` / `@children` nested-
/// block slots (structural facts); `decorators` is the escape hatch for any
/// metadata decorator (e.g. a `@doc("…")` description). A redeclared field's
/// decorators are merged with those of the same-named inherited field
/// (own wins per-decorator), so shared field docs can live on an interface.
fn type_fields_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let path = DataPath::from_value(&args[0])?;
    let dr = resolve_path(caller, "type_fields", &path)?;
    let (own, effective, decs) = match dr.inner() {
        DataKind::Type(t) => (
            t.fields().collect::<Vec<_>>(),
            t.effective_fields(),
            t.merged_field_decorators(),
        ),
        DataKind::Interface(i) => (
            i.fields().collect::<Vec<_>>(),
            i.effective_fields(),
            i.merged_field_decorators(),
        ),
        _ => {
            return Err(format!(
                "type_fields: '{}' is not a type or interface declaration",
                path.segments.join(".")
            ));
        }
    };
    let records = order_fields(own, effective)
        .iter()
        .map(|f| field_record(f, &decs))
        .collect();
    Ok(Value::list(records))
}

/// `child_types(&T)` — reflect a type (or interface) into references to the
/// element types of its `@child` / `@children` block slots, in the same
/// own-then-inherited order as `type_fields`. Each result is a type
/// reference (`Value::DataPath`) suitable for handing straight to
/// `type_table` / `type_fields` / `ast_string`, so a repetition block can
/// auto-document every top-level block a `@document` declares:
///
///   repeat { each = child_types(MyDoc)  as = :b
///     type_table { type = b }
///   }
///
/// The element type is taken from the slot's declared field type, peeling
/// `list<…>` and `&…` wrappers (`@children("k") xs: list<X>` → `X`). Slots
/// that accept a union or interface resolve to that type's name (a
/// downstream `type_fields` only handles `type` / `interface`, so union
/// slots won't expand to per-variant tables — a known limitation). Scalar
/// (non-block) fields are skipped.
fn child_types_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let path = DataPath::from_value(&args[0])?;
    let dr = resolve_path(caller, "child_types", &path)?;
    let (own, effective) = match dr.inner() {
        DataKind::Type(t) => (t.fields().collect::<Vec<_>>(), t.effective_fields()),
        DataKind::Interface(i) => (i.fields().collect::<Vec<_>>(), i.effective_fields()),
        _ => {
            return Err(format!(
                "child_types: '{}' is not a type or interface declaration",
                path.segments.join(".")
            ));
        }
    };
    let refs = order_fields(own, effective)
        .iter()
        .filter(|f| f.child_kind_or_union().is_some() || f.children_kind_or_union().is_some())
        // Fully-qualified segments (resolved in the declaring file's
        // namespace), so the returned refs resolve from any namespace —
        // a chained `type_table { type = b }` must work wherever the
        // repeater body evaluates.
        .filter_map(|f| f.element_type_fqn_segments())
        .map(|segments| Value::DataPath {
            kind: "type".to_string(),
            segments,
        })
        .collect();
    Ok(Value::list(refs))
}

/// `namespace_decls(ns)` — references to every top-level declaration
/// (type / interface / union / symbol_set) in the namespace `ns` (dotted;
/// `""` is the root namespace). Each result is a `Value::DataPath` over the
/// declaration's fully-qualified name, so it flows straight into
/// `decl_info` / `doc_comment` / `type_fields` / `ast_string` — the chain a
/// schema-documentation page relies on:
///
///   repeat { each = namespace_decls("app")  as = :d
///     h3 { decl_info(d).name }
///     p { doc_comment(d) }
///     type_table { type = d }
///   }
fn namespace_decls_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let ns = String::from_value(&args[0])?;
    let segs: Vec<String> = if ns.is_empty() {
        Vec::new()
    } else {
        ns.split('.').map(str::to_string).collect()
    };
    let refs: Vec<Value> = caller
        .decls_in_namespace(&segs)
        .iter()
        .map(|dr| Value::DataPath {
            kind: dr.kind().to_string(),
            segments: decl_fqn_segments(dr),
        })
        .collect();
    Ok(Value::list(refs))
}

/// FQN segments of the declaration a namespace-level navigator points at,
/// so the returned `DataPath` re-resolves from any namespace.
fn decl_fqn_segments(dr: &DataRef<'_>) -> Vec<String> {
    match dr.inner() {
        DataKind::Type(t) => t.fqn_segments(),
        DataKind::Interface(i) => i.fqn_segments(),
        DataKind::Union(u) => u.fqn_segments(),
        DataKind::Symbols(s) => s.fqn_segments(),
        _ => Vec::new(),
    }
}

/// `decl_info(&T)` — a record describing a top-level declaration: its name,
/// kind, doc comment, and schema classification (block / table / decorator /
/// document). Lets a documentation repeater label and categorize each
/// reference `namespace_decls` returns, and filter out imports.
fn decl_info_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let path = DataPath::from_value(&args[0])?;
    let dr = resolve_path(caller, "decl_info", &path)?;
    decl_info_record(&dr).ok_or_else(|| {
        format!(
            "decl_info: '{}' is not a top-level declaration",
            path.segments.join(".")
        )
    })
}

/// Build the `decl_info` record. `None` for non-declaration navigators.
fn decl_info_record(dr: &DataRef<'_>) -> Option<Value> {
    let (name, full_name, is_imported, extends): (String, String, bool, Vec<String>) =
        match dr.inner() {
            DataKind::Type(t) => (
                t.name().to_string(),
                t.full_name(),
                t.is_imported(),
                extends_strings(t.extends()),
            ),
            DataKind::Interface(i) => (
                i.name().to_string(),
                i.full_name(),
                false,
                extends_strings(i.extends()),
            ),
            DataKind::Union(u) => (u.name().to_string(), u.full_name(), false, Vec::new()),
            DataKind::Symbols(s) => (s.name().to_string(), s.full_name(), false, Vec::new()),
            _ => return None,
        };

    let decs = collect_decorators(dr).unwrap_or_default();
    let dec_arg = |n: &str| -> Value {
        match decs.iter().find(|d| d.name() == n || d.full_name() == n) {
            Some(d) => Value::Utf8(decorator_first_arg(d)),
            None => Value::None,
        }
    };
    let has_dec = |n: &str| decs.iter().any(|d| d.name() == n || d.full_name() == n);

    let mut m = BTreeMap::new();
    m.insert("name".to_string(), Value::Utf8(name));
    m.insert("full_name".to_string(), Value::Utf8(full_name));
    m.insert("kind".to_string(), Value::Utf8(dr.kind().to_string()));
    m.insert(
        "doc".to_string(),
        Value::Utf8(collect_doc_comment(dr).unwrap_or_default()),
    );
    m.insert("is_imported".to_string(), Value::Bool(is_imported));
    m.insert("is_document".to_string(), Value::Bool(has_dec("document")));
    m.insert("block_kind".to_string(), dec_arg("block"));
    m.insert("table_kind".to_string(), dec_arg("table"));
    m.insert("decorator_name".to_string(), dec_arg("decorator"));
    m.insert(
        "extends".to_string(),
        Value::list(extends.into_iter().map(Value::Utf8).collect()),
    );
    Some(Value::Record {
        ty: vec!["DeclInfo".to_string()],
        fields: std::sync::Arc::new(m),
    })
}

/// Render each `extends` parent path as a dotted string.
fn extends_strings(extends: &[Vec<String>]) -> Vec<String> {
    extends.iter().map(|p| p.join(".")).collect()
}

/// `doc_comment(&T)` — the doc comment (`#` / `//` lines immediately above
/// a declaration) attached to a reference, or `""` when absent. Works for
/// type / interface / union / variant / symbol_set / field references.
fn doc_comment_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let path = DataPath::from_value(&args[0])?;
    let dr = resolve_path(caller, "doc_comment", &path)?;
    Ok(Value::Utf8(collect_doc_comment(&dr).unwrap_or_default()))
}

/// The doc comment attached to a navigator, mirroring `collect_decorators`.
/// `None` for navigators with no comment-carrying AST node.
fn collect_doc_comment(dr: &DataRef<'_>) -> Option<String> {
    match dr.inner() {
        DataKind::Type(t) => t.doc_comment(),
        DataKind::Interface(i) => i.doc_comment(),
        DataKind::Union(u) => u.doc_comment(),
        DataKind::Variant(v) => v.doc_comment(),
        DataKind::Symbols(s) => s.doc_comment(),
        DataKind::TypeField(f) => f.doc_comment(),
        _ => None,
    }
}

/// Own fields (declaration order) followed by inherited fields not
/// redeclared. Own-first reads better in a docs table than the ancestors-
/// first order `effective_fields` returns on its own.
fn order_fields<'a>(own: Vec<TypeField<'a>>, effective: Vec<TypeField<'a>>) -> Vec<TypeField<'a>> {
    let own_names: HashSet<&str> = own.iter().map(TypeField::name).collect();
    let mut out = own;
    for f in effective {
        if !own_names.contains(f.name()) {
            out.push(f);
        }
    }
    out
}

/// Build the per-field documentation record, taking decorators from the
/// merged map (so an inherited `@doc` surfaces on a redeclared field).
fn field_record<'a>(f: &TypeField<'a>, decs: &HashMap<String, Vec<Decorator<'a>>>) -> Value {
    let child = f.child_kind_or_union();
    let children = f.children_kind_or_union();
    let accepts = child
        .as_ref()
        .or(children.as_ref())
        .map(child_accepts)
        .unwrap_or_default();

    let empty: Vec<Decorator<'a>> = Vec::new();
    let decorators: Vec<Value> = decs
        .get(f.name())
        .unwrap_or(&empty)
        .iter()
        .map(|d| {
            let mut m = BTreeMap::new();
            m.insert("name".to_string(), Value::Utf8(d.name().to_string()));
            m.insert("arg".to_string(), Value::Utf8(decorator_first_arg(d)));
            Value::Record {
                ty: vec!["Decorator".to_string()],
                fields: std::sync::Arc::new(m),
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
    m.insert(
        "decorators".to_string(),
        Value::List(std::sync::Arc::new(decorators)),
    );
    Value::Record {
        ty: vec!["TypeField".to_string()],
        fields: std::sync::Arc::new(m),
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
            Value::List(std::sync::Arc::new(vec![
                Value::Utf8("block".into()),
                Value::Utf8("schemaless".into()),
            ]))
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
        assert_eq!(
            v,
            Value::List(std::sync::Arc::new(vec![Value::Utf8("schemaless".into())]))
        );
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
    fn type_fields_reflects_an_interface() {
        let v = eval_field(
            r#"
            @schemaless
            interface Widget { @doc("css width") width: utf8? }
            @schemaless out = type_fields(Widget)
            "#,
            "out",
        );
        let Value::List(fields) = v else {
            panic!("expected a list, got {v:?}");
        };
        assert_eq!(fields.len(), 1);
        assert_eq!(
            record_field(&fields[0], "name"),
            &Value::Utf8("width".into())
        );
        let Value::List(decs) = record_field(&fields[0], "decorators") else {
            panic!("decorators not a list");
        };
        assert_eq!(record_field(&decs[0], "name"), &Value::Utf8("doc".into()));
        assert_eq!(
            record_field(&decs[0], "arg"),
            &Value::Utf8("css width".into())
        );
    }

    #[test]
    fn redeclared_field_inherits_ancestor_doc() {
        // A concrete type must redeclare an interface field to accept it on
        // instances; reflection still surfaces the interface's `@doc`.
        let v = eval_field(
            r#"
            @schemaless
            interface Base { @doc("shared width") width: utf8? }
            @block("w") @schemaless
            type W extends Base { @doc("the name") @inline(0) name: utf8  width: utf8? }
            @schemaless out = type_fields(W)
            "#,
            "out",
        );
        let Value::List(fields) = v else {
            panic!("expected a list, got {v:?}");
        };
        let width = fields
            .iter()
            .find(|f| record_field(f, "name") == &Value::Utf8("width".into()))
            .expect("width field present");
        let Value::List(decs) = record_field(width, "decorators") else {
            panic!("decorators not a list");
        };
        assert_eq!(
            record_field(&decs[0], "arg"),
            &Value::Utf8("shared width".into()),
            "redeclared field inherited the interface @doc"
        );
    }

    fn datapath_segments(v: &Value) -> Vec<String> {
        match v {
            Value::DataPath { segments, .. } => segments.clone(),
            other => panic!("expected a data path, got {other:?}"),
        }
    }

    #[test]
    fn child_types_returns_block_element_type_refs() {
        // A @document's @child / @children slots resolve to references to
        // their element types, own-first then inherited, peeling `list<…>`.
        let v = eval_field(
            r#"
            @document
            type MyDoc {
              @children("project_meta") metas: list<ProjectMeta>
              @child("settings") settings: Settings
              note: utf8?
            }
            @block("project_meta") type ProjectMeta { @inline(0) id: identifier }
            @block("settings") type Settings { theme: utf8 }
            @schemaless out = child_types(MyDoc)
            "#,
            "out",
        );
        let Value::List(refs) = v else {
            panic!("expected a list, got {v:?}");
        };
        let names: Vec<Vec<String>> = refs.iter().map(datapath_segments).collect();
        assert_eq!(
            names,
            vec![
                vec!["ProjectMeta".to_string()],
                vec!["Settings".to_string()],
            ],
            "block slots only, scalar `note` skipped, list<…> peeled"
        );
    }

    #[test]
    fn child_types_each_ref_reflects_via_type_fields() {
        // A returned reference is usable straight in `type_fields` — the
        // chain a repetition block / `type_table` relies on.
        let v = eval_field(
            r#"
            @document
            type MyDoc { @child("settings") settings: Settings }
            @block("settings") type Settings { @doc("UI theme") theme: utf8 }
            @schemaless out = type_fields(at(child_types(MyDoc), 0))
            "#,
            "out",
        );
        let Value::List(fields) = v else {
            panic!("expected a list, got {v:?}");
        };
        assert_eq!(
            record_field(&fields[0], "name"),
            &Value::Utf8("theme".into())
        );
    }

    #[test]
    fn child_types_empty_for_scalar_only_type() {
        let v = eval_field(
            r#"
            @block("svc") @schemaless
            type Svc { id: utf8  port: u32? }
            @schemaless out = child_types(Svc)
            "#,
            "out",
        );
        assert_eq!(v, Value::List(std::sync::Arc::new(vec![])));
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

    #[test]
    fn type_fields_resolves_namespaced_types_same_as_root() {
        // A type under `namespace lib` reflects exactly like its
        // root-namespace twin (regression: namespaced lookups returned
        // nothing because resolution only tried the root namespace).
        let namespaced = eval_field(
            r#"
            namespace lib
            @block("gizmo") @schemaless
            type Gizmo { @inline(0) id: utf8  name: utf8 }
            @schemaless out = type_fields(Gizmo)
            "#,
            "out",
        );
        let root = eval_field(
            r#"
            @block("gizmo") @schemaless
            type Gizmo { @inline(0) id: utf8  name: utf8 }
            @schemaless out = type_fields(Gizmo)
            "#,
            "out",
        );
        assert_eq!(namespaced, root);
        let Value::List(items) = &namespaced else {
            panic!("expected list, got {namespaced:?}");
        };
        assert_eq!(items.len(), 2, "{items:?}");
    }

    #[test]
    fn child_types_returns_fully_qualified_refs_for_namespaced_types() {
        let v = eval_field(
            r#"
            namespace lib
            @block("gizmo") @schemaless
            type Gizmo { @inline(0) id: utf8 }
            @schemaless
            type LibModel { @children("gizmo") gizmos: list<Gizmo> }
            @schemaless out = child_types(LibModel)
            "#,
            "out",
        );
        assert_eq!(
            v,
            Value::List(std::sync::Arc::new(vec![Value::DataPath {
                kind: "type".into(),
                segments: vec!["lib".into(), "Gizmo".into()],
            }]))
        );
    }

    #[test]
    fn child_types_chains_into_type_fields_for_namespaced_types() {
        // The refs `child_types` returns must be consumable by
        // `type_fields` regardless of the namespace they're used in.
        let v = eval_field(
            r#"
            namespace lib
            @block("gizmo") @schemaless
            type Gizmo { @inline(0) id: utf8  name: utf8 }
            @schemaless
            type LibModel { @children("gizmo") gizmos: list<Gizmo> }
            @schemaless out = type_fields(at(child_types(LibModel), 0))
            "#,
            "out",
        );
        let Value::List(items) = &v else {
            panic!("expected list, got {v:?}");
        };
        assert_eq!(items.len(), 2, "{items:?}");
        assert_eq!(record_field(&items[0], "name"), &Value::Utf8("id".into()));
        assert_eq!(record_field(&items[1], "name"), &Value::Utf8("name".into()));
    }

    #[test]
    fn namespace_decls_lists_decls_in_a_namespace_as_resolvable_refs() {
        // Every top-level decl under `namespace lib` is returned as a
        // DataPath whose segments are the FQN; root-namespace decls are
        // excluded.
        let v = eval_field(
            r#"
            namespace lib
            @block("svc") @schemaless
            type Svc { id: utf8 }
            interface Widget { width: utf8? }
            union Color { Red none  Blue none }
            symbol_set Sizes { small  large }
            @schemaless out = namespace_decls("lib")
            "#,
            "out",
        );
        let Value::List(refs) = v else {
            panic!("expected a list, got {v:?}");
        };
        let segs: Vec<Vec<String>> = refs.iter().map(datapath_segments).collect();
        assert_eq!(
            segs,
            vec![
                vec!["lib".to_string(), "Svc".to_string()],
                vec!["lib".to_string(), "Widget".to_string()],
                vec!["lib".to_string(), "Color".to_string()],
                vec!["lib".to_string(), "Sizes".to_string()],
            ],
            "types, then interfaces, unions, symbol sets"
        );
    }

    #[test]
    fn namespace_decls_refs_chain_into_type_fields() {
        let v = eval_field(
            r#"
            namespace lib
            @block("svc") @schemaless
            type Svc { @inline(0) id: utf8  name: utf8 }
            @schemaless out = type_fields(at(namespace_decls("lib"), 0))
            "#,
            "out",
        );
        let Value::List(fields) = &v else {
            panic!("expected list, got {v:?}");
        };
        assert_eq!(record_field(&fields[0], "name"), &Value::Utf8("id".into()));
        assert_eq!(
            record_field(&fields[1], "name"),
            &Value::Utf8("name".into())
        );
    }

    #[test]
    fn namespace_decls_empty_string_targets_root_namespace() {
        // A root-namespace user type is found via `""`; the namespaced
        // twin is not.
        let v = eval_field(
            r#"
            @block("svc") @schemaless
            type RootSvc { id: utf8 }
            @schemaless out = namespace_decls("")
            "#,
            "out",
        );
        let Value::List(refs) = v else {
            panic!("expected a list, got {v:?}");
        };
        let names: Vec<String> = refs
            .iter()
            .map(|r| match r {
                Value::DataPath { segments, .. } => segments.join("."),
                other => panic!("expected data path, got {other:?}"),
            })
            .collect();
        assert!(names.contains(&"RootSvc".to_string()), "{names:?}");
        // A non-matching namespace yields nothing — namespace filtering
        // excludes decls outside the requested namespace.
        assert!(!names.contains(&"lib.RootSvc".to_string()), "{names:?}");
    }

    #[test]
    fn namespace_decls_empty_for_unknown_namespace() {
        let v = eval_field(
            r#"
            @block("svc") @schemaless
            type RootSvc { id: utf8 }
            @schemaless out = namespace_decls("nope")
            "#,
            "out",
        );
        assert_eq!(v, Value::List(std::sync::Arc::new(vec![])));
    }

    #[test]
    fn decl_info_classifies_block_decorator_and_document_types() {
        let block = eval_field(
            r#"
            @block("svc") @schemaless
            type Svc { id: utf8 }
            @schemaless out = decl_info(Svc)
            "#,
            "out",
        );
        assert_eq!(record_field(&block, "name"), &Value::Utf8("Svc".into()));
        assert_eq!(record_field(&block, "kind"), &Value::Utf8("type".into()));
        assert_eq!(
            record_field(&block, "block_kind"),
            &Value::Utf8("svc".into())
        );
        assert_eq!(record_field(&block, "is_document"), &Value::Bool(false));
        assert_eq!(record_field(&block, "is_imported"), &Value::Bool(false));

        let doc = eval_field(
            r#"
            @document
            type MyDoc { note: utf8? }
            @schemaless out = decl_info(MyDoc)
            "#,
            "out",
        );
        assert_eq!(record_field(&doc, "is_document"), &Value::Bool(true));
        assert_eq!(record_field(&doc, "block_kind"), &Value::None);

        let deco = eval_field(
            r#"
            @decorator("doc")
            type DocDec { text: utf8 }
            @schemaless out = decl_info(DocDec)
            "#,
            "out",
        );
        assert_eq!(
            record_field(&deco, "decorator_name"),
            &Value::Utf8("doc".into())
        );
    }

    #[test]
    fn decl_info_reports_extends() {
        let v = eval_field(
            r#"
            interface Base { id: identifier? }
            @block("svc") @schemaless
            type Svc extends Base { id: identifier?  name: utf8 }
            @schemaless out = decl_info(Svc)
            "#,
            "out",
        );
        let Value::List(extends) = record_field(&v, "extends") else {
            panic!("extends not a list");
        };
        assert_eq!(extends.as_ref(), &[Value::Utf8("Base".into())]);
    }

    #[test]
    fn doc_comment_reads_leading_comments_on_type_and_field() {
        let v = eval_field(
            r#"
            # A service definition.
            # Second line.
            @block("svc") @schemaless
            type Svc {
              # the service name
              @inline(0) name: utf8
            }
            @schemaless out = doc_comment(Svc)
            "#,
            "out",
        );
        assert_eq!(v, Value::Utf8("A service definition.\nSecond line.".into()));

        let field = eval_field(
            r#"
            @block("svc") @schemaless
            type Svc {
              # the service name
              @inline(0) name: utf8
            }
            @schemaless out = doc_comment(Svc.name)
            "#,
            "out",
        );
        assert_eq!(field, Value::Utf8("the service name".into()));
    }

    #[test]
    fn doc_comment_empty_when_absent_and_skips_detached_comment() {
        // A comment separated from the declaration by a blank line is not
        // a doc comment.
        let v = eval_field(
            r#"
            # unrelated banner

            @block("svc") @schemaless
            type Svc { id: utf8 }
            @schemaless out = doc_comment(Svc)
            "#,
            "out",
        );
        assert_eq!(v, Value::Utf8(String::new()));
    }
}
