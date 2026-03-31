//! Accumulator system for stream-to-structured aggregation.
//!
//! Accumulators collect data from stream records into structured output fields
//! using operators like sum, min, max, count, collect, etc.

use crate::eval::value::Value;
use indexmap::IndexMap;

/// Overflow policy for bounded collection accumulators.
#[derive(Debug, Clone, Copy)]
pub enum OverflowPolicy {
    DropOldest,
    DropNewest,
    Error,
}

/// An accumulator operator with its current state.
#[derive(Debug, Clone)]
pub enum AccumulatorState {
    Sum(f64),
    Min(Option<Value>),
    Max(Option<Value>),
    Count(i64),
    First(Option<Value>),
    Last(Option<Value>),
    Collect {
        values: Vec<Value>,
        max_size: Option<usize>,
        overflow: OverflowPolicy,
    },
    CollectUnique {
        values: Vec<Value>,
        max_size: Option<usize>,
        overflow: OverflowPolicy,
    },
}

impl AccumulatorState {
    pub fn sum() -> Self {
        Self::Sum(0.0)
    }
    pub fn min() -> Self {
        Self::Min(None)
    }
    pub fn max() -> Self {
        Self::Max(None)
    }
    pub fn count() -> Self {
        Self::Count(0)
    }
    pub fn first() -> Self {
        Self::First(None)
    }
    pub fn last() -> Self {
        Self::Last(None)
    }
    pub fn collect(max_size: Option<usize>, overflow: OverflowPolicy) -> Self {
        Self::Collect {
            values: Vec::new(),
            max_size,
            overflow,
        }
    }
    pub fn collect_unique(max_size: Option<usize>, overflow: OverflowPolicy) -> Self {
        Self::CollectUnique {
            values: Vec::new(),
            max_size,
            overflow,
        }
    }

    /// Feed a value into the accumulator.
    pub fn feed(&mut self, value: Value) -> Result<(), String> {
        match self {
            AccumulatorState::Sum(total) => {
                let n = value_to_f64(&value)?;
                *total += n;
                Ok(())
            }
            AccumulatorState::Min(current) => {
                if current.is_none() || value_lt(&value, current.as_ref().unwrap()) {
                    *current = Some(value);
                }
                Ok(())
            }
            AccumulatorState::Max(current) => {
                if current.is_none() || value_gt(&value, current.as_ref().unwrap()) {
                    *current = Some(value);
                }
                Ok(())
            }
            AccumulatorState::Count(n) => {
                *n += 1;
                Ok(())
            }
            AccumulatorState::First(current) => {
                if current.is_none() {
                    *current = Some(value);
                }
                Ok(())
            }
            AccumulatorState::Last(current) => {
                *current = Some(value);
                Ok(())
            }
            AccumulatorState::Collect {
                values,
                max_size,
                overflow,
            } => {
                if let Some(max) = max_size {
                    if values.len() >= *max {
                        match overflow {
                            OverflowPolicy::DropOldest => {
                                values.remove(0);
                            }
                            OverflowPolicy::DropNewest => return Ok(()),
                            OverflowPolicy::Error => {
                                return Err(format!("accumulator overflow: max {} items", max));
                            }
                        }
                    }
                }
                values.push(value);
                Ok(())
            }
            AccumulatorState::CollectUnique {
                values,
                max_size,
                overflow,
            } => {
                if values.contains(&value) {
                    return Ok(());
                }
                if let Some(max) = max_size {
                    if values.len() >= *max {
                        match overflow {
                            OverflowPolicy::DropOldest => {
                                values.remove(0);
                            }
                            OverflowPolicy::DropNewest => return Ok(()),
                            OverflowPolicy::Error => {
                                return Err(format!("accumulator overflow: max {} items", max));
                            }
                        }
                    }
                }
                values.push(value);
                Ok(())
            }
        }
    }

    /// Finalize the accumulator and return its result value.
    pub fn finalize(self) -> Value {
        match self {
            AccumulatorState::Sum(total) => Value::Float(total),
            AccumulatorState::Min(v) => v.unwrap_or(Value::Null),
            AccumulatorState::Max(v) => v.unwrap_or(Value::Null),
            AccumulatorState::Count(n) => Value::Int(n),
            AccumulatorState::First(v) => v.unwrap_or(Value::Null),
            AccumulatorState::Last(v) => v.unwrap_or(Value::Null),
            AccumulatorState::Collect { values, .. } => Value::List(values),
            AccumulatorState::CollectUnique { values, .. } => Value::List(values),
        }
    }
}

/// A handle for managing multiple named accumulators.
pub struct AccumulatorHandle {
    fields: IndexMap<String, AccumulatorState>,
}

impl AccumulatorHandle {
    pub fn new() -> Self {
        Self {
            fields: IndexMap::new(),
        }
    }

    /// Register a named accumulator field.
    pub fn register(&mut self, name: String, state: AccumulatorState) {
        self.fields.insert(name, state);
    }

    /// Feed a value to a named accumulator.
    pub fn feed(&mut self, name: &str, value: Value) -> Result<(), String> {
        let state = self
            .fields
            .get_mut(name)
            .ok_or_else(|| format!("unknown accumulator field: {}", name))?;
        state.feed(value)
    }

