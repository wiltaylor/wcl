//! The op vocabulary: every structural edit a curator (or an editing UI)
//! makes to a wskill, defined once.
//!
//! Two properties make this the shared half rather than a second reading of
//! the format:
//!
//! - **Every op is id-addressed** — `(kind, id)`, with the kind optional and
//!   inferred when unambiguous. Spans shift under every reformat, and *the
//!   curator has never seen a rendered page*, so a span-addressed op is a UI
//!   affordance rather than a curation primitive. Where a host does hold a
//!   span (the editor drags an edge between rendered nodes), it resolves the
//!   span to a [`NodeRef`] and calls the same function — see
//!   [`edit_related`].
//! - **An op returns [`Change`]s, it does not write them.** Applying one is
//!   a parse → mutate → print over the files the [`Graph`] says the target
//!   is written in; committing the result is the caller's — the editor runs
//!   its validating commit pipeline, which rolls back on a schema violation.
//!
//! What is NOT here: **where a brand-new top-level block goes**. That is a
//! placement convention of the host (the editor derives it from where the
//! existing instances live), so [`Op::CreateIndex`] names an [`IndexHome`]
//! rather than choosing one. [`index_block`] and [`check_new_index_id`] are
//! public so a host doing its own file creation still builds and validates
//! the index the one way.

use std::fmt;
use std::path::{Path, PathBuf};

use wcl_lang::ast::{self, Expr, Item};
use wcl_lang::edit::find_block_by_kind_label;
use wcl_lang::{Span, edit as ast_edit, format as wcl_format, is_identifier, parse_for_edit};

use crate::Error;
use crate::model::{Graph, Index};
use crate::registry::ast_label;

/// The id addressing a training course's top level — the one nav level with
/// no block of its own, since a course's structure IS its lesson data.
/// Double-underscored so it cannot collide with an authored block id, which
/// WCL identifiers never start with.
pub const COURSE_ID: &str = "__course";

/// A node an op addresses: its id, and its kind when the caller knows it.
///
/// The kind is optional because an agent writing ops by hand shouldn't be
/// punished for omitting what the graph can infer; it is *carried* because
/// unit ids are only assumed unique across kinds and nothing enforces it —
/// an op that silently hits the wrong kind is the failure mode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeRef {
    pub kind: Option<String>,
    pub id: String,
}

impl NodeRef {
    /// A reference by id alone, resolved against whatever the graph holds.
    pub fn new(id: impl Into<String>) -> NodeRef {
        NodeRef {
            kind: None,
            id: id.into(),
        }
    }

    /// A reference that also pins the kind.
    pub fn kinded(kind: impl Into<String>, id: impl Into<String>) -> NodeRef {
        NodeRef {
            kind: Some(kind.into()),
            id: id.into(),
        }
    }
}

impl fmt::Display for NodeRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            Some(k) => write!(f, "{k}:{}", self.id),
            None => write!(f, "{}", self.id),
        }
    }
}

/// Which way a structural move goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dir {
    Up,
    Down,
}

/// One structural edit.
#[derive(Debug, Clone)]
pub enum Op {
    /// Append a unit to an index's `related` list.
    PinUnit { index: NodeRef, unit: NodeRef },
    /// Drop a unit from an index's `related` list.
    UnpinUnit { index: NodeRef, unit: NodeRef },
    /// Rewrite a level's order — an index's `related` list, or a course
    /// level's lesson `n`s. `order` must be a permutation of what is there,
    /// so a stale caller cannot drop a member.
    ReorderChildren { index: NodeRef, order: Vec<String> },
    /// Link one node to another (`related`). An index source is a pin.
    RelatedAdd { from: NodeRef, to: NodeRef },
    /// Unlink one node from another.
    RelatedRemove { from: NodeRef, to: NodeRef },
    /// A new `index` block, at the [`IndexHome`] the caller names.
    CreateIndex {
        id: String,
        name: String,
        home: IndexHome,
    },
    /// Remove an index and everything nested in it.
    DeleteIndex { index: NodeRef },
    /// Swap an index with its adjacent `index` sibling.
    MoveIndex { index: NodeRef, dir: Dir },
    /// Lift a sub-index out to its parent's level, right after it.
    PromoteIndex { index: NodeRef },
    /// Nest an index under the one above it.
    DemoteIndex { index: NodeRef },
}

/// Where a brand-new index goes.
///
/// Not an address: the id-addressed vocabulary says WHICH index an op acts
/// on, and a brand-new one has no id to be found by yet — so its home is part
/// of the request. Nesting is a format fact this crate owns; a file is the
/// host's placement answer (the editor derives it from where the existing
/// indexes live).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexHome {
    /// Nested inside an existing top-level index.
    Under(NodeRef),
    /// Appended to a file the caller's placement chose.
    InFile(PathBuf),
}

