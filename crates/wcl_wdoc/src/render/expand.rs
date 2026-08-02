//! Body expansion for `wdoc_repeater` / `wdoc_component`, shared by the
//! HTML path (`render_repeat` / `render_component`) and the SVG/diagram
//! path (`diagram_children`). Both reduce to `Block::expand_bodies`
//! (wcl_lang), which is `OnceLock`-cached per block, so the collect and
//! render passes see byte-identical expansions.

use std::cell::RefCell;
use std::collections::HashSet;
use std::sync::Arc;

use wcl_lang::{Block, Document, EvalError, Expander, Value};

use crate::inline::InlinePatterns;

use super::lower::record_lower_error;
use super::{MAX_LOWER_DEPTH, field_bool, label_string};

/// `true` as soon as `pred` matches `block`, any descendant in its raw
/// block subtree, or — when a block is a `wdoc_component` instance — any
/// block in the component definition's body. Drives the player-asset
/// detection scans (`uses_pan_zoom` / `uses_terminal` / `uses_map` / …):
/// a feature used *only* inside a component body (which never appears in
/// the page's raw block tree, since the page holds the instance block)
/// still ships its JS/CSS.
///
/// Crossing into a component body increments `depth`; once it passes
/// `MAX_LOWER_DEPTH` the descent stops, bounding a self-referential
/// component (the def's body isn't slot-expanded here, so the binding
/// scope can't grow to trip the usual guard). Raw-subtree recursion is
/// bounded by the finite block tree. Detection-only: it reads the static
/// definition, so — unlike [`expand_container_children`] — it evaluates
/// no `each` / slot expressions and records no lowering errors.
pub(crate) fn block_tree_any<F: Fn(&Block<'_>) -> bool>(block: &Block<'_>, pred: &F) -> bool {
    fn go<F: Fn(&Block<'_>) -> bool>(block: &Block<'_>, pred: &F, depth: usize) -> bool {
        pred(block)
            || block.blocks().any(|b| go(&b, pred, depth))
            || (depth < MAX_LOWER_DEPTH
                && block
                    .doc()
                    .component_def(block.kind())
                    .is_some_and(|def| def.blocks().any(|b| go(&b, pred, depth + 1))))
    }
    go(block, pred, 0)
}

/// The blocks `block` generates, or `None` when it generates nothing and
/// is authored content in its own right. **The one place that decides
/// which kinds expand** — both the language's `@children` projections
/// (through [`WdocExpander`]) and the render path's
/// [`flatten_container_child`] ask here, so the two can't drift over what
/// counts as a generator.
///
/// `wdoc_content` is deliberately absent: it is a placement marker, so it
/// is `@contextual` (it may appear wherever a `WdocBlock` may) but the
/// renderer — not an expansion — fills it with the instance's own
/// children, and it must survive the flatten as itself.
fn generated_children<'a>(block: &Block<'a>) -> Option<Vec<Block<'a>>> {
    match block.kind() {
        "wdoc_repeater" => Some(expand_repeater_children(block)),
        "wdoc_instance" => Some(expand_instance_children(block)),
        kind => block
            .doc()
            .component_def(kind)
            .map(|def| expand_component_children(block, &def)),
    }
}

/// wdoc's [`Expander`] — the one answer to "what does this
/// `@contextual` block generate?", registered on every
/// [`wdoc_environment`](crate::wdoc_environment) so the language's
/// `@children` projections expand through exactly the code the renderer
/// does. One step only: the language recurses through nested contextual
/// blocks itself (and applies its own depth cap), which is what
/// [`flatten_container_child`] does for the render path.
pub struct WdocExpander;

impl Expander for WdocExpander {
    fn expand<'a>(&self, block: &Block<'a>) -> Vec<Block<'a>> {
        generated_children(block).unwrap_or_default()
    }
}

/// Flatten a container's children, replacing every `wdoc_repeater`,
/// `wdoc_component` instance, and `wdoc_instance` (recursively, in place)
/// with its expanded blocks; every other kind passes through unchanged —
/// including `container`, whose own children flatten when *it* is collected.
/// The single "repeater / component anywhere" entry point: every site that
/// iterates a container's child blocks (the diagram / SVG path, the
/// wireframe widget tree, the CSS `class` collection, …) runs this first so
/// data-generated blocks participate exactly like authored ones. The
/// expanded blocks carry their per-element / per-slot binding scope, so a
/// node's `id`, `label`, `x`/`y`, slot fields, etc. resolve against the data.
pub(crate) fn expand_container_children<'a>(block: &Block<'a>) -> Vec<Block<'a>> {
    let mut out = Vec::new();
    for child in block.blocks() {
        flatten_container_child(child, &mut out);
    }
    out
}