    /// Finalize all accumulators and return results as a map.
    pub fn finalize(self) -> Value {
        let mut map = IndexMap::new();
        for (name, state) in self.fields {
            map.insert(name, state.finalize());
        }
        Value::Map(map)
    }
}

impl Default for AccumulatorHandle {
    fn default() -> Self {
        Self::new()
    }
}

fn value_to_f64(v: &Value) -> Result<f64, String> {
    match v {
        Value::Int(i) => Ok(*i as f64),
        Value::Float(f) => Ok(*f),
        Value::BigInt(i) => Ok(*i as f64),
        _ => Err(format!("cannot convert {} to number", v.type_name())),
    }
}

fn value_lt(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(a), Value::Int(b)) => a < b,
        (Value::Float(a), Value::Float(b)) => a < b,
        (Value::Int(a), Value::Float(b)) => (*a as f64) < *b,
        (Value::Float(a), Value::Int(b)) => *a < (*b as f64),
        (Value::String(a), Value::String(b)) => a < b,
        _ => false,
    }
}

fn value_gt(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Int(a), Value::Int(b)) => a > b,
        (Value::Float(a), Value::Float(b)) => a > b,
        (Value::Int(a), Value::Float(b)) => (*a as f64) > *b,
        (Value::Float(a), Value::Int(b)) => *a > (*b as f64),
        (Value::String(a), Value::String(b)) => a > b,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sum_accumulator() {
        let mut acc = AccumulatorState::sum();
        acc.feed(Value::Int(10)).unwrap();
        acc.feed(Value::Int(20)).unwrap();
        acc.feed(Value::Float(5.5)).unwrap();
        assert_eq!(acc.finalize(), Value::Float(35.5));
    }

    #[test]
    fn count_accumulator() {
        let mut acc = AccumulatorState::count();
        acc.feed(Value::String("a".into())).unwrap();
        acc.feed(Value::String("b".into())).unwrap();
        acc.feed(Value::String("c".into())).unwrap();
        assert_eq!(acc.finalize(), Value::Int(3));
    }

    #[test]
    fn min_max_accumulator() {
        let mut min_acc = AccumulatorState::min();
        let mut max_acc = AccumulatorState::max();

        for v in [5, 3, 8, 1, 7] {
            min_acc.feed(Value::Int(v)).unwrap();
            max_acc.feed(Value::Int(v)).unwrap();
        }

        assert_eq!(min_acc.finalize(), Value::Int(1));
        assert_eq!(max_acc.finalize(), Value::Int(8));
    }

    #[test]
    fn first_last_accumulator() {
        let mut first = AccumulatorState::first();
        let mut last = AccumulatorState::last();

        for s in ["Alice", "Bob", "Carol"] {
            first.feed(Value::String(s.into())).unwrap();
            last.feed(Value::String(s.into())).unwrap();
        }

        assert_eq!(first.finalize(), Value::String("Alice".into()));
        assert_eq!(last.finalize(), Value::String("Carol".into()));
    }

    #[test]
    fn collect_accumulator() {
        let mut acc = AccumulatorState::collect(None, OverflowPolicy::DropOldest);
        acc.feed(Value::Int(1)).unwrap();
        acc.feed(Value::Int(2)).unwrap();
        acc.feed(Value::Int(3)).unwrap();
        assert_eq!(
            acc.finalize(),
            Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
        );
    }

    #[test]
    fn collect_unique_accumulator() {
        let mut acc = AccumulatorState::collect_unique(None, OverflowPolicy::DropOldest);
        acc.feed(Value::String("a".into())).unwrap();
        acc.feed(Value::String("b".into())).unwrap();
        acc.feed(Value::String("a".into())).unwrap(); // duplicate
        acc.feed(Value::String("c".into())).unwrap();
        assert_eq!(
            acc.finalize(),
            Value::List(vec![
                Value::String("a".into()),
                Value::String("b".into()),
                Value::String("c".into()),
            ])
        );
    }

    #[test]
    fn bounded_collect_drops_oldest() {
        let mut acc = AccumulatorState::collect(Some(3), OverflowPolicy::DropOldest);
        for i in 1..=5 {
            acc.feed(Value::Int(i)).unwrap();
        }
        assert_eq!(
            acc.finalize(),
            Value::List(vec![Value::Int(3), Value::Int(4), Value::Int(5)])
        );
    }

    #[test]
    fn accumulator_handle() {
        let mut handle = AccumulatorHandle::new();
        handle.register("total".into(), AccumulatorState::sum());
        handle.register("count".into(), AccumulatorState::count());

        handle.feed("total", Value::Int(10)).unwrap();
        handle.feed("total", Value::Int(20)).unwrap();
        handle.feed("count", Value::Null).unwrap();
        handle.feed("count", Value::Null).unwrap();

        let result = handle.finalize();
        if let Value::Map(m) = result {
            assert_eq!(m.get("total"), Some(&Value::Float(30.0)));
            assert_eq!(m.get("count"), Some(&Value::Int(2)));
        } else {
            panic!("expected Map");
        }
    }
}
