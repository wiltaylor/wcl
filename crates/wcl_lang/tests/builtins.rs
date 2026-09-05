//! End-to-end tests for the string and list builtins added in slice 5.

use wcl_lang::{ArithmeticFault, Document, Value};

fn eval(src: &str) -> Value {
    let doc = Document::open(src, "test").unwrap();
    doc.get("result")
        .expect("result field")
        .value()
        .expect("eval")
}

#[test]
fn string_split_join_round_trip() {
    assert_eq!(
        eval("@schemaless result = join(split(\"a,b,c\", \",\"), \"-\")\n"),
        Value::Utf8("a-b-c".into())
    );
}

#[test]
fn string_replace_and_case() {
    assert_eq!(
        eval("@schemaless result = replace(\"hello world\", \"world\", \"there\")\n"),
        Value::Utf8("hello there".into())
    );
    assert_eq!(
        eval("@schemaless result = to_upper(\"abc\")\n"),
        Value::Utf8("ABC".into())
    );
    assert_eq!(
        eval("@schemaless result = to_lower(\"AbC\")\n"),
        Value::Utf8("abc".into())
    );
    assert_eq!(
        eval("@schemaless result = trim(\"  hi  \")\n"),
        Value::Utf8("hi".into())
    );
}

#[test]
fn string_contains_and_prefix_suffix() {
    assert_eq!(
        eval("@schemaless result = contains(\"hello\", \"ell\")\n"),
        Value::Bool(true)
    );
    assert_eq!(
        eval("@schemaless result = starts_with(\"hello\", \"he\")\n"),
        Value::Bool(true)
    );
    assert_eq!(
        eval("@schemaless result = ends_with(\"hello\", \"xyz\")\n"),
        Value::Bool(false)
    );
}

#[test]
fn list_reverse_and_unique() {
    assert_eq!(
        eval("@schemaless result = reverse([1, 2, 3])\n"),
        Value::List(std::sync::Arc::new(vec![
            Value::I64(3),
            Value::I64(2),
            Value::I64(1)
        ]))
    );
    assert_eq!(
        eval("@schemaless result = unique([1, 2, 2, 3, 1])\n"),
        Value::List(std::sync::Arc::new(vec![
            Value::I64(1),
            Value::I64(2),
            Value::I64(3)
        ]))
    );
}

#[test]
fn list_sort_numeric_and_string() {
    assert_eq!(
        eval("@schemaless result = sort([3, 1, 2])\n"),
        Value::List(std::sync::Arc::new(vec![
            Value::I64(1),
            Value::I64(2),
            Value::I64(3)
        ]))
    );
    assert_eq!(
        eval("@schemaless result = sort([\"b\", \"a\", \"c\"])\n"),
        Value::List(std::sync::Arc::new(vec![
            Value::Utf8("a".into()),
            Value::Utf8("b".into()),
            Value::Utf8("c".into()),
        ]))
    );
}

#[test]
fn numeric_ordering_preserves_large_integers() {
    for (ty, lo, hi) in [
        ("i64", "9007199254740992", "9007199254740993"),
        ("u64", "18446744073709551614", "18446744073709551615"),
        (
            "i128",
            "170141183460469231731687303715884105726",
            "170141183460469231731687303715884105727",
        ),
        (
            "u128",
            "340282366920938463463374607431768211454",
            "340282366920938463463374607431768211455",
        ),
    ] {
        let low = format!("{lo}{ty}");
        let high = format!("{hi}{ty}");
        let identity = format!("fn (x: {ty}) -> {ty} {{ x }}");
        let expected = eval(&format!("@schemaless result = [{low}, {high}]"));
        for expression in [
            format!("sort([{high}, {low}])"),
            format!("sort_by([{high}, {low}], {identity})"),
        ] {
            assert_eq!(
                eval(&format!("@schemaless result = {expression}")),
                expected
            );
        }
        for (op, values, expected) in [
            ("min_by", format!("[{high}, {low}]"), &low),
            ("max_by", format!("[{low}, {high}]"), &high),
        ] {
            assert_eq!(
                eval(&format!("@schemaless result = {op}({values}, {identity})")),
                eval(&format!("@schemaless result = {expected}")),
            );
        }
    }
}

