//! Binary parser — reads bytes according to struct field definitions.

use crate::eval::value::Value;
use crate::lang::ast::TypeExpr;
use crate::schema::struct_registry::ResolvedStruct;
use crate::transform::error::TransformError;
use crate::transform::struct_parser::{EncodingConfig, Endianness, FieldEncoding};
use indexmap::IndexMap;

/// Binary data parser with cursor tracking.
pub struct BinaryParser<'a> {
    data: &'a [u8],
    cursor: usize,
    encoding: &'a EncodingConfig,
}

impl<'a> BinaryParser<'a> {
    pub fn new(data: &'a [u8], encoding: &'a EncodingConfig) -> Self {
        Self {
            data,
            cursor: 0,
            encoding,
        }
    }

    /// Parse a complete struct from the data.
    pub fn parse(
        data: &[u8],
        struct_def: &ResolvedStruct,
        encoding: &EncodingConfig,
    ) -> Result<Value, TransformError> {
        let mut parser = BinaryParser::new(data, encoding);
        parser.parse_struct(struct_def)
    }

    /// How many bytes remain.
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.cursor)
    }

    /// Parse one struct instance and return as Value::Map.
    pub fn parse_struct(&mut self, struct_def: &ResolvedStruct) -> Result<Value, TransformError> {
        let mut map = IndexMap::new();

        for field in &struct_def.fields {
            let field_encoding = self.encoding.field_overrides.get(&field.name);
            let endian = field_encoding
                .and_then(|e| e.endian)
                .unwrap_or(self.encoding.default_endian);

            // Check magic value assertion
            if let Some(ref fe) = field_encoding {
                if let Some(ref magic) = fe.magic {
                    let pos = self.cursor;
                    let value = self.read_type(&field.type_expr, endian)?;
                    let value_bytes = value_to_bytes(&value, &field.type_expr, endian);
                    if value_bytes != *magic {
                        return Err(TransformError::Codec(format!(
                            "magic value mismatch at offset {} for field '{}': expected {:?}, got {:?}",
                            pos, field.name, magic, value_bytes
                        )));
                    }
                    map.insert(field.name.clone(), value);
                    continue;
                }
            }

            let value = self.read_type(&field.type_expr, endian)?;
            map.insert(field.name.clone(), value);

            // Apply padding after
            if let Some(ref fe) = field_encoding {
                if let Some(pad) = fe.padding_after {
                    self.skip(pad)?;
                }
                if let Some(align) = fe.align {
                    let offset = self.cursor % align;
                    if offset != 0 {
                        self.skip(align - offset)?;
                    }
                }
            }
        }

        Ok(Value::Map(map))
    }

    fn read_type(
        &mut self,
        type_expr: &TypeExpr,
        endian: Endianness,
    ) -> Result<Value, TransformError> {
        match type_expr {
            TypeExpr::U8(_) => Ok(Value::Int(self.read_u8()? as i64)),
            TypeExpr::I8(_) => Ok(Value::Int(self.read_i8()? as i64)),
            TypeExpr::U16(_) => Ok(Value::Int(self.read_u16(endian)? as i64)),
            TypeExpr::I16(_) => Ok(Value::Int(self.read_i16(endian)? as i64)),
            TypeExpr::U32(_) => Ok(Value::Int(self.read_u32(endian)? as i64)),
            TypeExpr::I32(_) => Ok(Value::Int(self.read_i32(endian)? as i64)),
            TypeExpr::U64(_) => {
                let v = self.read_u64(endian)?;
                if v <= i64::MAX as u64 {
                    Ok(Value::Int(v as i64))
                } else {
                    Ok(Value::BigInt(v as i128))
                }
            }
            TypeExpr::I64(_) => Ok(Value::Int(self.read_i64(endian)?)),
            TypeExpr::F32(_) => Ok(Value::Float(self.read_f32(endian)? as f64)),
            TypeExpr::F64(_) => Ok(Value::Float(self.read_f64(endian)?)),
            TypeExpr::Bool(_) => {
                let b = self.read_u8()?;
                Ok(Value::Bool(b != 0))
            }
            TypeExpr::String(_) => {
                // Read null-terminated string by default
                let start = self.cursor;
                while self.cursor < self.data.len() && self.data[self.cursor] != 0 {
                    self.cursor += 1;
                }
                let s = String::from_utf8_lossy(&self.data[start..self.cursor]).to_string();
                if self.cursor < self.data.len() {
                    self.cursor += 1; // skip null terminator
                }
                Ok(Value::String(s))
            }
            TypeExpr::List(inner, _) => {
                // For list(u8), read remaining bytes as a list
                if matches!(inner.as_ref(), TypeExpr::U8(_)) {
                    let bytes: Vec<Value> = self.data[self.cursor..]
                        .iter()
                        .map(|&b| Value::Int(b as i64))
                        .collect();
                    self.cursor = self.data.len();
                    Ok(Value::List(bytes))
                } else {
                    Err(TransformError::Codec(
                        "binary parser: lists of non-u8 types require explicit count".to_string(),
                    ))
                }
            }
            _ => Err(TransformError::Codec(format!(
                "binary parser: unsupported type {:?}",
                type_expr
            ))),
        }
    }

    fn ensure_bytes(&self, n: usize) -> Result<(), TransformError> {
        if self.cursor + n > self.data.len() {
            Err(TransformError::Codec(format!(
                "unexpected EOF: need {} bytes at offset {}, but only {} remain",
                n,
                self.cursor,
                self.remaining()
            )))
        } else {
            Ok(())
        }
    }

    fn skip(&mut self, n: usize) -> Result<(), TransformError> {
        self.ensure_bytes(n)?;
        self.cursor += n;
        Ok(())
    }

    fn read_u8(&mut self) -> Result<u8, TransformError> {
        self.ensure_bytes(1)?;
        let v = self.data[self.cursor];
        self.cursor += 1;
        Ok(v)
    }

    fn read_i8(&mut self) -> Result<i8, TransformError> {
        Ok(self.read_u8()? as i8)
    }

    fn read_u16(&mut self, endian: Endianness) -> Result<u16, TransformError> {
        self.ensure_bytes(2)?;
        let bytes = &self.data[self.cursor..self.cursor + 2];
        self.cursor += 2;
        Ok(match endian {
            Endianness::Little => u16::from_le_bytes([bytes[0], bytes[1]]),
            Endianness::Big => u16::from_be_bytes([bytes[0], bytes[1]]),
        })
    }

    fn read_i16(&mut self, endian: Endianness) -> Result<i16, TransformError> {
        Ok(self.read_u16(endian)? as i16)
    }

    fn read_u32(&mut self, endian: Endianness) -> Result<u32, TransformError> {
        self.ensure_bytes(4)?;
        let bytes: [u8; 4] = self.data[self.cursor..self.cursor + 4].try_into().unwrap();
        self.cursor += 4;
        Ok(match endian {
            Endianness::Little => u32::from_le_bytes(bytes),
            Endianness::Big => u32::from_be_bytes(bytes),
        })
    }

    fn read_i32(&mut self, endian: Endianness) -> Result<i32, TransformError> {
        Ok(self.read_u32(endian)? as i32)
    }

    fn read_u64(&mut self, endian: Endianness) -> Result<u64, TransformError> {
        self.ensure_bytes(8)?;
        let bytes: [u8; 8] = self.data[self.cursor..self.cursor + 8].try_into().unwrap();
        self.cursor += 8;
        Ok(match endian {
            Endianness::Little => u64::from_le_bytes(bytes),
            Endianness::Big => u64::from_be_bytes(bytes),
        })
    }

    fn read_i64(&mut self, endian: Endianness) -> Result<i64, TransformError> {
        Ok(self.read_u64(endian)? as i64)
    }

    fn read_f32(&mut self, endian: Endianness) -> Result<f32, TransformError> {
        self.ensure_bytes(4)?;
        let bytes: [u8; 4] = self.data[self.cursor..self.cursor + 4].try_into().unwrap();
        self.cursor += 4;
        Ok(match endian {
            Endianness::Little => f32::from_le_bytes(bytes),
            Endianness::Big => f32::from_be_bytes(bytes),
        })
    }

    fn read_f64(&mut self, endian: Endianness) -> Result<f64, TransformError> {
        self.ensure_bytes(8)?;
        let bytes: [u8; 8] = self.data[self.cursor..self.cursor + 8].try_into().unwrap();
        self.cursor += 8;
        Ok(match endian {
            Endianness::Little => f64::from_le_bytes(bytes),
            Endianness::Big => f64::from_be_bytes(bytes),
        })
    }
}

/// Convert a value back to bytes (for magic value comparison).
fn value_to_bytes(value: &Value, type_expr: &TypeExpr, endian: Endianness) -> Vec<u8> {
    match (value, type_expr) {
        (Value::Int(i), TypeExpr::U8(_)) => vec![*i as u8],
        (Value::Int(i), TypeExpr::U16(_)) => match endian {
            Endianness::Little => (*i as u16).to_le_bytes().to_vec(),
            Endianness::Big => (*i as u16).to_be_bytes().to_vec(),
        },
        (Value::Int(i), TypeExpr::U32(_)) => match endian {
            Endianness::Little => (*i as u32).to_le_bytes().to_vec(),
            Endianness::Big => (*i as u32).to_be_bytes().to_vec(),
        },
        (Value::Int(i), TypeExpr::U64(_)) => match endian {
            Endianness::Little => (*i as u64).to_le_bytes().to_vec(),
            Endianness::Big => (*i as u64).to_be_bytes().to_vec(),
        },
        _ => vec![],
    }
}
