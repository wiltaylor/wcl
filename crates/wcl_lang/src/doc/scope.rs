//! Lexical scope of an evaluation site.
//!
//! A [`Scope`] is an `Rc`-backed chain of enclosing block frames (outermost
//! first). Bare identifiers inside a `&T` field are resolved by walking the
//! chain innermost → outermost and then falling through to the document root.

use std::rc::Rc;

use crate::ast;
use crate::value::Value;

use super::{Block, ItemCells};

#[derive(Clone)]
pub(crate) struct Scope<'a> {
    frames: Rc<[ScopeFrame<'a>]>,
}

/// One enclosing block in the lexical chain. Carries the block's AST +
/// cells (so name resolution can walk its fields/blocks/lets) and,
/// optionally, a set of pre-evaluated `name → value` **bindings** that
/// resolve like an inner `let` but come from the *renderer* rather than
/// source — this is how a `@contextual` block's parameters and loop
/// variable are injected into the scope a body subtree evaluates in.
/// A frame always has `ast`/`cells`, so absolute frame-indexing
/// (`frame_as_block` / `self_dataref` / `parent_dataref`) is unaffected by
/// the presence of bindings.
#[derive(Clone)]
pub(crate) struct ScopeFrame<'a> {
    pub(crate) ast: &'a ast::Block,
    pub(crate) cells: &'a ItemCells,
    /// Namespace of the file the frame's block lives in, so a `Block`
    /// view rebuilt from this frame resolves bare kinds in the right
    /// namespace (mirrors `Block::file_ns`).
    pub(crate) file_ns: &'a [String],
    pub(crate) kind_override: Option<&'a str>,
    pub(crate) bindings: Option<std::sync::Arc<Vec<(String, Value)>>>,
    /// Structural content supplied by a renderer-driven component expansion.
    /// Descendant placement markers can retrieve these block nodes without
    /// encoding them as values or rendered strings.
    pub(crate) content: Option<std::rc::Rc<std::collections::BTreeMap<String, Vec<Block<'a>>>>>,
    /// **Dynamic** expansion depth at the point this frame was created:
    /// 0 for plain lexical frames, `instance depth + 1` for a
    /// component/repeater binding frame. Carried explicitly because a
    /// component body's expansion scope is rebuilt from the
    /// *definition's* lexical scope (always shallow), so counting
    /// binding frames in the chain would never grow across nested
    /// instantiations — the recursion guard
    /// (`Block::binding_scope_depth`) takes the max of these instead.
    pub(crate) expansion_depth: usize,
}

impl<'a> Scope<'a> {
    pub(crate) fn root() -> Self {
        Self {
            frames: Rc::from([]),
        }
    }

    pub(crate) fn push(&self, frame: ScopeFrame<'a>) -> Self {
        let mut v: Vec<ScopeFrame<'a>> = self.frames.iter().cloned().collect();
        v.push(frame);
        Self { frames: v.into() }
    }

    pub(crate) fn frames(&self) -> &[ScopeFrame<'a>] {
        &self.frames
    }

    /// Build a `Scope` from an arbitrary slice of frames. Used by
    /// `self_dataref` / `parent_dataref` to construct the scope visible
    /// from a parent frame.
    pub(crate) fn from_frames(frames: &[ScopeFrame<'a>]) -> Self {
        Self {
            frames: frames.to_vec().into(),
        }
    }
}

impl std::fmt::Debug for Scope<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Scope")
            .field("depth", &self.frames.len())
            .finish()
    }
}