#[test]
fn numeric_sort_compares_mixed_types_exactly() {
    for (input, expected) in [
        (
            "9007199254740993, 9007199254740992.0",
            "9007199254740992.0, 9007199254740993",
        ),
        (
            "-9007199254740992.0, -9007199254740993",
            "-9007199254740993, -9007199254740992.0",
        ),
        (
            "340282366920938463463374607431768211455u128, -1i128, 0u64",
            "-1i128, 0u64, 340282366920938463463374607431768211455u128",
        ),
        (
            "340282366920938463463374607431768211456.0, 340282366920938463463374607431768211455u128",
            "340282366920938463463374607431768211455u128, 340282366920938463463374607431768211456.0",
        ),
        ("0.5, 0u128, -0.5, -1i128", "-1i128, -0.5, 0u128, 0.5"),
        ("1.0 / 0.0, 0, -1.0 / 0.0", "-1.0 / 0.0, 0, 1.0 / 0.0"),
        ("0.0, -0.0, 0", "0.0, -0.0, 0"),
        ("16777217i64, 16777216.0f32", "16777216.0f32, 16777217i64"),
        (
            "-170141183460469231731687303715884105727i128, -170141183460469231731687303715884105728.0, -170141183460469231731687303715884105727i128 - 1i128",
            "-170141183460469231731687303715884105728.0, -170141183460469231731687303715884105727i128 - 1i128, -170141183460469231731687303715884105727i128",
        ),
    ] {
        assert_eq!(
            eval(&format!("@schemaless result = sort([{input}])")),
            eval(&format!("@schemaless result = [{expected}]")),
            "{input}",
        );
    }
}

#[test]
fn numeric_ordering_rejects_nan() {
    for expression in [
        "sort([sqrt(-1)])".to_string(),
        "sort([1.0, sqrt(-1), 0.0])".to_string(),
        "sort_by([1], fn (x: i64) -> f64 { sqrt(-1) })".to_string(),
        "min_by([1], fn (x: i64) -> f64 { sqrt(-1) })".to_string(),
        "max_by([1], fn (x: i64) -> f64 { sqrt(-1) })".to_string(),
    ] {
        let error = eval_err(&format!("@schemaless result = {expression}"));
        assert!(error.contains("must not be NaN"), "{error}");
    }
}

#[test]
fn repeat_rejects_oversized_output_without_panicking() {
    for expression in [
        "repeat(\"abc\", 9223372036854775807)",
        "repeat(\"a\", 67108865)",
        "repeat(\"é\", 33554433)",
    ] {
        let error = eval_err(&format!("@schemaless result = {expression}"));
        assert!(error.contains("64 MiB limit"), "{error}");
    }
    for expression in [
        "repeat(\"\", 9223372036854775807)",
        "repeat(\"abc\", 0)",
        "repeat(\"abc\", -1)",
    ] {
        assert_eq!(
            eval(&format!("@schemaless result = {expression}")),
            Value::Utf8(String::new())
        );
    }
    assert_eq!(
        eval("@schemaless result = repeat(\"é\", 3)"),
        Value::Utf8("ééé".into())
    );
}

#[test]
fn list_index_of_take_drop_contains() {
    assert_eq!(
        eval("@schemaless result = index_of([10, 20, 30], 20)\n"),
        Value::I64(1)
    );
    assert_eq!(
        eval("@schemaless result = index_of([10, 20, 30], 99)\n"),
        Value::I64(-1)
    );
    assert_eq!(
        eval("@schemaless result = take([1, 2, 3, 4], 2)\n"),
        Value::List(std::sync::Arc::new(vec![Value::I64(1), Value::I64(2)]))
    );
    assert_eq!(
        eval("@schemaless result = drop([1, 2, 3, 4], 2)\n"),
        Value::List(std::sync::Arc::new(vec![Value::I64(3), Value::I64(4)]))
    );
    assert_eq!(
        eval("@schemaless result = list_contains([1, 2, 3], 2)\n"),
        Value::Bool(true)
    );
}

