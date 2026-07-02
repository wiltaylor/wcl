//! `wcl diff <old> <new>` — a WCL-aware document diff.
//!
//! Compares the *evaluated* document views (not the source text), so it is
//! robust to formatting-only churn: a whole-file reformat that doesn't
//! change any value produces an empty diff. Each top-level block is an
//! **entity**, keyed `kind:label` (its first label / id); top-level bare
//! fields fold into a synthetic `<document>` entity. Within an entity the
//! reified record is deep-compared, so changes are reported at field-path
//! granularity (`fields.due_date`), recursing into lists by index
//! (`tags[2]`).
//!
//! The diff renders two ways. The default is a re-parseable **WCL tree**
//! (one `added` / `removed` / `modified` block per entity, carrying the
//! actual old/new values); `--format json` emits the historical flat array
//! of change objects, each now also carrying `old` / `new` / `value`:
//! ```json
//! [
//!   {"op":"modified","entity":"domain_entity:task","field":"fields.due_date","kind":"added","new":"2026"},
//!   {"op":"added","entity":"spec:impl_due_dates","value":{"id":"impl_due_dates"}}
//! ]
//! ```
//!
//! ## Intentionally deferred
//! - Tensors are diffed as opaque leaves (a single `changed`); only lists
//!   recurse element-wise.
//! - List diffing is index-based, so reordering a list's elements reads as
//!   per-index churn rather than a move.

use std::collections::BTreeMap;

use serde_json::{Value as Json, json};
use wcl_lang::{Block, Document, Value};

/// Entity-level operation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ChangeOp {
    Added,
    Removed,
    Modified,
}

impl ChangeOp {
    fn as_str(self) -> &'static str {
        match self {
            ChangeOp::Added => "added",
            ChangeOp::Removed => "removed",
            ChangeOp::Modified => "modified",
        }
    }
}

/// Per-field operation inside a modified entity.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum FieldKind {
    Added,
    Removed,
    Changed,
}

impl FieldKind {
    fn as_str(self) -> &'static str {
        match self {
            FieldKind::Added => "added",
            FieldKind::Removed => "removed",
            FieldKind::Changed => "changed",
        }
    }
}

/// One reported change for an entity. `entity_value` is the whole reified
/// record for `Added` / `Removed`; `fields` carries the per-field edits for
/// `Modified` (and is empty otherwise).
#[derive(Debug, PartialEq)]
pub(crate) struct Change {
    op: ChangeOp,
    /// Entity key — `kind:label` for a block, or `<document>` for the
    /// top-level field group.
    entity: String,
    /// Whole-entity snapshot, for `Added` / `Removed`.
    entity_value: Option<Value>,
    /// Per-field edits, for `Modified`.
    fields: Vec<FieldChange>,
}

/// A single field-path edit within a modified entity, carrying the actual
/// old/new values (the absent side is `None` for an add/remove).
#[derive(Debug, PartialEq)]
pub(crate) struct FieldChange {
    /// Dotted / indexed field path within the entity (`fields.tags[2]`).
    path: String,
    kind: FieldKind,
    old: Option<Value>,
    new: Option<Value>,
}

impl Change {
    /// Entity key (`kind:label`, bare `kind`, or `<document>`).
    pub(crate) fn entity(&self) -> &str {
        &self.entity
    }
}

/// The synthetic entity key under which top-level bare fields are diffed.
const DOCUMENT_ENTITY: &str = "<document>";

/// Compute the entity/field diff between two evaluated documents.
/// Entities present on only one side become a single `Added` / `Removed`
/// change; entities on both sides are deep-compared field by field and, if
/// anything differs, yield one `Modified` change carrying the edits.
pub(crate) fn diff_documents(old: &Document, new: &Document) -> Vec<Change> {
    let old_entities = collect_entities(old);
    let new_entities = collect_entities(new);

    // Union of entity keys, in deterministic (sorted) order.
    let mut keys: Vec<&String> = old_entities.keys().chain(new_entities.keys()).collect();
    keys.sort_unstable();
    keys.dedup();

    let mut changes = Vec::new();
    for key in keys {
        match (old_entities.get(key), new_entities.get(key)) {
            (None, Some(v)) => changes.push(Change {
                op: ChangeOp::Added,
                entity: key.clone(),
                entity_value: Some(v.clone()),
                fields: Vec::new(),
            }),
            (Some(v), None) => changes.push(Change {
                op: ChangeOp::Removed,
                entity: key.clone(),
                entity_value: Some(v.clone()),
                fields: Vec::new(),
            }),
            (Some(old_val), Some(new_val)) => {
                let mut fields = Vec::new();
                diff_values(old_val, new_val, String::new(), &mut fields);
                if !fields.is_empty() {
                    changes.push(Change {
                        op: ChangeOp::Modified,
                        entity: key.clone(),
                        entity_value: None,
                        fields,
                    });
                }
            }
            (None, None) => unreachable!("key came from one of the maps"),
        }
    }
    changes
}

