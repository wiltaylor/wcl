//! View types: borrowed wrappers around AST nodes that expose
//! the cached, schema-aware document layer (decorators, type/
//! interface/union decls, fields, blocks, tables, …). Extracted
//! from `doc.rs` so the parent file can stay focused on the
//! Document container itself.

mod block;
mod decl;
mod decorator;
mod field;

// The view types are split across the submodules above by what they
// view, but they are one flat vocabulary to every consumer — re-exported
// here so `doc::views::TypeDecl` keeps meaning what it always did.
pub use block::{Block, Connection, RowView, TableView};
pub(crate) use decl::UnionChildKind;
pub use decl::{
    ChildKind, ConnectionDecl, DeclaresKind, InterfaceDecl, SymbolEntry, SymbolSetDecl, TypeDecl,
    UnionDecl, UnionVariant, UseDeclView, UseFormView, UseItem, VariantBodyView,
};
pub use decorator::{Decorator, NamedArg};
pub(crate) use field::LetView;
pub use field::{Field, TypeField};

use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::Ordering;

use crate::ast::{self, Span};
use crate::ast::{BuiltinType, TypeRef};
use crate::error::EvalError;
use crate::value::Value;

use super::cells::{DecoratorCell, FieldCell, ItemCellKind, ItemCells};
use super::eval::EvalCtx;
use super::imports::{BlockSlice, load_import_lazily, push_loaded_imports};
use super::lookup::{iter_blocks, iter_fields, iter_tables};
use super::schema_check::compute_schema_errors;
use super::scope::{Scope, ScopeFrame};
use super::types::variant_dispatch;
use super::types::{
    ALIAS_DEPTH, FieldShape, ResolvedType, build_effective_fields, build_merged_decorators,
    check_interface_conformance, coerce_value_to_type, dataref_concrete_type,
    is_descendant_of_walk, lookup_effective_field, same_type_decl,
};
use super::{Document, find_block, find_field, find_let, has_schemaless};
use super::{expr_to_path_segments, materialise_dataref_or_path, span_to_miette};

/// Join the contiguous run of line comments immediately above a
/// declaration (its `leading_trivia`) into a single doc-comment string.
/// Walks from the end so only the block touching the declaration counts —
/// a comment separated from the declaration by a blank line is unrelated
/// and dropped. Each line is trimmed; returns `None` when no attached
/// comment is present. Backs the `doc_comment` reflection builtin.
pub(crate) fn doc_comment_from_trivia(trivia: &[ast::Trivia]) -> Option<String> {
    let mut lines: Vec<&str> = Vec::new();
    for t in trivia.iter().rev() {
        match t {
            ast::Trivia::LineComment(s) => lines.push(s.trim()),
            ast::Trivia::BlankLine => break,
        }
    }
    if lines.is_empty() {
        return None;
    }
    lines.reverse();
    Some(lines.join("\n"))
}

/// How deep a `@contextual` block's expansion may nest before
/// [`Block::expand_children`] stops descending. Bounds a self-referential
/// expansion, whose block tree is otherwise unbounded; mirrors the
/// renderer-side lowering guard.
const MAX_EXPANSION_DEPTH: usize = 32;

/// Closed set of decorator names the document layer special-cases:
/// schema dispatch (`@block`, `@table`, `@document`, `@decorator`),
/// field shape (`@inline`, `@default`, `@child`, `@children`),
/// connection decomposition (`@connections`), per-block schema opt-out
/// (`@schemaless`), context-decided placement (`@contextual`), and the
/// host-neutral nested slot-type marker (`@block_slot`).
/// User-defined decorators are matched by their declared name and don't
/// go through this enum.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum BuiltinDecorator {
    /// `@block("kind")` — the type backing a block kind.
    Block,
    /// `@table("kind")` — the type backing a table's rows.
    Table,
    /// `@document` — the type validating the file's top level.
    Document,
    /// `@decorator("name")` — declares a decorator's own shape.
    Decorator,
    /// `@schemaless` — waive membership checking for this type.
    Schemaless,
    /// `@contextual` — placement is decided by the surrounding context.
    Contextual,
    /// `@block_slot` — host-neutral marker for a nested slot type.
    BlockSlot,
    /// `@declares_kind` — the type introduces a block kind of its own.
    DeclaresKind,
    /// `@inline(n)` — the field is filled from the block's nth label.
    Inline,
    /// `@default(value)` — value used when the field is absent.
    Default,
    /// `@child("kind")` — the field gathers one child block.
    Child,
    /// `@children("kind")` — the field gathers every child of a kind.
    Children,
    /// `@connections(schema)` — the field decomposes connection
    /// statements.
    Connections,
    /// `@dynamic` — the field's type is decided at evaluation.
    Dynamic,
    /// `@ref("kind")` — the field holds a reference to a block by id.
    Ref,
    /// `@by_ref` — the field is compared and stored by reference.
    ByRef,
}

