//! WCL Transform — declarative, streaming-capable data transformation engine.
//!
//! This module provides the runtime for transforms defined in WCL documents.
//! It includes codecs for format conversion, a streaming event model,
//! and a mapper for field-level transformations.

pub mod accumulator;
pub mod codec;
pub mod error;
pub mod event;
pub mod layout;
pub mod mapper;
pub mod pipeline;
pub mod state;
pub mod struct_parser;

pub use error::TransformError;
pub use event::Event;
pub use mapper::{map_record, map_records, FieldMapping, MapConfig, MapResult, WhereClause};

use crate::eval::value::{FunctionValue, Value};
use std::io::{Read, Write};
use std::path::Path;

/// Context for transforms that need access to struct definitions.
pub struct TransformContext<'a> {
    pub struct_registry: &'a crate::schema::struct_registry::StructRegistry,
}

/// Execute a transform: read input via codec, apply field mappings, write output via codec.
///
/// This is the main entry point for the transform runtime.
/// The optional `context` parameter provides struct/layout registries for binary codec.
#[allow(clippy::too_many_arguments)]
pub fn execute(
    input_codec: &str,
    input_reader: impl Read,
    output_codec: &str,
    output_writer: &mut dyn Write,
    config: &MapConfig,
    input_options: &codec::CodecOptions,
    output_options: &codec::CodecOptions,
    context: Option<&TransformContext>,
) -> Result<TransformStats, TransformError> {
    execute_with_custom(
        input_codec,
        input_reader,
        output_codec,
        output_writer,
        config,
        input_options,
        output_options,
        context,
        None,
        None,
    )
}

/// Execute a transform and write to a directory-capable output target.
#[allow(clippy::too_many_arguments)]
pub fn execute_with_custom_to_target(
    input_codec: &str,
    input_reader: impl Read,
    output_codec: &str,
    output_target: codec::native::OutputTarget<'_>,
    config: &MapConfig,
    input_options: &codec::CodecOptions,
    output_options: &codec::CodecOptions,
    context: Option<&TransformContext>,
    file_transform: Option<&FunctionValue>,
    custom_codecs: Option<&codec::custom::CustomCodecRegistry>,
) -> Result<TransformStats, TransformError> {
    execute_with_custom_internal(
        input_codec,
        input_reader,
        output_codec,
        output_target,
        config,
        input_options,
        output_options,
        context,
        file_transform,
        custom_codecs,
    )
}

/// Execute a transform and write output into a directory target.
#[allow(clippy::too_many_arguments)]
pub fn execute_with_custom_to_directory(
    input_codec: &str,
    input_reader: impl Read,
    output_codec: &str,
    output_dir: &Path,
    config: &MapConfig,
    input_options: &codec::CodecOptions,
    output_options: &codec::CodecOptions,
    context: Option<&TransformContext>,
    file_transform: Option<&FunctionValue>,
    custom_codecs: Option<&codec::custom::CustomCodecRegistry>,
) -> Result<TransformStats, TransformError> {
    execute_with_custom_to_target(
        input_codec,
        input_reader,
        output_codec,
        codec::native::OutputTarget::Directory(output_dir),
        config,
        input_options,
        output_options,
        context,
        file_transform,
        custom_codecs,
    )
}

