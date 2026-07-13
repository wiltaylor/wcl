//! Advisory schema warnings (`Document::schema_warnings`).
//!
//! `@document` schemas that co-govern a namespace *merge*
//! (`doc_schemas_for_ns`), and the merge resolves a field name
//! first-wins. When the colliding field is a `@child`/`@children`
//! gather slot, the shadowed schema's gathered blocks silently vanish
//! from templates iterating it — the failure mode that forced the WAD
//! schema's `sw_components` rename. These tests pin the
//! `DocumentFieldShadow` warning: when it fires, where it anchors, and
//! that it never leaks into `schema_errors()` (which gates builds).

use wcl_lang::{Document, Environment, EvalError, Registry, SchemaViolationKind, disk_loader};

fn open_with_lib(lib: &'static str, src: &str) -> Document {
    let mut reg = Registry::new();
    reg.register("lib/base.wcl", lib);
    let loader = reg.loader(disk_loader());
    Document::open_at_with_loader(src, "warnings.wcl", None, &Environment::new(), loader)
        .expect("test source parses")
}

fn shadow_warnings(doc: &Document) -> Vec<(Option<String>, String)> {
    doc.schema_warnings()
        .into_iter()
        .map(|w| match w {
            EvalError::SchemaViolation {
                kind: SchemaViolationKind::DocumentFieldShadow,
                detail,
                message,
                ..
            } => (detail, message),
            other => panic!("schema_warnings emitted a non-shadow diagnostic: {other}"),
        })
        .collect()
}

#[test]
fn root_gather_field_shadowing_imported_one_warns() {
    // The user's root @document declares a gather field named like the
    // library's — `each = components` in a template resolves to only
    // one of them.
    let lib = r#"
        @block("component")
        type LibComponent { name: utf8 }
        @document
        type LibDoc {
            @children("component") components: list<LibComponent>
        }
    "#;
    let src = r#"
        import <lib/base.wcl>
        @block("part")
        type Part { name: utf8 }
        @document
        type Mine {
            @children("part") components: list<Part>
        }
    "#;
    let doc = open_with_lib(lib, src);
    let warns = shadow_warnings(&doc);
    assert_eq!(warns.len(), 1, "exactly one shadow warning: {warns:?}");
    let (detail, message) = &warns[0];
    assert_eq!(detail.as_deref(), Some("components"));
    assert!(
        message.contains("Mine") && message.contains("LibDoc"),
        "message names both schemas: {message}"
    );

    // The warning anchors at the root-authored decl's field — its span
    // must lie inside the root source.
    let w = &doc.schema_warnings()[0];
    let EvalError::SchemaViolation { span, .. } = w else {
        panic!("shadow warning is a SchemaViolation");
    };
    let field_off = src.find("components").expect("field present in root src");
    assert_eq!(
        span.offset(),
        field_off,
        "warning anchors at the root-authored field"
    );
}

#[test]
fn two_imported_schemas_shadowing_each_other_warn() {
    // The WAD shape: both @documents live in imported files (the base
    // schema is imported from disk too), so a root-vs-imported rule
    // would miss this. The warning anchors at the later declaration.
    let mut reg = Registry::new();
    reg.register(
        "lib/first.wcl",
        r#"
        @block("widget")
        type Widget { name: utf8 }
        @document
        type FirstDoc {
            @children("widget") items: list<Widget>
        }
        "#,
    );
    reg.register(
        "lib/second.wcl",
        r#"
        @block("gizmo")
        type Gizmo { name: utf8 }
        @document
        type SecondDoc {
            @children("gizmo") items: list<Gizmo>
        }
        "#,
    );
    let loader = reg.loader(disk_loader());
    let doc = Document::open_at_with_loader(
        "import <lib/first.wcl>\nimport <lib/second.wcl>\n",
        "warnings.wcl",
        None,
        &Environment::new(),
        loader,
    )
    .expect("test source parses");
    let warns = shadow_warnings(&doc);
    assert_eq!(warns.len(), 1, "exactly one shadow warning: {warns:?}");
    assert_eq!(warns[0].0.as_deref(), Some("items"));
}

#[test]
fn distinct_field_names_do_not_warn() {
    let lib = r#"
        @block("component")
        type LibComponent { name: utf8 }
        @document
        type LibDoc {
            @children("component") components: list<LibComponent>
        }
    "#;
    let src = r#"
        import <lib/base.wcl>
        @block("part")
        type Part { name: utf8 }
        @document
        type Mine {
            @children("part") parts: list<Part>
        }
    "#;
    let doc = open_with_lib(lib, src);
    assert!(doc.schema_warnings().is_empty());
}

// NOTE: there is no test for two root-authored @documents in disjoint
// namespaces (the only non-co-governing pair shape): `namespace` is a
// file-level header, so every root-authored decl shares the root's
// file_ns — the shape isn't constructible from source. The pairwise
// co-governance guard still matters for synthetic (host-registered)
// types, whose file_ns is empty.

#[test]
fn scalar_scalar_collision_does_not_warn() {
    // Both sides declare a plain scalar of the same name (the
    // `schema_version: utf8` pattern) — benign, deliberately unflagged.
    let lib = r#"
        @document
        type LibDoc { schema_version: utf8 }
    "#;
    let src = r#"
        import <lib/base.wcl>
        @document
        type Mine { schema_version: utf8 }
        schema_version = "1.0.0"
    "#;
    let doc = open_with_lib(lib, src);
    assert!(
        doc.schema_warnings().is_empty(),
        "scalar/scalar collisions are not gather shadows: {:?}",
        doc.schema_warnings()
    );
}

#[test]
fn shadow_warning_never_appears_in_schema_errors() {
    let lib = r#"
        @block("component")
        type LibComponent { name: utf8 }
        @document
        type LibDoc {
            @children("component") components: list<LibComponent>
        }
    "#;
    let src = r#"
        import <lib/base.wcl>
        @block("part")
        type Part { name: utf8 }
        @document
        type Mine {
            @children("part") components: list<Part>
        }
    "#;
    let doc = open_with_lib(lib, src);
    assert!(!doc.schema_warnings().is_empty(), "warning fires");
    assert!(
        doc.schema_errors().is_empty(),
        "warnings must not gate: {:?}",
        doc.schema_errors()
    );
}
