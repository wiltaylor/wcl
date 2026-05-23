//! End-to-end tests for the string and list builtins added in slice 5.

use wcl_lang::{Document, Value};

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
        Value::List(vec![Value::I64(3), Value::I64(2), Value::I64(1)])
    );
    assert_eq!(
        eval("@schemaless result = unique([1, 2, 2, 3, 1])\n"),
        Value::List(vec![Value::I64(1), Value::I64(2), Value::I64(3)])
    );
}

#[test]
fn list_sort_numeric_and_string() {
    assert_eq!(
        eval("@schemaless result = sort([3, 1, 2])\n"),
        Value::List(vec![Value::I64(1), Value::I64(2), Value::I64(3)])
    );
    assert_eq!(
        eval("@schemaless result = sort([\"b\", \"a\", \"c\"])\n"),
        Value::List(vec![
            Value::Utf8("a".into()),
            Value::Utf8("b".into()),
            Value::Utf8("c".into()),
        ])
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
        Value::List(vec![Value::I64(1), Value::I64(2)])
    );
    assert_eq!(
        eval("@schemaless result = drop([1, 2, 3, 4], 2)\n"),
        Value::List(vec![Value::I64(3), Value::I64(4)])
    );
    assert_eq!(
        eval("@schemaless result = list_contains([1, 2, 3], 2)\n"),
        Value::Bool(true)
    );
}
