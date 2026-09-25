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
//! `Field::value()`. The same holds for the one type check a read runs:
//! a number that does not fit its declared type (`FieldTypeMismatch`
//! for `300` in a `u8`, `2.5` in an integer, an out-of-range list
//! element) fails the read.
//!
//! Every other type-level check (a string in an `i64`, variant
//! mismatches, …) is strict-only and excluded from the comparison; one
//! test below documents that asymmetry explicitly.
//!
//! A verdict is a (file, offset, message) triple, not an offset: both
//! paths must also agree on *which file* a violation belongs to, and
//! that file must be the one the field was written in — two files can
//! hold a field at the same offset — and on the violation itself.

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
fn is_membership_kind(kind: SchemaViolationKind) -> bool {
    matches!(
        kind,
        SchemaViolationKind::UnknownField | SchemaViolationKind::NoDocumentSchema
    )
}

/// Whether `error` is one both paths report: a membership violation, or
/// a number that does not fit its declared type. Everything else (other
/// type mismatches, child counts, kind registration, …) is strict-only
/// by design and excluded from the comparison.
fn is_compared(error: &EvalError) -> bool {
    match error {
        EvalError::SchemaViolation { kind, .. } if is_membership_kind(*kind) => true,
        EvalError::SchemaViolation {
            kind: SchemaViolationKind::FieldTypeMismatch,
            message,
            ..
        } => message.contains("is out of range for") || message.contains("is not a whole number"),
        _ => false,
    }
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

/// Where a violation points: the name of the source its span indexes
/// into, and the span's start.
type Site = (String, usize);

/// A violation as one path reports it: where it points, and what it says.
type Verdict = (Site, String);

/// The site of a field: the file it was written in (named as the
/// document names its sources) and its span start.
fn field_site(doc: &Document, field: &Field<'_>) -> Site {
    let file = match field.source_path() {
        Some(path) => path.display().to_string(),
        None => doc.source().name().to_string(),
    };
    (file, field.span().start)
}

/// Lazy verdict for one field: its site and message if `Field::value()`
/// fails with a compared violation attributed to *this* field. The
/// error's own source must name the field's file, and its span must
/// start at the field (so errors merely propagated from evaluating
/// another field's reference don't count against this one).
fn lazy_flag(doc: &Document, field: &Field<'_>) -> Option<Verdict> {
    match field.value() {
        Err(e @ EvalError::SchemaViolation { span, .. }) if is_compared(e) => {
            let site = (e.schema_source()?.name().to_string(), span.offset());
            (site == field_site(doc, field)).then(|| (site, e.to_string()))
        }
        _ => None,
    }
}

/// Strict verdict: the site and message of every compared violation in
/// `schema_diagnostics()` that points at a known literal field
/// (top-level blocks also produce `NoDocumentSchema`; restricting to
/// field sites keeps the comparison field-vs-field). The site's file is
/// the source the strict path paired the error with.
fn strict_flags(doc: &Document, field_sites: &BTreeSet<Site>) -> BTreeSet<Verdict> {
    doc.schema_diagnostics()
        .iter()
        .filter_map(|(e, source)| match e {
            EvalError::SchemaViolation { span, .. } if is_compared(e) => {
                let site = (source.as_ref()?.name().to_string(), span.offset());
                field_sites.contains(&site).then(|| (site, e.to_string()))
            }
            _ => None,
        })
        .collect()
}

/// Every strict violation read from a file names that file, and the
/// source it is paired with is the one it carries.
fn assert_strict_sources(doc: &Document, label: &str) {
    for (error, source) in doc.schema_diagnostics() {
        if let EvalError::SchemaViolation { .. } = &error {
            let carried = error.schema_source().map(|s| s.name().to_string());
            let paired = source.as_ref().map(|s| s.name().to_string());
            assert_eq!(
                carried, paired,
                "{label}: `{error}` carries one source and is paired with another"
            );
        }
    }
}

/// Assert that the strict and lazy paths flag exactly the same set of
/// fields with compared violations, each against the same file and with
/// the same message. Returns the agreed set of sites so callers can
/// additionally assert on expected counts and files.
fn assert_agreement_doc(doc: &Document, label: &str) -> BTreeSet<Site> {
    let fields = collect_fields(doc);
    let field_sites: BTreeSet<Site> = fields.iter().map(|f| field_site(doc, f)).collect();

    // Lazy first: `value()` caches its result, and the strict walk
    // tolerates already-cached errors, so this order also exercises
    // the cache interplay between the two paths.
    let lazy: BTreeSet<Verdict> = fields.iter().filter_map(|f| lazy_flag(doc, f)).collect();
    let strict = strict_flags(doc, &field_sites);
    assert_strict_sources(doc, label);

    let name_of = |(site, message): &Verdict| {
        fields
            .iter()
            .find(|f| field_site(doc, f) == *site)
            .map(|f| format!("{} ({}): {message}", f.name(), site.0))
            .unwrap_or_else(|| format!("<{} at {}>: {message}", site.0, site.1))
    };
    let strict_only: Vec<String> = strict.difference(&lazy).map(name_of).collect();
    let lazy_only: Vec<String> = lazy.difference(&strict).map(name_of).collect();
    assert!(
        strict_only.is_empty() && lazy_only.is_empty(),
        "strict/lazy schema verdicts disagree for {label}:\n  \
         flagged by strict only: {strict_only:?}\n  \
         flagged by lazy only:   {lazy_only:?}"
    );
    strict.into_iter().map(|(site, _)| site).collect()
}

fn assert_agreement(src: &str) -> BTreeSet<Site> {
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
fn violations_in_an_imported_file_name_that_file_on_both_paths() {
    // `data.wcl` holds an undeclared top-level field and an undeclared
    // block field. Both paths must report them against `data.wcl`, not
    // the root that imported it. The root's own `rogue` sits at the
    // same offset as `data.wcl`'s, so an offset-only verdict could not
    // tell them apart.
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("types.wcl"),
        "@block(\"server\") type Server { port: u16 }\n\
         @document type Root {\n  title: utf8\n  @children(\"server\") servers: list<Server>\n}\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("data.wcl"),
        "rogue = 1\nserver web {\n  port = 80\n  colour = \"red\"\n}\n",
    )
    .unwrap();
    let root = dir.path().join("main.wcl");
    std::fs::write(
        &root,
        "rogue = 2\nimport \"./types.wcl\"\nimport \"./data.wcl\"\n",
    )
    .unwrap();
    let doc = Document::from_file(&root).expect("document opens");
    let flagged = assert_agreement_doc(&doc, "violations in an imported file");
    let data = dir.path().join("data.wcl").canonicalize().unwrap();
    let in_data: Vec<&Site> = flagged
        .iter()
        .filter(|(file, _)| Path::new(file).canonicalize().ok().as_ref() == Some(&data))
        .collect();
    assert_eq!(flagged.len(), 3, "both `rogue`s and `colour`: {flagged:?}");
    assert_eq!(in_data.len(), 2, "data.wcl's two fields: {flagged:?}");
}

#[test]
fn same_namespace_kind_across_files_agrees_regardless_of_import_order() {
    // A `@block("decision")` declared in one `namespace lib2` file
    // schemas an instance in *another* lib2 file, while a second
    // imported library declares a colliding `decision` kind. Bare-kind
    // resolution must prefer the instance file's own namespace on both
    // paths (the lazy path rebuilds the block from a scope frame), for
    // either import order — `name` is only legal on lib2's schema, so
    // a wrong winner flags it.
    let colliding = r#"
        namespace other
        @document type OtherRoot { @children("card") cards: list<Card> }
        @block("card") type Card { @inline(0) id: utf8 }
        @block("decision") type OtherDecision { @inline(0) id: utf8  shape: utf8 }
    "#;
    let lib2_schema = r#"
        namespace lib2
        @block("decision") type Decision { @inline(0) id: utf8  name: utf8 }
        @document type Model2 { @children("decision") decisions: list<Decision> }
    "#;
    let lib2_data = "namespace lib2\ndecision \"d1\" { name = \"First\" }\n";
    for user in [
        "import <colliding.wcl>\nimport <lib2_schema.wcl>\nimport <lib2_data.wcl>\n",
        "import <lib2_schema.wcl>\nimport <lib2_data.wcl>\nimport <colliding.wcl>\n",
    ] {
        let mut reg = Registry::new();
        reg.register("colliding.wcl", colliding);
        reg.register("lib2_schema.wcl", lib2_schema);
        reg.register("lib2_data.wcl", lib2_data);
        let loader = reg.loader(disk_loader());
        let doc =
            Document::open_at_with_loader(user, "ns-kind.wcl", None, &Environment::new(), loader)
                .expect("document opens");
        let flagged = assert_agreement_doc(&doc, &format!("ns-scoped kind, root:\n{user}"));
        assert!(
            flagged.is_empty(),
            "no field is flagged for root:\n{user}\n{flagged:?}"
        );
    }
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

/// Regression test: strict validation must recurse below the first
/// block level. `compute_schema_errors` used to validate a block's own
/// fields and its direct children's kinds/cardinality but never invoked
/// `schema_errors()` on nested blocks, so an unknown field two levels
/// deep (`sneaky` inside `inner x` inside `outer a`) passed `wcl check`
/// while the lazy path correctly rejected it.
#[test]
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

/// The lazy membership error a field's `value()` reports, rendered.
fn lazy_membership_message(field: &Field<'_>) -> Option<String> {
    match field.value() {
        Err(e @ EvalError::SchemaViolation { kind, .. }) if is_membership_kind(*kind) => {
            Some(e.to_string())
        }
        _ => None,
    }
}

#[test]
fn membership_errors_are_worded_identically_on_both_paths() {
    // Both paths build the error through one helper, so the message a
    // host sees from `value()` is the one `wcl check` prints.
    for src in [
        "rogue = 1\n",
        "@document type Cfg { name: utf8 }\nrogue = 1\n",
        r#"
        @document type Cfg { @children("svc") svcs: list<Svc> }
        @block("svc") type Svc { region: utf8 }
        svc web { rogue = 1 }
        "#,
    ] {
        let doc = open(src);
        let lazy: Vec<String> = collect_fields(&doc)
            .iter()
            .filter_map(lazy_membership_message)
            .collect();
        let strict: Vec<String> = doc
            .schema_errors()
            .iter()
            .filter(|e| {
                matches!(e, EvalError::SchemaViolation { kind, .. } if is_membership_kind(*kind))
            })
            .map(ToString::to_string)
            .filter(|m| m.contains("'rogue'"))
            .collect();
        assert_eq!(lazy.len(), 1, "{src}: {lazy:?}");
        assert_eq!(lazy, strict, "{src}");
    }
}

#[test]
fn numeric_misfits_are_flagged_by_both_paths() {
    // A number that does not fit its declared type fails the read with
    // the FieldTypeMismatch the strict path reports, at the same field
    // and in the same words — whichever path runs first.
    let src = r#"
        @document type Cfg { port: u16  ratio: u8  sizes: list<u8>  label: utf8  @children("svc") svcs: list<Svc> }
        @block("svc") type Svc { weight: u8 }
        port = 70000
        ratio = 2.5
        sizes = [1, 256]
        label = "fits"
        svc web { weight = 256 }
    "#;
    let flagged = assert_agreement(src);
    assert_eq!(flagged.len(), 4, "every misfit but `label`: {flagged:?}");

    let doc = open(src);
    assert_eq!(doc.schema_diagnostics().len(), 4);
    let flagged = assert_agreement_doc(&doc, "strict path first");
    assert_eq!(flagged.len(), 4, "{flagged:?}");
    assert_eq!(
        doc.get("port").unwrap().value().unwrap_err().to_string(),
        "field 'port' declared as u16 but value 70000 is out of range for u16"
    );
}

#[test]
fn numeric_misfits_in_an_imported_file_name_that_file_on_both_paths() {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(
        dir.path().join("data.wcl"),
        "port = 70000\nserver web {\n  port = 65536\n}\n",
    )
    .unwrap();
    let root = dir.path().join("main.wcl");
    std::fs::write(
        &root,
        "@block(\"server\") type Server { port: u16 }\n\
         @document type Root {\n  port: u16\n  @children(\"server\") servers: list<Server>\n}\n\
         import \"./data.wcl\"\n",
    )
    .unwrap();
    let doc = Document::from_file(&root).expect("document opens");
    let flagged = assert_agreement_doc(&doc, "misfits in an imported file");
    let data = dir.path().join("data.wcl").canonicalize().unwrap();
    assert_eq!(flagged.len(), 2, "{flagged:?}");
    assert!(
        flagged
            .iter()
            .all(|(file, _)| Path::new(file).canonicalize().ok().as_ref() == Some(&data)),
        "both belong to data.wcl: {flagged:?}"
    );
}

#[test]
fn fitting_numbers_read_back_as_the_declared_type_on_both_paths() {
    let src = r#"
        @document type Cfg { port: u16  @children("svc") svcs: list<Svc> }
        @block("svc") type Svc { weight: u8 }
        port = 8080
        svc web { weight = 3 }
    "#;
    let flagged = assert_agreement(src);
    assert!(flagged.is_empty());
    let doc = open(src);
    assert!(doc.schema_errors().is_empty(), "{:#?}", doc.schema_errors());
    assert_eq!(
        doc.get("port").unwrap().value().unwrap(),
        wcl_lang::Value::U16(8080)
    );
    let web = doc.blocks().next().expect("svc web");
    assert_eq!(
        web.field("weight").unwrap().value().unwrap(),
        &wcl_lang::Value::U8(3)
    );
}

#[test]
fn string_kind_slot_claims_its_blocks_before_union_dispatch() {
    // Regression: a schema with both `@child("config")` and
    // `@children(Shape)` sent the `config` block to union dispatch too,
    // which reported "no variant of 'Shape' matches". The string-kind
    // slot claims it; only the other blocks are dispatched.
    let src = r#"
        union Shape { Circle { radius: f64 } Square { side: f64 } }
        @document type Root { @children("mixed") mixes: list<Mixed> }
        @block("config") type ConfigSpec { name: utf8 }
        @block("mixed") type Mixed {
          @child("config") cfg: ConfigSpec
          @children(Shape) shapes: list<Shape>
        }
        mixed "demo" {
          config { name = "alpha" }
          circle { radius = 3.0 }
        }
    "#;
    let flagged = assert_agreement(src);
    assert!(flagged.is_empty());
    let doc = open(src);
    assert!(doc.schema_errors().is_empty(), "{:#?}", doc.schema_errors());
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

/// Documents under `examples/` that cannot open as a plain wcl
/// document, relative to `examples/`. Agreement is only defined for
/// documents that load, so these are skipped — and every entry must
/// still fail to open, so the list cannot hide a fixture that has
/// since started loading.
const UNOPENABLE: &[&str] = &[
    // Import `<wdoc.wcl>`, which only the wdoc registry provides.
    "wdoc/main.wcl",
    "wdoc_relocatable/main.wcl",
    "wdoc_template.wcl",
    "wdoc_website.wcl",
    // Page fragments naming wdoc types, resolved only when
    // `wdoc/main.wcl` imports them.
    "wdoc/pages/data.wcl",
    "wdoc/pages/terminal.wcl",
];

/// Whether `path` is expected not to open: listed in [`UNOPENABLE`],
/// or an `examples/errors/` fixture declaring `// expect-exit: 1` (the
/// parse-or-load failure code `crates/wcl/tests/examples.rs` checks).
fn expected_unopenable(root: &Path, path: &Path) -> bool {
    let rel = path
        .strip_prefix(root)
        .expect("fixture under examples/")
        .to_string_lossy()
        .replace('\\', "/");
    if UNOPENABLE.contains(&rel.as_str()) {
        return true;
    }
    rel.starts_with("errors/")
        && std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
            .lines()
            .any(|l| l.trim() == "// expect-exit: 1")
}

/// Every fixture in `examples/` must open and produce identical
/// strict/lazy membership verdicts, except those
/// [`expected_unopenable`]. The `examples/errors/` fixtures are valuable
/// here precisely *because* they carry schema violations.
#[test]
fn examples_corpus_agrees() {
    let root = examples_dir();
    let mut files = Vec::new();
    wcl_files_under(&root, &mut files);
    files.sort();
    assert!(!files.is_empty(), "no .wcl fixtures found under examples/");

    let listed = |path: &Path| expected_unopenable(&root, path);

    let mut checked = 0usize;
    let mut unexpected = Vec::new();
    for path in &files {
        match (Document::from_file(path), listed(path)) {
            (Ok(doc), false) => {
                assert_agreement_doc(&doc, &path.display().to_string());
                checked += 1;
            }
            (Err(e), false) => unexpected.push(format!("{} failed to open: {e}", path.display())),
            (Ok(_), true) => unexpected.push(format!(
                "{} opens but is listed in UNOPENABLE — remove it from the list",
                path.display()
            )),
            (Err(_), true) => {}
        }
    }
    assert!(unexpected.is_empty(), "{}", unexpected.join("\n"));
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