/// Reify a document's top-level blocks (entities) and bare fields into a
/// `key -> Value` map. A block reifies to its schema-projected record; the
/// bare top-level fields reify to one `<document>` record. A block whose
/// value can't be evaluated is skipped with a stderr note (so a partial
/// document still diffs the rest rather than aborting).
fn collect_entities(doc: &Document) -> BTreeMap<String, Value> {
    let mut out: BTreeMap<String, Value> = BTreeMap::new();

    for block in doc.blocks() {
        let key = entity_key(&block, &out);
        match block.to_record_value() {
            Ok(v) => {
                out.insert(key, v);
            }
            Err(e) => {
                eprintln!("warning: entity '{key}' could not be evaluated, skipping: {e}");
            }
        }
    }

    // Top-level bare fields → a single synthetic entity, so a changed
    // document-level field isn't silently dropped.
    let mut doc_fields: BTreeMap<String, Value> = BTreeMap::new();
    for f in doc.fields() {
        if let Ok(v) = f.value() {
            doc_fields.insert(f.name().to_string(), v.clone());
        }
    }
    if !doc_fields.is_empty() {
        out.insert(
            DOCUMENT_ENTITY.to_string(),
            Value::Record {
                ty: Vec::new(),
                fields: std::sync::Arc::new(doc_fields),
            },
        );
    }
    out
}

/// Stable identity for a block entity: `kind:firstlabel`, or bare `kind`
/// when it has no label. Collisions (repeated unlabeled kinds, duplicate
/// ids) are disambiguated with a `#n` suffix so no entity is lost.
fn entity_key(block: &Block<'_>, taken: &BTreeMap<String, Value>) -> String {
    let base = match block.labels().ok().and_then(|ls| ls.into_iter().next()) {
        Some(Value::Identifier(s) | Value::Utf8(s) | Value::Ascii(s)) => {
            format!("{}:{}", block.kind(), s)
        }
        _ => block.kind().to_string(),
    };
    if !taken.contains_key(&base) {
        return base;
    }
    (2..)
        .map(|n| format!("{base}#{n}"))
        .find(|k| !taken.contains_key(k))
        .expect("infinite suffix sequence yields a free key")
}

