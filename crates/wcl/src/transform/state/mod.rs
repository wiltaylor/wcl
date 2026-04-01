//! State management for stateful transforms.
//!
//! Provides bounded, key-value state that persists across stream records
//! within a single transform execution.

mod memory;

use crate::eval::value::Value;
use std::collections::VecDeque;

/// Errors from state operations.
#[derive(Debug, Clone)]
pub enum StateError {
    BackendFailure(String),
    CapacityExceeded { max_keys: usize, key: String },
}

impl std::fmt::Display for StateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StateError::BackendFailure(msg) => write!(f, "state backend error: {}", msg),
            StateError::CapacityExceeded { max_keys, key } => {
                write!(
                    f,
                    "state capacity exceeded (max {} keys) when inserting '{}'",
                    max_keys, key
                )
            }
        }
    }
}

/// Eviction policy when a bounded backend reaches capacity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvictionPolicy {
    /// Remove least recently used key.
    Lru,
    /// Remove oldest inserted key.
    Fifo,
    /// Reject new insertions.
    RejectNew,
}

/// Configuration for a state block.
#[derive(Debug, Clone)]
pub struct StateConfig {
    pub max_keys: usize,
    pub eviction: EvictionPolicy,
    pub name: Option<String>,
}

impl Default for StateConfig {
    fn default() -> Self {
        Self {
            max_keys: 10_000,
            eviction: EvictionPolicy::Lru,
            name: None,
        }
    }
}

/// The storage backend trait.
pub trait StateBackend: Send {
    fn get(&self, key: &str) -> Result<Option<Value>, StateError>;
    fn set(&mut self, key: &str, value: Value) -> Result<Option<Value>, StateError>;
    fn delete(&mut self, key: &str) -> Result<Option<Value>, StateError>;
    fn has(&self, key: &str) -> Result<bool, StateError>;
    fn keys(&self) -> Result<Vec<String>, StateError>;
    fn clear(&mut self) -> Result<(), StateError>;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Bounded wrapper — wraps any StateBackend and enforces max_keys + eviction.
pub struct BoundedBackend {
    inner: Box<dyn StateBackend>,
    max_keys: usize,
    eviction: EvictionPolicy,
    access_order: VecDeque<String>,
}

impl BoundedBackend {
    pub fn new(inner: Box<dyn StateBackend>, config: &StateConfig) -> Self {
        Self {
            inner,
            max_keys: config.max_keys,
            eviction: config.eviction,
            access_order: VecDeque::new(),
        }
    }
}

impl StateBackend for BoundedBackend {
    fn get(&self, key: &str) -> Result<Option<Value>, StateError> {
        self.inner.get(key)
    }

    fn set(&mut self, key: &str, value: Value) -> Result<Option<Value>, StateError> {
        // Check capacity
        if !self.inner.has(key)? && self.inner.len() >= self.max_keys {
            match self.eviction {
                EvictionPolicy::RejectNew => {
                    return Err(StateError::CapacityExceeded {
                        max_keys: self.max_keys,
                        key: key.to_string(),
                    });
                }
                EvictionPolicy::Lru | EvictionPolicy::Fifo => {
                    // Remove the oldest/least-recently-used key
                    if let Some(evict_key) = self.access_order.pop_front() {
                        self.inner.delete(&evict_key)?;
                    }
                }
            }
        }

        // Track access order
        self.access_order.retain(|k| k != key);
        self.access_order.push_back(key.to_string());

        self.inner.set(key, value)
    }

    fn delete(&mut self, key: &str) -> Result<Option<Value>, StateError> {
        self.access_order.retain(|k| k != key);
        self.inner.delete(key)
    }

    fn has(&self, key: &str) -> Result<bool, StateError> {
        self.inner.has(key)
    }

    fn keys(&self) -> Result<Vec<String>, StateError> {
        self.inner.keys()
    }

    fn clear(&mut self) -> Result<(), StateError> {
        self.access_order.clear();
        self.inner.clear()
    }

