use crate::eval::value::{ScopeId, Value};
use crate::lang::Span;
use indexmap::IndexMap;
use std::collections::{HashMap, HashSet};

/// Entry in a scope
#[derive(Debug, Clone)]
pub struct ScopeEntry {
    pub name: String,
    pub kind: ScopeEntryKind,
    pub value: Option<Value>,
    pub span: Span,
    /// Names this entry depends on
    pub dependencies: HashSet<String>,
    /// Has been evaluated
    pub evaluated: bool,
    /// Number of times this entry has been read/referenced
    pub read_count: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeEntryKind {
    LetBinding,
    ExportLet,
    Attribute,
    BlockChild,
    TableEntry,
    IteratorVar,
}

/// Scope kind
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopeKind {
    Module,
    Block,
    Macro,
    ForLoop,
    Lambda,
}

/// A single scope
#[derive(Debug)]
pub struct Scope {
    pub id: ScopeId,
    pub kind: ScopeKind,
    pub parent: Option<ScopeId>,
    pub entries: IndexMap<String, ScopeEntry>,
    pub children: Vec<ScopeId>,
}

/// Arena-based scope storage
#[derive(Debug)]
pub struct ScopeArena {
    scopes: Vec<Scope>,
    next_id: u32,
}

impl ScopeArena {
    pub fn new() -> Self {
        ScopeArena {
            scopes: Vec::new(),
            next_id: 0,
        }
    }

    pub fn create_scope(&mut self, kind: ScopeKind, parent: Option<ScopeId>) -> ScopeId {
        let id = ScopeId(self.next_id);
        self.next_id += 1;
        let scope = Scope {
            id,
            kind,
            parent,
            entries: IndexMap::new(),
            children: Vec::new(),
        };
        self.scopes.push(scope);
        if let Some(parent_id) = parent {
            self.scopes[parent_id.0 as usize].children.push(id);
        }
        id
    }

    pub fn get(&self, id: ScopeId) -> &Scope {
        &self.scopes[id.0 as usize]
    }

    pub fn get_mut(&mut self, id: ScopeId) -> &mut Scope {
        &mut self.scopes[id.0 as usize]
    }

    /// Add an entry to a scope
    pub fn add_entry(&mut self, scope_id: ScopeId, entry: ScopeEntry) {
        let scope = self.get_mut(scope_id);
        scope.entries.insert(entry.name.clone(), entry);
    }

    /// Resolve a name by walking the scope chain
    pub fn resolve(&self, scope_id: ScopeId, name: &str) -> Option<(ScopeId, &ScopeEntry)> {
        let scope = self.get(scope_id);
        if let Some(entry) = scope.entries.get(name) {
            return Some((scope_id, entry));
        }
        if let Some(parent) = scope.parent {
            self.resolve(parent, name)
        } else {
            None
        }
    }

    /// Resolve mutably
    pub fn resolve_mut(
        &mut self,
        scope_id: ScopeId,
        name: &str,
    ) -> Option<(ScopeId, &mut ScopeEntry)> {
        let has_entry = self.get(scope_id).entries.contains_key(name);
        if has_entry {
            let scope = self.get_mut(scope_id);
            let entry = scope.entries.get_mut(name).unwrap();
            return Some((scope_id, entry));
        }
        let parent = self.get(scope_id).parent;
        if let Some(parent) = parent {
            self.resolve_mut(parent, name)
        } else {
            None
        }
    }

    /// Record a read of a named entry in the given scope (or ancestor).
    /// Increments the `read_count` on the entry that would be found by `resolve`.
    pub fn record_read(&mut self, scope_id: ScopeId, name: &str) {
        if let Some((found_scope, _)) = self.resolve(scope_id, name) {
            let scope = self.get_mut(found_scope);
            if let Some(entry) = scope.entries.get_mut(name) {
                entry.read_count += 1;
            }
        }
    }

    /// Check whether adding `name` in `scope_id` would shadow a binding in
    /// an ancestor scope. Returns the span of the shadowed entry if found.
    pub fn check_shadowing(&self, scope_id: ScopeId, name: &str) -> Option<Span> {
        let parent = self.get(scope_id).parent?;
        if let Some((_, entry)) = self.resolve(parent, name) {
            Some(entry.span)
        } else {
            None
        }
    }

    /// Get all scopes for iteration.
    pub fn all_scopes(&self) -> &[Scope] {
        &self.scopes
    }

    /// Iterate over all scopes and their entries.
    pub fn all_entries(&self) -> impl Iterator<Item = (ScopeId, &ScopeEntry)> {
        self.scopes
            .iter()
            .flat_map(|scope| scope.entries.values().map(move |entry| (scope.id, entry)))
    }

