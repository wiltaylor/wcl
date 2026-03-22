use crate::eval::value::Value;
use crate::serde_impl::error::Error;
use indexmap::IndexMap;
use serde::de::{self, DeserializeSeed, EnumAccess, MapAccess, SeqAccess, VariantAccess, Visitor};

pub struct Deserializer {
    value: Value,
}

impl Deserializer {
    pub fn from_value(value: Value) -> Self {
        Deserializer { value }
    }
}

impl<'de> de::Deserializer<'de> for Deserializer {
    type Error = Error;

    fn deserialize_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.value {
            Value::String(s) => visitor.visit_string(s),
            Value::Int(i) => visitor.visit_i64(i),
            Value::Float(f) => visitor.visit_f64(f),
            Value::Bool(b) => visitor.visit_bool(b),
            Value::Null => visitor.visit_none(),
            Value::Identifier(s) => visitor.visit_string(s),
            Value::Symbol(s) => visitor.visit_string(s),
            Value::List(items) => {
                let seq = SeqDeserializer {
                    iter: items.into_iter(),
                };
                visitor.visit_seq(seq)
            }
            Value::Map(map) => {
                let map_de = MapDeserializer {
                    iter: map.into_iter(),
                    value: None,
                };
                visitor.visit_map(map_de)
            }
            Value::BlockRef(br) => {
                // Deserialize block as a map with id and attributes
                let mut map = IndexMap::new();
                if let Some(id) = br.id {
                    map.insert("id".to_string(), Value::String(id));
                }
                for (k, v) in br.attributes {
                    map.insert(k, v);
                }
                let map_de = MapDeserializer {
                    iter: map.into_iter(),
                    value: None,
                };
                visitor.visit_map(map_de)
            }
            Value::Set(items) => {
                let seq = SeqDeserializer {
                    iter: items.into_iter(),
                };
                visitor.visit_seq(seq)
            }
            Value::Function(_) => Err(Error::Message(
                "cannot deserialize function values".to_string(),
            )),
        }
    }

    fn deserialize_bool<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.value {
            Value::Bool(b) => visitor.visit_bool(b),
            _ => Err(Error::TypeMismatch {
                expected: "bool".to_string(),
                got: self.value.type_name().to_string(),
            }),
        }
    }

    fn deserialize_i8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.value {
            Value::Int(i) => visitor.visit_i8(i as i8),
            _ => Err(Error::TypeMismatch {
                expected: "i8".to_string(),
                got: self.value.type_name().to_string(),
            }),
        }
    }

    fn deserialize_i16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.value {
            Value::Int(i) => visitor.visit_i16(i as i16),
            _ => Err(Error::TypeMismatch {
                expected: "i16".to_string(),
                got: self.value.type_name().to_string(),
            }),
        }
    }

    fn deserialize_i32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.value {
            Value::Int(i) => visitor.visit_i32(i as i32),
            _ => Err(Error::TypeMismatch {
                expected: "i32".to_string(),
                got: self.value.type_name().to_string(),
            }),
        }
    }

    fn deserialize_i64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.value {
            Value::Int(i) => visitor.visit_i64(i),
            _ => Err(Error::TypeMismatch {
                expected: "i64".to_string(),
                got: self.value.type_name().to_string(),
            }),
        }
    }

    fn deserialize_u8<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.value {
            Value::Int(i) => visitor.visit_u8(i as u8),
            _ => Err(Error::TypeMismatch {
                expected: "u8".to_string(),
                got: self.value.type_name().to_string(),
            }),
        }
    }

    fn deserialize_u16<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.value {
            Value::Int(i) => visitor.visit_u16(i as u16),
            _ => Err(Error::TypeMismatch {
                expected: "u16".to_string(),
                got: self.value.type_name().to_string(),
            }),
        }
    }

    fn deserialize_u32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.value {
            Value::Int(i) => visitor.visit_u32(i as u32),
            _ => Err(Error::TypeMismatch {
                expected: "u32".to_string(),
                got: self.value.type_name().to_string(),
            }),
        }
    }

    fn deserialize_u64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.value {
            Value::Int(i) => visitor.visit_u64(i as u64),
            _ => Err(Error::TypeMismatch {
                expected: "u64".to_string(),
                got: self.value.type_name().to_string(),
            }),
        }
    }

    fn deserialize_f32<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.value {
            Value::Float(f) => visitor.visit_f32(f as f32),
            Value::Int(i) => visitor.visit_f32(i as f32),
            _ => Err(Error::TypeMismatch {
                expected: "f32".to_string(),
                got: self.value.type_name().to_string(),
            }),
        }
    }

    fn deserialize_f64<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.value {
            Value::Float(f) => visitor.visit_f64(f),
            Value::Int(i) => visitor.visit_f64(i as f64),
            _ => Err(Error::TypeMismatch {
                expected: "f64".to_string(),
                got: self.value.type_name().to_string(),
            }),
        }
    }

    fn deserialize_char<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.value {
            Value::String(s) if s.len() == 1 => visitor.visit_char(s.chars().next().unwrap()),
            _ => Err(Error::TypeMismatch {
                expected: "char".to_string(),
                got: self.value.type_name().to_string(),
            }),
        }
    }

    fn deserialize_str<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.value {
            Value::String(s) => visitor.visit_string(s),
            Value::Identifier(s) => visitor.visit_string(s),
            Value::Symbol(s) => visitor.visit_string(s),
            _ => Err(Error::TypeMismatch {
                expected: "string".to_string(),
                got: self.value.type_name().to_string(),
            }),
        }
    }

    fn deserialize_string<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_str(visitor)
    }

    fn deserialize_bytes<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, Error> {
        Err(Error::Message("bytes not supported".to_string()))
    }

    fn deserialize_byte_buf<V: Visitor<'de>>(self, _visitor: V) -> Result<V::Value, Error> {
        Err(Error::Message("byte_buf not supported".to_string()))
    }

    fn deserialize_option<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.value {
            Value::Null => visitor.visit_none(),
            _ => visitor.visit_some(self),
        }
    }

    fn deserialize_unit<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.value {
            Value::Null => visitor.visit_unit(),
            _ => Err(Error::TypeMismatch {
                expected: "null".to_string(),
                got: self.value.type_name().to_string(),
            }),
        }
    }

    fn deserialize_unit_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_unit(visitor)
    }

    fn deserialize_newtype_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        visitor: V,
    ) -> Result<V::Value, Error> {
        visitor.visit_newtype_struct(self)
    }

    fn deserialize_seq<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.value {
            Value::List(items) => {
                let seq = SeqDeserializer {
                    iter: items.into_iter(),
                };
                visitor.visit_seq(seq)
            }
            Value::Set(items) => {
                let seq = SeqDeserializer {
                    iter: items.into_iter(),
                };
                visitor.visit_seq(seq)
            }
            _ => Err(Error::TypeMismatch {
                expected: "list".to_string(),
                got: self.value.type_name().to_string(),
            }),
        }
    }

    fn deserialize_tuple<V: Visitor<'de>>(
        self,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_tuple_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _len: usize,
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_seq(visitor)
    }

    fn deserialize_map<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        match self.value {
            Value::Map(map) => {
                let map_de = MapDeserializer {
                    iter: map.into_iter(),
                    value: None,
                };
                visitor.visit_map(map_de)
            }
            Value::BlockRef(br) => {
                let mut map = IndexMap::new();
                if let Some(id) = br.id {
                    map.insert("id".to_string(), Value::String(id));
                }
                for (k, v) in br.attributes {
                    map.insert(k, v);
                }
                let map_de = MapDeserializer {
                    iter: map.into_iter(),
                    value: None,
                };
                visitor.visit_map(map_de)
            }
            _ => Err(Error::TypeMismatch {
                expected: "map".to_string(),
                got: self.value.type_name().to_string(),
            }),
        }
    }

    fn deserialize_struct<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _fields: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        self.deserialize_map(visitor)
    }

    fn deserialize_enum<V: Visitor<'de>>(
        self,
        _name: &'static str,
        _variants: &'static [&'static str],
        visitor: V,
    ) -> Result<V::Value, Error> {
        match self.value {
            Value::String(s) => visitor.visit_enum(StringIntoDeserializer(s)),
            _ => Err(Error::Message(
                "enum deserialization requires string".to_string(),
            )),
        }
    }

    fn deserialize_identifier<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        self.deserialize_str(visitor)
    }

    fn deserialize_ignored_any<V: Visitor<'de>>(self, visitor: V) -> Result<V::Value, Error> {
        visitor.visit_unit()
    }
}

