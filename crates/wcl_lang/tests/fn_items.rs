//! End-to-end tests for `fn name(…) -> T body` item declarations —
//! sugar for `let name = fn(…)` that is additionally registered in the
//! symbol index and re-printed in `fn` form.

use wcl_lang::{Document, SymbolKind, Value};

fn eval(src: &str) -> Value {
    let doc = Document::open(src, "test").unwrap();
    doc.get("result")
        .expect("result field")
        .value()
        .expect("eval")
}

#[test]
fn fn_item_is_callable_from_fields() {
    let src = "fn double(x: i64) -> i64 { x * 2 }\n\
               @schemaless result = double(21)\n";
    assert_eq!(eval(src), Value::I64(42));
}

#[test]
fn fn_item_with_expression_body_and_decorator() {
    let src = "@doc(\"Triple.\")\nfn triple(x: i64) -> i64 x * 3\n\
               @schemaless result = triple(5)\n";
    assert_eq!(eval(src), Value::I64(15));
}

#[test]
fn fn_items_compose_like_lets() {
    let src = "fn double(x: i64) -> i64 x * 2\n\
               fn quad(x: i64) -> i64 double(double(x))\n\
               @schemaless result = quad(3)\n";
    assert_eq!(eval(src), Value::I64(12));
}

#[test]
fn fn_item_is_in_the_symbol_index() {
    let src = "fn helper(x: i64) -> i64 x\n@schemaless a = 1\n";
    let doc = Document::open(src, "test").unwrap();
    let rec = doc
        .symbols()
        .iter()
        .find(|r| r.fqn == "helper")
        .expect("fn item indexed");
    assert!(matches!(rec.kind, SymbolKind::FnDecl), "{:?}", rec.kind);
    // Plain lets stay out of the index.
    let src2 = "let helper = fn(x: i64) -> i64 x\n@schemaless a = 1\n";
    let doc2 = Document::open(src2, "test").unwrap();
    assert!(
        doc2.symbols().iter().all(|r| r.fqn != "helper"),
        "let-bound fns stay unindexed"
    );
}

#[test]
fn fn_item_is_invisible_to_data_and_schema() {
    // Not resolvable as data (`get` fails) and exempt from the
    // @document schema like a let.
    let src = "@document type Cfg { a: i64 }\n\
               fn helper(x: i64) -> i64 x\n\
               a = helper(7)\n";
    let doc = Document::open(src, "test").unwrap();
    assert!(doc.schema_errors().is_empty(), "{:?}", doc.schema_errors());
    assert!(doc.get("helper").is_none(), "fn item is not a data path");
    assert_eq!(
        doc.get("a").expect("a").value().expect("eval"),
        Value::I64(7)
    );
}

#[test]
fn field_named_fn_still_parses() {
    assert_eq!(
        eval("@schemaless fn = 7\n@schemaless result = fn\n"),
        Value::I64(7)
    );
}

#[test]
fn fn_item_round_trips_through_the_formatter() {
    let src = "@doc(\"Doubles.\")\nfn double(x: i64) -> i64 { x * 2 }\n@schemaless fn = 7\n";
    let ast = wcl_lang::parse_for_edit(src, "t").expect("parse");
    let printed = wcl_lang::format::to_source(&ast);
    assert_eq!(printed, src);
    let ast2 = wcl_lang::parse_for_edit(&printed, "t").expect("reparse");
    assert_eq!(wcl_lang::format::to_source(&ast2), printed);
}

#[test]
fn fn_item_inside_a_block_scopes_to_the_block() {
    let src = "@document type Cfg { @children(\"box\") boxes: list<BoxK> }\n\
               @block(\"box\") type BoxK { w: i64 }\n\
               box b {\n  fn grow(x: i64) -> i64 x + 10\n  w = grow(5)\n}\n\
               @schemaless result = boxes.b.w\n";
    assert_eq!(eval(src), Value::I64(15));
}

#[test]
fn duplicate_fn_and_field_names_collide() {
    let src = "fn a(x: i64) -> i64 x\n@schemaless a = 1\n";
    assert!(
        Document::open(src, "test").is_err(),
        "fn item and field sharing a name is a duplicate declaration"
    );
}

#[test]
fn closure_recursing_through_a_builtin_does_constant_work_per_level() {
    // Regression: a call pushed its parameters onto the caller's locals,
    // and a closure literal captures every local in scope, so a closure
    // recursing back through its enclosing `fn` doubled its captures at
    // every level — `down(26)` took about a minute. A call now sees only
    // its own captures and parameters. 99 levels are 198 calls, just
    // inside the 200-level cap.
    let src = "fn down(n: i64) -> i64 \
               if n <= 0 { 0 } else { sum(map([n - 1], fn(x: i64) -> i64 down(x))) }\n\
               @schemaless x = down(99)\n";
    let started = std::time::Instant::now();
    let value = std::thread::Builder::new()
        .stack_size(64 * 1024 * 1024)
        .spawn(move || {
            let doc = Document::open(src, "limits").expect("parses");
            doc.get("x").expect("field x").value()
        })
        .expect("spawn")
        .join()
        .expect("no panic")
        .expect("evaluates");
    assert_eq!(value, Value::I64(0));
    // Exponential work would not finish at all; linear work is instant
    // even unoptimised.
    assert!(
        started.elapsed() < std::time::Duration::from_secs(10),
        "took {:?}",
        started.elapsed()
    );
}

#[test]
fn a_call_sees_its_captures_not_the_callers_locals() {
    // Captures are lexical: a `fn` reads the locals where its literal
    // was written, never the ones in force where it is called.
    let src = "fn get_y() -> i64 y\n\
               @schemaless x = { let y = 5; get_y() }\n\
               @schemaless z = { let y = 1; let f = fn() -> i64 y; { let y = 2; f() } }\n";
    let doc = Document::open(src, "limits").expect("parses");
    assert!(doc.get("x").expect("field x").value().is_err());
    assert_eq!(
        doc.get("z").expect("field z").value().expect("evaluates"),
        Value::I64(1)
    );
}