    /// Topological sort of entries in a scope based on dependencies.
    /// Returns ordered names, or `Err` containing the names involved in a cycle.
    pub fn topo_sort(&self, scope_id: ScopeId) -> Result<Vec<String>, Vec<String>> {
        let scope = self.get(scope_id);
        let names: Vec<String> = scope.entries.keys().cloned().collect();

        // in_degree[n] = number of same-scope deps n still waits on
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        // dependents[d] = list of names that depend on d
        let mut dependents: HashMap<String, Vec<String>> = HashMap::new();

        for name in &names {
            in_degree.insert(name.clone(), 0);
        }

        for (name, entry) in &scope.entries {
            for dep in &entry.dependencies {
                // Only count deps that live in this scope
                if scope.entries.contains_key(dep.as_str()) {
                    *in_degree.get_mut(name).unwrap() += 1;
                    dependents
                        .entry(dep.clone())
                        .or_default()
                        .push(name.clone());
                }
            }
        }

        // Start with all zero-in-degree nodes
        let mut queue: Vec<String> = names
            .iter()
            .filter(|n| in_degree[*n] == 0)
            .cloned()
            .collect();

        let mut result = Vec::new();

        while let Some(name) = queue.pop() {
            result.push(name.clone());
            if let Some(deps) = dependents.get(&name) {
                for dep in deps {
                    let degree = in_degree.get_mut(dep).unwrap();
                    *degree -= 1;
                    if *degree == 0 {
                        queue.push(dep.clone());
                    }
                }
            }
        }

        if result.len() != names.len() {
            // Cycle detected — collect the culprits
            let in_cycle: Vec<String> = names.into_iter().filter(|n| in_degree[n] > 0).collect();
            Err(in_cycle)
        } else {
            Ok(result)
        }
    }
}

impl Default for ScopeArena {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::span::{FileId, Span};

    fn dummy_span() -> Span {
        Span::new(FileId(0), 0, 0)
    }

    fn make_entry(name: &str, kind: ScopeEntryKind, deps: &[&str]) -> ScopeEntry {
        ScopeEntry {
            name: name.into(),
            kind,
            value: None,
            span: dummy_span(),
            dependencies: deps.iter().map(|s| s.to_string()).collect(),
            evaluated: false,
            read_count: 0,
        }
    }

    // ── create_scope ──────────────────────────────────────────────────────────

    #[test]
    fn create_scope_assigns_sequential_ids() {
        let mut arena = ScopeArena::new();
        let a = arena.create_scope(ScopeKind::Module, None);
        let b = arena.create_scope(ScopeKind::Block, Some(a));
        let c = arena.create_scope(ScopeKind::Block, Some(a));

        assert_eq!(a, ScopeId(0));
        assert_eq!(b, ScopeId(1));
        assert_eq!(c, ScopeId(2));
    }

    #[test]
    fn create_scope_parent_tracks_children() {
        let mut arena = ScopeArena::new();
        let parent = arena.create_scope(ScopeKind::Module, None);
        let child = arena.create_scope(ScopeKind::Block, Some(parent));

        assert!(arena.get(parent).children.contains(&child));
        assert_eq!(arena.get(child).parent, Some(parent));
    }

    #[test]
    fn create_scope_root_has_no_parent() {
        let mut arena = ScopeArena::new();
        let root = arena.create_scope(ScopeKind::Module, None);
        assert_eq!(arena.get(root).parent, None);
    }

    // ── add_entry ─────────────────────────────────────────────────────────────

    #[test]
    fn add_entry_visible_in_scope() {
        let mut arena = ScopeArena::new();
        let s = arena.create_scope(ScopeKind::Module, None);
        arena.add_entry(s, make_entry("x", ScopeEntryKind::LetBinding, &[]));
        assert!(arena.get(s).entries.contains_key("x"));
    }

    #[test]
    fn add_entry_overwrites_same_name() {
        let mut arena = ScopeArena::new();
        let s = arena.create_scope(ScopeKind::Module, None);
        arena.add_entry(s, make_entry("x", ScopeEntryKind::LetBinding, &[]));
        arena.add_entry(s, make_entry("x", ScopeEntryKind::Attribute, &[]));
        assert_eq!(arena.get(s).entries["x"].kind, ScopeEntryKind::Attribute);
    }

    // ── resolve ───────────────────────────────────────────────────────────────

    #[test]
    fn resolve_finds_in_current_scope() {
        let mut arena = ScopeArena::new();
        let s = arena.create_scope(ScopeKind::Module, None);
        arena.add_entry(s, make_entry("port", ScopeEntryKind::Attribute, &[]));

        let result = arena.resolve(s, "port");
        assert!(result.is_some());
        let (found_id, entry) = result.unwrap();
        assert_eq!(found_id, s);
        assert_eq!(entry.name, "port");
    }

