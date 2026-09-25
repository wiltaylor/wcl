//! A semantic diff of two evaluated documents.
//!
//! [`diff_documents`] compares what two documents *evaluate to*, not
//! their source text, so formatting-only churn — a whole-file reformat
//! that changes no value — produces an empty diff. Each top-level block
//! is an **entity**, keyed `kind:label` by its first label; the
//! top-level bare fields fold into one synthetic [`DOCUMENT_ENTITY`].
//! Within an entity the reified record is deep-compared, so changes are
//! reported per field path (`fields.due_date`), recursing into lists by
//! index (`tags[2]`).
//!
//! A block or field that fails to evaluate is left out of the comparison
//! rather than failing it, and is listed in [`Diff::warnings`] so the
//! gap is never silent.
//!
//! The result is plain data. `wcl diff` renders it as a re-parseable WCL
//! document; other hosts can render it however they like.
//!
//! Two limits are deliberate. Tensors compare as opaque leaves (one
//! [`FieldKind::Changed`]); only lists recurse element by element. And
//! list comparison is by index, so reordering a list reads as per-index
//! churn rather than a move.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use crate::{Block, Document, EvalError, Value};

/// The synthetic entity key under which top-level bare fields are
/// compared.
pub const DOCUMENT_ENTITY: &str = "<document>";

/// What happened to a whole entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeOp {
    /// The entity exists only in the new document.
    Added,
    /// The entity exists only in the old document.
    Removed,
    /// The entity exists in both, and at least one field differs.
    Modified,
}

impl ChangeOp {
    /// The lower-case name: `added`, `removed` or `modified`.
    pub fn as_str(self) -> &'static str {
        match self {
            ChangeOp::Added => "added",
            ChangeOp::Removed => "removed",
            ChangeOp::Modified => "modified",
        }
    }
}

/// What happened to one field path inside a modified entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldKind {
    /// The path has a value only on the new side. An optional field
    /// going from `none` to a value also counts as added.
    Added,
    /// The path has a value only on the old side, or went to `none`.
    Removed,
    /// Both sides have a value, and they differ.
    Changed,
}

impl FieldKind {
    /// The lower-case name: `added`, `removed` or `changed`.
    pub fn as_str(self) -> &'static str {
        match self {
            FieldKind::Added => "added",
            FieldKind::Removed => "removed",
            FieldKind::Changed => "changed",
        }
    }
}

/// One reported change to an entity.
#[derive(Debug, Clone, PartialEq)]
pub struct Change {
    /// Whether the entity was added, removed or modified.
    pub op: ChangeOp,
    /// The entity key: `kind:label` for a block, bare `kind` for an
    /// unlabelled one (with a `#n` suffix when keys collide), or
    /// [`DOCUMENT_ENTITY`] for the top-level fields.
    pub entity: String,
    /// The whole reified record, for [`ChangeOp::Added`] and
    /// [`ChangeOp::Removed`]. `None` for a modification.
    pub entity_value: Option<Value>,
    /// The per-field edits, for [`ChangeOp::Modified`]. Empty otherwise.
    pub fields: Vec<FieldChange>,
}

/// One field-path edit within a modified entity, with the values on
/// each side. The absent side of an add or remove is `None`.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldChange {
    /// Dotted and indexed path within the entity (`fields.tags[2]`).
    /// Empty when the compared values themselves differ at the root.
    pub path: String,
    /// Whether the path was added, removed or changed.
    pub kind: FieldKind,
    /// The old value, absent for an addition.
    pub old: Option<Value>,
    /// The new value, absent for a removal.
    pub new: Option<Value>,
}

/// Which of the two compared documents something belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    /// The first argument to [`diff_documents`].
    Old,
    /// The second argument to [`diff_documents`].
    New,
}

/// What could not be evaluated, and so was left out of the comparison.
#[derive(Debug, Clone, PartialEq)]
pub enum Skipped {
    /// A top-level block, by the entity key it would have had.
    Entity(String),
    /// A top-level bare field, by name.
    Field(String),
}

/// A block or field left out of the diff because it failed to evaluate.
///
/// Its `Display` is the one-line explanation `wcl diff` prints after
/// `warning: `, for example
/// `field 'port' could not be evaluated, skipping: <error>`.
#[derive(Debug, Clone, PartialEq)]
pub struct DiffWarning {
    /// The document the item belongs to.
    pub side: Side,
    /// The item that was skipped.
    pub skipped: Skipped,
    /// Why it failed to evaluate.
    pub error: EvalError,
}

impl fmt::Display for DiffWarning {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (what, name) = match &self.skipped {
            Skipped::Entity(key) => ("entity", key),
            Skipped::Field(name) => ("field", name),
        };
        write!(
            f,
            "{what} '{name}' could not be evaluated, skipping: {}",
            self.error
        )
    }
}

/// The result of [`diff_documents`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Diff {
    /// One entry per entity that differs, sorted by entity key.
    pub changes: Vec<Change>,
    /// Everything that failed to evaluate: the old document's blocks and
    /// then its fields, then the new document's, each in source order.
    pub warnings: Vec<DiffWarning>,
}

