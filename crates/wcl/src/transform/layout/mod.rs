//! Layout engine — orchestrates section execution for binary/text format parsing.
//!
//! A layout defines how structs compose into a complete format. Sections can be
//! structured (fully buffered) or streamed (processed one record at a time).

use crate::eval::value::Value;
use crate::schema::struct_registry::{ResolvedStruct, StructRegistry};
use crate::transform::error::TransformError;
use crate::transform::struct_parser::{self, EncodingConfig, Endianness};
use indexmap::IndexMap;

/// A compiled layout section.
#[derive(Debug, Clone)]
pub struct LayoutSection {
    pub name: String,
    pub struct_name: String,
    pub kind: SectionKind,
    pub encoding: EncodingConfig,
    pub count: Option<CountSpec>,
}

/// Whether a section is structured (buffered) or streamed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionKind {
    Structured,
    Stream,
}

/// How to determine the number of records in a stream section.
#[derive(Debug, Clone)]
pub enum CountSpec {
    /// Fixed count known at layout definition time.
    Fixed(usize),
    /// Count from a field in a previously-parsed structured section.
    FieldRef { section: String, field: String },
}

/// A compiled layout definition.
#[derive(Debug, Clone)]
pub struct LayoutDef {
    pub name: String,
    pub sections: Vec<LayoutSection>,
}

/// Result of executing a layout against input data.
#[derive(Debug)]
pub struct LayoutResult {
    /// Structured section values, keyed by section name.
    pub structured: IndexMap<String, Value>,
    /// Streamed section records, keyed by section name.
    pub streams: IndexMap<String, Vec<Value>>,
}

/// Execute a layout against binary input data.
pub fn execute_layout(
    data: &[u8],
    layout: &LayoutDef,
    registry: &StructRegistry,
) -> Result<LayoutResult, TransformError> {
    let mut cursor = 0usize;
    let mut structured = IndexMap::new();
    let mut streams = IndexMap::new();

    for section in &layout.sections {
        let struct_def = registry.get(&section.struct_name).ok_or_else(|| {
            TransformError::Codec(format!(
                "layout section '{}' references unknown struct '{}'",
                section.name, section.struct_name
            ))
        })?;

        match section.kind {
            SectionKind::Structured => {
                let remaining = &data[cursor..];
                let value = struct_parser::parse_binary(remaining, struct_def, &section.encoding)?;
                // Advance cursor by the struct size
                let size = estimate_struct_size(struct_def, &section.encoding);
                cursor += size;
                structured.insert(section.name.clone(), value);
            }
            SectionKind::Stream => {
                let count = resolve_count(&section.count, &structured)?;
                let remaining = &data[cursor..];
                let records = struct_parser::parse_binary_records(
                    remaining,
                    struct_def,
                    &section.encoding,
                    count,
                )?;
                // Advance cursor
                let struct_size = estimate_struct_size(struct_def, &section.encoding);
                cursor += struct_size * records.len();
                streams.insert(section.name.clone(), records);
            }
        }
    }

    Ok(LayoutResult {
        structured,
        streams,
    })
}

/// Estimate the byte size of a struct based on its field types.
fn estimate_struct_size(struct_def: &ResolvedStruct, _encoding: &EncodingConfig) -> usize {
    use crate::lang::ast::TypeExpr;
    let mut size = 0;
    for field in &struct_def.fields {
        size += match &field.type_expr {
            TypeExpr::U8(_) | TypeExpr::I8(_) | TypeExpr::Bool(_) => 1,
            TypeExpr::U16(_) | TypeExpr::I16(_) => 2,
            TypeExpr::U32(_) | TypeExpr::I32(_) | TypeExpr::F32(_) => 4,
            TypeExpr::U64(_) | TypeExpr::I64(_) | TypeExpr::F64(_) => 8,
            TypeExpr::I128(_) | TypeExpr::U128(_) => 16,
            _ => 0, // Variable-length fields can't be estimated
        };
    }
    size
}

