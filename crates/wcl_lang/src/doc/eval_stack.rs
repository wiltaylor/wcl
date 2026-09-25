//! What the current thread is evaluating right now.
//!
//! Every lazily-forced cache — a field or `let` cell, a `@connections`
//! projection, the connection-operand index — pushes a frame here while
//! it computes and pops it when done. Two questions read the stack:
//!
//! - **Is this cell already being forced on this thread?** Then the
//!   request is re-entrant: a genuine cycle, or (for `a = a`) the
//!   binding being defined, which scope lookup skips outward.
//! - **How deep is the evaluation?** A long chain of references that
//!   never loops (`a0 = a1 + 1`, `a1 = a2 + 1`, …) recurses once per
//!   link, and so does a `fn` that calls itself. User-`fn` calls push a
//!   frame too, so one cap, [`MAX_EVAL_DEPTH`], bounds them together: a
//!   field or `let` past it fails with [`EvalError::EvalDepthExceeded`],
//!   a call with [`EvalError::CallDepthExceeded`], instead of the
//!   thread's stack overflowing.
//!
//! The stack is per thread on purpose. A `Document` is `Sync`, and two
//! threads forcing the same cell each see only their own frames: the
//! second computes the value independently rather than mistaking the
//! first thread's in-flight work for a cycle, and the cell keeps
//! whichever identical result lands first. Frames are keyed by the
//! address of the cache they guard, so documents sharing a thread never
//! see each other's frames.

use std::cell::RefCell;

use crate::ast::Span;
use crate::diagnostics::EvalError;

/// Most frames the evaluation stack may hold on one thread — nested
/// field, `let`, projection and user-`fn` evaluations combined. Sized for
/// a 2 MiB thread stack (Rust's default for spawned threads, and the
/// LSP's tokio workers) in an optimised build: the heaviest frame, a
/// `fn` called back through a higher-order builtin such as `map`, takes
/// about 7 KiB, so a full stack stays under 1.5 MiB and leaves the
/// host's own frames room beneath it.
pub(crate) const MAX_EVAL_DEPTH: usize = 200;

/// Identity of one frame on the evaluation stack.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FrameKey {
    /// A cache keyed by its address alone: a field / `let` cell or a
    /// document's connection-operand index.
    Cell(usize),
    /// A named projection over a cache: `(cache address, name hash)`.
    /// The root `@connections` memo and a block's `typed_proj_memo` hold
    /// one projection per field name.
    Named(usize, u64),
    /// A user-`fn` call. Counts towards the depth but is never looked
    /// up, since recursion through a function is capped separately.
    Call,
}

impl FrameKey {
    /// Key a frame on the address of the cache it guards.
    pub(crate) fn cell<T>(cache: &T) -> Self {
        FrameKey::Cell(std::ptr::from_ref(cache) as usize)
    }

    /// Key a frame on one named projection over `cache`.
    pub(crate) fn named<T>(cache: &T, name: &str) -> Self {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        name.hash(&mut h);
        FrameKey::Named(std::ptr::from_ref(cache) as usize, h.finish())
    }
}

thread_local! {
    /// The frames this thread is inside, outermost first.
    static STACK: RefCell<Vec<FrameKey>> = const { RefCell::new(Vec::new()) };
}

/// A frame on the evaluation stack, popped when dropped — so an early
/// return or a panic unwinding through the evaluator can't leave a
/// stale frame that would later read as a cycle.
pub(crate) struct EvalFrame(());

impl Drop for EvalFrame {
    fn drop(&mut self) {
        STACK.with(|s| s.borrow_mut().pop());
    }
}

/// Why [`enter`] refused a frame.
pub(crate) enum Refused {
    /// The key is already on this thread's stack.
    Cycle,
    /// The stack is at [`MAX_EVAL_DEPTH`].
    TooDeep,
}

impl Refused {
    /// The error a refused frame reports: a cycle on `name`, or the
    /// depth cap. Both point at `span`.
    pub(crate) fn into_error(self, name: &str, span: Span) -> EvalError {
        match self {
            Refused::Cycle => EvalError::Cycle {
                field: name.to_string(),
                span: super::span_to_miette(span),
            },
            Refused::TooDeep => EvalError::eval_depth_exceeded(MAX_EVAL_DEPTH, span),
        }
    }
}

/// Push `key`, or refuse when it is already being evaluated on this
/// thread or the stack is full.
pub(crate) fn enter(key: FrameKey) -> Result<EvalFrame, Refused> {
    STACK.with(|s| {
        let mut s = s.borrow_mut();
        if key != FrameKey::Call && s.contains(&key) {
            return Err(Refused::Cycle);
        }
        if s.len() >= MAX_EVAL_DEPTH {
            return Err(Refused::TooDeep);
        }
        s.push(key);
        Ok(EvalFrame(()))
    })
}

/// `true` while `key` is being evaluated further up this thread's stack.
pub(crate) fn is_active(key: FrameKey) -> bool {
    STACK.with(|s| s.borrow().contains(&key))
}