    #[test]
    fn resolve_walks_to_parent() {
        let mut arena = ScopeArena::new();
        let parent = arena.create_scope(ScopeKind::Module, None);
        let child = arena.create_scope(ScopeKind::Block, Some(parent));

        arena.add_entry(
            parent,
            make_entry("base_port", ScopeEntryKind::LetBinding, &[]),
        );

        let result = arena.resolve(child, "base_port");
        assert!(result.is_some());
        let (found_id, _) = result.unwrap();
        assert_eq!(found_id, parent);
    }

    #[test]
    fn resolve_returns_none_for_missing_name() {
        let mut arena = ScopeArena::new();
        let s = arena.create_scope(ScopeKind::Module, None);
        assert!(arena.resolve(s, "nonexistent").is_none());
    }

    #[test]
    fn resolve_child_shadows_parent() {
        let mut arena = ScopeArena::new();
        let parent = arena.create_scope(ScopeKind::Module, None);
        let child = arena.create_scope(ScopeKind::Block, Some(parent));

        arena.add_entry(parent, make_entry("port", ScopeEntryKind::LetBinding, &[]));
        arena.add_entry(child, make_entry("port", ScopeEntryKind::Attribute, &[]));

        let (found_id, entry) = arena.resolve(child, "port").unwrap();
        assert_eq!(found_id, child);
        assert_eq!(entry.kind, ScopeEntryKind::Attribute);
    }

    // ── topo_sort ─────────────────────────────────────────────────────────────

    #[test]
    fn topo_sort_no_deps() {
        let mut arena = ScopeArena::new();
        let s = arena.create_scope(ScopeKind::Block, None);
        arena.add_entry(s, make_entry("a", ScopeEntryKind::Attribute, &[]));
        arena.add_entry(s, make_entry("b", ScopeEntryKind::Attribute, &[]));
        arena.add_entry(s, make_entry("c", ScopeEntryKind::Attribute, &[]));

        let order = arena.topo_sort(s).expect("no cycle expected");
        // All three names appear exactly once
        assert_eq!(order.len(), 3);
        let mut sorted = order.clone();
        sorted.sort();
        assert_eq!(sorted, vec!["a", "b", "c"]);
    }

    #[test]
    fn topo_sort_simple_chain() {
        // a <- b <- c  (c depends on b, b depends on a)
        // Expected order: a, b, c  (a before b before c)
        let mut arena = ScopeArena::new();
        let s = arena.create_scope(ScopeKind::Block, None);
        arena.add_entry(s, make_entry("c", ScopeEntryKind::Attribute, &["b"]));
        arena.add_entry(s, make_entry("b", ScopeEntryKind::Attribute, &["a"]));
        arena.add_entry(s, make_entry("a", ScopeEntryKind::Attribute, &[]));

        let order = arena.topo_sort(s).expect("no cycle expected");
        assert_eq!(order.len(), 3);
        let pos = |name: &str| order.iter().position(|n| n == name).unwrap();
        assert!(pos("a") < pos("b"), "a must come before b");
        assert!(pos("b") < pos("c"), "b must come before c");
    }

    #[test]
    fn topo_sort_diamond_deps() {
        // a <- b, a <- c, b <- d, c <- d
        let mut arena = ScopeArena::new();
        let s = arena.create_scope(ScopeKind::Block, None);
        arena.add_entry(s, make_entry("a", ScopeEntryKind::Attribute, &[]));
        arena.add_entry(s, make_entry("b", ScopeEntryKind::Attribute, &["a"]));
        arena.add_entry(s, make_entry("c", ScopeEntryKind::Attribute, &["a"]));
        arena.add_entry(s, make_entry("d", ScopeEntryKind::Attribute, &["b", "c"]));

        let order = arena.topo_sort(s).expect("no cycle expected");
        assert_eq!(order.len(), 4);
        let pos = |name: &str| order.iter().position(|n| n == name).unwrap();
        assert!(pos("a") < pos("b"));
        assert!(pos("a") < pos("c"));
        assert!(pos("b") < pos("d"));
        assert!(pos("c") < pos("d"));
    }

    #[test]
    fn topo_sort_cycle_detection() {
        // a depends on b, b depends on a — direct cycle
        let mut arena = ScopeArena::new();
        let s = arena.create_scope(ScopeKind::Block, None);
        arena.add_entry(s, make_entry("a", ScopeEntryKind::Attribute, &["b"]));
        arena.add_entry(s, make_entry("b", ScopeEntryKind::Attribute, &["a"]));

        let result = arena.topo_sort(s);
        assert!(result.is_err(), "cycle should be detected");
        let culprits = result.unwrap_err();
        assert!(culprits.contains(&"a".to_string()));
        assert!(culprits.contains(&"b".to_string()));
    }