/// The SVG/diagram alias for [`expand_container_children`], kept for
/// call-site readability where children become diagram shapes.
pub(crate) fn diagram_children<'a>(block: &Block<'a>) -> Vec<Block<'a>> {
    expand_container_children(block)
}

fn flatten_container_child<'a>(child: Block<'a>, out: &mut Vec<Block<'a>>) {
    // Stop runaway self-referential expansion (mirrors the HTML guard).
    if child.binding_scope_depth() > MAX_LOWER_DEPTH {
        return;
    }
    match generated_children(&child) {
        Some(generated) => {
            for c in generated {
                flatten_container_child(c, out);
            }
        }
        None => out.push(child),
    }
}

/// Expand a `wdoc_repeater`'s body once per element of its `each` list,
/// binding the element to the symbol named by `as` (default `it`).
/// Returns the flattened body child blocks, each carrying the per-element
/// binding scope. Empty when `each` doesn't evaluate to a list.
pub(crate) fn expand_repeater_children<'a>(block: &Block<'a>) -> Vec<Block<'a>> {
    // A present `each` whose expression fails to evaluate (e.g. an
    // unresolved reference) is a genuine error — record it so the build
    // surfaces a diagnostic instead of silently expanding to nothing. A
    // present-but-non-list value stays silently empty (existing
    // semantics).
    let each = match block.field("each").map(|f| f.value().cloned()) {
        Some(Ok(v)) => Some(v),
        Some(Err(e)) => {
            record_lower_error(block, e.clone());
            None
        }
        None => None,
    };
    let Some(Value::List(items)) = each else {
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
    let binding_sets: Vec<Arc<Vec<(String, Value)>>> = std::sync::Arc::unwrap_or_clone(items)
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

thread_local! {
    /// Body addresses whose `project` is currently rendering on this thread.
    /// Guards a body that projects itself (`project a` inside `body a`),
    /// which plain block recursion — unlike repeater / component expansion —
    /// would not bound via `binding_scope_depth`. Mirrors `ACTIVE_COLLECTS`.
    static ACTIVE_PROJECTS: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
}

/// RAII guard returned by [`enter_project`]; removes the address from the
/// active set when dropped.
pub(crate) struct ProjectGuard(String);

impl Drop for ProjectGuard {
    fn drop(&mut self) {
        ACTIVE_PROJECTS.with(|set| {
            set.borrow_mut().remove(&self.0);
        });
    }
}

/// Begin projecting the body at `path`. Returns a guard while that address
/// was not already being projected on this thread; returns `None` on a
/// re-entrant cycle, in which case the caller renders nothing.
pub(crate) fn enter_project(path: &str) -> Option<ProjectGuard> {
    ACTIVE_PROJECTS.with(|set| {
        if set.borrow().contains(path) {
            None
        } else {
            set.borrow_mut().insert(path.to_string());
            Some(ProjectGuard(path.to_string()))
        }
    })
}

/// The document path(s) a `project` block addresses — its `from` field
/// evaluated to one or more `Value::DataPath` references (a single `@child`
/// body, or a list from a `@children` body slot). Empty when `from` is
/// absent or doesn't evaluate to a reference, which the caller surfaces as a
/// diagnostic.
fn project_target_paths(block: &Block<'_>) -> Vec<String> {
    fn path_of(v: &Value) -> Option<String> {
        match v {
            Value::DataPath { segments, .. } if !segments.is_empty() => Some(segments.join(".")),
            _ => None,
        }
    }
    let Some(Ok(value)) = block.field("from").map(|f| f.value().cloned()) else {
        return Vec::new();
    };
    match value {
        Value::List(items) => items.iter().filter_map(path_of).collect(),
        v => path_of(&v).into_iter().collect(),
    }
}

/// Handle the structural block kinds every backend treats identically,
/// so HTML / PDF / Markdown dispatch them once instead of three times:
///
/// - an `@only` / `@except`-filtered block contributes nothing;
/// - speaker `notes` and Markdown `frontmatter` never render as page
///   content (front matter is read separately by the Markdown target);
/// - a `partial` deposit renders at its source only with
///   `show_here = true`;
/// - a `collect` gathers every matching `partial`'s body across the
///   document, cycle-guarded (the guard is held across the recursion).
///
/// Children are fed back through `recurse` (the backend's own per-block
/// entry point). Returns `Some(outcome)` when the block was one of these
/// kinds and is fully handled; `None` means "not structural — run your
/// own dispatch". `fragment` is deliberately NOT handled here: HTML
/// wraps fragment children in a step-reveal `<div>`, so only the static
/// backends treat it as transparent.
pub(crate) fn walk_structural<E>(
    doc: &Document,
    block: &Block<'_>,
    patterns: &InlinePatterns,
    recurse: &mut dyn for<'b> FnMut(&Block<'b>) -> Result<(), E>,
) -> Option<Result<(), E>> {
    if !crate::visibility::block_visible(block, patterns) {
        return Some(Ok(()));
    }
    match block.kind() {
        "notes" | "frontmatter" => Some(Ok(())),
        "partial" => {
            if field_bool(block, "show_here") == Some(true) {
                for child in block.blocks() {
                    if let Err(e) = recurse(&child) {
                        return Some(Err(e));
                    }
                }
            }
            Some(Ok(()))
        }
        "collect" => {
            let tag = label_string(block).unwrap_or_default();
            let Some(_guard) = enter_collect(&tag) else {
                return Some(Ok(()));
            };
            for child in collect_partials(doc, &tag) {
                if let Err(e) = recurse(&child) {
                    return Some(Err(e));
                }
            }
            Some(Ok(()))
        }
        // A `body` is content attached to a data record, reached only via
        // `project`; it never renders at its own definition site. (It isn't a
        // `WdocBlock`, so it normally can't appear as page content at all —
        // this arm keeps it inert if it ever does.)
        "body" => Some(Ok(())),
        "project" => {
            let paths = project_target_paths(block);
            if paths.is_empty() {
                record_lower_error(
                    block,
                    EvalError::user_error(
                        "`project`'s `from` did not resolve to a body reference; it must name an \
                         addressable `body` (e.g. a `@by_ref` property of the data being \
                         generated from)"
                            .to_string(),
                        block.span(),
                    ),
                );
                return Some(Ok(()));
            }
            for path in paths {
                // A body that projects itself renders nothing for the inner
                // hit (the guard); the outer projection still completes.
                let Some(_guard) = enter_project(&path) else {
                    continue;
                };
                match doc.get(&path).and_then(|dr| dr.as_block()) {
                    Some(body) if body.kind() == "body" => {
                        for child in body.blocks() {
                            if let Err(e) = recurse(&child) {
                                return Some(Err(e));
                            }
                        }
                    }
                    _ => record_lower_error(
                        block,
                        EvalError::user_error(
                            format!("`project` target `{path}` did not resolve to a `body`"),
                            block.span(),
                        ),
                    ),
                }
            }
            Some(Ok(()))
        }
        _ => None,
    }
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
        // An *absent* instance field falls to the slot's `default`; a
        // *present* field whose expression fails to evaluate is a genuine
        // error — record it so the build surfaces a diagnostic instead of
        // silently binding the slot to `none`.
        let val = match instance.field(&name).map(|f| f.value().cloned()) {
            Some(Ok(v)) => Some(v),
            Some(Err(e)) => {
                record_lower_error(instance, e.clone());
                None
            }
            None => None,
        }
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

/// The component name a `wdoc_instance` selects — its `component` field
/// evaluated to a string (a `utf8` / `symbol` / `identifier` value). `None`
/// when the field is absent or not a name-like scalar.
fn instance_component_name(instance: &Block<'_>) -> Option<String> {
    instance
        .field("component")
        .and_then(|f| f.value().ok().cloned())
        .and_then(|v| match v {
            Value::Utf8(s) | Value::Symbol(s) | Value::Identifier(s) => Some(s),
            _ => None,
        })
}

/// The `wdoc_component` definition a `wdoc_instance` targets — the component
/// whose name is the instance's `component` field value. `None` when the
/// field names no declared component.
pub(crate) fn instance_target_def<'a>(instance: &Block<'a>) -> Option<Block<'a>> {
    instance
        .doc()
        .component_def(&instance_component_name(instance)?)
}

/// Expand a `wdoc_instance` — the render-by-reference counterpart to writing
/// a component's name as a block. Resolves the target `wdoc_component` from
/// the instance's `component` value, then binds each declared `wdoc_slot`
/// from the instance's like-named field (or the slot `default`) via the
/// shared [`expand_component_children`]. Empty when `component` names no
/// declared component.
pub(crate) fn expand_instance_children<'a>(instance: &Block<'a>) -> Vec<Block<'a>> {
    let Some(def) = instance_target_def(instance) else {
        return Vec::new();
    };
    expand_component_children(instance, &def)
}