/// Deep-compare two values, appending a `FieldChange` for every differing
/// leaf or sub-record/element. Records recurse key by key and lists recurse
/// by index (a key/index on only one side is `Added`/`Removed`); any other
/// unequal pair — scalars, variants, tensors, type mismatch — is a single
/// `Changed` at `path`. Equal values contribute nothing (so formatting-only
/// churn is invisible).
fn diff_values(old: &Value, new: &Value, path: String, out: &mut Vec<FieldChange>) {
    if old == new {
        return;
    }
    match (old, new) {
        (Value::Record { fields: a, .. }, Value::Record { fields: b, .. }) => {
            let mut keys: Vec<&String> = a.keys().chain(b.keys()).collect();
            keys.sort_unstable();
            keys.dedup();
            for k in keys {
                let child = join(&path, k);
                match (a.get(k), b.get(k)) {
                    (Some(av), Some(bv)) => diff_values(av, bv, child, out),
                    (None, Some(bv)) => out.push(FieldChange {
                        path: child,
                        kind: FieldKind::Added,
                        old: None,
                        new: Some(bv.clone()),
                    }),
                    (Some(av), None) => out.push(FieldChange {
                        path: child,
                        kind: FieldKind::Removed,
                        old: Some(av.clone()),
                        new: None,
                    }),
                    (None, None) => unreachable!("key came from one of the maps"),
                }
            }
        }
        (Value::List(a), Value::List(b)) => {
            for i in 0..a.len().max(b.len()) {
                let child = index(&path, i);
                match (a.get(i), b.get(i)) {
                    (Some(av), Some(bv)) => diff_values(av, bv, child, out),
                    (None, Some(bv)) => out.push(FieldChange {
                        path: child,
                        kind: FieldKind::Added,
                        old: None,
                        new: Some(bv.clone()),
                    }),
                    (Some(av), None) => out.push(FieldChange {
                        path: child,
                        kind: FieldKind::Removed,
                        old: Some(av.clone()),
                        new: None,
                    }),
                    (None, None) => unreachable!("index below the longer length"),
                }
            }
        }
        // An optional field reifies as `none` when unset, so a none→value
        // edit reads as the field being *added* (and value→none as
        // removed) rather than a bland "changed".
        (Value::None, _) => out.push(FieldChange {
            path,
            kind: FieldKind::Added,
            old: None,
            new: Some(new.clone()),
        }),
        (_, Value::None) => out.push(FieldChange {
            path,
            kind: FieldKind::Removed,
            old: Some(old.clone()),
            new: None,
        }),
        _ => out.push(FieldChange {
            path,
            kind: FieldKind::Changed,
            old: Some(old.clone()),
            new: Some(new.clone()),
        }),
    }
}

/// Join a dotted field path with a child key.
fn join(path: &str, seg: &str) -> String {
    if path.is_empty() {
        seg.to_string()
    } else {
        format!("{path}.{seg}")
    }
}

/// Append a list index to a field path (`tags` + 2 → `tags[2]`).
fn index(path: &str, i: usize) -> String {
    format!("{path}[{i}]")
}

// ---------------------------------------------------------------------------
// JSON rendering (the historical `--format json` shape)
// ---------------------------------------------------------------------------

/// Render the changes as the flat JSON array. Each `Modified` entity expands
/// to one object per edited field (`{op,entity,field,kind,old?,new?}`); an
/// `Added` / `Removed` entity is one object carrying the whole `value`.
pub(crate) fn changes_to_json(changes: &[Change]) -> Json {
    let mut arr = Vec::new();
    for c in changes {
        match c.op {
            ChangeOp::Added | ChangeOp::Removed => {
                let mut obj = serde_json::Map::new();
                obj.insert("op".to_string(), json!(c.op.as_str()));
                obj.insert("entity".to_string(), json!(c.entity));
                if let Some(v) = &c.entity_value {
                    obj.insert(
                        "value".to_string(),
                        serde_json::to_value(v).unwrap_or(Json::Null),
                    );
                }
                arr.push(Json::Object(obj));
            }
            ChangeOp::Modified => {
                for f in &c.fields {
                    let mut obj = serde_json::Map::new();
                    obj.insert("op".to_string(), json!("modified"));
                    obj.insert("entity".to_string(), json!(c.entity));
                    obj.insert(
                        "field".to_string(),
                        json!(if f.path.is_empty() {
                            "<value>"
                        } else {
                            &f.path
                        }),
                    );
                    obj.insert("kind".to_string(), json!(f.kind.as_str()));
                    if let Some(o) = &f.old {
                        obj.insert(
                            "old".to_string(),
                            serde_json::to_value(o).unwrap_or(Json::Null),
                        );
                    }
                    if let Some(n) = &f.new {
                        obj.insert(
                            "new".to_string(),
                            serde_json::to_value(n).unwrap_or(Json::Null),
                        );
                    }
                    arr.push(Json::Object(obj));
                }
            }
        }
    }
    Json::Array(arr)
}

// ---------------------------------------------------------------------------
// WCL rendering (the default output)
// ---------------------------------------------------------------------------

