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
    let standard_codecs;
    let custom_codecs = match custom_codecs {
        Some(registry) => registry,
        None => {
            standard_codecs = codec::custom::standard_registry()?;
            &standard_codecs
        }
    };

    if let Some(custom) = custom_codecs.get(input_codec) {
        if custom.decoder.is_some() || file_transform.is_some() {
            let ctx = context.ok_or_else(|| {
                TransformError::Other("stream codec decoding requires transform context".into())
            })?;
            let mut decoded = codec::custom::decode_custom_file_with_options(
                input_reader,
                custom,
                input_options,
                ctx.struct_registry,
            )?;
            let value = if let Some(run) = file_transform {
                decoded
                    .session
                    .call_function(run, &[decoded.value.clone()])?
            } else {
                decoded.value
            };
            let written = if let Some(output_custom) = custom_codecs.get(output_codec) {
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
            } else {
                let records = match value {
                    Value::List(records) => records,
                    other => vec![other],
                };
                match output_codec {
                    "text" => {
                        let separator = output_options
                            .get("separator")
                            .and_then(|v| v.as_string())
                            .unwrap_or("\t");
                        let header = output_options
                            .get("header")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false);
                        codec::text_codec::encode_text_records(
                            &records,
                            output_writer,
                            separator,
                            header,
                        )?;
                    }
                    _ => return Err(TransformError::UnknownCodec(output_codec.to_string())),
                }
                records.len()
            };
            return Ok(TransformStats {
                records_read: 1,
                records_written: written,
                records_filtered: 0,
            });
        }
    }

    // Decode input records
    let records = if let Some(custom) = custom_codecs.get(input_codec) {
        codec::custom::decode_custom_records_with_options(input_reader, custom, input_options)?
    } else {
        match input_codec {
            "binary" => {
                return Err(TransformError::Other(
                    "codec::binary layout transforms have been removed; define a custom codec decoder that returns a file object with streams".into(),
                ));
            }
            "text" => {
                let separator = input_options
                    .get("separator")
                    .and_then(|v| v.as_string())
                    .unwrap_or("\t");
                let has_header = input_options
                    .get("has_header")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                codec::text_codec::decode_text_records(input_reader, separator, has_header)?
            }
            _ => return Err(TransformError::UnknownCodec(input_codec.to_string())),
        }
    };

    // Apply mappings
    let transformed = map_records(&records, config)?;

    // Encode output
    if let Some(custom) = custom_codecs.get(output_codec) {
        codec::custom::encode_custom_records(&transformed, custom, output_options, output_writer)?;
    } else {
        match output_codec {
            "binary" => {
                return Err(TransformError::Other(
                    "codec::binary layout transforms have been removed; define a custom codec encoder that consumes a file object with streams".into(),
                ));
            }
            "text" => {
                let separator = output_options
                    .get("separator")
                    .and_then(|v| v.as_string())
                    .unwrap_or("\t");
                let header = output_options
                    .get("header")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                codec::text_codec::encode_text_records(
                    &transformed,
                    output_writer,
                    separator,
                    header,
                )?;
            }
            _ => return Err(TransformError::UnknownCodec(output_codec.to_string())),
        }
    }

    Ok(TransformStats {
        records_read: records.len(),
        records_written: transformed.len(),
        records_filtered: records.len() - transformed.len(),
    })
}

fn contains_stream(value: &Value) -> bool {
    match value {
        Value::Stream(_) => true,
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
