// Integration tests for wcl_eval — expression evaluation, variable scoping,
// built-in functions, and block evaluation.

use indexmap::IndexMap;

// ---------------------------------------------------------------------------
// Helper
// ---------------------------------------------------------------------------

/// Parse and evaluate a WCL source string, returning the top-level attribute
/// map. Let-bindings are intentionally excluded from the output (they are
/// file-private); only `Attribute`, `BlockChild`, and `export let` entries
/// appear.
fn eval(source: &str) -> IndexMap<String, wcl_eval::Value> {
    let file_id = wcl_core::FileId(0);
    let (ast, diags) = wcl_core::parse(source, file_id);
    for d in diags.diagnostics() {
        eprintln!("parse: {}", d.message);
    }
    let mut evaluator = wcl_eval::Evaluator::new();
    evaluator.evaluate(&ast)
}

/// Convenience: evaluate and grab a single attribute value, panicking if
/// it is absent.
fn eval_attr(source: &str, name: &str) -> wcl_eval::Value {
    let out = eval(source);
    out.get(name)
        .unwrap_or_else(|| panic!("attribute '{}' not found in output; got: {:?}", name, out.keys().collect::<Vec<_>>()))
        .clone()
}

// ---------------------------------------------------------------------------
// Expression evaluation — integer arithmetic
// ---------------------------------------------------------------------------

#[test]
fn expr_int_add() {
    assert_eq!(eval_attr("x = 1 + 2", "x"), wcl_eval::Value::Int(3));
}

#[test]
fn expr_int_sub() {
    assert_eq!(eval_attr("x = 10 - 3", "x"), wcl_eval::Value::Int(7));
}

#[test]
fn expr_int_mul() {
    assert_eq!(eval_attr("x = 4 * 5", "x"), wcl_eval::Value::Int(20));
}

#[test]
fn expr_int_div() {
    // Integer division truncates
    assert_eq!(eval_attr("x = 10 / 3", "x"), wcl_eval::Value::Int(3));
}

#[test]
fn expr_int_mod() {
    assert_eq!(eval_attr("x = 10 % 3", "x"), wcl_eval::Value::Int(1));
}

#[test]
fn expr_int_div_exact() {
    assert_eq!(eval_attr("x = 12 / 4", "x"), wcl_eval::Value::Int(3));
}

// ---------------------------------------------------------------------------
// Expression evaluation — float arithmetic
// ---------------------------------------------------------------------------

#[test]
fn expr_float_add() {
    assert_eq!(
        eval_attr("x = 1.5 + 2.5", "x"),
        wcl_eval::Value::Float(4.0)
    );
}

#[test]
fn expr_float_mul() {
    assert_eq!(
        eval_attr("x = 3.14 * 2.0", "x"),
        wcl_eval::Value::Float(6.28)
    );
}

// ---------------------------------------------------------------------------
// Expression evaluation — int/float promotion
// ---------------------------------------------------------------------------

#[test]
fn expr_int_float_promotion_add() {
    // 1 + 2.0 should promote to float
    assert_eq!(
        eval_attr("x = 1 + 2.0", "x"),
        wcl_eval::Value::Float(3.0)
    );
}

#[test]
fn expr_float_int_promotion_add() {
    assert_eq!(
        eval_attr("x = 2.0 + 1", "x"),
        wcl_eval::Value::Float(3.0)
    );
}

#[test]
fn expr_int_float_promotion_mul() {
    assert_eq!(
        eval_attr("x = 3 * 2.0", "x"),
        wcl_eval::Value::Float(6.0)
    );
}

// ---------------------------------------------------------------------------
// Expression evaluation — string concatenation
// ---------------------------------------------------------------------------