    #[test]
    fn topo_sort_cycle_with_independent_node() {
        // a depends on b, b depends on a; c is independent
        let mut arena = ScopeArena::new();
        let s = arena.create_scope(ScopeKind::Block, None);
        arena.add_entry(s, make_entry("a", ScopeEntryKind::Attribute, &["b"]));
        arena.add_entry(s, make_entry("b", ScopeEntryKind::Attribute, &["a"]));
        arena.add_entry(s, make_entry("c", ScopeEntryKind::Attribute, &[]));

        let result = arena.topo_sort(s);
        assert!(result.is_err());
        let culprits = result.unwrap_err();
        // c should NOT be reported as part of the cycle
        assert!(!culprits.contains(&"c".to_string()));
    }

    #[test]
    fn topo_sort_ignores_out_of_scope_deps() {
        // "a" depends on "external" which does not exist in this scope
        // This should not count as a real dependency and should not cause issues
        let mut arena = ScopeArena::new();
        let s = arena.create_scope(ScopeKind::Block, None);
        arena.add_entry(s, make_entry("a", ScopeEntryKind::Attribute, &["external"]));
        arena.add_entry(s, make_entry("b", ScopeEntryKind::Attribute, &["a"]));

        let order = arena
            .topo_sort(s)
            .expect("out-of-scope dep should be ignored");
        assert_eq!(order.len(), 2);
        let pos = |name: &str| order.iter().position(|n| n == name).unwrap();
        assert!(pos("a") < pos("b"));
    }

    // ── resolve_mut ───────────────────────────────────────────────────────────

    #[test]
    fn resolve_mut_sets_value() {
        let mut arena = ScopeArena::new();
        let s = arena.create_scope(ScopeKind::Module, None);
        arena.add_entry(s, make_entry("x", ScopeEntryKind::LetBinding, &[]));

        let (_, entry) = arena.resolve_mut(s, "x").unwrap();
        entry.value = Some(Value::Int(99));
        entry.evaluated = true;

        let resolved = arena.resolve(s, "x").unwrap().1;
        assert_eq!(resolved.value, Some(Value::Int(99)));
        assert!(resolved.evaluated);
    }

    #[test]
    fn resolve_mut_walks_to_parent() {
        let mut arena = ScopeArena::new();
        let parent = arena.create_scope(ScopeKind::Module, None);
        let child = arena.create_scope(ScopeKind::Block, Some(parent));

        arena.add_entry(parent, make_entry("cfg", ScopeEntryKind::LetBinding, &[]));

        let result = arena.resolve_mut(child, "cfg");
        assert!(result.is_some());
        let (found_id, _) = result.unwrap();
        assert_eq!(found_id, parent);
    }

    // ── record_read ─────────────────────────────────────────────────

    #[test]
    fn record_read_increments_count() {
        let mut arena = ScopeArena::new();
        let s = arena.create_scope(ScopeKind::Module, None);
        arena.add_entry(s, make_entry("x", ScopeEntryKind::LetBinding, &[]));
        assert_eq!(arena.get(s).entries["x"].read_count, 0);

        arena.record_read(s, "x");
        assert_eq!(arena.get(s).entries["x"].read_count, 1);

        arena.record_read(s, "x");
        assert_eq!(arena.get(s).entries["x"].read_count, 2);
    }

    #[test]
    fn record_read_walks_to_parent() {
        let mut arena = ScopeArena::new();
        let parent = arena.create_scope(ScopeKind::Module, None);
        let child = arena.create_scope(ScopeKind::Block, Some(parent));
        arena.add_entry(parent, make_entry("x", ScopeEntryKind::LetBinding, &[]));

        arena.record_read(child, "x");
        assert_eq!(arena.get(parent).entries["x"].read_count, 1);
    }

    // ── check_shadowing ─────────────────────────────────────────────

    #[test]
    fn check_shadowing_detects_parent_binding() {
        let mut arena = ScopeArena::new();
        let parent = arena.create_scope(ScopeKind::Module, None);
        let child = arena.create_scope(ScopeKind::Block, Some(parent));
        arena.add_entry(parent, make_entry("x", ScopeEntryKind::LetBinding, &[]));

        let result = arena.check_shadowing(child, "x");
        assert!(result.is_some());
    }

    #[test]
    fn check_shadowing_returns_none_when_no_parent_binding() {
        let mut arena = ScopeArena::new();
        let parent = arena.create_scope(ScopeKind::Module, None);
        let child = arena.create_scope(ScopeKind::Block, Some(parent));

        let result = arena.check_shadowing(child, "x");
        assert!(result.is_none());
    }

    #[test]
    fn check_shadowing_returns_none_for_root_scope() {
        let mut arena = ScopeArena::new();
        let root = arena.create_scope(ScopeKind::Module, None);

        let result = arena.check_shadowing(root, "x");
        assert!(result.is_none());
    }
}
