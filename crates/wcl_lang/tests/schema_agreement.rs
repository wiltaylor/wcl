//! Strict-vs-lazy schema verdict agreement.
//!
//! WCL validates schema membership through two independent paths:
//!
//! - **strict** — `Document::schema_errors()` walks every source and
//!   collects all violations eagerly (`doc.rs` + `doc/schema_check.rs`);
//! - **lazy** — `Field::value()` runs a per-field membership check
//!   (`Field::schema_membership_error` in `doc/views.rs`) before
//!   evaluating, so a single `get` on an undeclared field fails even
//!   when nobody ran the strict validator.
//!
//! Today only convention keeps the two in agreement. These tests pin
//! the contract down: for every literal field, the strict path flags a
//! *membership* violation (`UnknownField` / `NoDocumentSchema`) at that
//! field if and only if the lazy path reports the same violation from
//! `Field::value()`.
//!
//! Type-level checks (`FieldTypeMismatch`, variant mismatches, …) are
//! intentionally strict-only and are excluded from the comparison; one
//! test below documents that asymmetry explicitly.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use proptest::prelude::*;
use wcl_lang::{
    Block, Document, Environment, EvalError, Field, Registry, SchemaViolationKind, disk_loader,
};

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
}

fn open(src: &str) -> Document {
    Document::open(src, "agreement.wcl").expect("test source parses")
}

/// The violation kinds the *membership* checks emit on both paths.
/// Everything else (type mismatches, child counts, kind registration,
/// …) is strict-only by design and excluded from the comparison.
fn is_membership_kind(kind: SchemaViolationKind) -> bool {
    matches!(
        kind,
        SchemaViolationKind::UnknownField | SchemaViolationKind::NoDocumentSchema
    )
}

/// Collect every literal field reachable from the document: top-level
/// fields across all sources, plus fields of (recursively) nested
/// blocks. Depth-limited so a pathological fixture can't recurse
/// forever through computed children.
fn collect_fields<'a>(doc: &'a Document) -> Vec<Field<'a>> {
    let mut out: Vec<Field<'a>> = doc.fields().collect();
    for b in doc.blocks() {
        collect_block_fields(&b, &mut out, 0);
    }
    out
}

fn collect_block_fields<'a>(block: &Block<'a>, out: &mut Vec<Field<'a>>, depth: usize) {
    if depth > 16 {
        return;
    }
    out.extend(block.fields());
    for child in block.blocks() {
        collect_block_fields(&child, out, depth + 1);
    }
}

/// Lazy verdict for one field: `Some(start)` if `Field::value()` fails
/// with a membership violation attributed to *this* field (matched by
/// span start, so errors merely propagated from evaluating another
/// field's reference don't count against this one).
fn lazy_flag(field: &Field<'_>) -> Option<usize> {
    match field.value() {
        Err(EvalError::SchemaViolation { kind, span, .. })
            if is_membership_kind(*kind) && span.offset() == field.span().start =>
        {
            Some(field.span().start)
        }
        _ => None,
    }
}

/// Strict verdict: the span starts of every membership violation in
/// `schema_errors()` that points at a known literal field (top-level
/// blocks also produce `NoDocumentSchema`; restricting to field spans
/// keeps the comparison field-vs-field).
fn strict_flags(doc: &Document, field_starts: &BTreeSet<usize>) -> BTreeSet<usize> {
    doc.schema_errors()
        .iter()
        .filter_map(|e| match e {
            EvalError::SchemaViolation { kind, span, .. }
                if is_membership_kind(*kind) && field_starts.contains(&span.offset()) =>
            {
                Some(span.offset())
            }
            _ => None,
        })
        .collect()
}

/// Assert that the strict and lazy paths flag exactly the same set of
/// fields with membership violations. Returns the agreed set (span
/// starts) so callers can additionally assert on expected counts.
fn assert_agreement_doc(doc: &Document, label: &str) -> BTreeSet<usize> {
    let fields = collect_fields(doc);
    let field_starts: BTreeSet<usize> = fields.iter().map(|f| f.span().start).collect();

    // Lazy first: `value()` caches its result, and the strict walk
    // tolerates already-cached errors, so this order also exercises
    // the cache interplay between the two paths.
    let lazy: BTreeSet<usize> = fields.iter().filter_map(lazy_flag).collect();
    let strict = strict_flags(doc, &field_starts);

    let name_of = |start: &usize| {
        fields
            .iter()
            .find(|f| f.span().start == *start)
            .map(|f| f.name().to_string())
            .unwrap_or_else(|| format!("<offset {start}>"))
    };
    let strict_only: Vec<String> = strict.difference(&lazy).map(name_of).collect();
    let lazy_only: Vec<String> = lazy.difference(&strict).map(name_of).collect();
    assert!(
        strict_only.is_empty() && lazy_only.is_empty(),
        "strict/lazy schema verdicts disagree for {label}:\n  \
         flagged by strict only: {strict_only:?}\n  \
         flagged by lazy only:   {lazy_only:?}"
    );
    strict
}

fn assert_agreement(src: &str) -> BTreeSet<usize> {
    assert_agreement_doc(&open(src), "inline source")
}