/// Execute a transform with optional WCL-authored custom codecs.
#[allow(clippy::too_many_arguments)]
pub fn execute_with_custom(
    input_codec: &str,
    input_reader: impl Read,
    output_codec: &str,
    output_writer: &mut dyn Write,
    config: &MapConfig,
    input_options: &codec::CodecOptions,
    output_options: &codec::CodecOptions,
    context: Option<&TransformContext>,
    file_transform: Option<&FunctionValue>,
    custom_codecs: Option<&codec::custom::CustomCodecRegistry>,
) -> Result<TransformStats, TransformError> {
    execute_with_custom_internal(
        input_codec,
        input_reader,
        output_codec,
        codec::native::OutputTarget::Stream(output_writer),
        config,
        input_options,
        output_options,
        context,
        file_transform,
        custom_codecs,
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_with_custom_internal(
    input_codec: &str,
    input_reader: impl Read,
    output_codec: &str,
    output_target: codec::native::OutputTarget<'_>,
    config: &MapConfig,
    input_options: &codec::CodecOptions,
    output_options: &codec::CodecOptions,
    context: Option<&TransformContext>,
    file_transform: Option<&FunctionValue>,
    custom_codecs: Option<&codec::custom::CustomCodecRegistry>,
) -> Result<TransformStats, TransformError> {
    let standard_codecs;
    let custom_codecs = match custom_codecs {
        Some(registry) => registry,
        None => {
            standard_codecs = codec::custom::standard_registry()?;
            &standard_codecs
        }
    };

    if let Some(custom) = custom_codecs.get(input_codec) {
        let empty_struct_registry;
        let struct_registry = match context {
            Some(ctx) => ctx.struct_registry,
            None => {
                empty_struct_registry = crate::schema::struct_registry::StructRegistry::new();
                &empty_struct_registry
            }
        };
        let mut decoded = codec::custom::decode_custom_file_with_options(
            input_reader,
            custom,
            input_options,
            struct_registry,
        )?;
        let mut value = if let Some(run) = file_transform {
            decoded
                .session
                .call_function(run, &[decoded.value.clone()])?
        } else {
            decoded.value
        };

        let mut records_read = 1usize;
        let mut records_filtered = 0usize;
        if file_transform.is_none() {
            let records = materialize_records(value)?;
            records_read = records.len();
            let mapped = map_records(&records, config)?;
            records_filtered = records_read.saturating_sub(mapped.len());
            value = Value::List(mapped);
        }

        let native_codecs = codec::native::NativeCodecRegistry::standard();
        let written = if let Some(native) = native_codecs.get(output_codec) {
            codec::native::encode_native_value(&value, native, output_options, output_target)?
        } else {
            let output_custom = custom_codecs
                .get(output_codec)
                .ok_or_else(|| TransformError::UnknownCodec(output_codec.to_string()))?;
            match output_target {
                codec::native::OutputTarget::Stream(output_writer) => {
                    if contains_stream(&value) {
                        codec::custom::encode_custom_value_with_session(
                            &mut decoded.session,
                            &value,
                            output_custom,
                            output_options,
                            output_writer,
                        )?
                    } else {
                        codec::custom::encode_custom_value(
                            &value,
                            output_custom,
                            output_options,
                            output_writer,
                        )?
                    }
                }
                codec::native::OutputTarget::Directory(_) => {
                    return Err(TransformError::Codec(format!(
                        "codec '{}' does not support directory output",
                        output_codec
                    )));
                }
            }
        };
        return Ok(TransformStats {
            records_read,
            records_written: written,
            records_filtered,
        });
    }

    Err(TransformError::UnknownCodec(input_codec.to_string()))
}

fn materialize_records(value: Value) -> Result<Vec<Value>, TransformError> {
    match value {
        Value::List(records) => Ok(records),
        Value::NativeStream(stream) => {
            let mut records = Vec::new();
            loop {
                let mut state = stream.inner.lock().unwrap();
                if state.exhausted {
                    break;
                }
                let value = (state.next)().map_err(TransformError::Codec)?;
                let Some(value) = value else {
                    state.exhausted = true;
                    break;
                };
                drop(state);
                records.push(value);
            }
            Ok(records)
        }
        other => Ok(vec![other]),
    }
}

fn contains_stream(value: &Value) -> bool {
    match value {
        Value::Stream(_) => true,
        Value::NativeStream(_) => true,
        Value::Lazy(_) => true,
        Value::List(items) | Value::Set(items) => items.iter().any(contains_stream),
        Value::Map(map) => map.values().any(contains_stream),
        Value::Object(object) => object.fields.values().any(contains_stream),
        Value::BlockRef(block) => {
            block.attributes.values().any(contains_stream)
                || block
                    .children
                    .iter()
                    .any(|child| contains_stream(&Value::BlockRef(child.clone())))
                || block
                    .decorators
                    .iter()
                    .any(|decorator| decorator.args.values().any(contains_stream))
        }
        _ => false,
    }
}

/// Statistics from a transform execution.
#[derive(Debug, Clone)]
pub struct TransformStats {
    pub records_read: usize,
    pub records_written: usize,
    pub records_filtered: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::ast::Ident;
    use crate::lang::span::Span;

    fn make_ident(name: &str) -> crate::lang::ast::Expr {
        crate::lang::ast::Expr::Ident(Ident {
            name: name.to_string(),
            span: Span::dummy(),
        })
    }

    fn make_member(obj: crate::lang::ast::Expr, field: &str) -> crate::lang::ast::Expr {
        crate::lang::ast::Expr::MemberAccess(
            Box::new(obj),
            Ident {
                name: field.to_string(),
                span: Span::dummy(),
            },
            Span::dummy(),
        )
    }

    #[test]
    fn end_to_end_json_transform() {
        let input_json = r#"[{"name": "Alice", "age": 30}, {"name": "Bob", "age": 25}]"#;

        let config = MapConfig {
            mappings: vec![
                FieldMapping {
                    output_name: "user".into(),
                    expr: make_member(make_ident("in"), "name"),
                },
                FieldMapping {
                    output_name: "years".into(),
                    expr: make_member(make_ident("in"), "age"),
                },
            ],
            where_clauses: vec![],
        };

        let mut output = Vec::new();
        let stats = execute(
            "json",
            input_json.as_bytes(),
            "json",
            &mut output,
            &config,
            &indexmap::IndexMap::new(),
            &indexmap::IndexMap::new(),
            None,
        )
        .unwrap();

        assert_eq!(stats.records_read, 2);
        assert_eq!(stats.records_written, 2);
        assert_eq!(stats.records_filtered, 0);

        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("Alice"));
        assert!(output_str.contains("Bob"));
        assert!(output_str.contains("user"));
        assert!(output_str.contains("years"));
    }

    #[test]
    fn end_to_end_json_with_filter() {
        let input_json = r#"[{"name": "Alice", "active": true}, {"name": "Bob", "active": false}]"#;

        let config = MapConfig {
            mappings: vec![FieldMapping {
                output_name: "user".into(),
                expr: make_member(make_ident("in"), "name"),
            }],
            where_clauses: vec![WhereClause {
                expr: make_member(make_ident("in"), "active"),
            }],
        };

        let mut output = Vec::new();
        let stats = execute(
            "json",
            input_json.as_bytes(),
            "json",
            &mut output,
            &config,
            &indexmap::IndexMap::new(),
            &indexmap::IndexMap::new(),
            None,
        )
        .unwrap();

        assert_eq!(stats.records_read, 2);
        assert_eq!(stats.records_written, 1);
        assert_eq!(stats.records_filtered, 1);

        let output_str = String::from_utf8(output).unwrap();
        assert!(output_str.contains("Alice"));
        assert!(!output_str.contains("Bob"));
    }
}
