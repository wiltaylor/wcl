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

use std::io::{Read, Write};

/// Execute a transform: read input via codec, apply field mappings, write output via codec.
///
/// This is the main entry point for the transform runtime.
pub fn execute(
    input_codec: &str,
    input_reader: impl Read,
    output_codec: &str,
    output_writer: &mut dyn Write,
    config: &MapConfig,
    _input_options: &codec::CodecOptions,
    output_options: &codec::CodecOptions,
) -> Result<TransformStats, TransformError> {
    // Decode input records
    let records = match input_codec {
        "json" => codec::json::decode_json_records(input_reader)?,
        "yaml" => codec::yaml::decode_yaml_records(input_reader)?,
        "csv" => codec::csv_codec::decode_csv_records(input_reader, true, b',')?,
        "toml" => codec::toml_codec::decode_toml_records(input_reader)?,
        "hcl" => codec::hcl_codec::decode_hcl_records(input_reader)?,
        "xml" => codec::xml::decode_xml_records(input_reader)?,
        "msgpack" => codec::msgpack::decode_msgpack_records(input_reader)?,
        _ => return Err(TransformError::UnknownCodec(input_codec.to_string())),
    };

    // Apply mappings
    let transformed = map_records(&records, config)?;

    // Encode output
    let pretty = output_options
        .get("pretty")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    match output_codec {
        "json" => codec::json::encode_json_records(&transformed, output_writer, pretty)?,
        "yaml" => codec::yaml::encode_yaml_records(&transformed, output_writer, pretty)?,
        "csv" => codec::csv_codec::encode_csv_records(&transformed, output_writer, b',')?,
        "toml" => codec::toml_codec::encode_toml_records(&transformed, output_writer, pretty)?,
        "hcl" => codec::hcl_codec::encode_hcl_records(&transformed, output_writer)?,
        "xml" => codec::xml::encode_xml_records(&transformed, output_writer, "root")?,
        "msgpack" => codec::msgpack::encode_msgpack_records(&transformed, output_writer)?,
        _ => return Err(TransformError::UnknownCodec(output_codec.to_string())),
    }

    Ok(TransformStats {
        records_read: records.len(),
        records_written: transformed.len(),
        records_filtered: records.len() - transformed.len(),
    })
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
