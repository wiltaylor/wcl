//! Lowering dispatch: invoke a block's `lower` function, classify the
//! fundamental variants it returns, and recurse into the custom ones.
//!
//! Backend-neutral — what a lowered node *renders as* lives with the
//! backend that renders it ([`crate::html::lower`], [`crate::svg::lower`],
//! [`crate::markdown::emit`], [`crate::pdf::collect`], and the terminal's
//! own `draw_variant`). This module also owns the diagnostic sinks a render
//! pass accumulates, since every one of those walkers records into them.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use miette::NamedSource;
use wcl_lang::{Block, Document, EvalError, FnValue, Value};

use crate::content::Content;
use crate::render::include::resolve_content;

/// A function that renders one block to markup — the signature every
/// kind-specific renderer is registered under.
pub(crate) type BlockRenderer<'a> =
    dyn Fn(&[Value], Option<&str>, Option<&str>, &str) -> String + 'a;

thread_local! {
    /// First eval error swallowed while lowering a block during the
    /// current render pass. The lowering primitives recover (a failed
    /// field becomes `none`, a failed block renders nothing) so a single
    /// bad expression can't abort the whole document mid-tree — but the
    /// error is captured here so the backend entry point can surface it as
    /// a diagnostic and a non-zero exit, instead of silently dropping the
    /// block. First error wins; rendering is single-threaded per pass, so
    /// a thread-local is a safe document-scoped sink. Use
    /// [`scoped_eval_errors`] to bound a pass and collect what it caught.
    static LOWER_EVAL_ERR: RefCell<Option<(EvalError, NamedSource<String>)>> =
        const { RefCell::new(None) };

    /// First edge-routing failure recorded during the current render pass.
    /// The orthogonal router (`routing::route_elbow`) returns `None` when it
    /// genuinely cannot route an edge around the intervening shapes; rather
    /// than draw a misleading line straight through them, the diagram pass
    /// stashes a human-readable message here and the backend surfaces it as a
    /// hard `BuildError`. First message wins; see [`record_route_error`].
    static ROUTE_ERR: RefCell<Option<String>> = const { RefCell::new(None) };

    /// Non-fatal warnings collected during the current render pass: a
    /// dropped diagram edge, a block with no lowering, an image with no
    /// usable intrinsic size, … Unlike [`ROUTE_ERR`] these don't fail the
    /// build — the backend drains them to stderr after rendering. All
    /// distinct messages are collected (identical ones dedup, since some
    /// recording sites run once per pass per page or twice per diagram);
    /// see [`record_render_warning`].
    static RENDER_WARN: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };

    /// First file-backed listing that could not be read during the current
    /// render pass — a missing `source_file`, an anchor that isn't there, a
    /// line range past the end. Like [`ROUTE_ERR`] the backend turns it into
    /// a hard `BuildError`: a listing that names a file is a listing the
    /// build is meant to keep honest, so a broken one stops the build rather
    /// than rendering an empty card. First message wins; see
    /// [`record_include_error`].
    static INCLUDE_ERR: RefCell<Option<String>> = const { RefCell::new(None) };

    /// The directory the document being rendered lives in, for the current
    /// pass. A `source_file` is relative to the document that names it, and
    /// the pass that resolves it ([`crate::render::include`]) runs deep
    /// inside walkers that never carried a path — the HTML variant
    /// recursion in particular. Pass-scoped, set by [`DocDirGuard`], which
    /// restores whatever an outer pass had when it drops.
    static DOC_DIR: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

/// Record a listing that could not be read. First one wins; cleared by
/// [`take_include_error`].
pub(crate) fn record_include_error(msg: String) {
    INCLUDE_ERR.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = Some(msg);
        }
    });
}

/// Take and clear the first include failure recorded during this pass.
pub(crate) fn take_include_error() -> Option<String> {
    INCLUDE_ERR.with(|slot| slot.borrow_mut().take())
}

