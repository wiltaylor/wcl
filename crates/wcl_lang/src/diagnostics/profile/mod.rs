//! Opt-in performance profiler for [`Document`](crate::Document) evaluation.
//!
//! When a document is opened via one of the `*_profiled` constructors,
//! every field force, user-function invocation, and builtin dispatch is
//! timed and aggregated into a tree of [`ProfileNode`]s keyed by
//! [`ProfileKey`]. Re-reading an already-cached field does **not**
//! re-enter — only the first force is timed, which is what users want.
//!
//! This file is the data model a host reads back;
//! [`collect`](self::collect) is the live collector that fills it in,
//! and is crate-private — a consumer never builds a profile, it only
//! receives one.
//!
//! The collector is std-only — no external dependencies. The data model
//! is plain enough that consumers can serialise it themselves (the
//! `wcl` CLI uses `serde_json`); this crate intentionally does not.

mod collect;
#[cfg(test)]
mod tests;

pub(crate) use collect::{ProfileGuard, ProfileState};

use std::collections::BTreeMap;
use std::time::Duration;

/// A snapshot of a document's profile data.
#[derive(Debug, Clone)]
pub struct Profile {
    /// The root of the recorded call tree.
    pub root: ProfileNode,
}

impl Profile {
    /// The root of the recorded call tree.
    pub fn root(&self) -> &ProfileNode {
        &self.root
    }
}

/// One node in the call tree: a unique operation reached along a
/// specific stack from the root. Aggregated stats only — no per-call
/// history.
#[derive(Debug, Clone)]
pub struct ProfileNode {
    /// What this node measures.
    pub key: ProfileKey,
    /// How many times it was entered.
    pub count: u64,
    /// Total time spent inside it.
    pub total: Duration,
    /// Fastest single entry.
    pub min: Duration,
    /// Slowest single entry.
    pub max: Duration,
    /// Nodes entered from within this one, keyed for stable output order.
    pub children: BTreeMap<ProfileKey, ProfileNode>,
}

impl ProfileNode {
    /// An empty node for `key`, with no samples yet.
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

    /// Fold one timing sample into this node's count, total and extremes.
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
    /// The synthetic root of the tree.
    Root,
    /// Forcing one document field, named by its dotted path.
    Field {
        /// Dotted path of the field being forced.
        path: String,
    },
    /// Calling a function the document declares.
    UserFn {
        /// Name of the function being called.
        name: String,
    },
    /// Calling a builtin.
    Builtin {
        /// Name of the builtin being called.
        name: String,
    },
}
