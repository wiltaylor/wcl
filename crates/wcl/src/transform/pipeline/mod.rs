//! Pipeline system — chains transforms with type-checked boundaries.
//!
//! A pipeline is a sequence of transform steps where each step's output
//! feeds into the next step's input. The system supports streaming fusion
//! (records flow through all steps without intermediate materialization).

use crate::eval::value::Value;
use crate::transform::error::TransformError;
use crate::transform::mapper;
use crate::transform::mapper::{MapConfig, MapResult};

/// A single step in a pipeline.
#[derive(Debug, Clone)]
pub struct PipelineStep {
    pub name: String,
    pub config: MapConfig,
}

/// A compiled pipeline definition.
#[derive(Debug, Clone)]
pub struct PipelineDef {
    pub name: String,
    pub steps: Vec<PipelineStep>,
}

/// Result of pipeline execution.
#[derive(Debug, Clone)]
pub struct PipelineStats {
    pub records_in: usize,
    pub records_out: usize,
    pub steps_executed: usize,
}

/// Execute a pipeline: pass records through each step in sequence.
///
/// Records flow through all steps via streaming fusion — each record
/// passes through all steps before the next record enters.
pub fn execute_pipeline(
    inputs: &[Value],
    pipeline: &PipelineDef,
) -> Result<(Vec<Value>, PipelineStats), TransformError> {
    let mut current = inputs.to_vec();
    let steps_executed = pipeline.steps.len();

    for step in &pipeline.steps {
        current = execute_step(&current, &step.config)?;
    }

    Ok((
        current.clone(),
        PipelineStats {
            records_in: inputs.len(),
            records_out: current.len(),
            steps_executed,
        },
    ))
}

/// Execute a single pipeline step with streaming fusion.
///
/// For each input record, apply the map config. Records that pass
/// the where filter are collected; filtered records are dropped.
fn execute_step(inputs: &[Value], config: &MapConfig) -> Result<Vec<Value>, TransformError> {
    let mut evaluator = crate::eval::evaluator::Evaluator::new();
    let mut outputs = Vec::new();

    for input in inputs {
        match mapper::map_record(input, config, &mut evaluator)? {
            MapResult::Emit(output) => outputs.push(output),
            MapResult::Filtered => {}
        }
    }

    Ok(outputs)
}