/// Build the exact same value shape that wdoc passes into
/// `sort_connected` at runtime: a list of `Value::Variant`s whose
/// record payloads carry `id: Value::Identifier` and `children:
/// Value::List<...>`. Plus a parallel list of edge records with
/// `source` / `destination: Value::Identifier`. Constructing the
/// values in Rust is the only way to get `Value::Identifier` (bare
/// identifiers can't appear in expression position) without
/// reparsing through the schema pipeline.
fn node(id: &str, children: Vec<Value>) -> Value {
    let mut map = std::collections::BTreeMap::new();
    map.insert("id".to_string(), Value::Identifier(id.to_string()));
    map.insert(
        "children".to_string(),
        Value::List(std::sync::Arc::new(children)),
    );
    Value::Variant {
        union: vec!["Node".into()],
        variant: "N".into(),
        payload: wcl_lang::VariantPayload::Record(std::sync::Arc::new(map)),
    }
}

fn edge(src: &str, dst: &str) -> Value {
    let mut map = std::collections::BTreeMap::new();
    map.insert("source".to_string(), Value::Identifier(src.to_string()));
    map.insert(
        "destination".to_string(),
        Value::Identifier(dst.to_string()),
    );
    Value::Record {
        ty: vec!["Edge".into()],
        fields: std::sync::Arc::new(map),
    }
}

/// `sort_them` is a thin top-level function that just forwards to
/// the `sort_connected` builtin. We invoke it via
/// `Document::call_function` so the test never has to parse the
/// items / edges back into WCL syntax — Rust constructs them
/// directly with the precise `Value::Identifier` shape wdoc emits
/// at runtime. `@schemaless` keeps the binding out of any document
/// type's required-field check.
const SORT_DOC: &str = r#"
@schemaless sort_them = fn(xs: list<i64>, es: list<i64>) -> list<i64> sort_connected(xs, es)
"#;

fn sort_via_doc(items: Vec<Value>, edges: Vec<Value>) -> Vec<Value> {
    let doc = wcl_lang::Document::open(SORT_DOC, "test").expect("parse");
    let result = doc
        .call_function(
            "sort_them",
            &[
                Value::List(std::sync::Arc::new(items)),
                Value::List(std::sync::Arc::new(edges)),
            ],
        )
        .expect("call sort_them");
    match result {
        Value::List(xs) => std::sync::Arc::unwrap_or_clone(xs),
        other => panic!("expected list, got {other:?}"),
    }
}

fn variant_id(v: &Value) -> Option<&str> {
    let wcl_lang::Value::Variant {
        payload: wcl_lang::VariantPayload::Record(map),
        ..
    } = v
    else {
        return None;
    };
    match map.get("id") {
        Some(wcl_lang::Value::Identifier(s)) => Some(s.as_str()),
        _ => None,
    }
}

fn variant_children(v: &Value) -> Option<&[Value]> {
    let wcl_lang::Value::Variant {
        payload: wcl_lang::VariantPayload::Record(map),
        ..
    } = v
    else {
        return None;
    };
    match map.get("children") {
        Some(wcl_lang::Value::List(xs)) => Some(xs),
        _ => None,
    }
}

#[test]
fn sort_connected_flat_groups_adjacent() {
    // a (no edges), b — c (edge). Greedy BFS picks the highest-degree
    // seed first (b/c are tied — tie-broken by original index, so b
    // first), emits its neighbour, then trails the isolated node.
    let items = sort_via_doc(
        vec![node("a", vec![]), node("b", vec![]), node("c", vec![])],
        vec![edge("b", "c")],
    );
    let ids: Vec<&str> = items.iter().filter_map(variant_id).collect();
    assert_eq!(ids, ["b", "c", "a"]);
}

#[test]
fn sort_connected_lifts_cross_subtree_edges() {
    // The edge `deep_a -> deep_b` lives between leaves that are
    // descendants of two different top-level items. Sort_connected
    // must lift this edge to make `outer_a` and `outer_b` adjacent
    // at the top level.
    let items = sort_via_doc(
        vec![
            node("outer_a", vec![node("deep_a", vec![])]),
            node("middle", vec![]),
            node("outer_b", vec![node("deep_b", vec![])]),
        ],
        vec![edge("deep_a", "deep_b")],
    );
    let ids: Vec<&str> = items.iter().filter_map(variant_id).collect();
    // outer_a and outer_b must end up adjacent; middle drops to the end.
    assert!(
        ids == ["outer_a", "outer_b", "middle"] || ids == ["outer_b", "outer_a", "middle"],
        "expected outer_a/outer_b grouped, got {ids:?}"
    );
}

