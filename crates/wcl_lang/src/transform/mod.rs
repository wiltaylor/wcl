//! WCL Transform — declarative, streaming-capable data transformation engine.
//!
//! This module provides the runtime for transforms defined in WCL documents.
//! It includes codecs for format conversion, a streaming event model,
//! and a mapper for field-level transformations.

pub mod accumulator;
pub mod codec;
pub mod error;
pub mod event;
pub mod mapper;
pub mod pipeline;
pub mod state;
pub mod struct_parser;

pub use error::TransformError;
pub use event::Event;
pub use mapper::{map_record, map_records, FieldMapping, MapConfig, MapResult, WhereClause};

use crate::eval::value::{FunctionValue, Value};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;

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

/// Encode an already evaluated WCL value through an output codec.
pub fn encode_value_with_custom_to_target(
    value: &Value,
    output_codec: &str,
    output_target: codec::native::OutputTarget<'_>,
    output_options: &codec::CodecOptions,
    custom_codecs: Option<&codec::custom::CustomCodecRegistry>,
) -> Result<usize, TransformError> {
    encode_value_with_custom_to_target_internal(
        value,
        output_codec,
        output_target,
        output_options,
        custom_codecs,
        None,
        None,
    )
}

/// Encode an evaluated value while allowing WCL codec functions to read files.
pub fn encode_value_with_custom_to_target_with_file_access(
    value: &Value,
    output_codec: &str,
    output_target: codec::native::OutputTarget<'_>,
    output_options: &codec::CodecOptions,
    custom_codecs: Option<&codec::custom::CustomCodecRegistry>,
    base_dir: PathBuf,
) -> Result<usize, TransformError> {
    encode_value_with_custom_to_target_internal(
        value,
        output_codec,
        output_target,
        output_options,
        custom_codecs,
        None,
        Some(base_dir),
    )
}

/// Encode an already evaluated WCL value into a directory-capable output codec.
pub fn encode_value_with_custom_to_directory(
    value: &Value,
    output_codec: &str,
    output_dir: &Path,
    output_options: &codec::CodecOptions,
    custom_codecs: Option<&codec::custom::CustomCodecRegistry>,
) -> Result<usize, TransformError> {
    encode_value_with_custom_to_target(
        value,
        output_codec,
        codec::native::OutputTarget::Directory(output_dir),
        output_options,
        custom_codecs,
    )
}

/// Encode an evaluated value into a directory while allowing WCL codec file reads.
pub fn encode_value_with_custom_to_directory_with_file_access(
    value: &Value,
    output_codec: &str,
    output_dir: &Path,
    output_options: &codec::CodecOptions,
    custom_codecs: Option<&codec::custom::CustomCodecRegistry>,
    base_dir: PathBuf,
) -> Result<usize, TransformError> {
    encode_value_with_custom_to_target_with_file_access(
        value,
        output_codec,
        codec::native::OutputTarget::Directory(output_dir),
        output_options,
        custom_codecs,
        base_dir,
    )
}

/// Look up a named value or block from a loaded WCL document and encode it.
pub fn encode_document_value_with_custom_to_target(
    document: &crate::Document,
    value_name: &str,
    output_codec: &str,
    output_target: codec::native::OutputTarget<'_>,
    output_options: &codec::CodecOptions,
    custom_codecs: Option<&codec::custom::CustomCodecRegistry>,
) -> Result<usize, TransformError> {
    let value = document.values.get(value_name).ok_or_else(|| {
        TransformError::Codec(format!(
            "loaded WCL document does not contain value or block '{value_name}'"
        ))
    })?;
    encode_value_with_custom_to_target(
        value,
        output_codec,
        output_target,
        output_options,
        custom_codecs,
    )
}

/// Look up a named value or block from a loaded WCL document and encode it into a directory.
pub fn encode_document_value_with_custom_to_directory(
    document: &crate::Document,
    value_name: &str,
    output_codec: &str,
    output_dir: &Path,
    output_options: &codec::CodecOptions,
    custom_codecs: Option<&codec::custom::CustomCodecRegistry>,
) -> Result<usize, TransformError> {
    encode_document_value_with_custom_to_target(
        document,
        value_name,
        output_codec,
        codec::native::OutputTarget::Directory(output_dir),
        output_options,
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

        let written = encode_value_with_custom_to_target_internal(
            &value,
            output_codec,
            output_target,
            output_options,
            Some(custom_codecs),
            Some(&mut decoded.session),
            None,
        )?;
        return Ok(TransformStats {
            records_read,
            records_written: written,
            records_filtered,
        });
    }

    Err(TransformError::UnknownCodec(input_codec.to_string()))
}

