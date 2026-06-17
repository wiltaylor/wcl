//! `wcl diff <old.wcl> <new.wcl>` — a WCL-aware document diff.
//!
//! Compares the *evaluated* document views (not the source text), so it is
//! robust to formatting-only churn: a whole-file reformat that doesn't
//! change any value produces an empty diff. Each top-level block is an
//! **entity**, keyed `kind:label` (its first label / id); top-level bare
//! fields fold into a synthetic `<document>` entity. Within an entity the
//! reified record is deep-compared, so changes are reported at field-path
//! granularity (`fields.due_date`).
//!
//! Output is a JSON array of changes, one object each:
//! ```json
//! [
//!   {"op":"modified","entity":"domain_entity:task","field":"fields.due_date","kind":"added"},
//!   {"op":"added","entity":"spec:impl_due_dates"}
//! ]
//! ```

use std::collections::BTreeMap;

use serde_json::{Value as Json, json};
use wcl_lang::{Block, Document, Value};

/// One reported change. `field` / `kind` are present only for `modified`
/// (a per-field-path edit inside an entity that exists on both sides).
#[derive(Debug, PartialEq)]
pub(crate) struct Change {
    /// `"added"` | `"removed"` | `"modified"`.
    op: &'static str,
    /// Entity key — `kind:label` for a block, or `<document>` for the
    /// top-level field group.
    entity: String,
    /// Dotted field path within the entity (modified only).
    field: Option<String>,
    /// `"added"` | `"removed"` | `"changed"` for the field (modified only).
    kind: Option<&'static str>,
}

impl Change {
    /// Render as a JSON object, omitting `field` / `kind` when absent (the
    /// `serde(skip_serializing_if)` equivalent, hand-rolled to avoid a
    /// `serde` derive dependency).
    pub(crate) fn to_json(&self) -> Json {
        let mut obj = serde_json::Map::new();
        obj.insert("op".to_string(), json!(self.op));
        obj.insert("entity".to_string(), json!(self.entity));
        if let Some(field) = &self.field {
            obj.insert("field".to_string(), json!(field));
        }
        if let Some(kind) = self.kind {
            obj.insert("kind".to_string(), json!(kind));
        }
        Json::Object(obj)
    }
}

/// Render a list of changes as a JSON array.
pub(crate) fn changes_to_json(changes: &[Change]) -> Json {
    Json::Array(changes.iter().map(Change::to_json).collect())
}

/// The synthetic entity key under which top-level bare fields are diffed.
const DOCUMENT_ENTITY: &str = "<document>";

/// Compute the entity/field diff between two evaluated documents.
/// Entities present on only one side become a single `added` / `removed`
/// change; entities on both sides are deep-compared field by field.
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
            (None, Some(_)) => changes.push(Change {
                op: "added",
                entity: key.clone(),
                field: None,
                kind: None,
            }),
            (Some(_), None) => changes.push(Change {
                op: "removed",
                entity: key.clone(),
                field: None,
                kind: None,
            }),
            (Some(old_val), Some(new_val)) => {
                let mut fields = Vec::new();
                diff_values(old_val, new_val, String::new(), &mut fields);
                for (path, kind) in fields {
                    changes.push(Change {
                        op: "modified",
                        entity: key.clone(),
                        field: Some(if path.is_empty() {
                            "<value>".to_string()
                        } else {
                            path
                        }),
                        kind: Some(kind),
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

/// Deep-compare two values, appending `(path, kind)` entries for every
/// differing leaf or sub-record. Two records recurse key by key (a key on
/// only one side is `added`/`removed`); any other unequal pair — scalars,
/// lists, variants, type mismatch — is a single `changed` at `path`.
/// Equal values contribute nothing (so formatting-only churn is invisible).
fn diff_values(old: &Value, new: &Value, path: String, out: &mut Vec<(String, &'static str)>) {
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
                    (None, Some(_)) => out.push((child, "added")),
                    (Some(_), None) => out.push((child, "removed")),
                    (None, None) => unreachable!("key came from one of the maps"),
                }
            }
        }
        // An optional field reifies as `none` when unset, so a none→value
        // edit reads as the field being *added* (and value→none as
        // removed) rather than a bland "changed".
        (Value::None, _) => out.push((path, "added")),
        (_, Value::None) => out.push((path, "removed")),
        _ => out.push((path, "changed")),
    }
}

/// Join a dotted field path with a child segment.
fn join(path: &str, seg: &str) -> String {
    if path.is_empty() {
        seg.to_string()
    } else {
        format!("{path}.{seg}")
    }
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

    fn paths(old: &Value, new: &Value) -> Vec<(String, &'static str)> {
        let mut out = Vec::new();
        diff_values(old, new, String::new(), &mut out);
        out
    }

    #[test]
    fn equal_values_produce_no_change() {
        let a = record(&[("x", Value::I64(1))]);
        assert!(paths(&a, &a).is_empty());
    }

    #[test]
    fn changed_leaf_is_reported() {
        let a = record(&[("x", Value::I64(1))]);
        let b = record(&[("x", Value::I64(2))]);
        assert_eq!(paths(&a, &b), vec![("x".to_string(), "changed")]);
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
        assert_eq!(
            paths(&a, &b),
            vec![("fields.due_date".to_string(), "added")]
        );
        assert_eq!(
            paths(&b, &a),
            vec![("fields.due_date".to_string(), "removed")]
        );
    }

    #[test]
    fn none_to_value_reads_as_added() {
        // An optional field unset on one side reifies as `none`.
        let a = record(&[("due_date", Value::None)]);
        let b = record(&[("due_date", Value::Utf8("2026".into()))]);
        assert_eq!(paths(&a, &b), vec![("due_date".to_string(), "added")]);
        assert_eq!(paths(&b, &a), vec![("due_date".to_string(), "removed")]);
    }

    #[test]
    fn lists_compare_as_leaves() {
        let a = record(&[("xs", Value::list(vec![Value::I64(1)]))]);
        let b = record(&[("xs", Value::list(vec![Value::I64(1), Value::I64(2)]))]);
        assert_eq!(paths(&a, &b), vec![("xs".to_string(), "changed")]);
    }
}
