//! Binary struct writer — inverse of BinaryParser.
//!
//! Writes a `Value::Map` as bytes according to a struct definition and
//! encoding configuration.

use crate::eval::value::Value;
use crate::lang::ast::TypeExpr;
use crate::schema::struct_registry::ResolvedStruct;
use crate::transform::error::TransformError;
use crate::transform::struct_parser::{EncodingConfig, Endianness};
use std::io::Write;

/// Write a `Value::Map` as binary bytes according to a struct definition.
///
/// Returns the total number of bytes written.
pub fn write_struct(
    value: &Value,
    struct_def: &ResolvedStruct,
    encoding: &EncodingConfig,
    writer: &mut dyn Write,
) -> Result<usize, TransformError> {
    let map = match value {
        Value::Map(m) => m,
        _ => {
            return Err(TransformError::TypeMismatch {
                expected: "map".into(),
                got: value.type_name().to_string(),
            })
        }
    };

    let mut total = 0;

    for field in &struct_def.fields {
        let field_value = map
            .get(&field.name)
            .ok_or_else(|| TransformError::MissingField(field.name.clone()))?;

        let field_encoding = encoding.field_overrides.get(&field.name);
        let endian = field_encoding
            .and_then(|e| e.endian)
            .unwrap_or(encoding.default_endian);

        let bytes_written = write_field(field_value, &field.type_expr, endian, writer)?;
        total += bytes_written;

        // Apply padding after
        if let Some(fe) = field_encoding {
            if let Some(pad) = fe.padding_after {
                let zeros = vec![0u8; pad];
                writer.write_all(&zeros).map_err(TransformError::Io)?;
                total += pad;
            }
            if let Some(align) = fe.align {
                let offset = total % align;
                if offset != 0 {
                    let pad = align - offset;
                    let zeros = vec![0u8; pad];
                    writer.write_all(&zeros).map_err(TransformError::Io)?;
                    total += pad;
                }
            }
        }
    }

    Ok(total)
}

fn write_field(
    value: &Value,
    type_expr: &TypeExpr,
    endian: Endianness,
    writer: &mut dyn Write,
) -> Result<usize, TransformError> {
    match type_expr {
        TypeExpr::U8(_) => {
            let v = value_to_i64(value)?;
            writer.write_all(&[v as u8]).map_err(TransformError::Io)?;
            Ok(1)
        }
        TypeExpr::I8(_) => {
            let v = value_to_i64(value)?;
            writer
                .write_all(&(v as i8).to_le_bytes())
                .map_err(TransformError::Io)?;
            Ok(1)
        }
        TypeExpr::U16(_) => {
            let v = value_to_i64(value)? as u16;
            let bytes = match endian {
                Endianness::Little => v.to_le_bytes(),
                Endianness::Big => v.to_be_bytes(),
            };
            writer.write_all(&bytes).map_err(TransformError::Io)?;
            Ok(2)
        }
        TypeExpr::I16(_) => {
            let v = value_to_i64(value)? as i16;
            let bytes = match endian {
                Endianness::Little => v.to_le_bytes(),
                Endianness::Big => v.to_be_bytes(),
            };
            writer.write_all(&bytes).map_err(TransformError::Io)?;
            Ok(2)
        }
        TypeExpr::U32(_) => {
            let v = value_to_i64(value)? as u32;
            let bytes = match endian {
                Endianness::Little => v.to_le_bytes(),
                Endianness::Big => v.to_be_bytes(),
            };
            writer.write_all(&bytes).map_err(TransformError::Io)?;
            Ok(4)
        }
        TypeExpr::I32(_) => {
            let v = value_to_i64(value)? as i32;
            let bytes = match endian {
                Endianness::Little => v.to_le_bytes(),
                Endianness::Big => v.to_be_bytes(),
            };
            writer.write_all(&bytes).map_err(TransformError::Io)?;
            Ok(4)
        }
        TypeExpr::U64(_) => {
            let v = value_to_u64(value)?;
            let bytes = match endian {
                Endianness::Little => v.to_le_bytes(),
                Endianness::Big => v.to_be_bytes(),
            };
            writer.write_all(&bytes).map_err(TransformError::Io)?;
            Ok(8)
        }
        TypeExpr::I64(_) => {
            let v = value_to_i64(value)?;
            let bytes = match endian {
                Endianness::Little => v.to_le_bytes(),
                Endianness::Big => v.to_be_bytes(),
            };
            writer.write_all(&bytes).map_err(TransformError::Io)?;
            Ok(8)
        }
        TypeExpr::F32(_) => {
            let v = value_to_f64(value)? as f32;
            let bytes = match endian {
                Endianness::Little => v.to_le_bytes(),
                Endianness::Big => v.to_be_bytes(),
            };
            writer.write_all(&bytes).map_err(TransformError::Io)?;
            Ok(4)
        }
        TypeExpr::F64(_) => {
            let v = value_to_f64(value)?;
            let bytes = match endian {
                Endianness::Little => v.to_le_bytes(),
                Endianness::Big => v.to_be_bytes(),
            };
            writer.write_all(&bytes).map_err(TransformError::Io)?;
            Ok(8)
        }
        TypeExpr::Bool(_) => {
            let v = match value {
                Value::Bool(b) => *b,
                _ => {
                    return Err(TransformError::TypeMismatch {
                        expected: "bool".into(),
                        got: value.type_name().to_string(),
                    })
                }
            };
            writer
                .write_all(&[if v { 1u8 } else { 0u8 }])
                .map_err(TransformError::Io)?;
            Ok(1)
        }
        TypeExpr::String(_) => {
            let s = match value {
                Value::String(s) => s.as_bytes(),
                _ => {
                    return Err(TransformError::TypeMismatch {
                        expected: "string".into(),
                        got: value.type_name().to_string(),
                    })
                }
            };
            writer.write_all(s).map_err(TransformError::Io)?;
            writer.write_all(&[0u8]).map_err(TransformError::Io)?; // null terminator
            Ok(s.len() + 1)
        }
        TypeExpr::List(inner, _) => {
            if matches!(inner.as_ref(), TypeExpr::U8(_)) {
                if let Value::List(items) = value {
                    let bytes: Vec<u8> = items
                        .iter()
                        .map(|v| match v {
                            Value::Int(i) => Ok(*i as u8),
                            _ => Err(TransformError::TypeMismatch {
                                expected: "int (u8)".into(),
                                got: v.type_name().to_string(),
                            }),
                        })
                        .collect::<Result<_, _>>()?;
                    writer.write_all(&bytes).map_err(TransformError::Io)?;
                    Ok(bytes.len())
                } else {
                    Err(TransformError::TypeMismatch {
                        expected: "list".into(),
                        got: value.type_name().to_string(),
                    })
                }
            } else {
                Err(TransformError::Codec(
                    "binary writer: lists of non-u8 types not supported".to_string(),
                ))
            }
        }
        _ => Err(TransformError::Codec(format!(
            "binary writer: unsupported type {:?}",
            type_expr
        ))),
    }
}