#[test]
fn expr_string_concat() {
    assert_eq!(
        eval_attr(r#"x = "hello" + " " + "world""#, "x"),
        wcl_eval::Value::String("hello world".to_string())
    );
}

#[test]
fn expr_string_concat_empty() {
    assert_eq!(
        eval_attr(r#"x = "" + "abc""#, "x"),
        wcl_eval::Value::String("abc".to_string())
    );
}

// ---------------------------------------------------------------------------
// Expression evaluation — comparison operators
// ---------------------------------------------------------------------------

#[test]
fn expr_cmp_lt_true() {
    assert_eq!(eval_attr("x = 1 < 2", "x"), wcl_eval::Value::Bool(true));
}

#[test]
fn expr_cmp_lt_false() {
    assert_eq!(eval_attr("x = 2 < 1", "x"), wcl_eval::Value::Bool(false));
}

#[test]
fn expr_cmp_gt_true() {
    assert_eq!(eval_attr("x = 2 > 1", "x"), wcl_eval::Value::Bool(true));
}

#[test]
fn expr_cmp_gt_false() {
    assert_eq!(eval_attr("x = 1 > 2", "x"), wcl_eval::Value::Bool(false));
}

#[test]
fn expr_cmp_lte_equal() {
    assert_eq!(eval_attr("x = 1 <= 1", "x"), wcl_eval::Value::Bool(true));
}

#[test]
fn expr_cmp_lte_less() {
    assert_eq!(eval_attr("x = 0 <= 1", "x"), wcl_eval::Value::Bool(true));
}

#[test]
fn expr_cmp_lte_greater() {
    assert_eq!(eval_attr("x = 2 <= 1", "x"), wcl_eval::Value::Bool(false));
}

#[test]
fn expr_cmp_gte_equal() {
    assert_eq!(eval_attr("x = 2 >= 2", "x"), wcl_eval::Value::Bool(true));
}

#[test]
fn expr_cmp_gte_greater() {
    assert_eq!(eval_attr("x = 3 >= 2", "x"), wcl_eval::Value::Bool(true));
}

#[test]
fn expr_cmp_eq_true() {
    assert_eq!(eval_attr("x = 1 == 1", "x"), wcl_eval::Value::Bool(true));
}

#[test]
fn expr_cmp_eq_false() {
    assert_eq!(eval_attr("x = 1 == 2", "x"), wcl_eval::Value::Bool(false));
}

#[test]
fn expr_cmp_neq_true() {
    assert_eq!(eval_attr("x = 1 != 2", "x"), wcl_eval::Value::Bool(true));
}

#[test]
fn expr_cmp_neq_false() {
    assert_eq!(eval_attr("x = 1 != 1", "x"), wcl_eval::Value::Bool(false));
}

#[test]
fn expr_cmp_string_lt() {
    assert_eq!(
        eval_attr(r#"x = "apple" < "banana""#, "x"),
        wcl_eval::Value::Bool(true)
    );
}

// ---------------------------------------------------------------------------
// Expression evaluation — boolean operators
// ---------------------------------------------------------------------------

#[test]
fn expr_bool_and_true_false() {
    assert_eq!(
        eval_attr("x = true && false", "x"),
        wcl_eval::Value::Bool(false)
    );
}

#[test]
fn expr_bool_and_true_true() {
    assert_eq!(
        eval_attr("x = true && true", "x"),
        wcl_eval::Value::Bool(true)
    );
}

#[test]
fn expr_bool_or_true_false() {
    assert_eq!(
        eval_attr("x = true || false", "x"),
        wcl_eval::Value::Bool(true)
    );
}

#[test]
fn expr_bool_or_false_false() {
    assert_eq!(
        eval_attr("x = false || false", "x"),
        wcl_eval::Value::Bool(false)
    );
}

#[test]
fn expr_bool_not_true() {
    assert_eq!(eval_attr("x = !true", "x"), wcl_eval::Value::Bool(false));
}

#[test]
fn expr_bool_not_false() {
    assert_eq!(eval_attr("x = !false", "x"), wcl_eval::Value::Bool(true));
}

// ---------------------------------------------------------------------------
// Expression evaluation — short-circuit operators
// ---------------------------------------------------------------------------

#[test]
fn expr_short_circuit_or_does_not_eval_rhs() {
    // true || (1 / 0) — RHS should never be evaluated
    let out = eval("x = true || (1 / 0)");
    assert_eq!(out.get("x"), Some(&wcl_eval::Value::Bool(true)));
}

#[test]
fn expr_short_circuit_and_does_not_eval_rhs() {
    // false && (1 / 0) — RHS should never be evaluated
    let out = eval("x = false && (1 / 0)");
    assert_eq!(out.get("x"), Some(&wcl_eval::Value::Bool(false)));
}

// ---------------------------------------------------------------------------
// Expression evaluation — ternary
// ---------------------------------------------------------------------------

#[test]
fn expr_ternary_true_branch() {
    assert_eq!(eval_attr("x = true ? 1 : 2", "x"), wcl_eval::Value::Int(1));
}

#[test]
fn expr_ternary_false_branch() {
    assert_eq!(
        eval_attr("x = false ? 1 : 2", "x"),
        wcl_eval::Value::Int(2)
    );
}

#[test]
fn expr_ternary_with_expression_condition() {
    assert_eq!(
        eval_attr("x = (3 > 2) ? 100 : 200", "x"),
        wcl_eval::Value::Int(100)
    );
}

// ---------------------------------------------------------------------------
// Expression evaluation — unary minus
// ---------------------------------------------------------------------------

#[test]
fn expr_unary_neg_int() {
    assert_eq!(eval_attr("x = -5", "x"), wcl_eval::Value::Int(-5));
}

#[test]
fn expr_unary_neg_expr() {
    assert_eq!(
        eval_attr("x = -(3 + 2)", "x"),
        wcl_eval::Value::Int(-5)
    );
}

#[test]
fn expr_unary_neg_float() {
    assert_eq!(
        eval_attr("x = -3.14", "x"),
        wcl_eval::Value::Float(-3.14)
    );
}

// ---------------------------------------------------------------------------
// Expression evaluation — string interpolation
// ---------------------------------------------------------------------------

#[test]
fn expr_string_interp_int() {
    let src = r#"
let x = 5
msg = "x is ${x}"
"#;
    assert_eq!(
        eval_attr(src, "msg"),
        wcl_eval::Value::String("x is 5".to_string())
    );
}

#[test]
fn expr_string_interp_multiple() {
    let src = r#"
let a = 1
let b = 2
msg = "${a} + ${b}"
"#;
    assert_eq!(
        eval_attr(src, "msg"),
        wcl_eval::Value::String("1 + 2".to_string())
    );
}

#[test]
fn expr_string_interp_expression() {
    let src = r#"msg = "result is ${1 + 2}""#;
    assert_eq!(
        eval_attr(src, "msg"),
        wcl_eval::Value::String("result is 3".to_string())
    );
}

// ---------------------------------------------------------------------------
// Expression evaluation — list literals
// ---------------------------------------------------------------------------

#[test]
fn expr_list_literal() {
    let v = eval_attr("x = [1, 2, 3]", "x");
    assert_eq!(
        v,
        wcl_eval::Value::List(vec![
            wcl_eval::Value::Int(1),
            wcl_eval::Value::Int(2),
            wcl_eval::Value::Int(3),
        ])
    );
}

#[test]
fn expr_list_empty() {
    let v = eval_attr("x = []", "x");
    assert_eq!(v, wcl_eval::Value::List(vec![]));
}

#[test]
fn expr_list_mixed_types() {
    let v = eval_attr(r#"x = [1, "two", true]"#, "x");
    assert_eq!(
        v,
        wcl_eval::Value::List(vec![
            wcl_eval::Value::Int(1),
            wcl_eval::Value::String("two".to_string()),
            wcl_eval::Value::Bool(true),
        ])
    );
}

// ---------------------------------------------------------------------------
// Expression evaluation — map literals
// ---------------------------------------------------------------------------

#[test]
fn expr_map_literal() {
    let v = eval_attr("x = { a = 1, b = 2 }", "x");
    let map = v.as_map().expect("expected map");
    assert_eq!(map.get("a"), Some(&wcl_eval::Value::Int(1)));
    assert_eq!(map.get("b"), Some(&wcl_eval::Value::Int(2)));
    assert_eq!(map.len(), 2);
}

#[test]
fn expr_map_empty() {
    let v = eval_attr("x = {}", "x");
    let map = v.as_map().expect("expected map");
    assert!(map.is_empty());
}

#[test]
fn expr_map_string_values() {
    let v = eval_attr(r#"x = { name = "alice", city = "sf" }"#, "x");
    let map = v.as_map().expect("expected map");
    assert_eq!(
        map.get("name"),
        Some(&wcl_eval::Value::String("alice".to_string()))
    );
    assert_eq!(
        map.get("city"),
        Some(&wcl_eval::Value::String("sf".to_string()))
    );
}

// ---------------------------------------------------------------------------
// Expression evaluation — operator precedence and parentheses
// ---------------------------------------------------------------------------

#[test]
fn expr_precedence_mul_over_add() {
    // 2 + 3 * 4 = 2 + 12 = 14
    assert_eq!(eval_attr("x = 2 + 3 * 4", "x"), wcl_eval::Value::Int(14));
}

#[test]
fn expr_parens_override_precedence() {
    // (2 + 3) * 4 = 20
    assert_eq!(
        eval_attr("x = (2 + 3) * 4", "x"),
        wcl_eval::Value::Int(20)
    );
}

#[test]
fn expr_nested_parens() {
    // (1 + 2) * (3 + 4) = 3 * 7 = 21
    assert_eq!(
        eval_attr("x = (1 + 2) * (3 + 4)", "x"),
        wcl_eval::Value::Int(21)
    );
}

#[test]
fn expr_complex_arithmetic() {
    // 10 - 2 * 3 + 1 = 10 - 6 + 1 = 5
    assert_eq!(
        eval_attr("x = 10 - 2 * 3 + 1", "x"),
        wcl_eval::Value::Int(5)
    );
}

// ---------------------------------------------------------------------------
// Variable scoping — let bindings
// ---------------------------------------------------------------------------

#[test]
fn scope_let_binding_not_in_output() {
    // let bindings are private — they should NOT appear in the output map
    let out = eval("let x = 5");
    assert!(!out.contains_key("x"), "let binding should be private");
}

#[test]
fn scope_let_binding_usable_in_attribute() {
    let src = r#"
let x = 5
y = x + 1
"#;
    assert_eq!(eval_attr(src, "y"), wcl_eval::Value::Int(6));
}

#[test]
fn scope_let_binding_chain() {
    let src = r#"
let a = 10
let b = a * 2
result = b + 5
"#;
    assert_eq!(eval_attr(src, "result"), wcl_eval::Value::Int(25));
}

#[test]
fn scope_forward_reference_topo_sort() {
    // y references x which is declared after y — topo sort should handle this
    let src = r#"
y = x + 1
x = 5
"#;
    assert_eq!(eval_attr(src, "y"), wcl_eval::Value::Int(6));
    assert_eq!(eval_attr(src, "x"), wcl_eval::Value::Int(5));
}

#[test]
fn scope_multiple_attributes_independent() {
    let src = r#"
a = 1
b = 2
c = 3
"#;
    let out = eval(src);
    assert_eq!(out["a"], wcl_eval::Value::Int(1));
    assert_eq!(out["b"], wcl_eval::Value::Int(2));
    assert_eq!(out["c"], wcl_eval::Value::Int(3));
}

#[test]
fn scope_attribute_depends_on_multiple_lets() {
    let src = r#"
let base = 100
let offset = 5
result = base + offset
"#;
    assert_eq!(eval_attr(src, "result"), wcl_eval::Value::Int(105));
}

// ---------------------------------------------------------------------------
// Block evaluation
// ---------------------------------------------------------------------------

#[test]
fn block_simple_no_error() {
    // Evaluating a block with no inline id should not error
    let src = r#"
server {
  port = 8080
}
"#;
    // Blocks don't appear in output currently (value is not set), but
    // evaluation should not panic or produce diagnostics
    let out = eval(src);
    // The block is registered under __block_server; value is None so absent
    let _ = out; // just verify no panic
}

#[test]
fn block_inline_id_no_error() {
    let src = r#"
server "my-srv" {
  port = 8080
}
"#;
    // Should parse and evaluate without error
    let _ = eval(src);
}

#[test]
fn block_attributes_independent_of_output() {
    // Attributes defined outside blocks still appear in output
    let src = r#"
global_port = 9000
server "web" {
  port = 8080
}
"#;
    let out = eval(src);
    assert_eq!(out["global_port"], wcl_eval::Value::Int(9000));
}

#[test]
fn block_referencing_parent_scope_let() {
    // Inside a block, attributes can reference top-level lets
    let src = r#"
let default_port = 8080
top_val = default_port
server "web" {
  port = default_port
}
"#;
    let out = eval(src);
    // top_val is an attribute so should appear
    assert_eq!(out["top_val"], wcl_eval::Value::Int(8080));
}

#[test]
fn block_nested_no_error() {
    let src = r#"
server "api-gateway" {
  host = "0.0.0.0"
  port = 443

  upstream "backend" {
    host = "10.0.1.1"
    port = 8080
  }
}
"#;
    let _ = eval(src);
}

#[test]
fn block_multiple_same_kind() {
    // Multiple blocks of the same kind with different inline ids
    let src = r#"
region = "us-east"
server "web-1" {
  port = 8080
}
server "web-2" {
  port = 8081
}
"#;
    let out = eval(src);
    assert_eq!(out["region"], wcl_eval::Value::String("us-east".to_string()));
}

#[test]
fn block_attribute_referencing_sibling_let_in_parent() {
    let src = r#"
let timeout = 30
visible = timeout * 2
service "api" {
  timeout_ms = timeout * 1000
}
"#;
    let out = eval(src);
    assert_eq!(out["visible"], wcl_eval::Value::Int(60));
}

// ---------------------------------------------------------------------------
// Built-in functions — string
// ---------------------------------------------------------------------------

#[test]
fn builtin_upper() {
    assert_eq!(
        eval_attr(r#"x = upper("hello")"#, "x"),
        wcl_eval::Value::String("HELLO".to_string())
    );
}

#[test]
fn builtin_lower() {
    assert_eq!(
        eval_attr(r#"x = lower("HELLO")"#, "x"),
        wcl_eval::Value::String("hello".to_string())
    );
}

#[test]
fn builtin_trim() {
    assert_eq!(
        eval_attr(r#"x = trim("  hi  ")"#, "x"),
        wcl_eval::Value::String("hi".to_string())
    );
}

#[test]
fn builtin_trim_tabs_newlines() {
    assert_eq!(
        eval_attr("x = trim(\"\t\nfoo\n\")", "x"),
        wcl_eval::Value::String("foo".to_string())
    );
}

#[test]
fn builtin_length() {
    assert_eq!(
        eval_attr(r#"x = length("hello")"#, "x"),
        wcl_eval::Value::Int(5)
    );
}

#[test]
fn builtin_length_empty() {
    assert_eq!(
        eval_attr(r#"x = length("")"#, "x"),
        wcl_eval::Value::Int(0)
    );
}

#[test]
fn builtin_contains_string_true() {
    assert_eq!(
        eval_attr(r#"x = contains("hello", "ell")"#, "x"),
        wcl_eval::Value::Bool(true)
    );
}

#[test]
fn builtin_contains_string_false() {
    assert_eq!(
        eval_attr(r#"x = contains("hello", "xyz")"#, "x"),
        wcl_eval::Value::Bool(false)
    );
}

#[test]
fn builtin_starts_with_true() {
    assert_eq!(
        eval_attr(r#"x = starts_with("hello", "he")"#, "x"),
        wcl_eval::Value::Bool(true)
    );
}

#[test]
fn builtin_starts_with_false() {
    assert_eq!(
        eval_attr(r#"x = starts_with("hello", "lo")"#, "x"),
        wcl_eval::Value::Bool(false)
    );
}

#[test]
fn builtin_ends_with_true() {
    assert_eq!(
        eval_attr(r#"x = ends_with("hello", "lo")"#, "x"),
        wcl_eval::Value::Bool(true)
    );
}

#[test]
fn builtin_ends_with_false() {
    assert_eq!(
        eval_attr(r#"x = ends_with("hello", "he")"#, "x"),
        wcl_eval::Value::Bool(false)
    );
}

#[test]
fn builtin_replace() {
    assert_eq!(
        eval_attr(r#"x = replace("hello", "l", "r")"#, "x"),
        wcl_eval::Value::String("herro".to_string())
    );
}

#[test]
fn builtin_replace_no_match() {
    assert_eq!(
        eval_attr(r#"x = replace("hello", "z", "r")"#, "x"),
        wcl_eval::Value::String("hello".to_string())
    );
}

#[test]
fn builtin_split() {
    // split(separator, string) — separator is first arg
    let v = eval_attr(r#"x = split(",", "a,b,c")"#, "x");
    assert_eq!(
        v,
        wcl_eval::Value::List(vec![
            wcl_eval::Value::String("a".to_string()),
            wcl_eval::Value::String("b".to_string()),
            wcl_eval::Value::String("c".to_string()),
        ])
    );
}

#[test]
fn builtin_split_no_sep() {
    // splitting a string that doesn't contain the separator returns a one-element list
    let v = eval_attr(r#"x = split(",", "hello")"#, "x");
    assert_eq!(
        v,
        wcl_eval::Value::List(vec![wcl_eval::Value::String("hello".to_string())])
    );
}

#[test]
fn builtin_join() {
    // join(separator, list) — separator is first arg
    let src = r#"x = join(",", ["a", "b", "c"])"#;
    assert_eq!(
        eval_attr(src, "x"),
        wcl_eval::Value::String("a,b,c".to_string())
    );
}

#[test]
fn builtin_join_empty_sep() {
    let src = r#"x = join("", ["a", "b", "c"])"#;
    assert_eq!(
        eval_attr(src, "x"),
        wcl_eval::Value::String("abc".to_string())
    );
}

// ---------------------------------------------------------------------------
// Built-in functions — math
// ---------------------------------------------------------------------------

#[test]
fn builtin_abs_negative() {
    assert_eq!(eval_attr("x = abs(-5)", "x"), wcl_eval::Value::Int(5));
}

#[test]
fn builtin_abs_positive() {
    assert_eq!(eval_attr("x = abs(5)", "x"), wcl_eval::Value::Int(5));
}

#[test]
fn builtin_abs_float() {
    assert_eq!(
        eval_attr("x = abs(-3.14)", "x"),
        wcl_eval::Value::Float(3.14)
    );
}

#[test]
fn builtin_min_int() {
    assert_eq!(eval_attr("x = min(1, 2)", "x"), wcl_eval::Value::Int(1));
}

#[test]
fn builtin_min_same() {
    assert_eq!(eval_attr("x = min(5, 5)", "x"), wcl_eval::Value::Int(5));
}

#[test]
fn builtin_max_int() {
    assert_eq!(eval_attr("x = max(1, 2)", "x"), wcl_eval::Value::Int(2));
}

#[test]
fn builtin_max_larger_first() {
    assert_eq!(eval_attr("x = max(10, 3)", "x"), wcl_eval::Value::Int(10));
}

#[test]
fn builtin_floor() {
    assert_eq!(eval_attr("x = floor(3.7)", "x"), wcl_eval::Value::Int(3));
}

#[test]
fn builtin_floor_negative() {
    assert_eq!(eval_attr("x = floor(-3.2)", "x"), wcl_eval::Value::Int(-4));
}

#[test]
fn builtin_ceil() {
    assert_eq!(eval_attr("x = ceil(3.2)", "x"), wcl_eval::Value::Int(4));
}

#[test]
fn builtin_ceil_exact() {
    assert_eq!(eval_attr("x = ceil(3.0)", "x"), wcl_eval::Value::Int(3));
}

#[test]
fn builtin_round_half_up() {
    assert_eq!(eval_attr("x = round(3.5)", "x"), wcl_eval::Value::Int(4));
}

#[test]
fn builtin_round_down() {
    assert_eq!(eval_attr("x = round(3.4)", "x"), wcl_eval::Value::Int(3));
}

#[test]
fn builtin_round_int_input() {
    assert_eq!(eval_attr("x = round(5)", "x"), wcl_eval::Value::Int(5));
}

// ---------------------------------------------------------------------------
// Built-in functions — collections
// ---------------------------------------------------------------------------

#[test]
fn builtin_len_list() {
    assert_eq!(
        eval_attr("x = len([1, 2, 3])", "x"),
        wcl_eval::Value::Int(3)
    );
}

#[test]
fn builtin_len_list_empty() {
    assert_eq!(eval_attr("x = len([])", "x"), wcl_eval::Value::Int(0));
}

#[test]
fn builtin_len_map() {
    assert_eq!(
        eval_attr("x = len({ a = 1, b = 2 })", "x"),
        wcl_eval::Value::Int(2)
    );
}

#[test]
fn builtin_keys() {
    let v = eval_attr("x = keys({ a = 1, b = 2 })", "x");
    // keys returns a List of Strings; order is insertion order
    assert_eq!(
        v,
        wcl_eval::Value::List(vec![
            wcl_eval::Value::String("a".to_string()),
            wcl_eval::Value::String("b".to_string()),
        ])
    );
}

#[test]
fn builtin_values() {
    let v = eval_attr("x = values({ a = 1, b = 2 })", "x");
    assert_eq!(
        v,
        wcl_eval::Value::List(vec![wcl_eval::Value::Int(1), wcl_eval::Value::Int(2)])
    );
}

#[test]
fn builtin_flatten() {
    let v = eval_attr("x = flatten([[1, 2], [3]])", "x");
    assert_eq!(
        v,
        wcl_eval::Value::List(vec![
            wcl_eval::Value::Int(1),
            wcl_eval::Value::Int(2),
            wcl_eval::Value::Int(3),
        ])
    );
}

#[test]
fn builtin_flatten_empty_inner() {
    let v = eval_attr("x = flatten([[], [1]])", "x");
    assert_eq!(
        v,
        wcl_eval::Value::List(vec![wcl_eval::Value::Int(1)])
    );
}

#[test]
fn builtin_concat() {
    let v = eval_attr("x = concat([1], [2])", "x");
    assert_eq!(
        v,
        wcl_eval::Value::List(vec![wcl_eval::Value::Int(1), wcl_eval::Value::Int(2)])
    );
}

#[test]
fn builtin_concat_multiple_elements() {
    let v = eval_attr("x = concat([1, 2], [3, 4, 5])", "x");
    assert_eq!(
        v,
        wcl_eval::Value::List(vec![
            wcl_eval::Value::Int(1),
            wcl_eval::Value::Int(2),
            wcl_eval::Value::Int(3),
            wcl_eval::Value::Int(4),
            wcl_eval::Value::Int(5),
        ])
    );
}

#[test]
fn builtin_distinct() {
    let v = eval_attr("x = distinct([1, 1, 2, 3, 2])", "x");
    assert_eq!(
        v,
        wcl_eval::Value::List(vec![
            wcl_eval::Value::Int(1),
            wcl_eval::Value::Int(2),
            wcl_eval::Value::Int(3),
        ])
    );
}

#[test]
fn builtin_distinct_no_dupes() {
    let v = eval_attr("x = distinct([1, 2, 3])", "x");
    assert_eq!(
        v,
        wcl_eval::Value::List(vec![
            wcl_eval::Value::Int(1),
            wcl_eval::Value::Int(2),
            wcl_eval::Value::Int(3),
        ])
    );
}

#[test]
fn builtin_sort_ints() {
    let v = eval_attr("x = sort([3, 1, 2])", "x");
    assert_eq!(
        v,
        wcl_eval::Value::List(vec![
            wcl_eval::Value::Int(1),
            wcl_eval::Value::Int(2),
            wcl_eval::Value::Int(3),
        ])
    );
}

#[test]
fn builtin_sort_strings() {
    let v = eval_attr(r#"x = sort(["banana", "apple", "cherry"])"#, "x");
    assert_eq!(
        v,
        wcl_eval::Value::List(vec![
            wcl_eval::Value::String("apple".to_string()),
            wcl_eval::Value::String("banana".to_string()),
            wcl_eval::Value::String("cherry".to_string()),
        ])
    );
}

#[test]
fn builtin_reverse() {
    let v = eval_attr("x = reverse([1, 2, 3])", "x");
    assert_eq!(
        v,
        wcl_eval::Value::List(vec![
            wcl_eval::Value::Int(3),
            wcl_eval::Value::Int(2),
            wcl_eval::Value::Int(1),
        ])
    );
}

#[test]
fn builtin_reverse_single() {
    let v = eval_attr("x = reverse([42])", "x");
    assert_eq!(
        v,
        wcl_eval::Value::List(vec![wcl_eval::Value::Int(42)])
    );
}

#[test]
fn builtin_range_basic() {
    let v = eval_attr("x = range(1, 5)", "x");
    assert_eq!(
        v,
        wcl_eval::Value::List(vec![
            wcl_eval::Value::Int(1),
            wcl_eval::Value::Int(2),
            wcl_eval::Value::Int(3),
            wcl_eval::Value::Int(4),
        ])
    );
}

#[test]
fn builtin_range_from_zero() {
    let v = eval_attr("x = range(0, 3)", "x");
    assert_eq!(
        v,
        wcl_eval::Value::List(vec![
            wcl_eval::Value::Int(0),
            wcl_eval::Value::Int(1),
            wcl_eval::Value::Int(2),
        ])
    );
}

#[test]
fn builtin_range_empty_when_start_ge_end() {
    let v = eval_attr("x = range(5, 5)", "x");
    assert_eq!(v, wcl_eval::Value::List(vec![]));
}

#[test]
fn builtin_range_with_step() {
    let v = eval_attr("x = range(0, 10, 2)", "x");
    assert_eq!(
        v,
        wcl_eval::Value::List(vec![
            wcl_eval::Value::Int(0),
            wcl_eval::Value::Int(2),
            wcl_eval::Value::Int(4),
            wcl_eval::Value::Int(6),
            wcl_eval::Value::Int(8),
        ])
    );
}

// ---------------------------------------------------------------------------
// Built-in functions — type coercion
// ---------------------------------------------------------------------------

#[test]
fn builtin_to_string_int() {
    assert_eq!(
        eval_attr("x = to_string(42)", "x"),
        wcl_eval::Value::String("42".to_string())
    );
}

#[test]
fn builtin_to_string_float() {
    assert_eq!(
        eval_attr("x = to_string(3.14)", "x"),
        wcl_eval::Value::String("3.14".to_string())
    );
}

#[test]
fn builtin_to_string_bool() {
    assert_eq!(
        eval_attr("x = to_string(true)", "x"),
        wcl_eval::Value::String("true".to_string())
    );
}

#[test]
fn builtin_to_int_string() {
    assert_eq!(
        eval_attr(r#"x = to_int("42")"#, "x"),
        wcl_eval::Value::Int(42)
    );
}

#[test]
fn builtin_to_int_float() {
    // Float truncates toward zero
    assert_eq!(
        eval_attr("x = to_int(3.9)", "x"),
        wcl_eval::Value::Int(3)
    );
}

#[test]
fn builtin_to_int_bool_true() {
    assert_eq!(
        eval_attr("x = to_int(true)", "x"),
        wcl_eval::Value::Int(1)
    );
}

#[test]
fn builtin_to_int_bool_false() {
    assert_eq!(
        eval_attr("x = to_int(false)", "x"),
        wcl_eval::Value::Int(0)
    );
}

#[test]
fn builtin_to_float_string() {
    assert_eq!(
        eval_attr(r#"x = to_float("3.14")"#, "x"),
        wcl_eval::Value::Float(3.14)
    );
}

#[test]
fn builtin_to_float_int() {
    assert_eq!(
        eval_attr("x = to_float(7)", "x"),
        wcl_eval::Value::Float(7.0)
    );
}

#[test]
fn builtin_to_bool_int_one() {
    assert_eq!(
        eval_attr("x = to_bool(1)", "x"),
        wcl_eval::Value::Bool(true)
    );
}

#[test]
fn builtin_to_bool_int_zero() {
    assert_eq!(
        eval_attr("x = to_bool(0)", "x"),
        wcl_eval::Value::Bool(false)
    );
}

#[test]
fn builtin_to_bool_string_true() {
    assert_eq!(
        eval_attr(r#"x = to_bool("true")"#, "x"),
        wcl_eval::Value::Bool(true)
    );
}

#[test]
fn builtin_to_bool_string_false() {
    assert_eq!(
        eval_attr(r#"x = to_bool("false")"#, "x"),
        wcl_eval::Value::Bool(false)
    );
}

#[test]
fn builtin_type_of_int() {
    assert_eq!(
        eval_attr("x = type_of(42)", "x"),
        wcl_eval::Value::String("int".to_string())
    );
}

#[test]
fn builtin_type_of_float() {
    assert_eq!(
        eval_attr("x = type_of(3.14)", "x"),
        wcl_eval::Value::String("float".to_string())
    );
}

#[test]
fn builtin_type_of_string() {
    assert_eq!(
        eval_attr(r#"x = type_of("hello")"#, "x"),
        wcl_eval::Value::String("string".to_string())
    );
}

#[test]
fn builtin_type_of_bool() {
    assert_eq!(
        eval_attr("x = type_of(true)", "x"),
        wcl_eval::Value::String("bool".to_string())
    );
}

#[test]
fn builtin_type_of_list() {
    assert_eq!(
        eval_attr("x = type_of([1, 2])", "x"),
        wcl_eval::Value::String("list".to_string())
    );
}

#[test]
fn builtin_type_of_map() {
    assert_eq!(
        eval_attr("x = type_of({ a = 1 })", "x"),
        wcl_eval::Value::String("map".to_string())
    );
}

#[test]
fn builtin_type_of_null() {
    assert_eq!(
        eval_attr("x = type_of(null)", "x"),
        wcl_eval::Value::String("null".to_string())
    );
}

// ---------------------------------------------------------------------------
// Built-in functions — aggregate
// ---------------------------------------------------------------------------

#[test]
fn builtin_sum_ints() {
    assert_eq!(
        eval_attr("x = sum([1, 2, 3, 4])", "x"),
        wcl_eval::Value::Int(10)
    );
}

#[test]
fn builtin_sum_floats() {
    assert_eq!(
        eval_attr("x = sum([1.5, 2.5])", "x"),
        wcl_eval::Value::Float(4.0)
    );
}

#[test]
fn builtin_sum_empty() {
    assert_eq!(eval_attr("x = sum([])", "x"), wcl_eval::Value::Int(0));
}

#[test]
fn builtin_avg() {
    assert_eq!(
        eval_attr("x = avg([1, 2, 3])", "x"),
        wcl_eval::Value::Float(2.0)
    );
}

// ---------------------------------------------------------------------------
// Built-in functions — higher-order
// ---------------------------------------------------------------------------

#[test]
fn builtin_map_lambda() {
    let src = r#"x = map([1, 2, 3], (n) => n * 2)"#;
    assert_eq!(
        eval_attr(src, "x"),
        wcl_eval::Value::List(vec![
            wcl_eval::Value::Int(2),
            wcl_eval::Value::Int(4),
            wcl_eval::Value::Int(6),
        ])
    );
}

#[test]
fn builtin_filter_lambda() {
    let src = r#"x = filter([1, 2, 3, 4, 5], (n) => n > 2)"#;
    assert_eq!(
        eval_attr(src, "x"),
        wcl_eval::Value::List(vec![
            wcl_eval::Value::Int(3),
            wcl_eval::Value::Int(4),
            wcl_eval::Value::Int(5),
        ])
    );
}

#[test]
fn builtin_every_all_true() {
    let src = r#"x = every([2, 4, 6], (n) => n > 0)"#;
    assert_eq!(eval_attr(src, "x"), wcl_eval::Value::Bool(true));
}

#[test]
fn builtin_every_one_false() {
    let src = r#"x = every([2, -1, 6], (n) => n > 0)"#;
    assert_eq!(eval_attr(src, "x"), wcl_eval::Value::Bool(false));
}

#[test]
fn builtin_some_one_true() {
    let src = r#"x = some([0, 0, 1], (n) => n > 0)"#;
    assert_eq!(eval_attr(src, "x"), wcl_eval::Value::Bool(true));
}

#[test]
fn builtin_some_all_false() {
    let src = r#"x = some([0, 0, 0], (n) => n > 0)"#;
    assert_eq!(eval_attr(src, "x"), wcl_eval::Value::Bool(false));
}

#[test]
fn builtin_reduce_sum() {
    let src = r#"x = reduce([1, 2, 3, 4], 0, (acc, n) => acc + n)"#;
    assert_eq!(eval_attr(src, "x"), wcl_eval::Value::Int(10));
}

#[test]
fn builtin_count_lambda() {
    let src = r#"x = count([1, 2, 3, 4, 5], (n) => n > 3)"#;
    assert_eq!(eval_attr(src, "x"), wcl_eval::Value::Int(2));
}

// ---------------------------------------------------------------------------
// Complex / integration scenarios
// ---------------------------------------------------------------------------

#[test]
fn integration_expressions_fixture() {
    // Mirrors tests/fixtures/expressions.wcl
    let src = r#"
let base = 10
let multiplier = 3

sum = 1 + 2
diff = 10 - 3
product = 4 * 5
quotient = 10 / 3
remainder = 10 % 3

pi = 3.14
area = pi * 100.0

greeting = "hello"
name = "world"
message = greeting + " " + name

is_big = base > 5
is_small = base < 5
is_equal = base == 10

both = true && false
either = true || false
negated = !true

status = is_big ? "big" : "small"

summary = "base=${base}, multiplier=${multiplier}"

computed = base * multiplier + 1
"#;
    let out = eval(src);

    assert_eq!(out["sum"], wcl_eval::Value::Int(3));
    assert_eq!(out["diff"], wcl_eval::Value::Int(7));
    assert_eq!(out["product"], wcl_eval::Value::Int(20));
    assert_eq!(out["quotient"], wcl_eval::Value::Int(3));
    assert_eq!(out["remainder"], wcl_eval::Value::Int(1));

    assert_eq!(out["pi"], wcl_eval::Value::Float(3.14));
    assert_eq!(out["area"], wcl_eval::Value::Float(314.0));

    assert_eq!(
        out["message"],
        wcl_eval::Value::String("hello world".to_string())
    );

    assert_eq!(out["is_big"], wcl_eval::Value::Bool(true));
    assert_eq!(out["is_small"], wcl_eval::Value::Bool(false));
    assert_eq!(out["is_equal"], wcl_eval::Value::Bool(true));

    assert_eq!(out["both"], wcl_eval::Value::Bool(false));
    assert_eq!(out["either"], wcl_eval::Value::Bool(true));
    assert_eq!(out["negated"], wcl_eval::Value::Bool(false));

    assert_eq!(
        out["status"],
        wcl_eval::Value::String("big".to_string())
    );

    assert_eq!(
        out["summary"],
        wcl_eval::Value::String("base=10, multiplier=3".to_string())
    );

    assert_eq!(out["computed"], wcl_eval::Value::Int(31));
}

#[test]
fn integration_simple_fixture() {
    let src = r#"
name = "test-app"
version = "1.0.0"
debug = true
port = 8080
pi = 3.14159

tags = ["web", "api", "production"]

settings = {
  timeout = 30,
  retries = 3,
  verbose = false,
}
"#;
    let out = eval(src);

    assert_eq!(
        out["name"],
        wcl_eval::Value::String("test-app".to_string())
    );
    assert_eq!(
        out["version"],
        wcl_eval::Value::String("1.0.0".to_string())
    );
    assert_eq!(out["debug"], wcl_eval::Value::Bool(true));
    assert_eq!(out["port"], wcl_eval::Value::Int(8080));
    assert_eq!(out["pi"], wcl_eval::Value::Float(3.14159));

    let tags = out["tags"].as_list().expect("expected list");
    assert_eq!(tags.len(), 3);
    assert_eq!(tags[0], wcl_eval::Value::String("web".to_string()));

    let settings = out["settings"].as_map().expect("expected map");
    assert_eq!(settings["timeout"], wcl_eval::Value::Int(30));
    assert_eq!(settings["retries"], wcl_eval::Value::Int(3));
    assert_eq!(settings["verbose"], wcl_eval::Value::Bool(false));
}

#[test]
fn integration_string_processing_pipeline() {
    let src = r#"
let raw = "  Hello, World!  "
trimmed = trim(raw)
upped = upper(trimmed)
low = lower(trimmed)
has_world = contains(trimmed, "World")
starts = starts_with(trimmed, "Hello")
ends = ends_with(trimmed, "!")
replaced = replace(trimmed, "World", "WCL")
"#;
    let out = eval(src);

    assert_eq!(
        out["trimmed"],
        wcl_eval::Value::String("Hello, World!".to_string())
    );
    assert_eq!(
        out["upped"],
        wcl_eval::Value::String("HELLO, WORLD!".to_string())
    );
    assert_eq!(
        out["low"],
        wcl_eval::Value::String("hello, world!".to_string())
    );
    assert_eq!(out["has_world"], wcl_eval::Value::Bool(true));
    assert_eq!(out["starts"], wcl_eval::Value::Bool(true));
    assert_eq!(out["ends"], wcl_eval::Value::Bool(true));
    assert_eq!(
        out["replaced"],
        wcl_eval::Value::String("Hello, WCL!".to_string())
    );
}

#[test]
fn integration_collection_processing() {
    let src = r#"
let nums = [3, 1, 4, 1, 5, 9, 2, 6]
uniq = distinct(nums)
sorted_uniq = sort(distinct(nums))
rev = reverse([1, 2, 3])
total = sum(nums)
first_three = range(0, 3)
"#;
    let out = eval(src);

    // distinct preserves order and removes dupes
    let uniq = out["uniq"].as_list().expect("list");
    // Original: [3,1,4,1,5,9,2,6] → distinct: [3,1,4,5,9,2,6]
    assert_eq!(uniq.len(), 7);

    // sorted_uniq: [1,2,3,4,5,6,9]
    let su = out["sorted_uniq"].as_list().expect("list");
    assert_eq!(su[0], wcl_eval::Value::Int(1));
    assert_eq!(su[su.len() - 1], wcl_eval::Value::Int(9));

    assert_eq!(
        out["rev"],
        wcl_eval::Value::List(vec![
            wcl_eval::Value::Int(3),
            wcl_eval::Value::Int(2),
            wcl_eval::Value::Int(1),
        ])
    );

    // sum of [3,1,4,1,5,9,2,6] = 31
    assert_eq!(out["total"], wcl_eval::Value::Int(31));

    assert_eq!(
        out["first_three"],
        wcl_eval::Value::List(vec![
            wcl_eval::Value::Int(0),
            wcl_eval::Value::Int(1),
            wcl_eval::Value::Int(2),
        ])
    );
}

#[test]
fn integration_type_coercion_roundtrip() {
    let src = r#"
int_val = to_int("100")
float_val = to_float("2.71")
str_val = to_string(42)
bool_true = to_bool("true")
bool_false = to_bool("false")
type_int = type_of(42)
type_str = type_of("hello")
type_float = type_of(3.14)
type_bool = type_of(false)
type_null = type_of(null)
type_list = type_of([])
type_map = type_of({})
"#;
    let out = eval(src);

    assert_eq!(out["int_val"], wcl_eval::Value::Int(100));
    assert_eq!(out["float_val"], wcl_eval::Value::Float(2.71));
    assert_eq!(
        out["str_val"],
        wcl_eval::Value::String("42".to_string())
    );
    assert_eq!(out["bool_true"], wcl_eval::Value::Bool(true));
    assert_eq!(out["bool_false"], wcl_eval::Value::Bool(false));
    assert_eq!(
        out["type_int"],
        wcl_eval::Value::String("int".to_string())
    );
    assert_eq!(
        out["type_str"],
        wcl_eval::Value::String("string".to_string())
    );
    assert_eq!(
        out["type_float"],
        wcl_eval::Value::String("float".to_string())
    );
    assert_eq!(
        out["type_bool"],
        wcl_eval::Value::String("bool".to_string())
    );
    assert_eq!(
        out["type_null"],
        wcl_eval::Value::String("null".to_string())
    );
    assert_eq!(
        out["type_list"],
        wcl_eval::Value::String("list".to_string())
    );
    assert_eq!(
        out["type_map"],
        wcl_eval::Value::String("map".to_string())
    );
}

#[test]
fn integration_null_literal() {
    assert_eq!(eval_attr("x = null", "x"), wcl_eval::Value::Null);
}

#[test]
fn integration_null_eq() {
    assert_eq!(
        eval_attr("x = null == null", "x"),
        wcl_eval::Value::Bool(true)
    );
}

#[test]
fn integration_bool_literal_false() {
    assert_eq!(eval_attr("x = false", "x"), wcl_eval::Value::Bool(false));
}

#[test]
fn integration_chained_string_concat() {
    let src = r#"
let a = "foo"
let b = "bar"
let c = "baz"
result = a + "-" + b + "-" + c
"#;
    assert_eq!(
        eval_attr(src, "result"),
        wcl_eval::Value::String("foo-bar-baz".to_string())
    );
}

#[test]
fn integration_list_index_access() {
    let src = r#"
let items = [10, 20, 30]
first = items[0]
second = items[1]
third = items[2]
"#;
    // items is a let binding so not in output; but the derived attributes are
    // Note: attributes referencing lets via index access should work
    let out = eval(src);
    assert_eq!(out["first"], wcl_eval::Value::Int(10));
    assert_eq!(out["second"], wcl_eval::Value::Int(20));
    assert_eq!(out["third"], wcl_eval::Value::Int(30));
}

#[test]
fn integration_map_member_access() {
    let src = r#"
let cfg = { host = "localhost", port = 5432 }
db_host = cfg.host
db_port = cfg.port
"#;
    let out = eval(src);
    assert_eq!(
        out["db_host"],
        wcl_eval::Value::String("localhost".to_string())
    );
    assert_eq!(out["db_port"], wcl_eval::Value::Int(5432));
}

#[test]
fn integration_nested_map_access() {
    let src = r#"
let outer = { inner = { val = 42 } }
result = outer.inner.val
"#;
    let out = eval(src);
    assert_eq!(out["result"], wcl_eval::Value::Int(42));
}

#[test]
fn integration_lambda_in_attribute() {
    // User-defined lambda stored as a let, then applied via higher-order fn
    let src = r#"
let double = (x) => x * 2
result = map([1, 2, 3], double)
"#;
    let out = eval(src);
    assert_eq!(
        out["result"],
        wcl_eval::Value::List(vec![
            wcl_eval::Value::Int(2),
            wcl_eval::Value::Int(4),
            wcl_eval::Value::Int(6),
        ])
    );
}

#[test]
fn integration_export_let_appears_in_output() {
    // export let makes a value publicly visible
    let src = r#"
export let api_version = "v2"
"#;
    let out = eval(src);
    assert_eq!(
        out["api_version"],
        wcl_eval::Value::String("v2".to_string())
    );
}

#[test]
fn integration_export_let_usable_by_attributes() {
    let src = r#"
export let base_port = 8000
actual_port = base_port + 80
"#;
    let out = eval(src);
    assert_eq!(out["actual_port"], wcl_eval::Value::Int(8080));
    assert_eq!(out["base_port"], wcl_eval::Value::Int(8000));
}