/// One file's new contents. Applying an op produces these; committing them
/// is the caller's.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Change {
    pub file: PathBuf,
    pub text: String,
}

type Result<T> = std::result::Result<T, Error>;

/// Apply one op against `graph`, returning the files it rewrites.
///
/// The graph is the addressing table: it says which file each `(kind, id)`
/// is written in. Nothing is relocated by span — every block is found again
/// by kind and label in a fresh parse, so a reformat between the load and
/// the write cannot misfire.
pub fn apply(graph: &Graph, op: &Op) -> Result<Vec<Change>> {
    match op {
        Op::PinUnit { index, unit } => pin(graph, index, unit, true),
        Op::UnpinUnit { index, unit } => pin(graph, index, unit, false),
        Op::ReorderChildren { index, order } => reorder(graph, index, order),
        Op::RelatedAdd { from, to } => related(graph, from, to, true),
        Op::RelatedRemove { from, to } => related(graph, from, to, false),
        Op::CreateIndex { id, name, home } => create_index(graph, id, name, home),
        Op::DeleteIndex { index } => delete_index(graph, index),
        Op::MoveIndex { index, dir } => move_index(graph, index, *dir),
        Op::PromoteIndex { index } => promote_index(graph, index),
        Op::DemoteIndex { index } => demote_index(graph, index),
    }
}

// ---------------------------------------------------------------------------
// The `related` list: read, write, and the one add/remove
// ---------------------------------------------------------------------------

/// The `related` identifier list a block declares. `None` means the field is
/// a **computed expression**: it can be read from the evaluated document but
/// must never be rewritten, because a rewrite would clobber the expression
/// that produced it.
pub fn declared_related(block: &ast::Block) -> Option<Vec<String>> {
    let elements = match block.items.iter().find_map(|it| match it {
        Item::Field(f) if f.name == "related" => Some(&f.expr),
        _ => None,
    }) {
        Some(Expr::ListLit { elements, .. }) => elements,
        // A computed expression: readable through the evaluated document,
        // never rewritable.
        Some(_) => return None,
        None => return Some(Vec::new()),
    };
    // Every element must be a bare id. [`set_related`] writes bare ids, so a
    // list holding anything else — the annotated `{id, why}` record form
    // (`Link`), which the model already READS — must be refused rather than
    // silently rewritten without its reasons.
    elements
        .iter()
        .map(|e| match e {
            Expr::Identifier(s, _) | Expr::Utf8(s) | Expr::Ascii(s) => Some(s.clone()),
            _ => None,
        })
        .collect()
}

/// Rewrite a block's `related` field to exactly `ids`.
pub fn set_related(block: &mut ast::Block, ids: &[String]) {
    ast_edit::set_or_insert_field(
        block,
        "related",
        Expr::ListLit {
            elem_trivia: ids.iter().map(|_| Default::default()).collect(),
            elements: ids
                .iter()
                .map(|s| Expr::Identifier(s.clone(), Span::new(0, 0)))
                .collect(),
            trailing_trivia: Vec::new(),
            span: Span::new(0, 0),
        },
    )
}

/// The same field, two vocabularies: a unit *relates* to another unit, an
/// index *pins* one. Only the wording differs, so only the wording is
/// carried — [`rewrite_related`] is the single rewrite behind both.
#[derive(Debug, Clone, Copy)]
enum Wording {
    Related,
    Pin,
}

impl Wording {
    fn computed(self) -> Error {
        Error::Op(
            match self {
                Wording::Related => {
                    "the block's related list is computed — edit its source instead"
                }
                Wording::Pin => "the index's related list is computed — edit its source instead",
            }
            .to_string(),
        )
    }

    fn already(self, id: &str) -> Error {
        Error::Op(match self {
            Wording::Related => format!("already related to `{id}`"),
            Wording::Pin => format!("`{id}` is already pinned"),
        })
    }

    fn absent(self, id: &str) -> Error {
        Error::Op(match self {
            Wording::Related => format!("`{id}` is not in the related list"),
            Wording::Pin => format!("`{id}` is not pinned here"),
        })
    }
}

/// Add or remove one id in a block's `related` list — the single rewrite,
/// which every op that touches the field goes through.
fn rewrite_related(block: &mut ast::Block, id: &str, add: bool, words: Wording) -> Result<()> {
    let current = declared_related(block).ok_or_else(|| words.computed())?;
    let next: Vec<String> = if add {
        if current.iter().any(|s| s == id) {
            return Err(words.already(id));
        }
        let mut n = current;
        n.push(id.to_string());
        n
    } else {
        if !current.iter().any(|s| s == id) {
            return Err(words.absent(id));
        }
        current.into_iter().filter(|s| s != id).collect()
    };
    set_related(block, &next);
    Ok(())
}