/// Render the changes as a re-parseable WCL document. Entity keys and field
/// paths contain `:` / `.` / `[]`, so they are emitted as quoted string
/// labels; `kind` is a WCL symbol (`:changed`). An empty diff renders as a
/// comment-only document. The output is guaranteed to parse (validated by
/// the round-trip unit test); it is a report, not a faithful reconstruction
/// — see [`value_to_wcl`].
pub(crate) fn render_wcl(changes: &[Change], old_label: &str, new_label: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# wcl diff {old_label} -> {new_label} — generated\n"
    ));
    if changes.is_empty() {
        out.push_str("# no changes\n");
        return out;
    }
    out.push('\n');
    for c in changes {
        match c.op {
            ChangeOp::Modified => {
                out.push_str(&format!("modified {} {{\n", quote_wcl(&c.entity)));
                for f in &c.fields {
                    let path = if f.path.is_empty() {
                        "<value>"
                    } else {
                        &f.path
                    };
                    out.push_str(&format!("  field {} {{\n", quote_wcl(path)));
                    out.push_str(&format!("    kind = :{}\n", f.kind.as_str()));
                    if let Some(o) = &f.old {
                        out.push_str(&format!("    old = {}\n", value_to_wcl(o)));
                    }
                    if let Some(n) = &f.new {
                        out.push_str(&format!("    new = {}\n", value_to_wcl(n)));
                    }
                    out.push_str("  }\n");
                }
                out.push_str("}\n\n");
            }
            ChangeOp::Added | ChangeOp::Removed => {
                out.push_str(&format!("{} {} {{\n", c.op.as_str(), quote_wcl(&c.entity)));
                if let Some(v) = &c.entity_value {
                    out.push_str(&format!("  value = {}\n", value_to_wcl(v)));
                }
                out.push_str("}\n\n");
            }
        }
    }
    out
}

/// Render a `Value` as a WCL expression that is guaranteed to parse.
///
/// Lists and records are emitted structurally (records as *bare* record
/// literals — the reified `ty` prefix, e.g. `Entity { … }`, is dropped
/// because `TypeName { … }` is not an expression). Scalars, strings,
/// identifiers, symbols and `none` use their round-trippable `Display`.
/// Forms whose `Display` does not re-parse as an expression — variants,
/// tensors, functions, data-paths, and the empty record (`{}` is a block,
/// not a record) — are quoted as a string: the diff is a report, not a
/// rebuild.
fn value_to_wcl(v: &Value) -> String {
    match v {
        Value::List(items) => {
            let inner: Vec<String> = items.iter().map(value_to_wcl).collect();
            format!("[{}]", inner.join(", "))
        }
        Value::Record { fields, .. } => {
            if fields.is_empty() {
                quote_wcl(&v.to_string())
            } else {
                let inner: Vec<String> = fields
                    .iter()
                    .map(|(k, val)| format!("{k}: {}", value_to_wcl(val)))
                    .collect();
                format!("{{ {} }}", inner.join(", "))
            }
        }
        Value::Bool(_)
        | Value::I8(_)
        | Value::I16(_)
        | Value::I32(_)
        | Value::I64(_)
        | Value::I128(_)
        | Value::Isize(_)
        | Value::U8(_)
        | Value::U16(_)
        | Value::U32(_)
        | Value::U64(_)
        | Value::U128(_)
        | Value::Usize(_)
        | Value::F32(_)
        | Value::F64(_)
        | Value::Utf8(_)
        | Value::Ascii(_)
        | Value::Utf16(_)
        | Value::Utf32(_)
        | Value::Identifier(_)
        | Value::Symbol(_)
        | Value::None => v.to_string(),
        Value::Variant { .. }
        | Value::Tensor { .. }
        | Value::Function(_)
        | Value::DataPath { .. }
        // A resolved document never carries an unresolved unit literal;
        // quote it defensively rather than emit a non-re-parseable form.
        | Value::PendingUnit { .. } => quote_wcl(&v.to_string()),
    }
}

