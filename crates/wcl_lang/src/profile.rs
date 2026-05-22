//! Opt-in performance profiler for [`Document`](crate::Document) evaluation.
//!
//! When a document is opened via one of the `*_profiled` constructors,
//! every field force, user-function invocation, and builtin dispatch is
//! timed and aggregated into a tree of [`ProfileNode`]s keyed by
//! [`ProfileKey`]. Re-reading an already-cached field does **not**
//! re-enter — only the first force is timed, which is what users want.
//!
//! The collector is std-only — no external dependencies. The data model
//! is plain enough that consumers can serialise it themselves (the
//! `wcl` CLI uses `serde_json`); this crate intentionally does not.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// A snapshot of a document's profile data.
#[derive(Debug, Clone)]
pub struct Profile {
    pub root: ProfileNode,
}

impl Profile {
    pub fn root(&self) -> &ProfileNode {
        &self.root
    }
}

/// One node in the call tree: a unique operation reached along a
/// specific stack from the root. Aggregated stats only — no per-call
/// history.
#[derive(Debug, Clone)]
pub struct ProfileNode {
    pub key: ProfileKey,
    pub count: u64,
    pub total: Duration,
    pub min: Duration,
    pub max: Duration,
    pub children: BTreeMap<ProfileKey, ProfileNode>,
}

impl ProfileNode {
    fn new(key: ProfileKey) -> Self {
        Self {
            key,
            count: 0,
            total: Duration::ZERO,
            min: Duration::MAX,
            max: Duration::ZERO,
            children: BTreeMap::new(),
        }
    }

    /// Average elapsed time per invocation. Zero when `count == 0`.
    pub fn mean(&self) -> Duration {
        if self.count == 0 {
            Duration::ZERO
        } else {
            self.total / (self.count as u32).max(1)
        }
    }

    fn record(&mut self, elapsed: Duration) {
        self.count += 1;
        self.total += elapsed;
        if elapsed < self.min {
            self.min = elapsed;
        }
        if elapsed > self.max {
            self.max = elapsed;
        }
    }
}

/// What kind of operation a [`ProfileNode`] represents. Used as the
/// child-map key, so equal-keyed sibling calls aggregate into one node.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProfileKey {
    Root,
    Field { path: String },
    UserFn { name: String },
    Builtin { name: String },
}

// ─── Collector (crate-private) ───────────────────────────────────────

#[derive(Debug)]
pub(crate) struct ProfileState {
    root: ProfileNode,
    stack: Vec<StackFrame>,
}

#[derive(Debug)]
struct StackFrame {
    key: ProfileKey,
    start: Instant,
}

impl ProfileState {
    pub(crate) fn new_root() -> Mutex<Self> {
        Mutex::new(Self {
            root: ProfileNode::new(ProfileKey::Root),
            stack: Vec::new(),
        })
    }

    pub(crate) fn enter(&mut self, key: ProfileKey) {
        self.stack.push(StackFrame {
            key,
            start: Instant::now(),
        });
    }

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
    state: Option<&'a Mutex<ProfileState>>,
}

impl<'a> ProfileGuard<'a> {
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

// ─── Tests ───────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn enter_exit_records_one_invocation() {
        let cell = ProfileState::new_root();
        {
            let mut s = cell.lock().unwrap();
            s.enter(ProfileKey::Field { path: "a".into() });
        }
        sleep(Duration::from_millis(1));
        {
            let mut s = cell.lock().unwrap();
            s.exit();
        }
        let snap = cell.lock().unwrap().snapshot();
        let child = snap
            .root
            .children
            .get(&ProfileKey::Field { path: "a".into() });
        let child = child.expect("one entry under root");
        assert_eq!(child.count, 1);
        assert!(
            child.total >= Duration::from_micros(900),
            "{:?}",
            child.total
        );
        assert_eq!(child.min, child.max);
    }

    #[test]
    fn nested_calls_form_tree() {
        let cell = ProfileState::new_root();
        {
            let mut s = cell.lock().unwrap();
            s.enter(ProfileKey::Field {
                path: "outer".into(),
            });
            s.enter(ProfileKey::Builtin { name: "map".into() });
            s.enter(ProfileKey::UserFn { name: "".into() });
            s.exit(); // userfn
            s.enter(ProfileKey::UserFn { name: "".into() });
            s.exit(); // userfn (aggregates with prior sibling)
            s.exit(); // map
            s.exit(); // outer
        }
        let snap = cell.lock().unwrap().snapshot();
        let outer = snap
            .root
            .children
            .get(&ProfileKey::Field {
                path: "outer".into(),
            })
            .unwrap();
        let map_node = outer
            .children
            .get(&ProfileKey::Builtin { name: "map".into() })
            .unwrap();
        let fn_node = map_node
            .children
            .get(&ProfileKey::UserFn { name: "".into() })
            .unwrap();
        assert_eq!(fn_node.count, 2);
        assert!(fn_node.total >= fn_node.max);
        assert!(fn_node.min <= fn_node.max);
    }

    #[test]
    fn mean_is_zero_when_no_calls() {
        let n = ProfileNode::new(ProfileKey::Root);
        assert_eq!(n.mean(), Duration::ZERO);
    }
}
