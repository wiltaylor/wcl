//! Lexical scope of an evaluation site.
//!
//! A [`Scope`] is an `Rc`-backed chain of enclosing block frames (outermost
//! first). Bare identifiers inside a `&T` field are resolved by walking the
//! chain innermost → outermost and then falling through to the document root.

use std::rc::Rc;

use crate::ast;
use crate::value::Value;

use super::ItemCells;

#[derive(Clone)]
pub(crate) struct Scope<'a> {
    frames: Rc<[ScopeFrame<'a>]>,
}

/// One enclosing block in the lexical chain. Carries the block's AST +
/// cells (so name resolution can walk its fields/blocks/lets) and,
/// optionally, a set of pre-evaluated `name → value` **bindings** that
/// resolve like an inner `let` but come from the *renderer* rather than
/// source — this is how a `wdoc_component`'s slots and a `wdoc_repeater`'s
/// loop variable are injected into the scope a body subtree evaluates in.
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
