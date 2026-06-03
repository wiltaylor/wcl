//! Lowering dispatch: invoke a block's `lower` function and recursively
//! render the returned fundamental variants (HTML + SVG).

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::path::Path;

use miette::NamedSource;
use wcl_lang::{Block, Document, EvalError, FnValue, Value, VariantPayload};

use crate::inline::InlinePatterns;

use super::*;

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

/// Private placeholder emitted by an `HtmlFundamental::Children` variant.
/// `lower_html_block` substitutes it with the block's rendered child
/// blocks. U+FFF9 (interlinear annotation anchor) can't appear in
/// document content, so it can't collide with real output.
const WF_CHILDREN_SLOT: &str = "\u{FFF9}wdoc:children\u{FFF9}";

/// Placeholder emitted by a `wdoc_content` block (a component's content
/// slot). `render_component` substitutes it with the *instance's* own
/// rendered child blocks. Distinct from `WF_CHILDREN_SLOT` so a container
/// widget nested in a component body doesn't capture the component's
/// content (and vice-versa).
pub(crate) const WF_CONTENT_SLOT: &str = "\u{FFF9}wdoc:content\u{FFF9}";

/// Look up the `lower` function for a block kind. Tries the block's
/// own `lower` field first (per-instance override), then the kind's
/// `@block` type's `@default(...)` for `lower`. Returns `None` when
/// neither path produces a callable.
pub(crate) fn lookup_block_lower(doc: &Document, block: &Block<'_>, kind: &str) -> Option<FnValue> {
    if let Some(field) = block.field("lower")
        && let Ok(Value::Function(fv)) = field.value()
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
/// `SvgFundamental` variant values. `None` when the block has no
/// record, no `lower`, the call errors, or the result isn't a list.
/// Shared by [`lower_svg_block`] and the `timeline` renderer (which
/// intercepts `Card` variants before rendering the rest).
pub(crate) fn lower_to_values(doc: &Document, block: &Block<'_>, kind: &str) -> Option<Vec<Value>> {
    let arg = block_to_record(doc, block, kind)?;
    let fv = lookup_block_lower(doc, block, kind)?;
    match doc.call_value(&fv, &[arg]) {
        Ok(Value::List(items)) => Some(items),
        // A non-list result is a benign "nothing to render"; an error is a
        // genuine evaluation failure inside the `lower` — record it so the
        // backend surfaces a diagnostic rather than silently dropping.
        Ok(_) => None,
        Err(e) => {
            record_lower_error(block, e);
            None
        }
    }
}

/// Custom diagram-shape lowering. Resolves the block's `lower`
/// function, calls it with a record built from the block's fields,
/// and renders each returned variant.
pub(crate) fn lower_svg_block(
    doc: &Document,
    block: &Block<'_>,
    kind: &str,
    parent_w: f64,
    parent_h: f64,
) -> String {
    let Some(items) = lower_to_values(doc, block, kind) else {
        return String::new();
    };
    items
        .iter()
        .map(|v| render_svg_variant(doc, v, parent_w, parent_h, 0))
        .collect()
}

/// Custom HTML-block lowering (h1..h6, text, code, callout, wireframe
/// widgets, and friends). `base_dir` is threaded so a container widget's
/// `HtmlFundamental::Children` slot can render nested blocks (which may
/// themselves resolve `source`-relative assets) via `render_block`.
pub(crate) fn lower_html_block(
    doc: &Document,
    block: &Block<'_>,
    kind: &str,
    patterns: &InlinePatterns,
    base_dir: Option<&Path>,
) -> String {
    let Some(arg) = block_to_record(doc, block, kind) else {
        return String::new();
    };
    let Some(fv) = lookup_block_lower(doc, block, kind) else {
        return String::new();
    };
    let result = match doc.call_value(&fv, &[arg]) {
        Ok(v) => v,
        Err(_) => return String::new(),
    };
    let Value::List(items) = result else {
        return String::new();
    };
    let out: String = items
        .iter()
        .map(|v| render_html_variant(doc, v, 0, patterns))
        .collect();
    // A container widget's `lower` marks where its children go with an
    // `HtmlFundamental::Children` slot (rendered as WF_CHILDREN_SLOT).
    // Render this block's nested blocks — each dispatching to its own
    // `lower` via `render_block` — and splice them in. Leaf blocks never
    // emit the slot, so they skip this entirely. Nesting resolves bottom
    // up: a nested container's own slot is filled before it returns here.
    if out.contains(WF_CHILDREN_SLOT) {
        let kids: String = block
            .blocks()
            .filter_map(|b| render_block(doc, &b, patterns, base_dir))
            .collect();
        out.replace(WF_CHILDREN_SLOT, &kids)
    } else {
        out
    }
}

/// Shared tail for a custom (non-fundamental) variant: rebuild its record,
/// resolve the kind's `lower`, call it, and render each returned variant via
/// `render_each` at `depth + 1`. Any failure (no `lower`, call error, or a
/// non-list result) yields an empty string. Used by both the SVG and HTML
/// dispatchers below; the terminal's `draw_variant` has its own grid-mutating
/// recursion (no `String` return, no depth marker) and stays separate.
fn lower_recurse(
    doc: &Document,
    map: &BTreeMap<String, Value>,
    kind: &str,
    depth: usize,
    render_each: impl Fn(&Value, usize) -> String,
) -> String {
    let arg = payload_to_record(map, kind);
    let Some(fv) = lookup_type_lower(doc, kind) else {
        return String::new();
    };
    let Ok(Value::List(items)) = doc.call_value(&fv, &[arg]) else {
        return String::new();
    };
    items.iter().map(|v| render_each(v, depth + 1)).collect()
}

// `_parent_w` / `_parent_h` are threaded through so future variant
// kinds can pick them up; today's fundamentals carry pre-resolved
// geometry in the payload itself.
pub(crate) fn render_svg_variant(
    doc: &Document,
    value: &Value,
    _parent_w: f64,
    _parent_h: f64,
    depth: usize,
) -> String {
    if depth > MAX_LOWER_DEPTH {
        return depth_marker();
    }
    let Value::Variant {
        variant, payload, ..
    } = value
    else {
        return String::new();
    };
    let kind = kind_for_variant(variant);
    let VariantPayload::Record(map) = payload else {
        return String::new();
    };
    match kind.as_str() {
        "rect" => render_rect_payload(map),
        "circle" => render_circle_payload(map),
        "line" => render_line_payload(map),
        "label" => render_label_payload(map),
        "polygon" => render_polygon_payload(map),
        // Custom variant — look up its type's `lower` and recurse with the
        // variant's record payload as the new arg.
        other => lower_recurse(doc, map, other, depth, |v, d| {
            render_svg_variant(doc, v, _parent_w, _parent_h, d)
        }),
    }
}

pub(crate) fn render_html_variant(
    doc: &Document,
    value: &Value,
    depth: usize,
    patterns: &InlinePatterns,
) -> String {
    if depth > MAX_LOWER_DEPTH {
        return depth_marker();
    }
    let Value::Variant {
        variant, payload, ..
    } = value
    else {
        return String::new();
    };
    let kind = kind_for_variant(variant);
    // The child slot carries no fields — handle it before the record
    // guard so it works whether authored as `Children {}` or a unit
    // variant. `lower_html_block` swaps the sentinel for the block's
    // rendered children.
    if kind == "children" {
        return WF_CHILDREN_SLOT.to_string();
    }
    let VariantPayload::Record(map) = payload else {
        return String::new();
    };
    match kind.as_str() {
        "paragraph" => render_paragraph_payload(map),
        "table" => render_table_payload(map),
        "element" => render_element_payload(doc, map, depth, patterns),
        "raw" => render_raw_payload(map),
        // An icon, resolved against the registry so it records sprite
        // usage. Emitted by the stdlib `callout` lowering (and available
        // to any user HTML lowering).
        "icon" => render_icon_fundamental(map, patterns.icons()),
        // Inline prose run through the inline-pattern engine (bold /
        // italic / link / icon). The Rust regex engine stays a leaf; the
        // `<p>`/`<span>` wrappers around it live in WCL (the `text` lower).
        "inline" => render_inline_fundamental(doc, map, patterns),
        // Syntax-highlighted code body (syntect). Like `inline`, the
        // engine is a leaf; the `<pre><code>` wrapper is the `code` lower.
        "highlighted" => render_highlighted_fundamental(map),
        // LaTeX → self-contained SVG via RaTeX. Like `highlighted`, the
        // SVG is a Rust leaf; the centring `<div>` wrapper is in math.rs.
        "math" => crate::math::render_math_fundamental(map),
        other => lower_recurse(doc, map, other, depth, |v, d| {
            render_html_variant(doc, v, d, patterns)
        }),
    }
}

pub(crate) fn depth_marker() -> String {
    "<!-- wdoc: lowering depth limit reached -->".into()
}

/// Build a `Value::Record` from `block`'s declared fields, then grow any
/// numeric `width`/`height` to the effective (text-aware) dimensions the
/// layout solver reserved — see [`effective_dims`]. Used by the SVG /
/// HTML lowering paths, where a shape's `width`/`height` is geometry.
pub(crate) fn block_to_record(doc: &Document, block: &Block<'_>, kind: &str) -> Option<Value> {
    let raw = block_to_record_raw(doc, block, kind)?;
    let Value::Record { ty, mut fields } = raw else {
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
    let (eff_w, eff_h) = effective_dims(block);
    if fields.get("width").is_some_and(Value::is_numeric) {
        fields.insert("width".to_string(), Value::F64(eff_w));
    }
    if fields.get("height").is_some_and(Value::is_numeric) {
        fields.insert("height".to_string(), Value::F64(eff_h));
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
            labels.get(slot as usize).cloned().unwrap_or(Value::None)
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
                        Some(blocks) => Value::List(
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
                    Some(blocks) => Value::List(
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
        fields: map,
    })
}

pub(crate) fn payload_to_record(map: &BTreeMap<String, Value>, kind: &str) -> Value {
    Value::Record {
        ty: vec![kind_to_typename(kind)],
        fields: map.clone(),
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

// ── Fundamental renderers (block-side) ────────────────────────────