/// Render the changes as the `change` blocks of a WAD spec skeleton (see
/// `wad::run_spec`), matching the WAD schema's `SpecChange` / `FieldChange`
/// shape: the entity key is the inline label, `op` a symbol, and every value
/// is carried as *rendered WCL text* (a string), so arbitrary value shapes
/// stay schema-valid and legible. Indented two spaces to sit inside the
/// `spec` block.
pub(crate) fn render_spec_changes(changes: &[Change]) -> String {
    let mut out = String::new();
    for c in changes {
        out.push_str(&format!("  change {} {{\n", quote_wcl(&c.entity)));
        out.push_str(&format!("    op = :{}\n", c.op.as_str()));
        if let Some(v) = &c.entity_value {
            out.push_str(&format!("    value = {}\n", quote_wcl(&value_to_wcl(v))));
        }
        for f in &c.fields {
            let path = if f.path.is_empty() {
                "<value>"
            } else {
                &f.path
            };
            out.push_str(&format!("    field_change {} {{\n", quote_wcl(path)));
            out.push_str(&format!("      kind = {}\n", quote_wcl(f.kind.as_str())));
            if let Some(o) = &f.old {
                out.push_str(&format!("      old = {}\n", quote_wcl(&value_to_wcl(o))));
            }
            if let Some(n) = &f.new {
                out.push_str(&format!("      new = {}\n", quote_wcl(&value_to_wcl(n))));
            }
            out.push_str("    }\n");
        }
        out.push_str("  }\n");
    }
    out
}