/// Sets the document directory that file-backed listings resolve against
/// for as long as it is held, and puts back whatever an enclosing pass had
/// when it is dropped — so a pass nested inside another (a deck collected
/// during a site build, say) leaves the outer one intact.
///
/// A guard rather than a wrapper function because the passes that need it
/// are whole backend entry points; taking their bodies into a closure
/// would re-indent them for no gain.
pub(crate) struct DocDirGuard(Option<PathBuf>);

impl DocDirGuard {
    /// Resolve listings against `dir` until the guard drops.
    pub(crate) fn set(dir: Option<&Path>) -> Self {
        Self(
            DOC_DIR.with(|slot| {
                std::mem::replace(&mut *slot.borrow_mut(), dir.map(Path::to_path_buf))
            }),
        )
    }
}

impl Drop for DocDirGuard {
    fn drop(&mut self) {
        let previous = self.0.take();
        DOC_DIR.with(|slot| *slot.borrow_mut() = previous);
    }
}

/// Resolve every file-backed listing in `node`, recording a failure for
/// the backend to surface. A node whose listing could not be read is
/// still returned — the build is already failing, and dropping it would
/// only cost the reader the context around the error.
///
/// `block` is the block the node was lowered from, when there is one: a
/// listing on an authored `code` block gets a spanned diagnostic pointing
/// at it, the same as any other authoring error. A node produced deeper
/// in a lowering chain has no span to point at, so it goes to the
/// pass-scoped sink instead. Either way the build stops, with the same
/// message and the same exit code.
fn resolve_includes(node: &mut Content, block: Option<&Block<'_>>) {
    let dir = DOC_DIR.with(|slot| slot.borrow().clone());
    let Err(e) = resolve_content(node, dir.as_deref()) else {
        return;
    };
    let msg = e.into_message();
    match block {
        Some(b) => record_lower_error(b, EvalError::user_error(msg, b.span())),
        None => record_include_error(msg),
    }
}

/// Record a non-fatal render warning. Identical messages dedup; distinct
/// ones accumulate until [`take_render_warnings`] drains them.
pub(crate) fn record_render_warning(msg: String) {
    RENDER_WARN.with(|slot| {
        let mut slot = slot.borrow_mut();
        if !slot.contains(&msg) {
            slot.push(msg);
        }
    });
}

/// Record a non-fatal warning that an edge endpoint matched no shape id.
pub(crate) fn record_edge_warning(msg: String) {
    record_render_warning(msg);
}

/// Take and clear the render warnings recorded during the current pass.
pub(crate) fn take_render_warnings() -> Vec<String> {
    RENDER_WARN.with(|slot| std::mem::take(&mut *slot.borrow_mut()))
}

/// Record an edge-routing failure (a `route_elbow` that found no obstacle-free
/// path). `msg` should name the offending edge and hint at a fix. First one
/// wins; cleared by [`take_route_error`].
pub(crate) fn record_route_error(msg: String) {
    ROUTE_ERR.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = Some(msg);
        }
    });
}

/// Take and clear the first routing error recorded during the current pass.
pub(crate) fn take_route_error() -> Option<String> {
    ROUTE_ERR.with(|slot| slot.borrow_mut().take())
}

/// Record an eval error swallowed while lowering `block`, together with the
/// source file that block lives in so the backend can render the diagnostic
/// against the correct snippet — a cross-file span won't line up with the
/// root document's text. First one wins.
pub(crate) fn record_lower_error(block: &Block<'_>, err: EvalError) {
    LOWER_EVAL_ERR.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = Some((err, block.named_source()));
        }
    });
}

/// Run `f` as a self-contained render pass, returning its result alongside
/// the first eval error any lowering swallowed during it (and the source it
/// belongs to), if any. Saves and restores any outer sink so nested passes
/// compose.
pub(crate) fn scoped_eval_errors<T>(
    f: impl FnOnce() -> T,
) -> (T, Option<(EvalError, NamedSource<String>)>) {
    let outer = LOWER_EVAL_ERR.with(|slot| slot.borrow_mut().take());
    let result = f();
    let caught = LOWER_EVAL_ERR.with(|slot| slot.borrow_mut().take());
    LOWER_EVAL_ERR.with(|slot| *slot.borrow_mut() = outer);
    (result, caught)
}

