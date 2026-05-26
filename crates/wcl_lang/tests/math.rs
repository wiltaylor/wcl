//! End-to-end tests for the math / geometry builtins.

use wcl_lang::{Document, Value};

fn evalf(src: &str) -> f64 {
    let doc = Document::open(src, "test").unwrap();
    match doc
        .get("result")
        .expect("result field")
        .value()
        .expect("eval")
    {
        Value::F64(n) => n,
        other => panic!("expected f64, got {other:?}"),
    }
}

/// Exact results (no rounding error) compare directly.
#[test]
fn exact_results() {
    assert_eq!(evalf("@schemaless result = sin(0.0)\n"), 0.0);
    assert_eq!(evalf("@schemaless result = cos(0.0)\n"), 1.0);
    assert_eq!(evalf("@schemaless result = sqrt(16.0)\n"), 4.0);
    assert_eq!(evalf("@schemaless result = pow(2.0, 10.0)\n"), 1024.0);
    assert_eq!(evalf("@schemaless result = hypot(3.0, 4.0)\n"), 5.0);
    assert_eq!(evalf("@schemaless result = floor(3.9)\n"), 3.0);
    assert_eq!(evalf("@schemaless result = ceil(3.1)\n"), 4.0);
    assert_eq!(evalf("@schemaless result = round(2.5)\n"), 3.0);
    assert_eq!(evalf("@schemaless result = abs(-7.5)\n"), 7.5);
    assert_eq!(evalf("@schemaless result = sign(-3.0)\n"), -1.0);
    assert_eq!(evalf("@schemaless result = min(3.0, 7.5)\n"), 3.0);
    assert_eq!(evalf("@schemaless result = max(3.0, 7.5)\n"), 7.5);
    assert_eq!(evalf("@schemaless result = clamp(12.0, 0.0, 10.0)\n"), 10.0);
}

/// Integer arguments are widened to f64, so `sqrt(16)` works like
/// `sqrt(16.0)`.
#[test]
fn integer_args_are_coerced() {
    assert_eq!(evalf("@schemaless result = sqrt(144)\n"), 12.0);
    assert_eq!(evalf("@schemaless result = pow(2, 10)\n"), 1024.0);
    assert_eq!(evalf("@schemaless result = max(3, 7)\n"), 7.0);
}

#[test]
fn constants_and_radians() {
    let pi = evalf("@schemaless result = pi()\n");
    assert!((pi - std::f64::consts::PI).abs() < 1e-12);
    let tau = evalf("@schemaless result = tau()\n");
    assert!((tau - std::f64::consts::TAU).abs() < 1e-12);
    // sin(pi/2) == 1; cos(tau) == 1; radians(90) == pi/2.
    assert!((evalf("@schemaless result = sin(pi() / 2.0)\n") - 1.0).abs() < 1e-12);
    assert!((evalf("@schemaless result = cos(tau())\n") - 1.0).abs() < 1e-12);
    assert!((evalf("@schemaless result = sin(radians(90.0))\n") - 1.0).abs() < 1e-12);
}

/// `fold` + `max` computes a list maximum (the pattern charts use to
/// auto-fit a scale).
#[test]
fn fold_with_max_finds_list_maximum() {
    assert_eq!(
        evalf(
            "@schemaless result = fold([3.0, 9.0, 1.0, 5.0], 0.0, fn(a: f64, b: f64) -> f64 max(a, b))\n"
        ),
        9.0
    );
}

#[test]
fn non_numeric_argument_errors() {
    let doc = Document::open("@schemaless result = sqrt(\"x\")\n", "test").unwrap();
    let err = doc
        .get("result")
        .expect("result field")
        .value()
        .unwrap_err();
    assert!(format!("{err:?}").contains("expected a number"), "{err:?}");
}
