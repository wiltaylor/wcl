//! Body expansion for `wdoc_repeater` / `wdoc_component`, shared by the
//! HTML path (`render_repeat` / `render_component`) and the SVG/diagram
//! path (`diagram_children`). Both reduce to `Block::expand_bodies`
//! (wcl_lang), which is `OnceLock`-cached per block, so the collect and
//! render passes see byte-identical expansions.

use std::sync::Arc;

use wcl_lang::{Block, Value};

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