    fn len(&self) -> usize {
        self.inner.len()
    }
}

/// Manages state backends per scope × group key.
pub struct StateManager {
    configs: indexmap::IndexMap<String, StateConfig>,
    backends: std::collections::HashMap<(String, Option<String>), Box<dyn StateBackend>>,
}

impl StateManager {
    pub fn new() -> Self {
        Self {
            configs: indexmap::IndexMap::new(),
            backends: std::collections::HashMap::new(),
        }
    }

    /// Register a named state configuration.
    pub fn register_config(&mut self, name: String, config: StateConfig) {
        self.configs.insert(name, config);
    }

    /// Get or create a backend for the given scope and optional group key.
    pub fn backend_for(&mut self, scope: &str, group_key: Option<&str>) -> &mut dyn StateBackend {
        let key = (scope.to_string(), group_key.map(|s| s.to_string()));
        if !self.backends.contains_key(&key) {
            let config = self.configs.get(scope).cloned().unwrap_or_default();
            let inner = Box::new(memory::MemoryBackend::new());
            let bounded = BoundedBackend::new(inner, &config);
            self.backends.insert(key.clone(), Box::new(bounded));
        }
        self.backends.get_mut(&key).unwrap().as_mut()
    }
}

impl Default for StateManager {
    fn default() -> Self {
        Self::new()
    }
}

pub use memory::MemoryBackend;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_backend_basic() {
        let mut backend = MemoryBackend::new();
        assert_eq!(backend.len(), 0);
        assert!(backend.is_empty());

        backend.set("key1", Value::Int(42)).unwrap();
        assert_eq!(backend.get("key1").unwrap(), Some(Value::Int(42)));
        assert_eq!(backend.len(), 1);
        assert!(backend.has("key1").unwrap());
        assert!(!backend.has("key2").unwrap());

        backend.delete("key1").unwrap();
        assert_eq!(backend.len(), 0);
    }

    #[test]
    fn bounded_backend_eviction() {
        let config = StateConfig {
            max_keys: 3,
            eviction: EvictionPolicy::Fifo,
            name: None,
        };
        let inner = Box::new(MemoryBackend::new());
        let mut bounded = BoundedBackend::new(inner, &config);

        bounded.set("a", Value::Int(1)).unwrap();
        bounded.set("b", Value::Int(2)).unwrap();
        bounded.set("c", Value::Int(3)).unwrap();
        assert_eq!(bounded.len(), 3);

        // Adding a 4th key should evict "a"
        bounded.set("d", Value::Int(4)).unwrap();
        assert_eq!(bounded.len(), 3);
        assert!(!bounded.has("a").unwrap());
        assert!(bounded.has("d").unwrap());
    }

    #[test]
    fn bounded_backend_reject_new() {
        let config = StateConfig {
            max_keys: 2,
            eviction: EvictionPolicy::RejectNew,
            name: None,
        };
        let inner = Box::new(MemoryBackend::new());
        let mut bounded = BoundedBackend::new(inner, &config);

        bounded.set("a", Value::Int(1)).unwrap();
        bounded.set("b", Value::Int(2)).unwrap();

        let result = bounded.set("c", Value::Int(3));
        assert!(matches!(result, Err(StateError::CapacityExceeded { .. })));
        assert_eq!(bounded.len(), 2);
    }

    #[test]
    fn state_manager_per_group() {
        let mut mgr = StateManager::new();
        mgr.register_config("default".into(), StateConfig::default());

        // Set values in different groups
        mgr.backend_for("default", Some("group-a"))
            .set("key", Value::Int(1))
            .unwrap();
        mgr.backend_for("default", Some("group-b"))
            .set("key", Value::Int(2))
            .unwrap();

        // Each group has independent state
        let a = mgr
            .backend_for("default", Some("group-a"))
            .get("key")
            .unwrap();
        let b = mgr
            .backend_for("default", Some("group-b"))
            .get("key")
            .unwrap();
        assert_eq!(a, Some(Value::Int(1)));
        assert_eq!(b, Some(Value::Int(2)));
    }
}
