//! Struct parser — reads binary or text data according to struct field definitions.
//!
//! A struct definition (pure data shape) is combined with encoding configuration
//! from the layout to produce a ParsePlan. The plan is then executed against
//! input bytes or text to produce WCL Values.

pub mod binary;
pub mod plan;
pub mod writer;

use crate::eval::value::Value;
use crate::schema::struct_registry::ResolvedStruct;
use crate::transform::error::TransformError;
use indexmap::IndexMap;

/// Encoding configuration for a layout section.
/// Specifies how struct fields map to binary/text representation.
#[derive(Debug, Clone, Default)]
pub struct EncodingConfig {
    /// Default endianness for all fields.
    pub default_endian: Endianness,
    /// Per-field encoding overrides.
    pub field_overrides: IndexMap<String, FieldEncoding>,
}

/// Endianness for binary fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Endianness {
    #[default]
    Little,
    Big,
}

/// Per-field encoding settings.
#[derive(Debug, Clone, Default)]
pub struct FieldEncoding {
    pub endian: Option<Endianness>,
    pub magic: Option<Vec<u8>>,
    pub padding_after: Option<usize>,
    pub align: Option<usize>,
}

/// Parse binary data into a WCL Value::Map according to a struct definition.
pub fn parse_binary(
    data: &[u8],
    struct_def: &ResolvedStruct,
    encoding: &EncodingConfig,
) -> Result<Value, TransformError> {
    binary::BinaryParser::parse(data, struct_def, encoding)
}

/// Parse a list of records from binary data (for streaming sections).
pub fn parse_binary_records(
    data: &[u8],
    struct_def: &ResolvedStruct,
    encoding: &EncodingConfig,
    count: Option<usize>,
) -> Result<Vec<Value>, TransformError> {
    let mut parser = binary::BinaryParser::new(data, encoding);
    let mut records = Vec::new();
    let limit = count.unwrap_or(usize::MAX);

    while parser.remaining() > 0 && records.len() < limit {
        let record = parser.parse_struct(struct_def)?;
        records.push(record);
    }

    Ok(records)
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

    fn make_struct(fields: Vec<(&str, TypeExpr)>) -> ResolvedStruct {
        ResolvedStruct {
            name: "Test".to_string(),
            fields: fields
                .into_iter()
                .map(|(name, te)| StructField {
                    name: name.to_string(),
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
    fn parse_simple_binary_struct() {
        let def = make_struct(vec![("x", TypeExpr::U32(sp())), ("y", TypeExpr::U32(sp()))]);
        let encoding = EncodingConfig {
            default_endian: Endianness::Little,
            ..Default::default()
        };

        // Little-endian u32: 42 = 0x2A000000, 100 = 0x64000000
        let mut data = Vec::new();
        data.extend_from_slice(&42u32.to_le_bytes());
        data.extend_from_slice(&100u32.to_le_bytes());

        let result = parse_binary(&data, &def, &encoding).unwrap();
        if let Value::Map(m) = result {
            assert_eq!(m.get("x"), Some(&Value::Int(42)));
            assert_eq!(m.get("y"), Some(&Value::Int(100)));
        } else {
            panic!("expected Map, got {:?}", result);
        }
    }

    #[test]
    fn parse_mixed_types() {
        let def = make_struct(vec![
            ("id", TypeExpr::U16(sp())),
            ("value", TypeExpr::F32(sp())),
            ("flag", TypeExpr::Bool(sp())),
        ]);
        let encoding = EncodingConfig {
            default_endian: Endianness::Little,
            ..Default::default()
        };

        let mut data = Vec::new();
        data.extend_from_slice(&1000u16.to_le_bytes());
        data.extend_from_slice(&3.14f32.to_le_bytes());
        data.extend_from_slice(&[1u8]); // bool = true

        let result = parse_binary(&data, &def, &encoding).unwrap();
        if let Value::Map(m) = result {
            assert_eq!(m.get("id"), Some(&Value::Int(1000)));
            assert_eq!(m.get("flag"), Some(&Value::Bool(true)));
            // Float comparison with tolerance
            if let Some(Value::Float(f)) = m.get("value") {
                assert!((*f - 3.14f64).abs() < 0.01);
            } else {
                panic!("expected float for 'value'");
            }
        } else {
            panic!("expected Map");
        }
    }

    #[test]
    fn parse_big_endian() {
        let def = make_struct(vec![("magic", TypeExpr::U32(sp()))]);
        let encoding = EncodingConfig {
            default_endian: Endianness::Big,
            ..Default::default()
        };

        let data = 0xDEADBEEFu32.to_be_bytes();
        let result = parse_binary(&data, &def, &encoding).unwrap();
        if let Value::Map(m) = result {
            assert_eq!(m.get("magic"), Some(&Value::Int(0xDEADBEEF)));
        } else {
            panic!("expected Map");
        }
    }

    #[test]
    fn parse_multiple_records() {
        let def = make_struct(vec![
            ("id", TypeExpr::U16(sp())),
            ("value", TypeExpr::U16(sp())),
        ]);
        let encoding = EncodingConfig::default();

        let mut data = Vec::new();
        for i in 0..3u16 {
            data.extend_from_slice(&(i + 1).to_le_bytes());
            data.extend_from_slice(&((i + 1) * 10).to_le_bytes());
        }

        let records = parse_binary_records(&data, &def, &encoding, Some(3)).unwrap();
        assert_eq!(records.len(), 3);

        if let Value::Map(m) = &records[0] {
            assert_eq!(m.get("id"), Some(&Value::Int(1)));
            assert_eq!(m.get("value"), Some(&Value::Int(10)));
        }
        if let Value::Map(m) = &records[2] {
            assert_eq!(m.get("id"), Some(&Value::Int(3)));
            assert_eq!(m.get("value"), Some(&Value::Int(30)));
        }
    }
}
