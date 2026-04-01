//! In-memory state backend using HashMap.

use super::{StateBackend, StateError};
use crate::eval::value::Value;
use std::collections::HashMap;

/// In-memory HashMap-based state backend.
pub struct MemoryBackend {
    data: HashMap<String, Value>,
}

impl MemoryBackend {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }
}

impl Default for MemoryBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl StateBackend for MemoryBackend {
    fn get(&self, key: &str) -> Result<Option<Value>, StateError> {
        Ok(self.data.get(key).cloned())
    }

    fn set(&mut self, key: &str, value: Value) -> Result<Option<Value>, StateError> {
        Ok(self.data.insert(key.to_string(), value))
    }

    fn delete(&mut self, key: &str) -> Result<Option<Value>, StateError> {
        Ok(self.data.remove(key))
    }

    fn has(&self, key: &str) -> Result<bool, StateError> {
        Ok(self.data.contains_key(key))
    }

    fn keys(&self) -> Result<Vec<String>, StateError> {
        Ok(self.data.keys().cloned().collect())
    }

    fn clear(&mut self) -> Result<(), StateError> {
        self.data.clear();
        Ok(())
    }

    fn len(&self) -> usize {
        self.data.len()
    }
}