/// Lowering recursion guard. A lowering may emit other custom kinds
/// that themselves lower further; this caps how deep we'll follow
/// before bailing.
pub(crate) const MAX_LOWER_DEPTH: usize = 32;

/// Look up the `lower` function for a block kind. Tries the block's
/// own `lower` field first (per-instance override), then the kind's
/// `@block` type's `@default(...)` for `lower`. Returns `None` when
/// neither path produces a callable.
pub(crate) fn lookup_block_lower(doc: &Document, block: &Block<'_>, kind: &str) -> Option<FnValue> {
    if let Some(f) = block.field("lower")
        && let Ok(Value::Function(fv)) = f.value()
    {
        return Some(fv.clone());
    }
    lookup_type_lower(doc, kind)
}

/// Look up the `lower` function declared on a `@block` (or plain
/// `type`) by reading its `lower` field's `@default(...)` value.
/// Used both for block-side dispatch (after the instance check) and
/// for recursive variant dispatch (where no instance is available).
pub(crate) fn lookup_type_lower(doc: &Document, kind: &str) -> Option<FnValue> {
    let schema = doc
        .block_schema(kind)
        .or_else(|| doc.type_decl(&kind_to_typename(kind)))?;
    match schema.field("lower")?.default_value()? {
        Value::Function(fv) => Some(fv),
        _ => None,
    }
}

/// Call a block's `lower` function and return the raw list of
/// lowered variant values. `None` when the block has no
/// record, no `lower`, the call errors, or the result isn't a list.
pub(crate) fn lower_to_values(doc: &Document, block: &Block<'_>, kind: &str) -> Option<Vec<Value>> {
    // A block that can't produce a record or has no lowering would
    // render nothing on every backend. That silent vanishing is almost
    // always a missing `lower` or a data-only kind placed in a page
    // body — fail the build with a snippet instead. (A lowering that
    // wants to render nothing conditionally returns `none` or `[]`.)
    let Some(arg) = block_to_record(doc, block, kind) else {
        record_lower_error(
            block,
            EvalError::user_error(
                format!("block '{kind}' has no schema record to lower — it would render nothing"),
                block.span(),
            ),
        );
        return None;
    };
    let Some(fv) = lookup_block_lower(doc, block, kind) else {
        record_lower_error(
            block,
            EvalError::user_error(
                format!(
                    "block '{kind}' has no `lower` lowering — it would render nothing. \
                     Declare `fn lower(b: {kind}) -> …` for the kind, or remove the \
                     block from the page."
                ),
                block.span(),
            ),
        );
        return None;
    };
    match doc.call_value(&fv, &[arg]) {
        Ok(Value::List(items)) => Some(std::sync::Arc::unwrap_or_clone(items)),
        // The declared return type is a `list<…Fundamental>`; anything
        // else means the lowering is broken (the language doesn't
        // enforce fn return types at runtime) — surface a diagnostic
        // instead of silently dropping the block. `none` stays a benign
        // "nothing to render" so a conditional lowering can opt out.
        Ok(Value::None) => None,
        Ok(other) => {
            record_lower_error(
                block,
                EvalError::user_error(
                    format!(
                        "the `lower` lowering for block kind '{kind}' returned {} — \
                         expected a list of fundamentals",
                        other.type_name()
                    ),
                    block.span(),
                ),
            );
            None
        }
        Err(e) => {
            record_lower_error(block, e);
            None
        }
    }
}

/// One node a block's `lower` produced.
///
/// A lowering returns either the **semantic content IR** — the closed,
/// target-neutral vocabulary every backend consumes from one declaration
/// (`lib/content.wcl`) — or a member of the HTML element vocabulary, which
/// only the HTML backend understands and the others reverse-engineer.
/// Splitting them here is what lets a backend match the IR exhaustively:
/// a `Content` node reaches it typed, so a variant nobody handles is a
/// compile error rather than silence.
pub(crate) enum Lowered {
    /// A typed content node — rendered from the same declaration on every
    /// backend.
    Content(Content),
    /// An `Html` (or a custom variant that will expand into one),
    /// still carried as a raw value for the per-backend walkers.
    Html(Value),
}