/// Execute a pipeline with streaming fusion — each record flows through
/// all steps before the next record enters. More memory-efficient than
/// materializing intermediate results.
pub fn execute_fused(
    inputs: &[Value],
    pipeline: &PipelineDef,
) -> Result<(Vec<Value>, PipelineStats), TransformError> {
    let mut evaluators: Vec<crate::eval::evaluator::Evaluator> = pipeline
        .steps
        .iter()
        .map(|_| crate::eval::evaluator::Evaluator::new())
        .collect();
    let mut outputs = Vec::new();

    for input in inputs {
        let mut current = input.clone();
        let mut filtered = false;

        for (i, step) in pipeline.steps.iter().enumerate() {
            match mapper::map_record(&current, &step.config, &mut evaluators[i])? {
                MapResult::Emit(output) => {
                    current = output;
                }
                MapResult::Filtered => {
                    filtered = true;
                    break;
                }
            }
        }

        if !filtered {
            outputs.push(current);
        }
    }

    Ok((
        outputs.clone(),
        PipelineStats {
            records_in: inputs.len(),
            records_out: outputs.len(),
            steps_executed: pipeline.steps.len(),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::ast::{BinOp, Expr, Ident, StringLit, StringPart};
    use crate::lang::span::Span;
    use crate::transform::mapper::{FieldMapping, WhereClause};
    use indexmap::IndexMap;

    fn sp() -> Span {
        Span::dummy()
    }

    fn make_ident(name: &str) -> Expr {
        Expr::Ident(Ident {
            name: name.to_string(),
            span: sp(),
        })
    }

    fn make_member(obj: Expr, field: &str) -> Expr {
        Expr::MemberAccess(
            Box::new(obj),
            Ident {
                name: field.to_string(),
                span: sp(),
            },
            sp(),
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
    fn two_step_pipeline() {
        // Step 1: rename name -> user_name
        let step1 = PipelineStep {
            name: "rename".into(),
            config: MapConfig {
                mappings: vec![
                    FieldMapping {
                        output_name: "user_name".into(),
                        expr: make_member(make_ident("in"), "name"),
                    },
                    FieldMapping {
                        output_name: "age".into(),
                        expr: make_member(make_ident("in"), "age"),
                    },
                ],
                where_clauses: vec![],
            },
        };

        // Step 2: add computed field
        let step2 = PipelineStep {
            name: "enrich".into(),
            config: MapConfig {
                mappings: vec![
                    FieldMapping {
                        output_name: "name".into(),
                        expr: make_member(make_ident("in"), "user_name"),
                    },
                    FieldMapping {
                        output_name: "age_months".into(),
                        expr: Expr::BinaryOp(
                            Box::new(make_member(make_ident("in"), "age")),
                            BinOp::Mul,
                            Box::new(Expr::IntLit(12, sp())),
                            sp(),
                        ),
                    },
                ],
                where_clauses: vec![],
            },
        };

        let pipeline = PipelineDef {
            name: "test".into(),
            steps: vec![step1, step2],
        };

        let inputs = vec![
            make_record(vec![
                ("name", Value::String("Alice".into())),
                ("age", Value::Int(30)),
            ]),
            make_record(vec![
                ("name", Value::String("Bob".into())),
                ("age", Value::Int(25)),
            ]),
        ];

        let (results, stats) = execute_pipeline(&inputs, &pipeline).unwrap();
        assert_eq!(stats.records_in, 2);
        assert_eq!(stats.records_out, 2);
        assert_eq!(stats.steps_executed, 2);

        if let Value::Map(r) = &results[0] {
            assert_eq!(r.get("name"), Some(&Value::String("Alice".into())));
            assert_eq!(r.get("age_months"), Some(&Value::Int(360)));
        } else {
            panic!("expected Map");
        }
    }

    #[test]
    fn pipeline_with_filter() {
        let step1 = PipelineStep {
            name: "filter".into(),
            config: MapConfig {
                mappings: vec![FieldMapping {
                    output_name: "name".into(),
                    expr: make_member(make_ident("in"), "name"),
                }],
                where_clauses: vec![WhereClause {
                    expr: make_member(make_ident("in"), "active"),
                }],
            },
        };

        let pipeline = PipelineDef {
            name: "filter_pipe".into(),
            steps: vec![step1],
        };

        let inputs = vec![
            make_record(vec![
                ("name", Value::String("Alice".into())),
                ("active", Value::Bool(true)),
            ]),
            make_record(vec![
                ("name", Value::String("Bob".into())),
                ("active", Value::Bool(false)),
            ]),
        ];

        let (results, stats) = execute_pipeline(&inputs, &pipeline).unwrap();
        assert_eq!(stats.records_out, 1);
        if let Value::Map(r) = &results[0] {
            assert_eq!(r.get("name"), Some(&Value::String("Alice".into())));
        }
    }

    #[test]
    fn fused_pipeline_same_as_sequential() {
        let step1 = PipelineStep {
            name: "rename".into(),
            config: MapConfig {
                mappings: vec![FieldMapping {
                    output_name: "x".into(),
                    expr: make_member(make_ident("in"), "a"),
                }],
                where_clauses: vec![],
            },
        };

        let step2 = PipelineStep {
            name: "double".into(),
            config: MapConfig {
                mappings: vec![FieldMapping {
                    output_name: "result".into(),
                    expr: Expr::BinaryOp(
                        Box::new(make_member(make_ident("in"), "x")),
                        BinOp::Mul,
                        Box::new(Expr::IntLit(2, sp())),
                        sp(),
                    ),
                }],
                where_clauses: vec![],
            },
        };

        let pipeline = PipelineDef {
            name: "test".into(),
            steps: vec![step1, step2],
        };

        let inputs = vec![make_record(vec![("a", Value::Int(5))])];

        let (seq_results, _) = execute_pipeline(&inputs, &pipeline).unwrap();
        let (fused_results, _) = execute_fused(&inputs, &pipeline).unwrap();

        assert_eq!(seq_results, fused_results);
        if let Value::Map(r) = &fused_results[0] {
            assert_eq!(r.get("result"), Some(&Value::Int(10)));
        }
    }
}