#[test]
fn sort_connected_recurses_into_children() {
    // The outer item has three children; an edge connects c1 to c3
    // inside it. The recursion should reorder its children list.
    let items = sort_via_doc(
        vec![node(
            "outer",
            vec![node("c1", vec![]), node("c2", vec![]), node("c3", vec![])],
        )],
        vec![edge("c1", "c3")],
    );
    let outer = &items[0];
    let children = variant_children(outer).expect("outer children");
    let ids: Vec<&str> = children.iter().filter_map(variant_id).collect();
    // c1 and c3 group together; c2 trails as an isolated node.
    assert_eq!(ids, ["c1", "c3", "c2"]);
}

// ── sum ───────────────────────────────────────────────────────────

#[test]
fn sum_overflow_is_an_error_not_a_panic() {
    // `sum` accumulates in the element's own variant, so a narrow type
    // can run out of room part-way through the list. It phrases the fault
    // exactly as the `+` it stands in for — one wording, one `ArithmeticFault`.
    let msg = eval_err("@schemaless result = sum([127i8, 1i8])\n");
    assert!(
        msg.contains(&format!("{}", ArithmeticFault::overflow("i8"))),
        "{msg}"
    );
}

// ── eval ──────────────────────────────────────────────────────────

/// Error rendering for a `result` field that fails to evaluate.
fn eval_err(src: &str) -> String {
    let doc = Document::open(src, "test").unwrap();
    let e = doc
        .get("result")
        .expect("result field")
        .value()
        .expect_err("expected an evaluation error");
    format!("{e:?}")
}

#[test]
fn eval_parses_and_evaluates_an_expression() {
    assert_eq!(
        eval("@schemaless result = eval(\"1 + 2\")\n"),
        Value::I64(3)
    );
}

#[test]
fn eval_sees_current_scope_locals() {
    // The eval'd expression resolves a surrounding let-binding.
    assert_eq!(
        eval("@schemaless result = { let a = 5; eval(\"a + 1\") }\n"),
        Value::I64(6)
    );
}

#[test]
fn eval_can_return_a_function_value_that_is_then_called() {
    assert_eq!(
        eval("@schemaless result = { let f = eval(\"fn(x: i64) -> i64 x * 2\"); f(21) }\n"),
        Value::I64(42)
    );
}

#[test]
fn eval_surfaces_a_parse_error() {
    let msg = eval_err("@schemaless result = eval(\"1 +\")\n");
    assert!(msg.contains("eval"), "{msg}");
}

// ── ast_string ────────────────────────────────────────────────────

#[test]
fn ast_string_renders_a_type_declaration() {
    let v = eval("@block(\"svc\") type Svc { id: utf8 }\n@schemaless result = ast_string(Svc)\n");
    assert_eq!(
        v,
        Value::Utf8("@block(\"svc\")\ntype Svc {\n  id: utf8\n}".into())
    );
}

#[test]
fn ast_string_normalizes_messy_source() {
    // Input is intentionally mis-formatted (odd spacing/indentation); the
    // output is canonical.
    let v = eval(
        "type    Messy {\n      a:   utf8\n   b:  i64?\n}\n@schemaless result = ast_string(Messy)\n",
    );
    assert_eq!(
        v,
        Value::Utf8("type Messy {\n  a: utf8\n  b: i64?\n}".into())
    );
}

#[test]
fn ast_string_renders_a_union_declaration() {
    let v = eval("union Color { Red none  Blue none }\n@schemaless result = ast_string(Color)\n");
    assert_eq!(
        v,
        Value::Utf8("union Color {\n  Red none\n  Blue none\n}".into())
    );
}

#[test]
fn ast_string_renders_a_function_value() {
    let v = eval("@schemaless result = ast_string(fn(x: i64) -> i64 x * 2)\n");
    assert_eq!(v, Value::Utf8("fn(x: i64) -> i64 x * 2".into()));
}

#[test]
fn ast_string_rejects_a_non_reference_scalar() {
    let msg = eval_err("@schemaless result = ast_string(42)\n");
    assert!(msg.contains("data path"), "{msg}");
}

// ── fn_signature ──────────────────────────────────────────────────

/// Read a named field out of a `Value::Record`.
fn rfield<'v>(rec: &'v Value, key: &str) -> &'v Value {
    match rec {
        Value::Record { fields, .. } => fields.get(key).expect("record has key"),
        other => panic!("expected record, got {other:?}"),
    }
}