impl BuiltinDecorator {
    /// The decorator's canonical source name, without the leading `@`.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            BuiltinDecorator::Block => "block",
            BuiltinDecorator::Table => "table",
            BuiltinDecorator::Document => "document",
            BuiltinDecorator::Decorator => "decorator",
            BuiltinDecorator::Schemaless => "schemaless",
            BuiltinDecorator::Contextual => "contextual",
            BuiltinDecorator::BlockSlot => "block_slot",
            BuiltinDecorator::DeclaresKind => "declares_kind",
            BuiltinDecorator::Inline => "inline",
            BuiltinDecorator::Default => "default",
            BuiltinDecorator::Child => "child",
            BuiltinDecorator::Children => "children",
            BuiltinDecorator::Connections => "connections",
            BuiltinDecorator::Dynamic => "dynamic",
            BuiltinDecorator::Ref => "ref",
            BuiltinDecorator::ByRef => "by_ref",
        }
    }
}

/// Extract a `u64`-valued named argument from the first decorator in
/// `decs` whose `full_name()` matches `dec_name`. Returns `None` if the
/// decorator isn't present, the named arg isn't present, the eval
/// failed, or the value isn't a non-negative integer.
fn decorator_u64_named(
    decs: &[Decorator<'_>],
    dec: BuiltinDecorator,
    arg_name: &str,
) -> Option<u64> {
    let found = find_builtin_dec(decs, dec)?;
    found.named_arg(arg_name)?.ok()?.as_u64()
}

/// Borrow the first decorator on `decs` whose `full_name()` matches the
/// canonical name of `dec`. Used by view methods that special-case one
/// of the builtin decorators (e.g. `Field::default_value`, `Field::child`).
fn find_builtin_dec<'a, 'b>(
    decs: &'b [Decorator<'a>],
    dec: BuiltinDecorator,
) -> Option<&'b Decorator<'a>> {
    let name = dec.as_str();
    decs.iter().find(|d| d.full_name() == name)
}

/// Shared accessors for the four top-level declaration views (`TypeDecl`,
/// `InterfaceDecl`, `UnionDecl`, `SymbolSetDecl`). All four hold a
/// segment-path name plus the surrounding file namespace; the rendering of
/// `name`, `name_segments`, `full_name`, and `namespace` is identical.
pub trait DeclName<'a> {
    /// Path written in source (without the file namespace).
    fn name_segments(&self) -> &'a [String];

    /// The file namespace this declaration was parsed under.
    fn file_ns(&self) -> &'a [String];

    /// Last segment of the declared name.
    fn name(&self) -> &'a str {
        self.name_segments()
            .last()
            .map(String::as_str)
            .expect("name has at least one segment")
    }

    /// Fully-qualified name as a dotted string: `file_ns + name_segments`.
    fn full_name(&self) -> String {
        self.fqn_segments().join(".")
    }

    /// Fully-qualified name as segments: `file_ns + name_segments`.
    fn fqn_segments(&self) -> Vec<String> {
        self.file_ns()
            .iter()
            .chain(self.name_segments().iter())
            .cloned()
            .collect()
    }

    /// Namespace path containing this declaration: `file_ns +
    /// name_segments[..-1]`. Empty when the declaration sits directly in
    /// the file namespace with a single-segment name.
    fn namespace(&self) -> Vec<String> {
        let segs = self.name_segments();
        let mut v: Vec<String> = self.file_ns().to_vec();
        if segs.len() > 1 {
            v.extend(segs[..segs.len() - 1].iter().cloned());
        }
        v
    }
}

