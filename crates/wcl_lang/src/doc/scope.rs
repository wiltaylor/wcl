//! Lexical scope of an evaluation site.
//!
//! A [`Scope`] is an `Rc`-backed chain of enclosing block frames (outermost
//! first). Bare identifiers inside a `&T` field are resolved by walking the
//! chain innermost → outermost and then falling through to the document root.

use std::rc::Rc;

use crate::ast;

use super::ItemCells;

#[derive(Clone)]
pub(crate) struct Scope<'a> {
    frames: Rc<[ScopeFrame<'a>]>,
}

#[derive(Clone, Copy)]
pub(crate) struct ScopeFrame<'a> {
    pub(crate) ast: &'a ast::Block,
    pub(crate) cells: &'a ItemCells,
    pub(crate) kind_override: Option<&'a str>,
}

impl<'a> Scope<'a> {
    pub(crate) fn root() -> Self {
        Self {
            frames: Rc::from([]),
        }
    }

    pub(crate) fn push(&self, frame: ScopeFrame<'a>) -> Self {
        let mut v: Vec<ScopeFrame<'a>> = self.frames.iter().copied().collect();
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