#[test]
fn fn_signature_describes_a_user_function() {
    let v = eval(
        "@schemaless f = fn(x: i64, y: utf8) -> bool true\n@schemaless result = fn_signature(f)\n",
    );
    assert_eq!(rfield(&v, "is_builtin"), &Value::Bool(false));
    assert_eq!(rfield(&v, "doc"), &Value::Utf8("".into()));
    assert_eq!(rfield(&v, "return_type"), &Value::Utf8("bool".into()));
    // A bare function value carries no return description.
    assert_eq!(rfield(&v, "return_doc"), &Value::Utf8("".into()));
    assert_eq!(
        rfield(&v, "signature"),
        &Value::Utf8("fn(x: i64, y: utf8) -> bool".into())
    );
    let Value::List(params) = rfield(&v, "params") else {
        panic!("params not a list");
    };
    assert_eq!(params.len(), 2);
    assert_eq!(rfield(&params[0], "name"), &Value::Utf8("x".into()));
    assert_eq!(rfield(&params[0], "type"), &Value::Utf8("i64".into()));
    assert_eq!(rfield(&params[0], "doc"), &Value::Utf8("".into()));
    assert_eq!(rfield(&params[1], "name"), &Value::Utf8("y".into()));
    assert_eq!(rfield(&params[1], "type"), &Value::Utf8("utf8".into()));
}

#[test]
fn fn_signature_describes_a_builtin_by_name() {
    let v = eval("@schemaless result = fn_signature(\"concat\")\n");
    assert_eq!(rfield(&v, "is_builtin"), &Value::Bool(true));
    assert_eq!(rfield(&v, "return_type"), &Value::Utf8("utf8".into()));
    // Signature is derived from the structured params (carries names).
    assert_eq!(
        rfield(&v, "signature"),
        &Value::Utf8("fn(a: utf8, b: utf8) -> utf8".into())
    );
    // Function-level help text is present.
    let Value::Utf8(doc) = rfield(&v, "doc") else {
        panic!("doc not a string");
    };
    assert!(doc.contains("Concatenate"), "{doc}");
    // The return value carries a description.
    let Value::Utf8(rdoc) = rfield(&v, "return_doc") else {
        panic!("return_doc not a string");
    };
    assert!(!rdoc.is_empty(), "return_doc should not be empty");
    // Each parameter carries name / type / help text.
    let Value::List(params) = rfield(&v, "params") else {
        panic!("params not a list");
    };
    assert_eq!(params.len(), 2);
    assert_eq!(rfield(&params[0], "name"), &Value::Utf8("a".into()));
    assert_eq!(rfield(&params[0], "type"), &Value::Utf8("utf8".into()));
    let Value::Utf8(pdoc) = rfield(&params[0], "doc") else {
        panic!("param doc not a string");
    };
    assert!(!pdoc.is_empty(), "param doc should not be empty");
}

#[test]
fn fn_signature_rejects_unknown_builtin_and_non_function() {
    let msg = eval_err("@schemaless result = fn_signature(\"not_a_builtin\")\n");
    assert!(msg.contains("not a built-in"), "{msg}");
    let msg2 = eval_err("@schemaless result = fn_signature(42)\n");
    assert!(msg2.contains("function value or a built-in name"), "{msg2}");
}

// ── builtin_names ─────────────────────────────────────────────────

#[test]
fn builtin_names_lists_registered_builtins_sorted() {
    let v = eval("@schemaless result = builtin_names()\n");
    let Value::List(items) = v else {
        panic!("expected a list, got {v:?}");
    };
    let names: Vec<String> = items
        .iter()
        .map(|x| match x {
            Value::Utf8(s) => s.clone(),
            other => panic!("expected utf8 name, got {other:?}"),
        })
        .collect();
    // A representative sample across the builtin families is present.
    for expected in ["map", "concat", "sin", "fn_signature", "builtin_names"] {
        assert!(
            names.iter().any(|n| n == expected),
            "missing {expected}: {names:?}"
        );
    }
    // Sorted and de-duplicated by the registry's map keys.
    let mut sorted = names.clone();
    sorted.sort();
    assert_eq!(names, sorted, "names should be sorted");
}

// ── record builtins: keys / values / merge / map_values ──────────