/// Add or remove one link in an already-located block's `related` list.
///
/// This is the **one** implementation of the edge write. [`Op::RelatedAdd`]
/// and [`Op::RelatedRemove`] find the block by `(kind, id)` and call it; a
/// host that drags between rendered nodes resolves its span to the same
/// [`NodeRef`] ([`node_ref`]) and calls it too, which is what keeps a drag
/// and a curated op the same operation.
///
/// `from` is the resolved node the link is written ON: it is what the
/// self-link check and every refusal name, so a host that resolved a span
/// gets a message about the node rather than about a byte range.
pub fn edit_related(block: &mut ast::Block, from: &NodeRef, to: &str, add: bool) -> Result<()> {
    if !is_identifier(to) {
        return Err(format!("`{to}` is not a valid unit id").into());
    }
    if from.id == to {
        return Err(format!("`{from}` cannot relate to itself").into());
    }
    rewrite_related(block, to, add, Wording::Related)
}

/// The `(kind, id)` a parsed block is addressed by — how a host holding a
/// span resolves it into the id-addressed vocabulary. `None` when the block
/// carries no label to be named by.
pub fn node_ref(block: &ast::Block) -> Option<NodeRef> {
    Some(NodeRef::kinded(block.kind.clone(), ast_label(block)?))
}

fn related(graph: &Graph, from: &NodeRef, to: &NodeRef, add: bool) -> Result<Vec<Change>> {
    let site = locate(graph, from)?;
    let from = site.node.clone();
    let target = to.id.clone();
    edit_block(&site, move |block| edit_related(block, &from, &target, add))
}

// ---------------------------------------------------------------------------
// Index pins
// ---------------------------------------------------------------------------

fn pin(graph: &Graph, index: &NodeRef, unit: &NodeRef, add: bool) -> Result<Vec<Change>> {
    if is_course_level(graph, &index.id) {
        return Err(
            "a course has no pins — a lesson belongs to it by existing; reorder it instead".into(),
        );
    }
    let site = index_site(graph, index)?;
    let unit_id = unit.id.clone();
    edit_block(&site, move |block| {
        rewrite_related(block, &unit_id, add, Wording::Pin)
    })
}

fn reorder(graph: &Graph, index: &NodeRef, order: &[String]) -> Result<Vec<Change>> {
    if is_course_level(graph, &index.id) {
        return course_reorder(graph, &index.id, order);
    }
    let site = index_site(graph, index)?;
    let order = order.to_vec();
    edit_block(&site, move |block| {
        let current = declared_related(block).ok_or_else(|| Wording::Pin.computed())?;
        if !is_permutation(&current, &order) {
            return Err("`order` must be a permutation of the current list".into());
        }
        set_related(block, &order);
        Ok(())
    })
}

fn is_permutation(a: &[String], b: &[String]) -> bool {
    let (mut a, mut b) = (a.to_vec(), b.to_vec());
    a.sort();
    b.sort();
    a == b
}

// ---------------------------------------------------------------------------
// The course: order is each lesson's `n`, not a `related` list
// ---------------------------------------------------------------------------

/// Whether `id` names a level of the training course — its synthetic top
/// level, or one of its `module`s. Both order their lessons by `n`.
pub fn is_course_level(graph: &Graph, id: &str) -> bool {
    if id == COURSE_ID {
        return true;
    }
    graph
        .course
        .as_ref()
        .is_some_and(|c| c.modules.iter().any(|m| m.id == id))
}

/// Reorder a course level by rewriting its lessons' `n` to `1..=len`.
/// Lessons may live in several files; every touched file lands in ONE change
/// set, so the course is never half-renumbered.
fn course_reorder(graph: &Graph, level: &str, order: &[String]) -> Result<Vec<Change>> {
    let course = graph
        .course
        .as_ref()
        .ok_or("this wskill declares no course")?;
    let current: &[String] = if level == COURSE_ID {
        &course.lessons
    } else {
        &course
            .modules
            .iter()
            .find(|m| m.id == level)
            .ok_or_else(|| Error::Op(format!("no `module` with id `{level}`")))?
            .lessons
    };
    if !is_permutation(current, order) {
        return Err("`order` must be a permutation of the level's lessons".into());
    }

    // Group the renumbering by declaring file, so each file is parsed,
    // mutated and printed exactly once.
    let mut per_file: Vec<(PathBuf, Vec<(String, u32)>)> = Vec::new();
    for (i, id) in order.iter().enumerate() {
        let unit = graph
            .unit(id)
            .ok_or_else(|| Error::Op(format!("no `lesson` with id `{id}`")))?;
        let file = graph.root.join(&unit.anchor.file);
        match per_file.iter_mut().find(|(f, _)| *f == file) {
            Some((_, list)) => list.push((id.clone(), i as u32 + 1)),
            None => per_file.push((file, vec![(id.clone(), i as u32 + 1)])),
        }
    }

    let mut changes = Vec::new();
    for (file, lessons) in per_file {
        changes.extend(edit_file(&file, |src| {
            for (id, n) in lessons {
                let blk = find_block_by_kind_label(&mut src.items, "lesson", &id)
                    .ok_or_else(|| relocate_err("lesson", &id))?;
                ast_edit::set_or_insert_field(blk, "n", Expr::U32(n));
            }
            Ok(())
        })?);
    }
    Ok(changes)
}