/// Resolve a count specification against already-parsed structured sections.
fn resolve_count(
    spec: &Option<CountSpec>,
    structured: &IndexMap<String, Value>,
) -> Result<Option<usize>, TransformError> {
    match spec {
        None => Ok(None),
        Some(CountSpec::Fixed(n)) => Ok(Some(*n)),
        Some(CountSpec::FieldRef { section, field }) => {
            let section_val = structured.get(section).ok_or_else(|| {
                TransformError::Codec(format!(
                    "count references section '{}' which hasn't been parsed yet",
                    section
                ))
            })?;
            if let Value::Map(m) = section_val {
                match m.get(field) {
                    Some(Value::Int(n)) => Ok(Some(*n as usize)),
                    Some(Value::BigInt(n)) => Ok(Some(*n as usize)),
                    Some(other) => Err(TransformError::Codec(format!(
                        "count field '{}.{}' is {}, expected integer",
                        section,
                        field,
                        other.type_name()
                    ))),
                    None => Err(TransformError::Codec(format!(
                        "count field '{}.{}' not found",
                        section, field
                    ))),
                }
            } else {
                Err(TransformError::Codec(format!(
                    "section '{}' is not a map",
                    section
                )))
            }
        }
    }
}

/// Build an EncodingConfig from layout section decorators.
///
/// This is a convenience function for constructing encoding configs
/// from the decorator attributes on a layout section block.
pub fn encoding_from_decorators(decorators: &[crate::lang::ast::Decorator]) -> EncodingConfig {
    let mut config = EncodingConfig::default();

    for dec in decorators {
        match dec.name.name.as_str() {
            "le" => {
                // @le or @le("field_name")
                if dec.args.is_empty() {
                    config.default_endian = Endianness::Little;
                } else if let Some(crate::lang::ast::DecoratorArg::Positional(
                    crate::lang::ast::Expr::StringLit(s),
                )) = dec.args.first()
                {
                    if let [crate::lang::ast::StringPart::Literal(field_name)] = &s.parts[..] {
                        config
                            .field_overrides
                            .entry(field_name.clone())
                            .or_default()
                            .endian = Some(Endianness::Little);
                    }
                }
            }
            "be" => {
                if dec.args.is_empty() {
                    config.default_endian = Endianness::Big;
                } else if let Some(crate::lang::ast::DecoratorArg::Positional(
                    crate::lang::ast::Expr::StringLit(s),
                )) = dec.args.first()
                {
                    if let [crate::lang::ast::StringPart::Literal(field_name)] = &s.parts[..] {
                        config
                            .field_overrides
                            .entry(field_name.clone())
                            .or_default()
                            .endian = Some(Endianness::Big);
                    }
                }
            }
            "padding" => {
                if let Some(crate::lang::ast::DecoratorArg::Positional(
                    crate::lang::ast::Expr::IntLit(n, _),
                )) = dec.args.first()
                {
                    // Global padding — not field-specific
                    let _ = n; // TODO: apply to previous field
                }
            }
            _ => {}
        }
    }

    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::ast::TypeExpr;
    use crate::lang::span::Span;
    use crate::schema::struct_registry::StructField;

    fn sp() -> Span {
        Span::dummy()
    }

    fn make_struct(name: &str, fields: Vec<(&str, TypeExpr)>) -> ResolvedStruct {
        ResolvedStruct {
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
    fn layout_structured_plus_stream() {
        // Create a simple layout: header (u32 count) + stream of records (u16, u16)
        let mut registry = StructRegistry::new();

        let header = make_struct("Header", vec![("count", TypeExpr::U32(sp()))]);
        let record = make_struct(
            "Record",
            vec![("id", TypeExpr::U16(sp())), ("value", TypeExpr::U16(sp()))],
        );

        registry.structs.insert("Header".into(), header);
        registry.structs.insert("Record".into(), record);

        let layout = LayoutDef {
            name: "test_layout".into(),
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
        };

        // Build binary data: header.count = 3, then 3 records
        let mut data = Vec::new();
        data.extend_from_slice(&3u32.to_le_bytes()); // count = 3
        for i in 0..3u16 {
            data.extend_from_slice(&(i + 1).to_le_bytes()); // id
            data.extend_from_slice(&((i + 1) * 100).to_le_bytes()); // value
        }

        let result = execute_layout(&data, &layout, &registry).unwrap();

        // Check header
        if let Some(Value::Map(h)) = result.structured.get("header") {
            assert_eq!(h.get("count"), Some(&Value::Int(3)));
        } else {
            panic!("expected header map");
        }

        // Check records
        let records = result.streams.get("records").unwrap();
        assert_eq!(records.len(), 3);
        if let Value::Map(r) = &records[0] {
            assert_eq!(r.get("id"), Some(&Value::Int(1)));
            assert_eq!(r.get("value"), Some(&Value::Int(100)));
        }
        if let Value::Map(r) = &records[2] {
            assert_eq!(r.get("id"), Some(&Value::Int(3)));
            assert_eq!(r.get("value"), Some(&Value::Int(300)));
        }
    }
}