/// Lower `block` and classify what came back: [`lower_to_values`] plus the
/// `Value` → [`Content`] conversion for every node that belongs to the
/// content union.
///
/// A value tagged `Content` that fails to convert is a genuine authoring
/// error (a required field missing, a number out of range) — it records a
/// lowering diagnostic so the backend surfaces it and exits non-zero,
/// rather than the node quietly vanishing.
pub(crate) fn lower_block(doc: &Document, block: &Block<'_>, kind: &str) -> Option<Vec<Lowered>> {
    let values = lower_to_values(doc, block, kind)?;
    Some(
        values
            .iter()
            .filter_map(|value| classify(block, kind, value))
            .collect(),
    )
}

/// The content node a *recursively* lowered value carries, if any.
///
/// The value came out of another lowering rather than off a block, so
/// there is no span to point a diagnostic at; a conversion failure is
/// recorded as a render warning naming the error instead of a spanned
/// build failure. A block's own lowering still hard-fails, via
/// [`lower_block`].
pub(crate) fn recursed_content(value: &Value) -> Option<Content> {
    match crate::content::as_content(value)? {
        Ok(mut node) => {
            resolve_includes(&mut node, None);
            Some(node)
        }
        Err(e) => {
            record_render_warning(format!(
                "a lowering produced a malformed content node ({e}) — it renders as nothing"
            ));
            None
        }
    }
}

/// Classify one lowered value.
///
/// A content node that fails to convert records a spanned diagnostic (the
/// build exits non-zero) and yields `None`: falling back to the HTML walker
/// would render the broken node as whichever fundamental shares its variant
/// name, which is a confusing thing to put in a build that has already
/// failed.
fn classify(block: &Block<'_>, kind: &str, value: &Value) -> Option<Lowered> {
    match crate::content::as_content(value) {
        Some(Ok(mut node)) => {
            resolve_includes(&mut node, Some(block));
            Some(Lowered::Content(node))
        }
        Some(Err(e)) => {
            record_lower_error(
                block,
                EvalError::user_error(
                    format!("block '{kind}' lowered to a malformed content node: {e}"),
                    block.span(),
                ),
            );
            None
        }
        None => Some(Lowered::Html(value.clone())),
    }
}

/// Whether `value` belongs to the HTML element vocabulary.
///
/// A walker that handles only *some* of that vocabulary needs this before
/// it treats an unhandled variant as a custom one: `kind_for_variant` maps
/// `Html::Table` to the kind name `table`, which is a real
/// stdlib block whose `lower` would then be called with the fundamental's
/// payload. Its lowering is a stub today, so nothing renders differently —
/// but the day `table` grows a real one, the mistake would surface as
/// output nobody asked for.
pub(crate) fn is_html_fundamental(value: &Value) -> bool {
    matches!(value, Value::Variant { union, .. }
        if union.last().map(String::as_str) == Some("Html"))
}

/// Expand one custom (non-fundamental) variant: rebuild its record, resolve
/// its kind's `lower`, and return the values that lowering produced. Empty
/// when the kind declares no `lower` or the call fails — a custom variant
/// nothing can lower contributes nothing, as it always has.
///
/// This is the recursion that used to exist only on the HTML path, which is
/// why a user block whose `lower` returned another custom variant rendered
/// in the book and nowhere else. Every backend's fundamental walker calls
/// it now.
pub(crate) fn expand_custom_variant(
    doc: &Document,
    map: &BTreeMap<String, Value>,
    kind: &str,
) -> Vec<Value> {
    let arg = payload_to_record(map, kind);
    let Some(fv) = lookup_type_lower(doc, kind) else {
        return Vec::new();
    };
    let Ok(Value::List(items)) = doc.call_value(&fv, &[arg]) else {
        return Vec::new();
    };
    std::sync::Arc::unwrap_or_clone(items)
}