#[test]
fn record_keys_and_values_in_sorted_key_order() {
    assert_eq!(
        eval("@schemaless result = keys({ b: 2, a: 1 })\n"),
        Value::List(std::sync::Arc::new(vec![
            Value::Utf8("a".into()),
            Value::Utf8("b".into())
        ]))
    );
    assert_eq!(
        eval("@schemaless result = values({ b: 2, a: 1 })\n"),
        Value::List(std::sync::Arc::new(vec![Value::I64(1), Value::I64(2)]))
    );
}

#[test]
fn record_keys_works_on_variant_record_payloads() {
    let src = "union Shape { Circle { r: f64 } }\n\
               @schemaless result = keys(Shape::Circle { r: 2.0 })\n";
    assert_eq!(
        eval(src),
        Value::List(std::sync::Arc::new(vec![Value::Utf8("r".into())]))
    );
}

#[test]
fn record_merge_second_record_wins_on_clash() {
    assert_eq!(
        eval("@schemaless result = values(merge({ a: 1, b: 2 }, { b: 9, c: 3 }))\n"),
        Value::List(std::sync::Arc::new(vec![
            Value::I64(1),
            Value::I64(9),
            Value::I64(3)
        ]))
    );
}

#[test]
fn record_map_values_keeps_keys_and_transforms_values() {
    let src = "@schemaless result = map_values({ a: 1, b: 2 }, fn (x: i64) -> i64 { x * 10 })\n";
    let v = eval(src);
    let Value::Record { fields, .. } = v else {
        panic!("expected a record, got {v:?}");
    };
    assert_eq!(fields.get("a"), Some(&Value::I64(10)));
    assert_eq!(fields.get("b"), Some(&Value::I64(20)));
}

#[test]
fn record_builtins_reject_non_records() {
    let msg = eval_err("@schemaless result = keys([1, 2])\n");
    assert!(msg.contains("expected a record"), "{msg}");
    let msg = eval_err("@schemaless result = merge({ a: 1 }, 5)\n");
    assert!(msg.contains("second argument must be a record"), "{msg}");
    let msg = eval_err("@schemaless result = map_values(7, fn (x: i64) -> i64 { x })\n");
    assert!(msg.contains("must be a record"), "{msg}");
}

// ── predicate forms: any / all / find / sort_by / min_by / max_by ─

#[test]
fn any_all_short_circuit_semantics() {
    assert_eq!(
        eval("@schemaless result = any([1, 2, 3], fn (x: i64) -> bool { x > 2 })\n"),
        Value::Bool(true)
    );
    assert_eq!(
        eval("@schemaless result = any([], fn (x: i64) -> bool { x > 2 })\n"),
        Value::Bool(false)
    );
    assert_eq!(
        eval("@schemaless result = all([1, 2], fn (x: i64) -> bool { x > 0 })\n"),
        Value::Bool(true)
    );
    assert_eq!(
        eval("@schemaless result = all([], fn (x: i64) -> bool { x > 0 })\n"),
        Value::Bool(true)
    );
}

#[test]
fn find_returns_first_match_or_none() {
    assert_eq!(
        eval("@schemaless result = find([1, 2, 3], fn (x: i64) -> bool { x > 1 })\n"),
        Value::I64(2)
    );
    assert_eq!(
        eval("@schemaless result = find([1, 2], fn (x: i64) -> bool { x > 9 })\n"),
        Value::None
    );
}

#[test]
fn sort_by_orders_by_key_function() {
    assert_eq!(
        eval(
            "@schemaless result = sort_by([\"bb\", \"a\", \"ccc\"], fn (s: utf8) -> i64 { len(s) })\n"
        ),
        Value::List(std::sync::Arc::new(vec![
            Value::Utf8("a".into()),
            Value::Utf8("bb".into()),
            Value::Utf8("ccc".into()),
        ]))
    );
}

#[test]
fn min_by_max_by_pick_extremes_or_none_when_empty() {
    assert_eq!(
        eval("@schemaless result = min_by([3, 1, 2], fn (x: i64) -> i64 { x })\n"),
        Value::I64(1)
    );
    assert_eq!(
        eval("@schemaless result = max_by([3, 1, 2], fn (x: i64) -> i64 { x })\n"),
        Value::I64(3)
    );
    assert_eq!(
        eval("@schemaless result = max_by([], fn (x: i64) -> i64 { x })\n"),
        Value::None
    );
}

