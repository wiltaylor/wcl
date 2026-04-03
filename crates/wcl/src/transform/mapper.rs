//! Transform mapper — evaluates field mappings per record.
//!
//! Given an input record (Value::Map) and a set of field mappings,
//! evaluates each mapping expression and produces an output record.

use crate::eval::evaluator::Evaluator;
use crate::eval::scope::{ScopeEntry, ScopeEntryKind, ScopeKind};
use crate::eval::value::Value;
use crate::lang::ast::Expr;
use crate::lang::span::Span;
use crate::transform::error::TransformError;
use indexmap::IndexMap;
use std::collections::HashSet;

/// A single field mapping: output_name = expression.
#[derive(Debug, Clone)]
pub struct FieldMapping {
    pub output_name: String,
    pub expr: Expr,
}

/// A filter condition (where clause).
#[derive(Debug, Clone)]
pub struct WhereClause {
    pub expr: Expr,
}

/// Configuration for a transform's map block.
#[derive(Debug, Clone)]
pub struct MapConfig {
    pub mappings: Vec<FieldMapping>,
    pub where_clauses: Vec<WhereClause>,
}

/// Result of processing a single record through the mapper.
pub enum MapResult {
    /// Record was transformed successfully.
    Emit(Value),
    /// Record was filtered out by a where clause.
    Filtered,
}

/// Process a single input record through the map config.
///
/// Creates a temporary evaluator, binds the input record as `in`,
/// evaluates where clauses and field mappings, and returns the output record.
pub fn map_record(
    input: &Value,
    config: &MapConfig,
    evaluator: &mut Evaluator,
) -> Result<MapResult, TransformError> {
    // Create a scope for this record
    let scope_id = evaluator.scopes_mut().create_scope(ScopeKind::Block, None);

    // Bind `in` to the input record
    evaluator.scopes_mut().add_entry(
        scope_id,
        ScopeEntry {
            name: "in".to_string(),
            kind: ScopeEntryKind::LetBinding,
            value: Some(input.clone()),
            span: Span::dummy(),
            dependencies: HashSet::new(),
            evaluated: true,
            read_count: 0,
        },
    );

    // Also bind top-level fields directly for convenience
    if let Value::Map(ref map) = input {
        for (key, val) in map {
            evaluator.scopes_mut().add_entry(
                scope_id,
                ScopeEntry {
                    name: key.clone(),
                    kind: ScopeEntryKind::LetBinding,
                    value: Some(val.clone()),
                    span: Span::dummy(),
                    dependencies: HashSet::new(),
                    evaluated: true,
                    read_count: 0,
                },
            );
        }
    }

    // Evaluate where clauses
    for clause in &config.where_clauses {
        let result = evaluator
            .eval_expr(&clause.expr, scope_id)
            .map_err(|d| TransformError::Eval(d.message.clone()))?;
        match result.is_truthy() {
            Some(false) => return Ok(MapResult::Filtered),
            Some(true) => {}
            None => {
                return Err(TransformError::Eval(format!(
                    "where clause must evaluate to bool, got {}",
                    result.type_name()
                )));
            }
        }
    }

    // Evaluate field mappings
    let mut output = IndexMap::new();
    for mapping in &config.mappings {
        let value = evaluator
            .eval_expr(&mapping.expr, scope_id)
            .map_err(|d| TransformError::Eval(d.message.clone()))?;
        output.insert(mapping.output_name.clone(), value);
    }

    Ok(MapResult::Emit(Value::Map(output)))
}