/// Shared tail for a custom (non-fundamental) variant: rebuild its record,
/// resolve the kind's `lower`, call it, and render each returned variant via
/// `render_each` at `depth + 1`. Any failure (no `lower`, call error, or a
/// non-list result) yields an empty string. Used by both the SVG and HTML
/// dispatchers ([`crate::svg::lower`], [`crate::html::lower`]); the
/// terminal's `draw_variant` has its own grid-mutating recursion (no
/// `String` return, no depth marker) and stays separate.
pub(crate) fn lower_recurse(
    doc: &Document,
    map: &BTreeMap<String, Value>,
    kind: &str,
    depth: usize,
    render_each: impl Fn(&Value, usize) -> String,
) -> String {
    expand_custom_variant(doc, map, kind)
        .iter()
        .map(|v| render_each(v, depth + 1))
        .collect()
}

/// The marker string a recursion-bounded renderer emits when it hits
/// its depth limit.
pub(crate) fn depth_marker() -> String {
    "<!-- wdoc: lowering depth limit reached -->".into()
}

/// Build a `Value::Record` from `block`'s declared fields, then grow any
/// numeric `width`/`height` to the effective (text-aware) dimensions the
/// layout solver reserved — see [`effective_dims`]. Used by the SVG /
/// HTML lowering paths, where a shape's `width`/`height` is geometry.
pub(crate) fn block_to_record(doc: &Document, block: &Block<'_>, kind: &str) -> Option<Value> {
    let raw = block_to_record_raw(doc, block, kind)?;
    let Value::Record { ty, fields } = raw else {
        return Some(raw);
    };
    // Grow width / height in the record so a Process / Decision /
    // Terminator lowering whose text spans multiple lines (or one
    // very long line) sees the same effective dimensions the
    // layered solver did. Without this, the rendered rect would
    // stay at the declared size while the layout reserved a
    // larger cell — the text would spill out of the rect.
    // Only fields carrying numeric `width`/`height` geometry that the layout
    // solver may have grown are coerced; leave non-numeric fields alone.
    let (eff_w, eff_h) = crate::svg::effective_dims(block);
    if fields.get("width").is_some_and(Value::is_numeric)
        || fields.get("height").is_some_and(Value::is_numeric)
    {
        let mut grown = std::sync::Arc::unwrap_or_clone(fields);
        if grown.get("width").is_some_and(Value::is_numeric) {
            grown.insert("width".to_string(), Value::F64(eff_w));
        }
        if grown.get("height").is_some_and(Value::is_numeric) {
            grown.insert("height".to_string(), Value::F64(eff_h));
        }
        return Some(Value::record(ty, grown));
    }
    Some(Value::Record { ty, fields })
}