#[test]
fn group_by_groups_in_first_seen_key_order() {
    let v = eval(
        "@schemaless result = group_by([1, 2, 3, 4], fn (x: i64) -> utf8 { if x % 2 == 0 { \"even\" } else { \"odd\" } })\n",
    );
    let Value::List(groups) = v else {
        panic!("expected a list, got {v:?}");
    };
    assert_eq!(groups.len(), 2);
    let Value::Record { fields, .. } = &groups[0] else {
        panic!("expected a record group");
    };
    assert_eq!(fields.get("key"), Some(&Value::Utf8("odd".into())));
    assert_eq!(
        fields.get("items"),
        Some(&Value::List(std::sync::Arc::new(vec![
            Value::I64(1),
            Value::I64(3)
        ])))
    );
}

#[test]
fn predicate_must_return_bool() {
    let msg = eval_err("@schemaless result = any([1], fn (x: i64) -> i64 { x })\n");
    assert!(msg.contains("predicate must return bool"), "{msg}");
}

// ── enumerate / slice / string shapers ────────────────────────────

#[test]
fn enumerate_pairs_index_and_element() {
    assert_eq!(
        eval("@schemaless result = enumerate([\"a\", \"b\"])\n"),
        Value::List(std::sync::Arc::new(vec![
            Value::List(std::sync::Arc::new(vec![
                Value::I64(0),
                Value::Utf8("a".into())
            ])),
            Value::List(std::sync::Arc::new(vec![
                Value::I64(1),
                Value::Utf8("b".into()),
            ])),
        ]))
    );
}

#[test]
fn slice_clamps_and_works_on_strings_and_lists() {
    assert_eq!(
        eval("@schemaless result = slice(\"hello\", 1, 4)\n"),
        Value::Utf8("ell".into())
    );
    assert_eq!(
        eval("@schemaless result = slice([1, 2, 3, 4], 1, 3)\n"),
        Value::List(std::sync::Arc::new(vec![Value::I64(2), Value::I64(3)]))
    );
    // Out-of-range bounds clamp instead of erroring.
    assert_eq!(
        eval("@schemaless result = slice(\"hi\", 0, 99)\n"),
        Value::Utf8("hi".into())
    );
    assert_eq!(
        eval("@schemaless result = slice(\"hi\", 5, 2)\n"),
        Value::Utf8("".into())
    );
}

#[test]
fn chars_repeat_and_padding() {
    assert_eq!(
        eval("@schemaless result = chars(\"hi\")\n"),
        Value::List(std::sync::Arc::new(vec![
            Value::Utf8("h".into()),
            Value::Utf8("i".into())
        ]))
    );
    assert_eq!(
        eval("@schemaless result = repeat(\"ab\", 3)\n"),
        Value::Utf8("ababab".into())
    );
    assert_eq!(
        eval("@schemaless result = pad_start(\"7\", 3, \"0\")\n"),
        Value::Utf8("007".into())
    );
    assert_eq!(
        eval("@schemaless result = pad_end(\"x\", 4, \"-\")\n"),
        Value::Utf8("x---".into())
    );
    // Already wide enough: unchanged.
    assert_eq!(
        eval("@schemaless result = pad_start(\"abcd\", 2, \"0\")\n"),
        Value::Utf8("abcd".into())
    );
}

#[test]
fn path_and_glob_builtins() {
    assert_eq!(
        eval("@schemaless result = path_contains(\"src/\", \"src/core/mod.rs\")\n"),
        Value::Bool(true)
    );
    assert_eq!(
        eval("@schemaless result = path_contains(\"src/\", \"src2/x\")\n"),
        Value::Bool(false)
    );
    assert_eq!(
        eval("@schemaless result = glob_match(\"src/*.rs\", \"src/main.rs\")\n"),
        Value::Bool(true)
    );
    assert_eq!(
        eval("@schemaless result = glob_match(\"src/*.rs\", \"src/sub/mod.rs\")\n"),
        Value::Bool(false)
    );
    assert_eq!(
        eval("@schemaless result = glob_overlaps(\"src/*.rs\", \"src/main.rs\")\n"),
        Value::Bool(true)
    );
    assert_eq!(
        eval("@schemaless result = glob_overlaps(\"src/\", \"src/core/\")\n"),
        Value::Bool(true)
    );
    assert_eq!(
        eval("@schemaless result = glob_overlaps(\"src/*.rs\", \"docs/*.md\")\n"),
        Value::Bool(false)
    );
}
