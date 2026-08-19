//! The live collector: the tree as it is being built.
//!
//! Crate-private, and deliberately so — a host receives a [`Profile`]
//! snapshot and never drives the collector itself. The evaluator's only
//! contact with it is [`ProfileGuard`], which times a span by existing:
//! constructed on entry, folded into the tree on drop. When the document
//! is not profiling the guard holds `None` and every method is a no-op,
//! which is what keeps the hooks affordable on the hot path.

use std::sync::Mutex;
use std::time::Instant;

use super::{Profile, ProfileKey, ProfileNode};

#[derive(Debug)]
/// The profiler's mutable state: the tree built so far, plus the
/// stack of frames currently being timed.
pub(crate) struct ProfileState {
    /// The accumulated call tree.
    root: ProfileNode,
    /// Frames entered but not yet exited, outermost first.
    stack: Vec<StackFrame>,
}

#[derive(Debug)]
/// One in-progress timing: what is being measured, and since when.
struct StackFrame {
    /// What this frame measures.
    key: ProfileKey,
    /// When the frame was entered.
    start: Instant,
}

impl ProfileState {
    /// A fresh profiler state, wrapped for shared mutation.
    pub(crate) fn new_root() -> Mutex<Self> {
        Mutex::new(Self {
            root: ProfileNode::new(ProfileKey::Root),
            stack: Vec::new(),
        })
    }

    /// Push a frame and start timing it.
    pub(crate) fn enter(&mut self, key: ProfileKey) {
        self.stack.push(StackFrame {
            key,
            start: Instant::now(),
        });
    }

    /// Pop the innermost frame and fold its elapsed time into the tree.
    pub(crate) fn exit(&mut self) {
        let Some(frame) = self.stack.pop() else {
            return;
        };
        let elapsed = frame.start.elapsed();
        // Walk down from root through the surviving stack frames, then
        // record into the popped frame's child node.
        let mut node = &mut self.root;
        for f in &self.stack {
            node = node
                .children
                .entry(f.key.clone())
                .or_insert_with(|| ProfileNode::new(f.key.clone()));
        }
        let child = node
            .children
            .entry(frame.key.clone())
            .or_insert_with(|| ProfileNode::new(frame.key));
        child.record(elapsed);
    }

    /// Copy out the tree accumulated so far, leaving the state intact.
    pub(crate) fn snapshot(&self) -> Profile {
        Profile {
            root: self.root.clone(),
        }
    }
}

/// RAII guard: enters a node on construction, exits on drop. When the
/// document is not profiling, `state` is `None` and the guard is a
/// no-op.
pub(crate) struct ProfileGuard<'a> {
    /// The profiler to report to, or `None` when profiling is off — in
    /// which case the guard does nothing.
    state: Option<&'a Mutex<ProfileState>>,
}

impl<'a> ProfileGuard<'a> {
    /// Start timing `key`, if `state` is present. Timing stops on drop.
    pub(crate) fn enter(state: Option<&'a Mutex<ProfileState>>, key: ProfileKey) -> Self {
        if let Some(s) = state {
            // A poisoned profile mutex would mean a previous evaluator
            // call panicked mid-update. Recover the inner state and
            // keep going — losing recent timings is better than
            // poisoning the document.
            let mut g = s.lock().unwrap_or_else(|p| p.into_inner());
            g.enter(key);
        }
        Self { state }
    }
}

impl Drop for ProfileGuard<'_> {
    fn drop(&mut self) {
        if let Some(s) = self.state {
            let mut g = s.lock().unwrap_or_else(|p| p.into_inner());
            g.exit();
        }
    }
}