fn encode_value_with_custom_to_target_internal(
    value: &Value,
    output_codec: &str,
    output_target: codec::native::OutputTarget<'_>,
    output_options: &codec::CodecOptions,
    custom_codecs: Option<&codec::custom::CustomCodecRegistry>,
    mut source_session: Option<&mut codec::custom::CodecEvalSession>,
    file_base_dir: Option<PathBuf>,
) -> Result<usize, TransformError> {
    let standard_codecs;
    let custom_codecs = match custom_codecs {
        Some(registry) => registry,
        None => {
            standard_codecs = codec::custom::standard_registry()?;
            &standard_codecs
        }
    };

    let native_codecs = codec::native::NativeCodecRegistry::standard();
    if output_codec == "svg" && codec::native::is_svg_diagram_value(value) {
        let native = native_codecs
            .get(output_codec)
            .ok_or_else(|| TransformError::UnknownCodec(output_codec.to_string()))?;
        return codec::native::encode_native_value(value, native, output_options, output_target);
    }

    let uses_wdoc_codec = output_codec == "wdoc-html";
    let output_custom = custom_codecs
        .get(output_codec)
        .ok_or_else(|| TransformError::UnknownCodec(output_codec.to_string()))?;
    let registry = Arc::new(custom_codecs.clone());

    let written = if contains_session_stream(value) {
        let session = source_session.as_deref_mut().ok_or_else(|| {
            TransformError::Codec(
                "cannot encode WCL stream values without the source codec evaluation session"
                    .to_string(),
            )
        })?;
        if let Some(base_dir) = file_base_dir.as_ref() {
            session.enable_file_access(base_dir.clone());
        }
        if uses_wdoc_codec {
            codec::custom::encode_custom_value_with_session_and_registry_and_builtins(
                session,
                value,
                output_custom,
                output_options,
                output_target,
                registry,
                crate::wdoc::source::wdoc_functions().functions,
            )?
        } else {
            codec::custom::encode_custom_value_with_session_and_registry(
                session,
                value,
                output_custom,
                output_options,
                output_target,
                registry,
            )?
        }
    } else if uses_wdoc_codec && file_base_dir.is_some() {
        codec::custom::encode_custom_value_with_registry_and_builtins_and_file_access(
            value,
            output_custom,
            output_options,
            output_target,
            registry,
            crate::wdoc::source::wdoc_functions().functions,
            file_base_dir.expect("checked above"),
        )?
    } else if uses_wdoc_codec {
        codec::custom::encode_custom_value_with_registry_and_builtins(
            value,
            output_custom,
            output_options,
            output_target,
            registry,
            crate::wdoc::source::wdoc_functions().functions,
        )?
    } else {
        codec::custom::encode_custom_value_with_registry(
            value,
            output_custom,
            output_options,
            output_target,
            registry,
        )?
    };
    Ok(written)
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

fn contains_session_stream(value: &Value) -> bool {
    match value {
        Value::Stream(_) => true,
        Value::Lazy(_) => true,
        Value::List(items) | Value::Set(items) => items.iter().any(contains_session_stream),
        Value::Map(map) => map.values().any(contains_session_stream),
        Value::Object(object) => object.fields.values().any(contains_session_stream),
        Value::BlockRef(block) => {
            block.attributes.values().any(contains_session_stream)
                || block
                    .children
                    .iter()
                    .any(|child| contains_session_stream(&Value::BlockRef(child.clone())))
                || block
                    .decorators
                    .iter()
                    .any(|decorator| decorator.args.values().any(contains_session_stream))
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
    fn encodes_named_block_from_loaded_document() {
        let doc = crate::parse(
            r#"
import <html.wcl>

namespace html {
    p intro {
        content = "Hello"
    }
}
"#,
            crate::ParseOptions::default(),
        );
        assert!(
            !doc.has_errors(),
            "parse errors: {:?}",
            doc.errors()
                .into_iter()
                .map(|diagnostic| diagnostic.message.clone())
                .collect::<Vec<_>>()
        );

        let mut out = Vec::new();
        let written = encode_document_value_with_custom_to_target(
            &doc,
            "intro",
            "html",
            codec::native::OutputTarget::Stream(&mut out),
            &codec::CodecOptions::new(),
            None,
        )
        .expect("encode");

        assert_eq!(written, 1);
        let html = String::from_utf8(out).expect("utf8");
        assert!(html.contains("<p id=\"intro\">Hello</p>"), "{html}");
    }

    #[test]
    fn wdoc_html_requires_loaded_document_codec_registry() {
        let mut out = Vec::new();
        let err = encode_value_with_custom_to_target(
            &Value::Map(Default::default()),
            "wdoc-html",
            codec::native::OutputTarget::Stream(&mut out),
            &codec::CodecOptions::new(),
            None,
        )
        .expect_err("wdoc-html should not be available without a loaded registry");

        assert!(matches!(err, TransformError::UnknownCodec(name) if name == "wdoc-html"));
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
