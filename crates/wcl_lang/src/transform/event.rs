//! Streaming event model for the transform engine.
//!
//! All formats (JSON, YAML, CSV, binary structs, text structs) are unified
//! into this event stream. Transform rules are format-agnostic.

use crate::eval::value::Value;

/// A streaming event produced by parsing or consumed by output.
#[derive(Debug, Clone)]
pub enum Event {
    /// Enter a map/object context. Key is the optional field name.
    EnterMap(Option<String>),
    /// Exit the current map/object context.
    ExitMap,
    /// Enter a sequence/array context. Key is the optional field name.
    EnterSeq(Option<String>),
    /// Exit the current sequence context.
    ExitSeq,
    /// A scalar key-value pair.
    Scalar(Option<String>, Value),
    /// End of input.
    Eof,
}

/// Maintains the current path through the event tree for rule matching.
#[derive(Debug, Default)]
pub struct PathTracker {
    stack: Vec<PathSegment>,
}

/// A segment of the current path.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PathSegment {
    /// Map/struct key
    Key(String),
    /// Sequence index
    Index(usize),
}

impl PathTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_key(&mut self, key: &str) {
        self.stack.push(PathSegment::Key(key.to_string()));
    }

    pub fn push_index(&mut self, idx: usize) {
        self.stack.push(PathSegment::Index(idx));
    }

    pub fn pop(&mut self) {
        self.stack.pop();
    }

    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    pub fn segments(&self) -> &[PathSegment] {
        &self.stack
    }

    /// Return the current path as a dotted string (e.g. "header.version").
    pub fn path_string(&self) -> String {
        self.stack
            .iter()
            .map(|seg| match seg {
                PathSegment::Key(k) => k.as_str().to_string(),
                PathSegment::Index(i) => format!("[{}]", i),
            })
            .collect::<Vec<_>>()
            .join(".")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_tracker_basic() {
        let mut pt = PathTracker::new();
        assert_eq!(pt.depth(), 0);
        assert_eq!(pt.path_string(), "");

        pt.push_key("users");
        pt.push_index(0);
        pt.push_key("name");
        assert_eq!(pt.depth(), 3);
        assert_eq!(pt.path_string(), "users.[0].name");

        pt.pop();
        assert_eq!(pt.path_string(), "users.[0]");
    }
}
