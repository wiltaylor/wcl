//! Lowering dispatch: invoke a block's `lower` function and recursively
//! render the returned fundamental variants (HTML + SVG).

use std::cell::RefCell;
use std::collections::BTreeMap;

use miette::NamedSource;
use wcl_lang::{Block, Document, EvalError, FnValue, Value, VariantPayload};

use crate::content::Content;
use crate::inline::InlinePatterns;

use super::*;

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
        Ok(node) => Some(node),
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
        Some(Ok(node)) => Some(Lowered::Content(node)),
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

/// Custom diagram-shape lowering. Resolves the block's `lower`
/// function, calls it with a record built from the block's fields,
/// and renders each returned variant. `links` is the inline-pattern
/// resolver used by `Link` fundamentals; `None` renders their children
/// unwrapped.
pub(crate) fn lower_svg_block(
    doc: &Document,
    block: &Block<'_>,
    kind: &str,
    parent_w: f64,
    parent_h: f64,
    links: Option<&InlinePatterns>,
) -> String {
    let Some(items) = lower_to_values(doc, block, kind) else {
        return String::new();
    };
    items
        .iter()
        .map(|v| render_svg_variant(doc, v, parent_w, parent_h, 0, links))
        .collect()
}

/// Custom HTML-block lowering (h1..h6, text, code, callout, wireframe
/// widgets, and friends).
pub(crate) fn lower_html_block(
    doc: &Document,
    block: &Block<'_>,
    kind: &str,
    patterns: &InlinePatterns,
) -> String {
    // Route through `lower_block` so HTML shares the other backends'
    // error handling: a `lower` body that errors records a fatal eval
    // diagnostic (it used to be silently swallowed here), and a missing
    // lowering records a warning. A block lowering to the content IR
    // renders through the shared reading of it (`render_content`).
    let Some(items) = lower_block(doc, block, kind) else {
        return String::new();
    };
    items
        .iter()
        .map(|item| match item {
            Lowered::Content(node) => render_content(doc, node, patterns),
            Lowered::Html(value) => render_html_variant(doc, value, 0, patterns),
        })
        .collect()
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
    expand_custom_variant(doc, map, kind)
        .iter()
        .map(|v| render_each(v, depth + 1))
        .collect()
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
    links: Option<&InlinePatterns>,
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
        "polyline" => render_polyline_payload(map),
        // A recursive wrapper giving lowered sub-shapes a clickable
        // in-site link. Without a resolver the children pass through
        // unwrapped (PDF embedding, contexts with no page registry).
        "link" => {
            let children = match map.get("children") {
                Some(Value::List(items)) => items.as_slice(),
                _ => &[],
            };
            let inner: String = children
                .iter()
                .map(|v| render_svg_variant(doc, v, _parent_w, _parent_h, depth + 1, links))
                .collect();
            match (map_utf8(map, "href"), links) {
                (Some(href), Some(patterns)) => format!(
                    "<a href=\"{}\">{inner}</a>",
                    escape_html(&patterns.resolve_href(&href))
                ),
                _ => inner,
            }
        }
        // Custom variant — look up its type's `lower` and recurse with the
        // variant's record payload as the new arg.
        other => lower_recurse(doc, map, other, depth, |v, d| {
            render_svg_variant(doc, v, _parent_w, _parent_h, d, links)
        }),
    }
}

/// If `value` is an `Html::Head`, render its `children` (the
/// head fragment) and return it; otherwise `None`. Lets `render_template`
/// hoist a template's top-level head fundamentals into the page `<head>`.
pub(crate) fn head_fundamental_html_with_blocks(
    doc: &Document,
    value: &Value,
    patterns: &InlinePatterns,
    block_renderer: Option<&BlockRenderer<'_>>,
) -> Option<String> {
    let Value::Variant {
        variant, payload, ..
    } = value
    else {
        return None;
    };
    if kind_for_variant(variant) != "head" {
        return None;
    }
    let VariantPayload::Record(map) = payload else {
        return Some(String::new());
    };
    let head = match map.get("children") {
        Some(Value::List(items)) => items
            .iter()
            .map(|v| render_html_variant_with_blocks(doc, v, 0, patterns, block_renderer))
            .collect(),
        _ => String::new(),
    };
    Some(head)
}

pub(crate) fn render_html_variant(
    doc: &Document,
    value: &Value,
    depth: usize,
    patterns: &InlinePatterns,
) -> String {
    render_html_variant_with_blocks(doc, value, depth, patterns, None)
}

pub(crate) fn render_html_variant_with_blocks(
    doc: &Document,
    value: &Value,
    depth: usize,
    patterns: &InlinePatterns,
    block_renderer: Option<&BlockRenderer<'_>>,
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
        "blocks" => match (map.get("blocks"), block_renderer) {
            (Some(Value::List(handles)), Some(render)) => {
                let slot = map.get("slot").and_then(|value| match value {
                    Value::Symbol(name)
                    | Value::Identifier(name)
                    | Value::Utf8(name)
                    | Value::Ascii(name) => Some(name.as_str()),
                    _ => None,
                });
                let owner = map.get("owner").and_then(|value| match value {
                    Value::Identifier(name) | Value::Utf8(name) | Value::Ascii(name) => {
                        Some(name.as_str())
                    }
                    _ => None,
                });
                let fallback = if handles.is_empty() {
                    match map.get("fallback") {
                        Some(Value::List(items)) => items
                            .iter()
                            .map(|value| {
                                render_html_variant_with_blocks(
                                    doc,
                                    value,
                                    depth + 1,
                                    patterns,
                                    block_renderer,
                                )
                            })
                            .collect(),
                        _ => String::new(),
                    }
                } else {
                    String::new()
                };
                render(handles, slot, owner, &fallback)
            }
            _ => String::new(),
        },
        "paragraph" => render_paragraph_payload(doc, map, patterns),
        "table" => render_table_payload(map),
        "element" => render_element_payload(doc, map, depth, patterns, block_renderer),
        "raw" => render_raw_payload(map),
        "style" => map
            .get("name")
            .and_then(|value| match value {
                Value::Symbol(name)
                | Value::Identifier(name)
                | Value::Utf8(name)
                | Value::Ascii(name) => Some(name.as_str()),
                _ => None,
            })
            .and_then(|name| patterns.style(name))
            .map(|css| format!("<style>{css}</style>"))
            .unwrap_or_default(),
        // A `Head` reached in body context renders to nothing — its
        // children are hoisted into `<head>` only when the fundamental is
        // returned at a template's top level (see `head_fundamental_html`).
        "head" => String::new(),
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
        // A custom variant: expand it through its kind's own `lower`. What
        // that produces may be content — a user block is free to lower to
        // the semantic IR through a chain of its own variants.
        other => lower_recurse(doc, map, other, depth, |v, d| match recursed_content(v) {
            Some(node) => render_content(doc, &node, patterns),
            None => render_html_variant_with_blocks(doc, v, d, patterns, block_renderer),
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
    let (eff_w, eff_h) = effective_dims(block);
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
