//! Integration tests: which decorators a document accepts.

use wcl_lang::{Document, EvalError, SchemaViolationKind};

struct AcceptanceCase {
    name: &'static str,
    source: &'static str,
    expected: Option<SchemaViolationKind>,
    span_text: &'static str,
}

#[test]
fn original_decorator_acceptance_snippets_remain_the_contract() {
    let cases = [
        AcceptanceCase {
            name: "declared decorator",
            source: include_str!("fixtures/decorator_acceptance/declared.wcl"),
            expected: None,
            span_text: "",
        },
        AcceptanceCase {
            name: "declared repeatable decorator",
            source: include_str!("fixtures/decorator_acceptance/repeatable.wcl"),
            expected: None,
            span_text: "",
        },
        AcceptanceCase {
            name: "undeclared typo",
            source: include_str!("fixtures/decorator_acceptance/undeclared.wcl"),
            expected: Some(SchemaViolationKind::UndeclaredDecorator),
            span_text: "dve",
        },
        AcceptanceCase {
            name: "wrong argument type",
            source: include_str!("fixtures/decorator_acceptance/wrong_type.wcl"),
            expected: Some(SchemaViolationKind::FieldTypeMismatch),
            span_text: "workspace = 42",
        },
        AcceptanceCase {
            name: "unknown named argument",
            source: include_str!("fixtures/decorator_acceptance/unknown_named.wcl"),
            expected: Some(SchemaViolationKind::UnknownField),
            span_text: "workspaec = \"/workspace\"",
        },
        AcceptanceCase {
            name: "missing required argument",
            source: include_str!("fixtures/decorator_acceptance/missing_required.wcl"),
            expected: Some(SchemaViolationKind::MissingRequired),
            span_text: "dev",
        },
        AcceptanceCase {
            name: "surplus positional argument",
            source: include_str!("fixtures/decorator_acceptance/surplus_positional.wcl"),
            expected: Some(SchemaViolationKind::UnknownField),
            span_text: "\"/workspace\"",
        },
        AcceptanceCase {
            name: "illegal decorator position",
            source: include_str!("fixtures/decorator_acceptance/illegal_position.wcl"),
            expected: Some(SchemaViolationKind::DecoratorNotApplicable),
            span_text: "dev",
        },
    ];

    for case in cases {
        let errors = Document::open(case.source, case.name)
            .expect("acceptance fixture parses")
            .schema_errors();
        let Some(expected) = case.expected else {
            assert!(errors.is_empty(), "{}: {errors:#?}", case.name);
            continue;
        };
        assert_eq!(errors.len(), 1, "{}: {errors:#?}", case.name);
        let EvalError::SchemaViolation { kind, span, .. } = &errors[0] else {
            panic!("{}: expected a schema violation: {errors:#?}", case.name)
        };
        assert_eq!(*kind, expected, "{}: {errors:#?}", case.name);
        assert_eq!(
            &case.source[span.offset()..span.offset() + span.len()],
            case.span_text,
            "{}",
            case.name,
        );
    }
}