impl Diff {
    /// `true` when the documents compared equal. Warnings do not count:
    /// a skipped item is a gap in the comparison, not a difference.
    pub fn is_empty(&self) -> bool {
        self.changes.is_empty()
    }
}

/// Compare two evaluated documents entity by entity.
///
/// An entity present on one side only becomes a single
/// [`ChangeOp::Added`] or [`ChangeOp::Removed`] change carrying its whole
/// record. An entity on both sides is deep-compared with
/// [`diff_values`] and, if anything differs, yields one
/// [`ChangeOp::Modified`] change carrying the edits. Changes are sorted
/// by entity key.
///
/// ```
/// use wcl_lang::{Document, Value, diff};
///
/// let old = Document::open("@schemaless name = \"alpha\"\n@schemaless port = 80\n", "old.wcl").unwrap();
/// let new = Document::open("@schemaless name = \"alpha\"\n@schemaless port = 81\n", "new.wcl").unwrap();
/// let d = diff::diff_documents(&old, &new);
///
/// assert_eq!(d.changes.len(), 1);
/// let change = &d.changes[0];
/// assert_eq!(change.op, diff::ChangeOp::Modified);
/// assert_eq!(change.entity, diff::DOCUMENT_ENTITY);
/// assert_eq!(change.fields[0].path, "port");
/// assert_eq!(change.fields[0].kind, diff::FieldKind::Changed);
/// assert_eq!(change.fields[0].new, Some(Value::I64(81)));
/// assert!(d.warnings.is_empty());
/// ```
pub fn diff_documents(old: &Document, new: &Document) -> Diff {
    let mut warnings = Vec::new();
    let old_entities = collect_entities(old, Side::Old, &mut warnings);
    let new_entities = collect_entities(new, Side::New, &mut warnings);

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
                let fields = diff_values(old_val, new_val);
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
    Diff { changes, warnings }
}

/// Deep-compare two values and list every differing path.
///
/// Records recurse key by key and lists by index; a key or index on only
/// one side is [`FieldKind::Added`] or [`FieldKind::Removed`], and so is
/// a `none` on one side. Any other unequal pair — scalars, variants,
/// tensors, a type mismatch — is one [`FieldKind::Changed`] at its path.
/// Equal values yield nothing. Record type names are ignored: only the
/// fields are compared.
///
/// ```
/// use wcl_lang::{Value, diff};
///
/// let old = Value::list(vec![Value::I64(1), Value::I64(2)]);
/// let new = Value::list(vec![Value::I64(1), Value::I64(9), Value::I64(3)]);
/// let edits = diff::diff_values(&old, &new);
///
/// assert_eq!(edits.len(), 2);
/// assert_eq!((edits[0].path.as_str(), edits[0].kind), ("[1]", diff::FieldKind::Changed));
/// assert_eq!((edits[1].path.as_str(), edits[1].kind), ("[2]", diff::FieldKind::Added));
/// ```
pub fn diff_values(old: &Value, new: &Value) -> Vec<FieldChange> {
    let mut out = Vec::new();
    diff_values_at(old, new, String::new(), &mut out);
    out
}

/// Reify a document's top-level blocks (entities) and bare fields into a
/// `key -> Value` map. A block reifies to its schema-projected record; the
/// bare top-level fields reify to one [`DOCUMENT_ENTITY`] record. A block
/// or field whose value can't be evaluated is left out and recorded in
/// `warnings`, so a partial document still diffs the rest.
fn collect_entities(
    doc: &Document,
    side: Side,
    warnings: &mut Vec<DiffWarning>,
) -> BTreeMap<String, Value> {
    let mut out: BTreeMap<String, Value> = BTreeMap::new();

    for block in doc.blocks() {
        let key = entity_key(&block, &out);
        match block.to_record_value() {
            Ok(v) => {
                out.insert(key, v);
            }
            Err(error) => warnings.push(DiffWarning {
                side,
                skipped: Skipped::Entity(key),
                error,
            }),
        }
    }

    // Top-level bare fields → a single synthetic entity, so a changed
    // document-level field isn't silently dropped.
    let mut doc_fields: BTreeMap<String, Value> = BTreeMap::new();
    for f in doc.fields() {
        match f.value() {
            Ok(v) => {
                doc_fields.insert(f.name().to_string(), v.clone());
            }
            Err(error) => warnings.push(DiffWarning {
                side,
                skipped: Skipped::Field(f.name().to_string()),
                error: error.clone(),
            }),
        }
    }
    if !doc_fields.is_empty() {
        out.insert(
            DOCUMENT_ENTITY.to_string(),
            Value::Record {
                ty: Vec::new(),
                fields: Arc::new(doc_fields),
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

/// The recursive step of [`diff_values`], appending each difference
/// found below `path` to `out`.
fn diff_values_at(old: &Value, new: &Value, path: String, out: &mut Vec<FieldChange>) {
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
                    (Some(av), Some(bv)) => diff_values_at(av, bv, child, out),
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
                    (Some(av), Some(bv)) => diff_values_at(av, bv, child, out),
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