// ---------------------------------------------------------------------------
// The index tree: create / delete / reorder / promote / demote
// ---------------------------------------------------------------------------
//
// The pin ops above edit an index's CONTENTS; these edit the tree itself.
// All rewrite exactly one file: an index and its sub-indexes are one block
// subtree, so nesting never crosses files.

/// A fresh `index <id> { name = "…" }` block. Public so a host doing its own
/// placement still builds the block the one way.
pub fn index_block(id: &str, name: &str) -> ast::Block {
    ast_edit::build_block(
        "index",
        &[],
        vec![Expr::Identifier(id.to_string(), Span::new(0, 0))],
        vec![("name".to_string(), ast_edit::string_literal_expr(name))],
    )
}

/// Whether `id` is available as a new index id: a legal identifier that no
/// index at any nesting level already uses.
pub fn check_new_index_id(graph: &Graph, id: &str) -> Result<()> {
    if !is_identifier(id) {
        return Err(format!(
            "`{id}` is not a valid id (letters, digits, `_`, not starting with a digit)"
        )
        .into());
    }
    if graph.index(id).is_some() {
        return Err(format!("an `index` with id `{id}` already exists").into());
    }
    Ok(())
}

fn create_index(graph: &Graph, id: &str, name: &str, home: &IndexHome) -> Result<Vec<Change>> {
    check_new_index_id(graph, id)?;
    let block = index_block(id, name);
    match home {
        IndexHome::Under(parent) => {
            let site = index_site(graph, parent)?;
            if let Some(gp) = graph.parent_index(&parent.id) {
                return Err(format!(
                    "`{}` is itself nested under `{}` — sub-indexes nest one level deep",
                    parent.id, gp.id
                )
                .into());
            }
            edit_block(&site, move |pblock| {
                pblock.items.push(Item::Block(block));
                Ok(())
            })
        }
        IndexHome::InFile(file) => edit_file(file, move |src| {
            ast_edit::append_top_level_block(src, block);
            Ok(())
        }),
    }
}

/// Remove an index and everything nested in it. Its pins are just ids in a
/// `related` list, so nothing dangles elsewhere — the units stay put.
fn delete_index(graph: &Graph, index: &NodeRef) -> Result<Vec<Change>> {
    let site = index_site(graph, index)?;
    edit_source(&site, |src, id| {
        let (items, pos) =
            index_slot(&mut src.items, id).ok_or_else(|| relocate_err("index", id))?;
        items.remove(pos);
        Ok(())
    })
}

/// Swap an index with its adjacent `index` sibling. Top-level order is
/// document order, so an index at the edge of ITS file can't move further
/// here — the files' import order decides that.
fn move_index(graph: &Graph, index: &NodeRef, dir: Dir) -> Result<Vec<Change>> {
    let site = index_site(graph, index)?;
    let down = dir == Dir::Down;
    edit_source(&site, move |src, id| {
        let (items, pos) =
            index_slot(&mut src.items, id).ok_or_else(|| relocate_err("index", id))?;
        let target = index_sibling(items, pos, down).ok_or_else(|| {
            Error::Op(format!(
                "`{id}` is already the {} index at its level",
                if down { "last" } else { "first" }
            ))
        })?;
        items.swap(pos, target);
        Ok(())
    })
}

/// Lift a sub-index out to its parent's level, placed right after it.
fn promote_index(graph: &Graph, index: &NodeRef) -> Result<Vec<Change>> {
    let site = index_site(graph, index)?;
    let parent = graph
        .parent_index(&index.id)
        .ok_or_else(|| Error::Op(format!("`{}` is already a top-level index", index.id)))?
        .id
        .clone();
    edit_source(&site, move |src, id| {
        let (items, pos) =
            index_slot(&mut src.items, id).ok_or_else(|| relocate_err("index", id))?;
        let block = items.remove(pos);
        let (pitems, ppos) =
            index_slot(&mut src.items, &parent).ok_or_else(|| relocate_err("index", &parent))?;
        pitems.insert(ppos + 1, block);
        Ok(())
    })
}