/// Quote a string as a WCL inline string literal, escaping the characters
/// the lexer treats specially. Used for entity keys / field paths (which
/// aren't valid identifiers) and as the fallback for non-round-trippable
/// values.
fn quote_wcl(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(pairs: &[(&str, Value)]) -> Value {
        Value::Record {
            ty: Vec::new(),
            fields: std::sync::Arc::new(
                pairs
                    .iter()
                    .map(|(k, v)| ((*k).to_string(), v.clone()))
                    .collect(),
            ),
        }
    }

    fn diffs(old: &Value, new: &Value) -> Vec<FieldChange> {
        let mut out = Vec::new();
        diff_values(old, new, String::new(), &mut out);
        out
    }

    #[test]
    fn equal_values_produce_no_change() {
        let a = record(&[("x", Value::I64(1))]);
        assert!(diffs(&a, &a).is_empty());
    }

    #[test]
    fn changed_leaf_carries_old_and_new() {
        let a = record(&[("x", Value::I64(1))]);
        let b = record(&[("x", Value::I64(2))]);
        let d = diffs(&a, &b);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].path, "x");
        assert_eq!(d[0].kind, FieldKind::Changed);
        assert_eq!(d[0].old, Some(Value::I64(1)));
        assert_eq!(d[0].new, Some(Value::I64(2)));
    }

    #[test]
    fn nested_added_and_removed_fields() {
        let a = record(&[("fields", record(&[("name", Value::Utf8("t".into()))]))]);
        let b = record(&[(
            "fields",
            record(&[
                ("name", Value::Utf8("t".into())),
                ("due_date", Value::Utf8("2026".into())),
            ]),
        )]);
        let d = diffs(&a, &b);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].path, "fields.due_date");
        assert_eq!(d[0].kind, FieldKind::Added);
        assert_eq!(d[0].old, None);
        assert_eq!(d[0].new, Some(Value::Utf8("2026".into())));

        let r = diffs(&b, &a);
        assert_eq!(r[0].kind, FieldKind::Removed);
        assert_eq!(r[0].old, Some(Value::Utf8("2026".into())));
        assert_eq!(r[0].new, None);
    }

    #[test]
    fn none_to_value_reads_as_added() {
        let a = record(&[("due_date", Value::None)]);
        let b = record(&[("due_date", Value::Utf8("2026".into()))]);
        let d = diffs(&a, &b);
        assert_eq!(d[0].path, "due_date");
        assert_eq!(d[0].kind, FieldKind::Added);
        let r = diffs(&b, &a);
        assert_eq!(r[0].kind, FieldKind::Removed);
    }

    #[test]
    fn lists_recurse_by_index() {
        // Element changed at index 1, added at index 2.
        let a = record(&[("xs", Value::list(vec![Value::I64(1), Value::I64(2)]))]);
        let b = record(&[(
            "xs",
            Value::list(vec![Value::I64(1), Value::I64(9), Value::I64(3)]),
        )]);
        let d = diffs(&a, &b);
        assert_eq!(d.len(), 2);
        assert_eq!(d[0].path, "xs[1]");
        assert_eq!(d[0].kind, FieldKind::Changed);
        assert_eq!(d[1].path, "xs[2]");
        assert_eq!(d[1].kind, FieldKind::Added);
        assert_eq!(d[1].new, Some(Value::I64(3)));
    }

    #[test]
    fn list_shrink_reports_removed_tail() {
        let a = record(&[("xs", Value::list(vec![Value::I64(1), Value::I64(2)]))]);
        let b = record(&[("xs", Value::list(vec![Value::I64(1)]))]);
        let d = diffs(&a, &b);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].path, "xs[1]");
        assert_eq!(d[0].kind, FieldKind::Removed);
        assert_eq!(d[0].old, Some(Value::I64(2)));
    }

    #[test]
    fn value_to_wcl_strips_record_type_prefix() {
        // A reified block record carries a `ty`; the WCL form must be a
        // bare record literal so it re-parses as an expression.
        let v = Value::Record {
            ty: vec!["Entity".to_string()],
            fields: std::sync::Arc::new(
                [("id".to_string(), Value::Utf8("u".into()))]
                    .into_iter()
                    .collect(),
            ),
        };
        assert_eq!(value_to_wcl(&v), "{ id: \"u\" }");
    }

    #[test]
    fn rendered_wcl_reparses() {
        // Build a representative diff and assert the emitted WCL is
        // well-formed (the "never emit non-parsing WCL" contract).
        let changes = vec![
            Change {
                op: ChangeOp::Modified,
                entity: "domain_entity:task".to_string(),
                entity_value: None,
                fields: vec![FieldChange {
                    path: "fields.status".to_string(),
                    kind: FieldKind::Changed,
                    old: Some(Value::Utf8("draft".into())),
                    new: Some(Value::Utf8("active".into())),
                }],
            },
            Change {
                op: ChangeOp::Added,
                entity: "spec:impl".to_string(),
                entity_value: Some(Value::Record {
                    ty: vec!["Spec".to_string()],
                    fields: std::sync::Arc::new(
                        [
                            ("id".to_string(), Value::Identifier("impl".into())),
                            (
                                "tags".to_string(),
                                Value::list(vec![Value::Utf8("a".into())]),
                            ),
                        ]
                        .into_iter()
                        .collect(),
                    ),
                }),
                fields: Vec::new(),
            },
        ];
        let text = render_wcl(&changes, "old.wcl", "new.wcl");
        assert!(text.contains("modified \"domain_entity:task\""));
        assert!(text.contains("kind = :changed"));
        assert!(text.contains("old = \"draft\""));
        assert!(text.contains("new = \"active\""));
        assert!(text.contains("added \"spec:impl\""));
        // Re-parse to prove well-formedness (syntax only, no schema/eval).
        wcl_lang::parse_for_edit(&text, "<diff>").expect("rendered diff re-parses");
    }

    #[test]
    fn rendered_spec_changes_reparse_inside_a_spec_block() {
        // The `wad spec` skeleton embeds these blocks inside `spec … { }`;
        // prove the emitted shape parses in that position.
        let changes = vec![
            Change {
                op: ChangeOp::Modified,
                entity: "container:api".to_string(),
                entity_value: None,
                fields: vec![FieldChange {
                    path: "summary".to_string(),
                    kind: FieldKind::Changed,
                    old: Some(Value::Utf8("a".into())),
                    new: Some(Value::Utf8("b\"quoted\"".into())),
                }],
            },
            Change {
                op: ChangeOp::Added,
                entity: "system:core".to_string(),
                entity_value: Some(Value::Record {
                    ty: vec!["System".to_string()],
                    fields: std::sync::Arc::new(
                        [("id".to_string(), Value::Identifier("core".into()))]
                            .into_iter()
                            .collect(),
                    ),
                }),
                fields: Vec::new(),
            },
        ];
        let body = render_spec_changes(&changes);
        assert!(body.contains("change \"container:api\""));
        assert!(body.contains("field_change \"summary\""));
        let doc = format!(
            "spec t {{\n  title = \"t\"\n  status = :planning\n  created = \"2026-01-01\"\n  summary = \"s\"\n{body}}}\n"
        );
        wcl_lang::parse_for_edit(&doc, "<spec>").expect("rendered spec changes re-parse");
    }

    #[test]
    fn empty_diff_renders_comment_only() {
        let text = render_wcl(&[], "a.wcl", "b.wcl");
        assert!(text.contains("# no changes"));
        wcl_lang::parse_for_edit(&text, "<diff>").expect("empty diff re-parses");
    }
}