// ---------------------------------------------------------------------------
// Hand-written cases
// ---------------------------------------------------------------------------

#[test]
fn valid_document_schema_flags_nothing_on_either_path() {
    let flagged = assert_agreement(
        r#"
        @document type Cfg {
          name: utf8
          port: i64
          @children("svc") svcs: list<Svc>
        }
        @block("svc") type Svc { region: utf8 }
        name = "alpha"
        port = 8080
        svc web { region = "us-east-1" }
        "#,
    );
    assert!(flagged.is_empty(), "valid doc must not flag: {flagged:?}");
}

#[test]
fn unknown_top_level_field_flagged_by_both_paths() {
    let flagged = assert_agreement(
        r#"
        @document type Cfg { name: utf8 }
        name   = "alpha"
        rogue  = true
        "#,
    );
    assert_eq!(flagged.len(), 1, "exactly `rogue` must be flagged");
}

#[test]
fn missing_document_schema_flags_every_top_level_field() {
    let flagged = assert_agreement(
        r#"
        a = 1
        b = "two"
        c = false
        "#,
    );
    assert_eq!(flagged.len(), 3, "all three fields lack a @document");
}

#[test]
fn schemaless_fields_are_exempt_on_both_paths() {
    let flagged = assert_agreement(
        r#"
        @schemaless a = 1
        @schemaless b = "two"
        "#,
    );
    assert!(flagged.is_empty(), "@schemaless opts out of membership");
}

#[test]
fn schemaless_field_next_to_unknown_field_agrees() {
    let flagged = assert_agreement(
        r#"
        @document type Cfg { name: utf8 }
        name = "alpha"
        @schemaless extra = 1
        rogue = 2
        "#,
    );
    assert_eq!(flagged.len(), 1, "only `rogue` is flagged");
}

#[test]
fn unknown_field_inside_block_flagged_by_both_paths() {
    let flagged = assert_agreement(
        r#"
        @document type Cfg { @children("svc") svcs: list<Svc> }
        @block("svc") type Svc { region: utf8 }
        svc web {
          region     = "us-east-1"
          unexpected = "boom"
        }
        "#,
    );
    assert_eq!(flagged.len(), 1, "exactly `unexpected` must be flagged");
}

#[test]
fn fields_inside_unregistered_block_pass_membership_on_both_paths() {
    // The block kind itself is the strict violation (UnregisteredKind);
    // neither path attributes a *membership* error to the fields inside.
    let flagged = assert_agreement(
        r#"
        @document type Cfg { name: utf8 }
        name = "alpha"
        mystery thing {
          whatever = 1
        }
        "#,
    );
    assert!(
        flagged.is_empty(),
        "fields in an unregistered block carry no membership flags"
    );
}

#[test]
fn type_error_in_declared_field_is_strict_only_and_still_agrees() {
    // `port` is declared but holds the wrong type. The strict path
    // reports FieldTypeMismatch; the lazy membership check passes (the
    // name *is* declared). That asymmetry is by design — membership
    // verdicts still agree because neither path emits UnknownField /
    // NoDocumentSchema here.
    let src = r#"
        @document type Cfg { port: i64 }
        port = "not a number"
    "#;
    let flagged = assert_agreement(src);
    assert!(flagged.is_empty(), "no membership flags: {flagged:?}");

    let doc = open(src);
    assert!(
        doc.schema_errors().iter().any(|e| matches!(
            e,
            EvalError::SchemaViolation {
                kind: SchemaViolationKind::FieldTypeMismatch,
                ..
            }
        )),
        "strict path must still surface the type mismatch"
    );
}

#[test]
fn imported_document_schema_merges_with_root_authored_one() {
    // A library `@document` arrives via a system import; the root
    // declares its own `@document` that composes with it. A field
    // declared by either schema is legal on both paths; a field
    // declared by neither is flagged by both.
    let mut reg = Registry::new();
    reg.register("lib/base.wcl", "@document type Base { title: utf8 }\n");
    let loader = reg.loader(disk_loader());
    let doc = Document::open_at_with_loader(
        r#"
        import <lib/base.wcl>
        @document type Mine { count: i64 }
        title = "from the library schema"
        count = 3
        rogue = false
        "#,
        "merge.wcl",
        None,
        &Environment::new(),
        loader,
    )
    .expect("document with registry import opens");
    let flagged = assert_agreement_doc(&doc, "imported+root @document merge");
    assert_eq!(flagged.len(), 1, "only `rogue` is flagged: {flagged:?}");
}

#[test]
fn imported_library_schema_alone_governs_root_fields() {
    // No root-authored @document at all: the imported one governs.
    let mut reg = Registry::new();
    reg.register("lib/base.wcl", "@document type Base { title: utf8 }\n");
    let loader = reg.loader(disk_loader());
    let doc = Document::open_at_with_loader(
        r#"
        import <lib/base.wcl>
        title = "ok"
        rogue = 1
        "#,
        "lib-only.wcl",
        None,
        &Environment::new(),
        loader,
    )
    .expect("document opens");
    let flagged = assert_agreement_doc(&doc, "imported-only @document");
    assert_eq!(flagged.len(), 1, "only `rogue` is flagged: {flagged:?}");
}