/// Nest an index under the one above it. Sub-indexes render one level deep,
/// so an already-nested index refuses rather than building a tree nothing
/// projects.
fn demote_index(graph: &Graph, index: &NodeRef) -> Result<Vec<Change>> {
    if let Some(parent) = graph.parent_index(&index.id) {
        return Err(format!(
            "`{}` is already nested under `{}` — sub-indexes nest one level deep",
            index.id, parent.id
        )
        .into());
    }
    let site = index_site(graph, index)?;
    edit_source(&site, |src, id| {
        let (items, pos) =
            index_slot(&mut src.items, id).ok_or_else(|| relocate_err("index", id))?;
        let target = index_sibling(items, pos, false).ok_or_else(|| {
            Error::Op(format!(
                "no index above `{id}` to nest it under — move it down first"
            ))
        })?;
        let block = items.remove(pos);
        match &mut items[target] {
            Item::Block(b) => b.items.push(block),
            _ => return Err(relocate_err("index", id)),
        }
        Ok(())
    })
}

// ---------------------------------------------------------------------------
// Addressing: (kind, id) → the file it is written in
// ---------------------------------------------------------------------------

/// A located target: the node as the graph names it, and the absolute file
/// it is declared in.
struct Site {
    node: NodeRef,
    file: PathBuf,
}

/// Resolve a reference against the graph — a unit, else an index.
fn locate(graph: &Graph, r: &NodeRef) -> Result<Site> {
    if r.kind.as_deref() != Some("index")
        && let Some(u) = graph.unit(&r.id)
        && r.kind.as_ref().is_none_or(|k| *k == u.kind)
    {
        return Ok(Site {
            node: NodeRef::kinded(u.kind.clone(), u.id.clone()),
            file: graph.root.join(&u.anchor.file),
        });
    }
    if r.kind.as_ref().is_none_or(|k| k == "index")
        && let Some(i) = graph.index(&r.id)
    {
        return Ok(site_of_index(graph, i));
    }
    Err(no_such(r))
}

fn index_site(graph: &Graph, r: &NodeRef) -> Result<Site> {
    if let Some(kind) = &r.kind
        && kind != "index"
    {
        return Err(Error::Op(format!("`{r}` is not an index")));
    }
    graph
        .index(&r.id)
        .map(|i| site_of_index(graph, i))
        .ok_or_else(|| Error::Op(format!("no `index` with id `{}`", r.id)))
}

fn site_of_index(graph: &Graph, i: &Index) -> Site {
    Site {
        node: NodeRef::kinded("index", i.id.clone()),
        file: graph.root.join(&i.anchor.file),
    }
}

fn no_such(r: &NodeRef) -> Error {
    match &r.kind {
        Some(k) => Error::Op(format!("no `{k}` with id `{}`", r.id)),
        None => Error::Op(format!("nothing in this wskill is named `{}`", r.id)),
    }
}

fn relocate_err(kind: &str, id: &str) -> Error {
    Error::Op(format!("could not relocate {kind} `{id}`"))
}

/// Parse one file, apply `mutate`, print. The only place an op reads or
/// writes a file, so every op rewrites source the one way.
fn edit_file(
    file: &Path,
    mutate: impl FnOnce(&mut ast::Source) -> Result<()>,
) -> Result<Vec<Change>> {
    let text = std::fs::read_to_string(file)
        .map_err(|e| Error::Op(format!("failed to read {}: {e}", file.display())))?;
    let mut src = parse_for_edit(&text, file.display().to_string())?;
    mutate(&mut src)?;
    Ok(vec![Change {
        file: file.to_path_buf(),
        text: wcl_format::to_source(&src),
    }])
}

/// [`edit_file`] for the structural ops, which move a block between item
/// lists and so want the whole source plus the id they are addressed by.
fn edit_source(
    site: &Site,
    mutate: impl FnOnce(&mut ast::Source, &str) -> Result<()>,
) -> Result<Vec<Change>> {
    edit_file(&site.file, |src| mutate(src, &site.node.id))
}

/// [`edit_file`] for the ops that edit ONE block's fields: it is found again
/// by kind and label, never by span, so a reformat between the load and the
/// write cannot misfire.
fn edit_block(
    site: &Site,
    mutate: impl FnOnce(&mut ast::Block) -> Result<()>,
) -> Result<Vec<Change>> {
    edit_source(site, |src, id| {
        let kind = site.node.kind.as_deref().unwrap_or_default();
        let block = find_block_by_kind_label(&mut src.items, kind, id)
            .ok_or_else(|| relocate_err(kind, id))?;
        mutate(block)
    })
}