fn value_to_i64(value: &Value) -> Result<i64, TransformError> {
    match value {
        Value::Int(i) => Ok(*i),
        Value::BigInt(i) => Ok(*i as i64),
        Value::Float(f) => Ok(*f as i64),
        _ => Err(TransformError::TypeMismatch {
            expected: "number".into(),
            got: value.type_name().to_string(),
        }),
    }
}

fn value_to_u64(value: &Value) -> Result<u64, TransformError> {
    match value {
        Value::Int(i) => Ok(*i as u64),
        Value::BigInt(i) => Ok(*i as u64),
        Value::Float(f) => Ok(*f as u64),
        _ => Err(TransformError::TypeMismatch {
            expected: "number".into(),
            got: value.type_name().to_string(),
        }),
    }
}

fn value_to_f64(value: &Value) -> Result<f64, TransformError> {
    match value {
        Value::Float(f) => Ok(*f),
        Value::Int(i) => Ok(*i as f64),
        Value::BigInt(i) => Ok(*i as f64),
        _ => Err(TransformError::TypeMismatch {
            expected: "number".into(),
            got: value.type_name().to_string(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::span::Span;
    use crate::schema::struct_registry::StructField;
    use crate::transform::struct_parser;
    use indexmap::IndexMap;

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
    fn round_trip_simple_struct() {
        let def = make_struct(vec![("x", TypeExpr::U32(sp())), ("y", TypeExpr::U32(sp()))]);
        let encoding = EncodingConfig {
            default_endian: Endianness::Little,
            ..Default::default()
        };

        let mut map = IndexMap::new();
        map.insert("x".to_string(), Value::Int(42));
        map.insert("y".to_string(), Value::Int(100));
        let value = Value::Map(map);

        let mut buf = Vec::new();
        let written = write_struct(&value, &def, &encoding, &mut buf).unwrap();
        assert_eq!(written, 8);

        let parsed = struct_parser::parse_binary(&buf, &def, &encoding).unwrap();
        if let Value::Map(m) = parsed {
            assert_eq!(m.get("x"), Some(&Value::Int(42)));
            assert_eq!(m.get("y"), Some(&Value::Int(100)));
        } else {
            panic!("expected Map");
        }
    }

    #[test]
    fn round_trip_mixed_types() {
        let def = make_struct(vec![
            ("id", TypeExpr::U16(sp())),
            ("value", TypeExpr::F32(sp())),
            ("flag", TypeExpr::Bool(sp())),
        ]);
        let encoding = EncodingConfig {
            default_endian: Endianness::Little,
            ..Default::default()
        };

        let mut map = IndexMap::new();
        map.insert("id".to_string(), Value::Int(1000));
        map.insert("value".to_string(), Value::Float(3.14));
        map.insert("flag".to_string(), Value::Bool(true));
        let value = Value::Map(map);

        let mut buf = Vec::new();
        write_struct(&value, &def, &encoding, &mut buf).unwrap();

        let parsed = struct_parser::parse_binary(&buf, &def, &encoding).unwrap();
        if let Value::Map(m) = parsed {
            assert_eq!(m.get("id"), Some(&Value::Int(1000)));
            assert_eq!(m.get("flag"), Some(&Value::Bool(true)));
            if let Some(Value::Float(f)) = m.get("value") {
                assert!((*f - 3.14).abs() < 0.01);
            } else {
                panic!("expected float");
            }
        } else {
            panic!("expected Map");
        }
    }

    #[test]
    fn round_trip_big_endian() {
        let def = make_struct(vec![("magic", TypeExpr::U32(sp()))]);
        let encoding = EncodingConfig {
            default_endian: Endianness::Big,
            ..Default::default()
        };

        let mut map = IndexMap::new();
        map.insert("magic".to_string(), Value::Int(0xDEADBEEF));
        let value = Value::Map(map);

        let mut buf = Vec::new();
        write_struct(&value, &def, &encoding, &mut buf).unwrap();
        assert_eq!(buf, 0xDEADBEEFu32.to_be_bytes());

        let parsed = struct_parser::parse_binary(&buf, &def, &encoding).unwrap();
        if let Value::Map(m) = parsed {
            assert_eq!(m.get("magic"), Some(&Value::Int(0xDEADBEEF)));
        } else {
            panic!("expected Map");
        }
    }
}
