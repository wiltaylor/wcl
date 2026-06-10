//! Property tests for invariants that span the public API. These
//! complement the example-driven unit tests by exploring randomised
//! inputs where a single counterexample is enough to surface a bug
//! the curated fixtures might miss.

use proptest::prelude::*;
use wcl_lang::{Document, Value};

/// Evaluate a tiny WCL document of the form `@schemaless\nfoo = <expr>\n`
/// and return the field's `Value`.
fn eval_field(expr: &str) -> Value {
    let src = format!("@schemaless\nfoo = {expr}\n");
    let doc = Document::open(&src, "prop.wcl").expect("parse");
    doc.field("foo")
        .expect("field present")
        .value()
        .expect("eval ok")
        .clone()
}

proptest! {
    /// Equality of integer literals must be commutative across types
    /// (`a == b` ⇔ `b == a`) and must agree with the same comparison
    /// performed on the promoted i128 representation. This guards the
    /// `promote_pair` / `values_eq` pipeline against asymmetric
    /// promotions or off-by-one suffix mishandling.
    #[test]
    fn integer_equality_is_commutative_across_types(a in -1_000_000i64..1_000_000, b in -1_000_000i64..1_000_000) {
        // Same value reinterpreted with two different signed suffixes.
        let lr = eval_field(&format!("{a}i64 == {b}i32"));
        let rl = eval_field(&format!("{b}i32 == {a}i64"));
        prop_assert_eq!(&lr, &rl, "{} i64 vs {} i32 not commutative", a, b);
        // Agreement with native comparison.
        let expected = Value::Bool(a == b);
        prop_assert_eq!(lr, expected);
    }

    /// Round-tripping a `Value` through the custom JSON serializer and
    /// back into a `serde_json::Value` must be stable: re-serializing
    /// the parsed JSON yields the same canonical text. Guards against
    /// non-deterministic key ordering or float formatting drift.
    #[test]
    fn json_serialization_is_canonical_for_lists_of_ints(xs in proptest::collection::vec(any::<i32>(), 0..16)) {
        let v = Value::list(xs.into_iter().map(Value::I32).collect());
        let s1 = serde_json::to_string(&v).expect("serialize");
        let parsed: serde_json::Value = serde_json::from_str(&s1).expect("parse JSON");
        let s2 = serde_json::to_string(&parsed).expect("reserialize");
        prop_assert_eq!(s1, s2);
    }
}