// ---------------------------------------------------------------------------
// AST helpers
// ---------------------------------------------------------------------------

/// Is this AST item the `index` block labelled `id`?
fn is_index_item(it: &Item, id: &str) -> bool {
    matches!(it, Item::Block(b)
        if b.kind == "index" && ast_label(b).as_deref() == Some(id))
}

/// Does this AST block's subtree declare an `index` with `id`?
fn ast_subtree_has_index(b: &ast::Block, id: &str) -> bool {
    (b.kind == "index" && ast_label(b).as_deref() == Some(id))
        || b.items
            .iter()
            .any(|it| matches!(it, Item::Block(c) if ast_subtree_has_index(c, id)))
}

/// The items list OWNING the `index` block with `id`, and its position in it
/// — the handle every structural op needs (its siblings are right there).
fn index_slot<'a>(items: &'a mut Vec<Item>, id: &str) -> Option<(&'a mut Vec<Item>, usize)> {
    if let Some(i) = items.iter().position(|it| is_index_item(it, id)) {
        return Some((items, i));
    }
    // Descend into the one child whose subtree holds it (chosen immutably,
    // so the mutable reborrow below is the only live borrow).
    let child = items
        .iter()
        .position(|it| matches!(it, Item::Block(b) if ast_subtree_has_index(b, id)))?;
    match &mut items[child] {
        Item::Block(b) => index_slot(&mut b.items, id),
        _ => None,
    }
}