/// Collect every block of `kind` from a `@contextual` block's expansion,
/// recursing through nested contextual blocks (a repetition's body may
/// contain another repetition or an instance). The expansion-depth cap
/// inside [`Block::expand_children`] bounds the recursion.
fn push_generated_matching<'a>(
    contextual: &Block<'a>,
    kind: &str,
    out: &mut Vec<Block<'a>>,
) -> Result<(), EvalError> {
    for child in contextual.expand_children()? {
        if child.kind() == kind {
            out.push(child);
        } else if child.is_contextual() {
            push_generated_matching(&child, kind, out)?;
        }
    }
    Ok(())
}

/// Reify a block's first label into one address segment, or `None` for an
/// absent / unaddressable label (an anonymous nested block, or a non-scalar
/// label). Strings, symbols, and integers all address (`Value::as_path_segment`),
/// exactly the labels `BlockList::child` matches.
fn block_label_segment(b: &Block<'_>) -> Option<String> {
    b.labels().ok()?.first()?.as_path_segment()
}

/// Coerce a string value to an identifier — the value-level rule for
/// identifier-declared fields, so a quoted ref (`system = "web"`) and a
/// bare one (`system = web`) evaluate identically and `ref == id`
/// comparisons in templates hold. Non-string values pass through.
fn str_to_identifier(v: Value) -> Value {
    match v {
        Value::Utf8(s) | Value::Ascii(s) => Value::Identifier(s),
        other => other,
    }
}

/// Reify a single block at document path `base`. A block whose kind is
/// `@by_ref` becomes a `Value::DataPath` reference (segments = `base`)
/// rather than an inlined record, so its content is reached only by
/// re-resolving the path. Everything else reifies via `to_record_value_at`.
fn reify_block_at(b: &Block<'_>, base: &[String]) -> Result<Value, EvalError> {
    if b.schema().is_some_and(|s| s.is_by_ref()) {
        return Ok(Value::DataPath {
            kind: "block".to_string(),
            segments: base.to_vec(),
        });
    }
    b.to_record_value_at(base)
}

/// Materialise a [`DataRef`](crate::data::DataRef) into an owned [`Value`]
/// for expression consumption: leaf fields / pre-evaluated variant values
/// pass through, a single block reifies to a record, and a block list /
/// table / variant list reifies to a `Value::List`. Returns `None` for
/// kinds that have no list/record value (types, unions, symbols, …) so the
/// caller can fall back to a `Value::DataPath` handle for reflective
/// builtins.
///
/// Carries the document path that re-resolves `dr` from root (`base`),
/// threaded so `@by_ref` slots can emit resolvable references. For a block
/// list, each element's address extends `base` with that element's label.
pub(crate) fn dataref_to_value_at<'a>(
    dr: &crate::data::DataRef<'a>,
    base: &[String],
) -> Option<Result<Value, EvalError>> {
    use crate::data::DataKind;
    match dr.inner() {
        DataKind::Field(f) => Some(f.value().cloned().map_err(|e| e.clone())),
        DataKind::Error(e) => Some(Err(e.clone())),
        DataKind::VariantValue(v) => Some(Ok(v.clone())),
        DataKind::VariantValueList(vs) => Some(Ok(Value::List(std::sync::Arc::new(vs.clone())))),
        DataKind::Block(b) => Some(reify_block_at(b, base)),
        DataKind::BlockList(v) | DataKind::Table(v) => Some((|| {
            let mut out = Vec::with_capacity(v.len());
            for b in v {
                let elem_base = match block_label_segment(b) {
                    Some(seg) => {
                        let mut p = base.to_vec();
                        p.push(seg);
                        p
                    }
                    // Unaddressable element: pass an empty base so any nested
                    // `@by_ref` reference is emitted but simply won't resolve.
                    None => Vec::new(),
                };
                out.push(reify_block_at(b, &elem_base)?);
            }
            Ok(Value::List(std::sync::Arc::new(out)))
        })()),
        _ => None,
    }
}