// Seq deserializer
struct SeqDeserializer {
    iter: std::vec::IntoIter<Value>,
}

impl<'de> SeqAccess<'de> for SeqDeserializer {
    type Error = Error;

    fn next_element_seed<T: DeserializeSeed<'de>>(
        &mut self,
        seed: T,
    ) -> Result<Option<T::Value>, Error> {
        match self.iter.next() {
            Some(value) => seed.deserialize(Deserializer::from_value(value)).map(Some),
            None => Ok(None),
        }
    }
}

// Map deserializer
struct MapDeserializer {
    iter: indexmap::map::IntoIter<String, Value>,
    value: Option<Value>,
}

impl<'de> MapAccess<'de> for MapDeserializer {
    type Error = Error;

    fn next_key_seed<K: DeserializeSeed<'de>>(
        &mut self,
        seed: K,
    ) -> Result<Option<K::Value>, Error> {
        match self.iter.next() {
            Some((key, value)) => {
                self.value = Some(value);
                seed.deserialize(Deserializer::from_value(Value::String(key)))
                    .map(Some)
            }
            None => Ok(None),
        }
    }

    fn next_value_seed<V: DeserializeSeed<'de>>(&mut self, seed: V) -> Result<V::Value, Error> {
        let value = self
            .value
            .take()
            .ok_or_else(|| Error::Message("value missing".to_string()))?;
        seed.deserialize(Deserializer::from_value(value))
    }
}

// Wrapper used by deserialize_enum to implement EnumAccess for a variant name.
struct StringIntoDeserializer(String);

impl<'de> EnumAccess<'de> for StringIntoDeserializer {
    type Error = Error;
    type Variant = UnitVariant;

    fn variant_seed<V: DeserializeSeed<'de>>(
        self,
        seed: V,
    ) -> Result<(V::Value, Self::Variant), Error> {
        let val = seed.deserialize(Deserializer::from_value(Value::String(self.0)))?;
        Ok((val, UnitVariant))
    }
}

struct UnitVariant;

impl<'de> VariantAccess<'de> for UnitVariant {
    type Error = Error;

    fn unit_variant(self) -> Result<(), Error> {
        Ok(())
    }

    fn newtype_variant_seed<T: DeserializeSeed<'de>>(self, _seed: T) -> Result<T::Value, Error> {
        Err(Error::Message("expected unit variant".to_string()))
    }

    fn tuple_variant<V: Visitor<'de>>(self, _len: usize, _visitor: V) -> Result<V::Value, Error> {
        Err(Error::Message("expected unit variant".to_string()))
    }

    fn struct_variant<V: Visitor<'de>>(
        self,
        _fields: &'static [&'static str],
        _visitor: V,
    ) -> Result<V::Value, Error> {
        Err(Error::Message("expected unit variant".to_string()))
    }
}
