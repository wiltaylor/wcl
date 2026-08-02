//! `if` without an `else` branch: the missing branch yields `none`.
//!
//! The motivating shape is a conditional list element —
//! `["base", if e.current { "current" }]` — where the `none` an untaken
//! branch produces is dropped downstream instead of being spelled out as
//! `else { none }` at every site.

use wcl_lang::format::to_source;
use wcl_lang::{Document, Value, VariantPayload, parse_for_edit};

fn eval(src: &str) -> Value {
    let doc = Document::open(src, "test").unwrap();
    doc.get("result")
        .expect("result field")
        .value()
        .expect("eval")
}

#[test]
fn taken_branch_yields_its_value() {
    assert_eq!(
        eval("@schemaless result = if true { \"current\" }\n"),
        Value::Utf8("current".into())
    );
}

#[test]
fn untaken_branch_yields_none() {
    assert_eq!(
        eval("@schemaless result = if false { \"current\" }\n"),
        Value::None
    );
}

#[test]
fn else_if_chain_may_end_without_else() {
    assert_eq!(
        eval("@schemaless result = if false { 1 } else if false { 2 }\n"),
        Value::None
    );
    assert_eq!(
        eval("@schemaless result = if false { 1 } else if true { 2 }\n"),
        Value::I64(2)
    );
}

#[test]
fn conditional_list_element_is_none() {
    let v = eval("@schemaless result = [\"base\", if false { \"current\" }]\n");
    let Value::List(items) = v else {
        panic!("expected a list, got {v:?}");
    };
    assert_eq!(items.as_slice(), &[Value::Utf8("base".into()), Value::None]);
}

/// Pull the `class` list out of the `result` field's variant payload.
fn class_of(doc: &Document) -> Vec<Value> {
    let v = doc
        .get("result")
        .expect("result field")
        .value()
        .expect("eval");
    let Value::Variant { payload, .. } = &v else {
        panic!("expected a variant, got {v:?}");
    };
    let VariantPayload::Record(fields) = payload else {
        panic!("expected a record payload, got {payload:?}");
    };
    let Some(Value::List(items)) = fields.get("class") else {
        panic!("expected a class list, got {fields:?}");
    };
    items.to_vec()
}

#[test]
fn none_element_type_checks_in_a_declared_string_list() {
    // A `none` element is legal in a `list<utf8>` — including the strict
    // top-level field path, which is where the motivating `class` list is
    // authored. (A `none` *field* value still fails a required slot; the
    // exemption is for elements only.)
    let src = "@document\n\
               type Doc {\n\
               \x20 tags: list<utf8>\n\
               \x20 opt_tags: list<utf8>?\n\
               }\n\n\
               tags = [\"base\", if false { \"current\" }]\n\
               opt_tags = [\"base\", if false { \"current\" }]\n";
    let doc = Document::open(src, "test").unwrap();
    assert!(
        doc.schema_errors().is_empty(),
        "unexpected schema errors: {:?}",
        doc.schema_errors()
    );
    for name in ["tags", "opt_tags"] {
        let v = doc.get(name).expect(name).value().expect("eval");
        let Value::List(items) = v else {
            panic!("expected a list for {name}, got {v:?}");
        };
        assert_eq!(items.as_slice(), &[Value::Utf8("base".into()), Value::None]);
    }
}

#[test]
fn non_empty_does_not_count_none_elements() {
    // `none` elements are legal but invisible to consumers, so a list of
    // nothing but absences is empty as far as `@non_empty` is concerned.
    let doc = Document::open(
        "@document\n\
         type Doc {\n\
         \x20 @non_empty tags: list<utf8>\n\
         }\n\n\
         tags = [if false { \"x\" }]\n",
        "test",
    )
    .unwrap();
    let errs = doc.schema_errors();
    assert_eq!(errs.len(), 1, "expected one violation, got {errs:?}");
    assert!(
        format!("{:?}", errs[0]).contains("@non_empty"),
        "unexpected violation: {:?}",
        errs[0]
    );

    // One real element is enough, even alongside a `none`.
    let ok = Document::open(
        "@document\n\
         type Doc {\n\
         \x20 @non_empty tags: list<utf8>\n\
         }\n\n\
         tags = [\"base\", if false { \"x\" }]\n",
        "test",
    )
    .unwrap();
    assert!(
        ok.schema_errors().is_empty(),
        "unexpected schema errors: {:?}",
        ok.schema_errors()
    );
}

#[test]
fn none_element_survives_a_variant_string_list() {
    // The motivating site is wdoc's `InlineSpan`: a `class: list<utf8>?`
    // field of a variant record. Both authoring forms — the explicit
    // `Union::Variant { … }` and the bare record literal dispatched by
    // shape — must accept the `none` an untaken branch leaves behind.
    let decl = "union Span {\n\
                \x20 Plain { text: utf8 class: list<utf8>? }\n\
                }\n\n";
    let explicit = format!(
        "{decl}@schemaless result = Span::Plain {{ text: \"hi\", class: [\"base\", if false {{ \"current\" }}] }}\n"
    );
    let bare = format!(
        "{decl}@document\n\
         type Doc {{\n\
         \x20 result: Span\n\
         }}\n\n\
         result = {{ text: \"hi\", class: [\"base\", if false {{ \"current\" }}] }}\n"
    );
    for src in [explicit, bare] {
        let doc = Document::open(&src, "test").unwrap();
        assert!(
            doc.schema_errors().is_empty(),
            "unexpected schema errors for:\n{src}\n{:?}",
            doc.schema_errors()
        );
        assert_eq!(
            class_of(&doc),
            vec![Value::Utf8("base".into()), Value::None],
            "for:\n{src}"
        );
    }
}

#[test]
fn fmt_round_trips_without_adding_an_else() {
    let src = "@schemaless result = [\"base\", if false { \"current\" }]\n";
    let out = to_source(&parse_for_edit(src, "test").expect("input parses"));
    assert_eq!(out, src);
    let ast2 = parse_for_edit(&out, "test").expect("formatter output re-parses");
    assert_eq!(out, to_source(&ast2), "formatter is not idempotent");
}

#[test]
fn an_else_less_if_does_not_swallow_what_follows() {
    // The `}` ends the expression: the next field is its own item, and a
    // later `else` still binds to its own `if`.
    let src = "@schemaless a = if false { 1 }\n\
               @schemaless b = if true { 2 } else { 3 }\n";
    let doc = Document::open(src, "test").unwrap();
    assert_eq!(doc.get("a").unwrap().value().unwrap(), Value::None);
    assert_eq!(doc.get("b").unwrap().value().unwrap(), Value::I64(2));
}

#[test]
fn fmt_keeps_an_else_less_tail_on_an_else_if_chain() {
    let src = "@schemaless result = if a { 1 } else if b { 2 }\n";
    let out = to_source(&parse_for_edit(src, "test").expect("input parses"));
    assert_eq!(out, src);
}

#[test]
fn if_let_still_requires_an_else() {
    let err = Document::open("@schemaless result = if let Some(x) = y { x }\n", "test")
        .expect_err("`if let` without `else` must stay an error");
    assert!(
        format!("{err:?}").contains("'if let' requires an 'else' branch"),
        "unexpected diagnostic: {err:?}"
    );
}