#[test]
fn nested_blocks_two_levels_deep_agree_when_valid() {
    let flagged = assert_agreement(
        r#"
        @document type Cfg { @children("outer") outers: list<Outer> }
        @block("outer") type Outer {
          label: utf8
          @children("inner") inners: list<Inner>
        }
        @block("inner") type Inner { weight: i64 }
        outer a {
          label = "ok"
          inner x { weight = 1 }
        }
        "#,
    );
    assert!(flagged.is_empty(), "valid nested doc must not flag");
}

/// KNOWN DISAGREEMENT — strict validation does not recurse below the
/// first block level.
///
/// `Document::schema_errors()` calls `Block::schema_errors()` only on
/// *top-level* blocks, and `compute_schema_errors` (in
/// `doc/schema_check.rs`) validates a block's own fields and its direct
/// children's kinds/cardinality but never invokes `schema_errors()` on
/// nested blocks. An unknown field two levels deep (`sneaky` inside
/// `inner x` inside `outer a`) is therefore invisible to the strict
/// path — `wcl check` prints OK — while the lazy path
/// (`Field::schema_membership_error`, surfaced through `wcl get
/// outers.a.inners.x.sneaky`) correctly reports "field 'sneaky' is not
/// declared by schema 'Inner'". The doc comment on
/// `Document::schema_errors` ("collected recursively") describes the
/// intended behaviour, not the implemented one.
///
/// Un-ignore once the strict walk recurses into nested blocks.
#[test]
#[ignore = "strict path misses violations below depth 1; lazy path flags them (see comment)"]
fn nested_blocks_two_levels_deep_agree_on_unknown_field() {
    let flagged = assert_agreement(
        r#"
        @document type Cfg { @children("outer") outers: list<Outer> }
        @block("outer") type Outer {
          label: utf8
          @children("inner") inners: list<Inner>
        }
        @block("inner") type Inner { weight: i64 }
        outer a {
          label = "ok"
          inner x {
            weight = 1
            sneaky = "nope"
          }
        }
        "#,
    );
    assert_eq!(flagged.len(), 1, "exactly `sneaky` must be flagged");
}

// ---------------------------------------------------------------------------
// Fixture corpus
// ---------------------------------------------------------------------------

fn wcl_files_under(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            wcl_files_under(&path, out);
        } else if path.extension().is_some_and(|e| e == "wcl") {
            out.push(path);
        }
    }
}

/// Every fixture in `examples/` that opens (files with syntax errors or
/// unresolvable system imports are skipped — agreement is only defined
/// for documents that load) must produce identical strict/lazy
/// membership verdicts. The `examples/errors/` fixtures are valuable
/// here precisely *because* they carry schema violations.
#[test]
fn examples_corpus_agrees() {
    let mut files = Vec::new();
    wcl_files_under(&examples_dir(), &mut files);
    files.sort();
    assert!(!files.is_empty(), "no .wcl fixtures found under examples/");

    let mut checked = 0usize;
    for path in &files {
        // Guard: skip fixtures that don't open (syntax-error fixtures,
        // documents importing <wdoc.wcl> without the wdoc registry, …).
        if let Ok(doc) = Document::from_file(path) {
            assert_agreement_doc(&doc, &path.display().to_string());
            checked += 1;
        }
    }
    assert!(checked > 0, "corpus run checked no documents");
}

// ---------------------------------------------------------------------------
// Property: randomised small documents
// ---------------------------------------------------------------------------

const FIELD_NAMES: &[&str] = &["alpha", "beta", "gamma", "delta", "epsilon"];

proptest! {
    #![proptest_config(ProptestConfig { cases: 128, ..ProptestConfig::default() })]

    /// Generate a small document from a template grammar: an optional
    /// `@document` schema declaring a subset of names, plus fields
    /// drawn from a slightly larger pool (so some are undeclared).
    /// Strict and lazy must flag exactly the undeclared ones — and
    /// when no schema exists, every field.
    #[test]
    fn random_small_documents_agree(
        has_schema in any::<bool>(),
        declared in proptest::collection::btree_set(
            prop::sample::select(FIELD_NAMES), 0..FIELD_NAMES.len()),
        used in proptest::collection::btree_set(
            prop::sample::select(FIELD_NAMES), 0..FIELD_NAMES.len()),
    ) {
        let mut src = String::new();
        if has_schema {
            src.push_str("@document type Root {\n");
            for name in &declared {
                src.push_str(&format!("  {name}: utf8\n"));
            }
            src.push_str("}\n");
        }
        for name in &used {
            src.push_str(&format!("{name} = \"value of {name}\"\n"));
        }

        let doc = Document::open(&src, "prop.wcl").expect("generated source parses");
        let flagged = assert_agreement_doc(&doc, &format!("generated:\n{src}"));

        let expected = if has_schema {
            used.iter().filter(|n| !declared.contains(*n)).count()
        } else {
            used.len()
        };
        prop_assert_eq!(
            flagged.len(),
            expected,
            "wrong number of membership flags for:\n{}",
            src
        );
    }
}