/// The position of the adjacent `index` sibling in `dir`, skipping anything
/// else at that level (fields, a `body` block, units sharing the file).
fn index_sibling(items: &[Item], pos: usize, down: bool) -> Option<usize> {
    let is_index = |i: &usize| matches!(&items[*i], Item::Block(b) if b.kind == "index");
    if down {
        (pos + 1..items.len()).find(is_index)
    } else {
        (0..pos).rev().find(is_index)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testsupport::{mini_wskill, write};

    /// Apply an op and write the changes back, so a test can chain.
    fn run(root: &Path, op: Op) -> Result<Vec<Change>> {
        let graph = Graph::open(root)?;
        let changes = apply(&graph, &op)?;
        for c in &changes {
            std::fs::write(&c.file, &c.text).unwrap();
        }
        Ok(changes)
    }

    fn read(root: &Path, rel: &str) -> String {
        std::fs::read_to_string(root.join(rel)).unwrap()
    }

    #[test]
    fn pins_unpins_and_reorders_by_id() {
        let td = mini_wskill();
        let root = td.path();

        run(
            root,
            Op::ReorderChildren {
                index: NodeRef::new("lang"),
                order: vec!["beta".into(), "alpha".into()],
            },
        )
        .expect("reorder");
        assert!(read(root, "data/indexes.wcl").contains("related = [beta, alpha]"));

        run(
            root,
            Op::UnpinUnit {
                index: NodeRef::new("lang"),
                unit: NodeRef::new("alpha"),
            },
        )
        .expect("unpin");
        assert!(read(root, "data/indexes.wcl").contains("related = [beta]"));

        run(
            root,
            Op::PinUnit {
                index: NodeRef::new("lang"),
                unit: NodeRef::new("alpha"),
            },
        )
        .expect("pin");
        assert!(read(root, "data/indexes.wcl").contains("related = [beta, alpha]"));

        // A pin that is already there, and a bad permutation, are refused.
        let e = run(
            root,
            Op::PinUnit {
                index: NodeRef::new("lang"),
                unit: NodeRef::new("alpha"),
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("already pinned"), "{e}");
        let e = run(
            root,
            Op::ReorderChildren {
                index: NodeRef::new("lang"),
                order: vec!["beta".into()],
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("permutation"), "{e}");
    }

    /// An op names an index that isn't there, or names one by the wrong
    /// kind — both are refusals, not silent misses.
    #[test]
    fn refuses_an_unresolvable_target() {
        let td = mini_wskill();
        let e = run(
            td.path(),
            Op::PinUnit {
                index: NodeRef::new("nope"),
                unit: NodeRef::new("alpha"),
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("no `index` with id `nope`"), "{e}");

        let e = run(
            td.path(),
            Op::RelatedAdd {
                from: NodeRef::kinded("fact", "alpha"),
                to: NodeRef::new("beta"),
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("no `fact` with id `alpha`"), "{e}");
    }

    #[test]
    fn related_add_and_remove_are_id_addressed() {
        let td = mini_wskill();
        let root = td.path();
        // `alpha` already relates to `beta` in the fixture; drop and re-add.
        run(
            root,
            Op::RelatedRemove {
                from: NodeRef::new("alpha"),
                to: NodeRef::new("beta"),
            },
        )
        .expect("remove");
        assert!(read(root, "data/concepts/alpha.wcl").contains("related = []"));
        run(
            root,
            Op::RelatedAdd {
                from: NodeRef::new("alpha"),
                to: NodeRef::new("beta"),
            },
        )
        .expect("add");
        assert!(read(root, "data/concepts/alpha.wcl").contains("related = [beta]"));

        for (op, msg) in [
            (
                Op::RelatedAdd {
                    from: NodeRef::new("alpha"),
                    to: NodeRef::new("beta"),
                },
                "already related",
            ),
            (
                Op::RelatedAdd {
                    from: NodeRef::new("alpha"),
                    to: NodeRef::new("alpha"),
                },
                "itself",
            ),
            (
                Op::RelatedRemove {
                    from: NodeRef::new("alpha"),
                    to: NodeRef::new("gamma"),
                },
                "not in the related list",
            ),
        ] {
            let e = run(root, op).unwrap_err();
            assert!(e.to_string().contains(msg), "{e}");
        }

        // A computed list is read but never rewritten.
        write(
            root,
            "data/concepts/alpha.wcl",
            "concept alpha {\n  name = \"Alpha\"\n  related = flatten([])\n}\n",
        );
        let e = run(
            root,
            Op::RelatedAdd {
                from: NodeRef::new("alpha"),
                to: NodeRef::new("beta"),
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("computed"), "{e}");
    }

    /// The index tree ops: create nested, promote, demote, move, delete.
    #[test]
    fn edits_the_index_tree() {
        let td = mini_wskill();
        let root = td.path();
        // Nested creation, then promotion to the top level.
        run(
            root,
            Op::CreateIndex {
                id: "lang_sub".into(),
                name: "Sub".into(),
                home: IndexHome::Under(NodeRef::new("lang")),
            },
        )
        .expect("create nested");
        let g = Graph::open(root).unwrap();
        assert_eq!(g.index("lang").unwrap().children[0].id, "lang_sub");
        assert_eq!(
            g.parent_index("lang_sub").map(|i| i.id.as_str()),
            Some("lang")
        );

        // A duplicate id is refused, at any nesting level.
        let e = run(
            root,
            Op::CreateIndex {
                id: "lang_sub".into(),
                name: "Again".into(),
                home: IndexHome::Under(NodeRef::new("lang")),
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("already exists"), "{e}");

        run(
            root,
            Op::PromoteIndex {
                index: NodeRef::new("lang_sub"),
            },
        )
        .expect("promote");
        let g = Graph::open(root).unwrap();
        assert_eq!(g.indexes.len(), 2);
        assert_eq!(g.indexes[1].id, "lang_sub");

        run(
            root,
            Op::MoveIndex {
                index: NodeRef::new("lang_sub"),
                dir: Dir::Up,
            },
        )
        .expect("move up");
        assert_eq!(Graph::open(root).unwrap().indexes[0].id, "lang_sub");

        // Demote nests it under the index above — which is now `lang`… but
        // `lang_sub` sits first, so there is nothing above it.
        let e = run(
            root,
            Op::DemoteIndex {
                index: NodeRef::new("lang_sub"),
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("no index above"), "{e}");
        run(
            root,
            Op::MoveIndex {
                index: NodeRef::new("lang_sub"),
                dir: Dir::Down,
            },
        )
        .expect("move down");
        run(
            root,
            Op::DemoteIndex {
                index: NodeRef::new("lang_sub"),
            },
        )
        .expect("demote");
        assert_eq!(
            Graph::open(root).unwrap().index("lang").unwrap().children[0].id,
            "lang_sub"
        );

        run(
            root,
            Op::DeleteIndex {
                index: NodeRef::new("lang"),
            },
        )
        .expect("delete");
        let g = Graph::open(root).unwrap();
        assert!(g.indexes.is_empty(), "the subtree goes with it");
        // The units it pinned are untouched.
        assert!(g.unit("alpha").is_some());
    }

    /// A top-level index is appended to the file the caller's placement
    /// chose — the library owns what an index is, not where it lands.
    #[test]
    fn creates_a_top_level_index_in_the_named_file() {
        let td = mini_wskill();
        let root = td.path();
        run(
            root,
            Op::CreateIndex {
                id: "usage".into(),
                name: "Usage".into(),
                home: IndexHome::InFile(root.join("data/indexes.wcl")),
            },
        )
        .expect("create");
        assert!(read(root, "data/indexes.wcl").contains("index usage"));
        assert_eq!(Graph::open(root).unwrap().indexes.len(), 2);

        let e = run(
            td.path(),
            Op::CreateIndex {
                id: "9bad".into(),
                name: "Bad".into(),
                home: IndexHome::InFile(root.join("data/indexes.wcl")),
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("not a valid id"), "{e}");
    }

    /// A course reorders by rewriting each lesson's `n`; it has no pins.
    #[test]
    fn reorders_a_course_by_rewriting_n() {
        let td = mini_wskill();
        let root = td.path();
        write(
            root,
            "data/lessons.wcl",
            "lesson first { title = \"First\"  n = 1u32 }\n\n\
             lesson second { title = \"Second\"  n = 2u32 }\n",
        );
        run(
            root,
            Op::ReorderChildren {
                index: NodeRef::new(COURSE_ID),
                order: vec!["second".into(), "first".into()],
            },
        )
        .expect("reorder");
        let text = read(root, "data/lessons.wcl");
        // Source order is untouched; `n` carries the order.
        assert!(text.find("lesson first").unwrap() < text.find("lesson second").unwrap());
        let n_of = |id: &str| {
            let rest = &text[text.find(&format!("lesson {id}")).unwrap()..];
            let at = rest.find("n = ").unwrap();
            rest[at + 4..].trim_start().chars().next().unwrap()
        };
        assert_eq!(n_of("second"), '1', "{text}");
        assert_eq!(n_of("first"), '2', "{text}");

        let e = run(
            root,
            Op::PinUnit {
                index: NodeRef::new(COURSE_ID),
                unit: NodeRef::new("first"),
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("no pins"), "{e}");
    }

    /// The span-addressed path a rendered UI needs: resolve the block to its
    /// `(kind, id)`, then call the same function the id-addressed op calls.
    #[test]
    fn a_span_resolves_to_the_same_edit() {
        let src = parse_for_edit("concept alpha {\n  name = \"Alpha\"\n}\n", "t").unwrap();
        let mut src = src;
        let block = match &mut src.items[0] {
            Item::Block(b) => b,
            _ => unreachable!(),
        };
        let from = node_ref(block).expect("a labelled block resolves");
        assert_eq!(from, NodeRef::kinded("concept", "alpha"));
        edit_related(block, &from, "beta", true).expect("add");
        assert_eq!(declared_related(block).unwrap(), ["beta"]);
        // The resolved ref is load-bearing, not decoration: it is what the
        // self-link check reads and what the refusal names, so a host that
        // came in holding a span hears about the node.
        let e = edit_related(block, &from, "alpha", true).unwrap_err();
        assert_eq!(e.to_string(), "`concept:alpha` cannot relate to itself");
    }

    /// An annotated link (`{id, why}` — the `Link` form the model already
    /// reads) must not be rewritten: `set_related` writes bare ids, so a
    /// rewrite would drop the author's reasons. The refusal is the
    /// computed-list one, because it is the same rule — "I can read this, I
    /// must not write it".
    #[test]
    fn refuses_to_rewrite_an_annotated_related_list() {
        let td = mini_wskill();
        let root = td.path();
        write(
            root,
            "data/indexes.wcl",
            "index lang {\n  name = \"Language\"\n  \
             related = [{ id: alpha, why: \"start here\" }, beta]\n}\n",
        );
        let e = run(
            root,
            Op::UnpinUnit {
                index: NodeRef::new("lang"),
                unit: NodeRef::new("beta"),
            },
        )
        .unwrap_err();
        assert!(e.to_string().contains("computed"), "{e}");
        assert!(read(root, "data/indexes.wcl").contains("why:"), "untouched");
    }

    /// Pinning and relating are one rewrite of one field, in two
    /// vocabularies. The wording is all that differs — so the wording is all
    /// that is written twice.
    #[test]
    fn a_pin_and_a_link_are_the_same_rewrite_in_two_vocabularies() {
        let td = mini_wskill();
        let root = td.path();
        let pin_err = run(
            root,
            Op::PinUnit {
                index: NodeRef::new("lang"),
                unit: NodeRef::new("alpha"),
            },
        )
        .unwrap_err();
        let link_err = run(
            root,
            Op::RelatedAdd {
                from: NodeRef::new("alpha"),
                to: NodeRef::new("beta"),
            },
        )
        .unwrap_err();
        assert_eq!(pin_err.to_string(), "`alpha` is already pinned");
        assert_eq!(link_err.to_string(), "already related to `beta`");

        // And both refuse a computed list, each in its own words.
        write(
            root,
            "data/indexes.wcl",
            "index lang {\n  name = \"Language\"\n  related = flatten([])\n}\n",
        );
        let e = run(
            root,
            Op::UnpinUnit {
                index: NodeRef::new("lang"),
                unit: NodeRef::new("alpha"),
            },
        )
        .unwrap_err();
        assert_eq!(
            e.to_string(),
            "the index's related list is computed — edit its source instead"
        );
    }
}