/// Process a batch of input records through the map config.
pub fn map_records(inputs: &[Value], config: &MapConfig) -> Result<Vec<Value>, TransformError> {
    let mut evaluator = Evaluator::new();
    let mut results = Vec::new();

    for input in inputs {
        match map_record(input, config, &mut evaluator)? {
            MapResult::Emit(output) => results.push(output),
            MapResult::Filtered => {}
        }
    }

    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::ast::{Ident, StringLit, StringPart};

    fn make_ident(name: &str) -> Expr {
        Expr::Ident(Ident {
            name: name.to_string(),
            span: Span::dummy(),
        })
    }

    fn make_string(s: &str) -> Expr {
        Expr::StringLit(StringLit {
            parts: vec![StringPart::Literal(s.to_string())],
            heredoc: None,
            span: Span::dummy(),
        })
    }

    fn make_member(obj: Expr, field: &str) -> Expr {
        Expr::MemberAccess(
            Box::new(obj),
            Ident {
                name: field.to_string(),
                span: Span::dummy(),
            },
            Span::dummy(),
        )
    }

    fn make_record(fields: Vec<(&str, Value)>) -> Value {
        let mut map = IndexMap::new();
        for (k, v) in fields {
            map.insert(k.to_string(), v);
        }
        Value::Map(map)
    }

    #[test]
    fn simple_field_mapping() {
        let input = make_record(vec![
            ("name", Value::String("Alice".into())),
            ("age", Value::Int(30)),
        ]);

        let config = MapConfig {
            mappings: vec![
                FieldMapping {
                    output_name: "user_name".into(),
                    expr: make_member(make_ident("in"), "name"),
                },
                FieldMapping {
                    output_name: "user_age".into(),
                    expr: make_member(make_ident("in"), "age"),
                },
            ],
            where_clauses: vec![],
        };

        let mut evaluator = Evaluator::new();
        let result = map_record(&input, &config, &mut evaluator).unwrap();
        if let MapResult::Emit(Value::Map(m)) = result {
            assert_eq!(m.get("user_name"), Some(&Value::String("Alice".into())));
            assert_eq!(m.get("user_age"), Some(&Value::Int(30)));
        } else {
            panic!("expected Emit");
        }
    }

    #[test]
    fn direct_field_access() {
        // Fields are also bound directly (not just via `in.field`)
        let input = make_record(vec![("count", Value::Int(42))]);

        let config = MapConfig {
            mappings: vec![FieldMapping {
                output_name: "result".into(),
                expr: make_ident("count"),
            }],
            where_clauses: vec![],
        };

        let mut evaluator = Evaluator::new();
        let result = map_record(&input, &config, &mut evaluator).unwrap();
        if let MapResult::Emit(Value::Map(m)) = result {
            assert_eq!(m.get("result"), Some(&Value::Int(42)));
        } else {
            panic!("expected Emit");
        }
    }

    #[test]
    fn where_clause_filters() {
        let input_pass = make_record(vec![("active", Value::Bool(true))]);
        let input_fail = make_record(vec![("active", Value::Bool(false))]);

        let config = MapConfig {
            mappings: vec![FieldMapping {
                output_name: "status".into(),
                expr: make_string("ok"),
            }],
            where_clauses: vec![WhereClause {
                expr: make_ident("active"),
            }],
        };

        let mut evaluator = Evaluator::new();

        let result = map_record(&input_pass, &config, &mut evaluator).unwrap();
        assert!(matches!(result, MapResult::Emit(_)));

        let result = map_record(&input_fail, &config, &mut evaluator).unwrap();
        assert!(matches!(result, MapResult::Filtered));
    }

    #[test]
    fn batch_mapping() {
        let inputs = vec![
            make_record(vec![
                ("name", Value::String("Alice".into())),
                ("active", Value::Bool(true)),
            ]),
            make_record(vec![
                ("name", Value::String("Bob".into())),
                ("active", Value::Bool(false)),
            ]),
            make_record(vec![
                ("name", Value::String("Carol".into())),
                ("active", Value::Bool(true)),
            ]),
        ];

        let config = MapConfig {
            mappings: vec![FieldMapping {
                output_name: "user".into(),
                expr: make_ident("name"),
            }],
            where_clauses: vec![WhereClause {
                expr: make_ident("active"),
            }],
        };

        let results = map_records(&inputs, &config).unwrap();
        assert_eq!(results.len(), 2); // Bob filtered out
        if let Value::Map(ref m) = results[0] {
            assert_eq!(m.get("user"), Some(&Value::String("Alice".into())));
        }
        if let Value::Map(ref m) = results[1] {
            assert_eq!(m.get("user"), Some(&Value::String("Carol".into())));
        }
    }
}
