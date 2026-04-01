//! Binary codec — encodes and decodes binary data using layout definitions and
//! struct parsers.

use crate::eval::value::Value;
use crate::schema::layout_registry::LayoutRegistry;
use crate::schema::struct_registry::StructRegistry;
use crate::transform::error::TransformError;
use crate::transform::layout;
use crate::transform::struct_parser;
use std::io::Write;

/// Decode binary data into a list of `Value::Map` records using a named layout.
///
/// Structured sections are returned first (one record per section), followed
/// by all stream records.
pub fn decode_binary_records(
    data: &[u8],
    layout_name: &str,
    struct_registry: &StructRegistry,
    layout_registry: &LayoutRegistry,
) -> Result<Vec<Value>, TransformError> {
    let layout_def = layout_registry
        .get(layout_name)
        .ok_or_else(|| TransformError::Other(format!("layout '{}' not found", layout_name)))?;

    let result = layout::execute_layout(data, layout_def, struct_registry)?;

    let mut records = Vec::new();

    // Only return stream section records — structured sections (headers)
    // are used for metadata (count, offsets) but aren't transform records.
    // If there are no stream sections, return the structured sections as records.
    for (_name, stream_records) in &result.streams {
        records.extend(stream_records.iter().cloned());
    }

    if records.is_empty() {
        for (_name, value) in &result.structured {
            records.push(value.clone());
        }
    }

    Ok(records)
}

/// Encode a list of records as binary data using a named layout.
///
/// Records are consumed in order: first the structured sections (one record each),
/// then the remaining records are written using the stream section's struct.
pub fn encode_binary_records(
    records: &[Value],
    layout_name: &str,
    struct_registry: &StructRegistry,
    layout_registry: &LayoutRegistry,
    writer: &mut dyn Write,
) -> Result<(), TransformError> {
    let layout_def = layout_registry
        .get(layout_name)
        .ok_or_else(|| TransformError::Other(format!("layout '{}' not found", layout_name)))?;

    let mut record_idx = 0;

    for section in &layout_def.sections {
        let struct_def = struct_registry.get(&section.struct_name).ok_or_else(|| {
            TransformError::Codec(format!(
                "layout section '{}' references unknown struct '{}'",
                section.name, section.struct_name
            ))
        })?;

        match section.kind {
            layout::SectionKind::Structured => {
                if record_idx >= records.len() {
                    return Err(TransformError::Other(format!(
                        "not enough records for structured section '{}'",
                        section.name
                    )));
                }
                struct_parser::writer::write_struct(
                    &records[record_idx],
                    struct_def,
                    &section.encoding,
                    writer,
                )?;
                record_idx += 1;
            }
            layout::SectionKind::Stream => {
                let remaining = &records[record_idx..];
                for record in remaining {
                    struct_parser::writer::write_struct(
                        record,
                        struct_def,
                        &section.encoding,
                        writer,
                    )?;
                }
                record_idx = records.len();
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::ast::TypeExpr;
    use crate::lang::span::Span;
    use crate::schema::struct_registry::StructField;
    use crate::transform::layout::{CountSpec, LayoutDef, LayoutSection, SectionKind};
    use crate::transform::struct_parser::EncodingConfig;

    fn sp() -> Span {
        Span::dummy()
    }

    fn make_struct(
        name: &str,
        fields: Vec<(&str, TypeExpr)>,
    ) -> crate::schema::struct_registry::ResolvedStruct {
        crate::schema::struct_registry::ResolvedStruct {
            name: name.to_string(),
            fields: fields
                .into_iter()
                .map(|(n, te)| StructField {
                    name: n.to_string(),
                    type_expr: te,
                    required: true,
                    span: sp(),
                })
                .collect(),
            tag_field: None,
            variants: vec![],
            span: sp(),
        }
    }

    #[test]
    fn decode_binary_with_layout() {
        let mut struct_reg = StructRegistry::new();
        struct_reg.structs.insert(
            "Header".into(),
            make_struct("Header", vec![("count", TypeExpr::U32(sp()))]),
        );
        struct_reg.structs.insert(
            "Record".into(),
            make_struct(
                "Record",
                vec![("id", TypeExpr::U16(sp())), ("value", TypeExpr::U16(sp()))],
            ),
        );

        let mut layout_reg = LayoutRegistry::new();
        layout_reg.layouts.insert(
            "test".into(),
            LayoutDef {
                name: "test".into(),
                sections: vec![
                    LayoutSection {
                        name: "header".into(),
                        struct_name: "Header".into(),
                        kind: SectionKind::Structured,
                        encoding: EncodingConfig::default(),
                        count: None,
                    },
                    LayoutSection {
                        name: "records".into(),
                        struct_name: "Record".into(),
                        kind: SectionKind::Stream,
                        encoding: EncodingConfig::default(),
                        count: Some(CountSpec::FieldRef {
                            section: "header".into(),
                            field: "count".into(),
                        }),
                    },
                ],
            },
        );

        // Build binary: count=2, then 2 records
        let mut data = Vec::new();
        data.extend_from_slice(&2u32.to_le_bytes());
        data.extend_from_slice(&1u16.to_le_bytes());
        data.extend_from_slice(&100u16.to_le_bytes());
        data.extend_from_slice(&2u16.to_le_bytes());
        data.extend_from_slice(&200u16.to_le_bytes());

        let records = decode_binary_records(&data, "test", &struct_reg, &layout_reg).unwrap();
        // Only stream records are returned (header is metadata)
        assert_eq!(records.len(), 2);

        if let Value::Map(r) = &records[0] {
            assert_eq!(r.get("id"), Some(&Value::Int(1)));
            assert_eq!(r.get("value"), Some(&Value::Int(100)));
        } else {
            panic!("expected record map");
        }
    }

    #[test]
    fn unknown_layout_returns_error() {
        let struct_reg = StructRegistry::new();
        let layout_reg = LayoutRegistry::new();

        let result = decode_binary_records(&[], "nonexistent", &struct_reg, &layout_reg);
        assert!(result.is_err());
    }
}
