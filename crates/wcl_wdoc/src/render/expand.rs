//! Body expansion for `wdoc_repeater` / `wdoc_component`, shared by the
//! HTML path (`render_repeat` / `render_component`) and the SVG/diagram
//! path (`diagram_children`). Both reduce to `Block::expand_bodies`
//! (wcl_lang), which is `OnceLock`-cached per block, so the collect and
//! render passes see byte-identical expansions.

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Arc;

use wcl_lang::{Block, Document, Value};

use super::{MAX_LOWER_DEPTH, label_string};

/// Flatten a `diagram` / `container`'s children for the SVG path: each
/// `wdoc_repeater` / `wdoc_component`-instance child is replaced (in
/// place, recursively) by its expanded shape blocks, so generated nodes
/// participate in layout, position collection, and edge rendering. Every
/// other kind passes through unchanged — including `container`, whose own
/// children flatten when *it* is collected. The expanded shapes carry
/// their per-element / per-slot binding scope, so a node's `id`, `label`,
/// `x`/`y`, etc. resolve against the data.
pub(crate) fn diagram_children<'a>(block: &Block<'a>) -> Vec<Block<'a>> {
    let mut out = Vec::new();
    for child in block.blocks() {
        flatten_diagram_child(child, &mut out);
    }
    out
}

fn flatten_diagram_child<'a>(child: Block<'a>, out: &mut Vec<Block<'a>>) {
    // Stop runaway self-referential expansion (mirrors the HTML guard).
    if child.binding_scope_depth() > MAX_LOWER_DEPTH {
        return;
    }
    match child.kind() {
        "wdoc_repeater" => {
            for c in expand_repeater_children(&child) {
                flatten_diagram_child(c, out);
            }
        }
        kind => {
            if let Some(def) = child.doc().component_def(kind) {
                for c in expand_component_children(&child, &def) {
                    flatten_diagram_child(c, out);
                }
            } else {
                out.push(child);
            }
        }
    }
}

/// Expand a `wdoc_repeater`'s body once per element of its `each` list,
/// binding the element to the symbol named by `as` (default `it`).
/// Returns the flattened body child blocks, each carrying the per-element
/// binding scope. Empty when `each` doesn't evaluate to a list.
pub(crate) fn expand_repeater_children<'a>(block: &Block<'a>) -> Vec<Block<'a>> {
    let Some(Value::List(items)) = block.field("each").and_then(|f| f.value().ok().cloned()) else {
        return Vec::new();
    };
    let as_name = block
        .field("as")
        .and_then(|f| f.value().ok().cloned())
        .and_then(|v| match v {
            Value::Symbol(s) | Value::Identifier(s) | Value::Utf8(s) => Some(s),
            _ => None,
        })
        .unwrap_or_else(|| "it".to_string());
    let binding_sets: Vec<Arc<Vec<(String, Value)>>> = items
        .into_iter()
        .map(|el| Arc::new(vec![(as_name.clone(), el)]))
        .collect();
    block
        .expand_bodies(block, binding_sets)
        .into_iter()
        .flatten()
        .collect()
}

/// The bodies of every `partial` whose `tag` matches `want`, gathered
/// across the whole document — the root source and its eagerly-imported
/// files (`Document::blocks` walks `all_sources`) — recursing into nested
/// blocks, in deterministic document order. Returns the flattened child
/// blocks of the matching partials, ready to render via each backend's
/// per-block entrypoint, exactly like `expand_repeater_children`.
///
/// Does not descend into a partial's own body looking for further
/// partials — a body is content, not a nesting site — so a partial nested
/// inside another partial's body is part of that body, not collected
/// twice. Lazily-imported (block-scoped `import`) files are not reachable,
/// matching how `page` / `wdoc_component` already resolve.
pub(crate) fn collect_partials<'a>(doc: &'a Document, want: &str) -> Vec<Block<'a>> {
    let mut out = Vec::new();
    for top in doc.blocks() {
        gather_partials(&top, want, &mut out);
    }
    out
}

fn gather_partials<'a>(block: &Block<'a>, want: &str, out: &mut Vec<Block<'a>>) {
    if block.kind() == "partial" {
        if label_string(block).as_deref() == Some(want) {
            out.extend(block.blocks());
        }
        return;
    }
    for child in block.blocks() {
        gather_partials(&child, want, out);
    }
}

thread_local! {
    /// Tags whose `collect` is currently being rendered on this thread.
    /// Guards against a collected partial body that itself contains a
    /// `collect` of an ancestor tag (collect → partial → collect cycle),
    /// which plain block recursion — unlike repeater/component expansion —
    /// would not bound via `binding_scope_depth`.
    static ACTIVE_COLLECTS: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

/// RAII guard returned by [`enter_collect`]; removes the tag from the
/// active set when dropped.
pub(crate) struct CollectGuard(String);

impl Drop for CollectGuard {
    fn drop(&mut self) {
        ACTIVE_COLLECTS.with(|set| {
            set.borrow_mut().remove(&self.0);
        });
    }
}

/// Begin collecting `tag`. Returns a guard while the tag was not already
/// being collected on this thread; returns `None` if it is (a re-entrant
/// cycle), in which case the caller renders nothing.
pub(crate) fn enter_collect(tag: &str) -> Option<CollectGuard> {
    ACTIVE_COLLECTS.with(|set| {
        if set.borrow().contains(tag) {
            None
        } else {
            set.borrow_mut().insert(tag.to_string());
            Some(CollectGuard(tag.to_string()))
        }
    })
}

/// Expand a `wdoc_component` instance's `wdoc_body` once, binding each
/// declared `wdoc_slot` to the instance's matching field (or the slot's
/// `default`). Returns the body child blocks under the slot bindings.
/// Empty when the component declares no `wdoc_body`.
pub(crate) fn expand_component_children<'a>(
    instance: &Block<'a>,
    def: &Block<'a>,
) -> Vec<Block<'a>> {
    let mut bindings: Vec<(String, Value)> = Vec::new();
    for slot in def.blocks().filter(|b| b.kind() == "wdoc_slot") {
        let Some(name) = label_string(&slot) else {
            continue;
        };
        let val = instance
            .field(&name)
            .and_then(|f| f.value().ok().cloned())
            .or_else(|| slot.field("default").and_then(|f| f.value().ok().cloned()))
            .unwrap_or(Value::None);
        bindings.push((name, val));
    }
    let Some(body) = def.block("wdoc_body") else {
        return Vec::new();
    };
    instance
        .expand_bodies(&body, vec![Arc::new(bindings)])
        .into_iter()
        .next()
        .unwrap_or_default()
}