/// Build a `Value::Record` from `block`'s declared fields, with **no**
/// dimension grow. Schema is looked up via `doc.block_schema(kind)`. Each
/// declared field is populated from either the matching `@inline(N)` label
/// slot or the literal block field; missing values become `Value::None` so
/// optional fields cleanly reach the lowering function. The terminal-widget
/// path uses this directly: a TUI widget's `width`/`height` is a cell count
/// (`i64`), not SVG geometry, so it must not be coerced/grown.
pub(crate) fn block_to_record_raw(doc: &Document, block: &Block<'_>, kind: &str) -> Option<Value> {
    let schema = doc.block_schema(kind)?;
    // A label that fails to evaluate (e.g. an interpolation `$"…${name}…"`
    // referencing an unresolved binding) is a genuine error — record it so
    // the backend surfaces a diagnostic, then fall back to no labels so the
    // rest of the record still builds.
    let labels = match block.labels() {
        Ok(labels) => labels,
        Err(e) => {
            record_lower_error(block, e);
            Vec::new()
        }
    };
    let mut map = BTreeMap::new();
    for f in schema.fields() {
        let name = f.name();
        let is_children_slot =
            f.children_kind_or_union().is_some() || f.child_kind_or_union().is_some();
        let val = if let Some(slot) = f.inline_slot() {
            // An `@inline(N)` field is normally the positional label slot, but
            // also honour an explicit `name = value` field of the same name, so
            // `node { text = "x" }` lowers the same as `node "x"`. Prefer the
            // label when it was actually given.
            if let Some(v) = labels.get(slot as usize).cloned() {
                v
            } else if let Some(field) = block.field(name) {
                match field.value() {
                    Ok(v) => v.clone(),
                    Err(e) => {
                        record_lower_error(block, e.clone());
                        Value::None
                    }
                }
            } else {
                f.default_value().unwrap_or(Value::None)
            }
        } else if is_children_slot {
            // A `@children(...)` / `@child(...)` slot. Always materialise
            // through the projection — never the raw `block.field(name)`
            // value — so that a *computed* splice (`spans = map(data, …)`)
            // is schema-completed exactly like statically-nested blocks:
            // each child record carries every declared field (optionals →
            // `none`), which the `lower` (e.g. `s.id` / `s.class`) relies
            // on. The projection yields a coerced variant list for a union
            // slot (a `Value`) or a block list for a concrete-kind slot.
            match block.typed_field(name) {
                Some(dr) => match dr.value() {
                    Ok(v) => v,
                    Err(_) => match dr.as_block_list() {
                        Some(blocks) => Value::list(
                            blocks
                                .iter()
                                .filter_map(|b| block_to_record(doc, b, b.kind()))
                                .collect(),
                        ),
                        None => f.default_value().unwrap_or(Value::None),
                    },
                },
                None => f.default_value().unwrap_or(Value::None),
            }
        } else if let Some(field) = block.field(name) {
            // A present field whose expression fails to evaluate is a
            // genuine error — record it (then fall back to `none` so the
            // record still builds for the rest of the fields).
            match field.value() {
                Ok(v) => v.clone(),
                Err(e) => {
                    record_lower_error(block, e.clone());
                    Value::None
                }
            }
        } else if let Some(dr) = block.typed_field(name) {
            // A schema-projected field with no raw AST entry: a leaf typed
            // projection (e.g. a `@connections` list, which has a `Value`).
            match dr.value() {
                Ok(v) => v,
                Err(_) => match dr.as_block_list() {
                    Some(blocks) => Value::list(
                        blocks
                            .iter()
                            .filter_map(|b| block_to_record(doc, b, b.kind()))
                            .collect(),
                    ),
                    None => f.default_value().unwrap_or(Value::None),
                },
            }
        } else {
            // Fall back to the schema's declared default
            // (`name = expr` inline-default or `@default(expr)`)
            // so a lowering that consumes `block.x` doesn't crash
            // when the block omits the field but the type
            // declared a value-typed default.
            f.default_value().unwrap_or(Value::None)
        };
        map.insert(name.to_string(), val);
    }
    Some(Value::Record {
        ty: vec![kind_to_typename(kind)],
        fields: std::sync::Arc::new(map),
    })
}

/// Wrap a variant payload as a record tagged with its kind.
pub(crate) fn payload_to_record(map: &BTreeMap<String, Value>, kind: &str) -> Value {
    Value::Record {
        ty: vec![kind_to_typename(kind)],
        fields: std::sync::Arc::new(map.clone()),
    }
}

/// Best-effort kind→type-name mapping. With our naming convention
/// (variant name = capitalised kind), `"process"` ↔ `Process`.
pub(crate) fn kind_to_typename(kind: &str) -> String {
    let mut s = String::with_capacity(kind.len());
    let mut up = true;
    for c in kind.chars() {
        if c == '_' {
            up = true;
            continue;
        }
        if up {
            s.extend(c.to_uppercase());
            up = false;
        } else {
            s.push(c);
        }
    }
    s
}

/// The block kind a content variant corresponds to.
pub(crate) fn kind_for_variant(variant: &str) -> String {
    let mut s = String::with_capacity(variant.len());
    for (i, c) in variant.chars().enumerate() {
        if i > 0 && c.is_uppercase() {
            s.push('_');
        }
        s.extend(c.to_lowercase());
    }
    s
}
