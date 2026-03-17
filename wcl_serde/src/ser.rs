use serde::ser::{self, Serialize, SerializeSeq, SerializeMap, SerializeStruct, SerializeTuple, SerializeTupleStruct, SerializeStructVariant, SerializeTupleVariant};
use crate::error::Error;

pub struct Serializer {
    output: String,
    indent: usize,
    pretty: bool,
}

impl Serializer {
    pub fn new(pretty: bool) -> Self {
        Serializer { output: String::new(), indent: 0, pretty }
    }

    pub fn into_output(self) -> String { self.output }

    fn write_indent(&mut self) {
        if self.pretty {
            for _ in 0..self.indent {
                self.output.push_str("    ");
            }
        }
    }

    fn write_newline(&mut self) {
        if self.pretty {
            self.output.push('\n');
        }
    }
}

impl<'a> ser::Serializer for &'a mut Serializer {
    type Ok = ();
    type Error = Error;
    type SerializeSeq = SeqSerializer<'a>;
    type SerializeTuple = SeqSerializer<'a>;
    type SerializeTupleStruct = SeqSerializer<'a>;
    type SerializeTupleVariant = SeqSerializer<'a>;
    type SerializeMap = MapSerializer<'a>;
    type SerializeStruct = StructSerializer<'a>;
    type SerializeStructVariant = StructSerializer<'a>;

    fn serialize_bool(self, v: bool) -> Result<(), Error> {
        self.output.push_str(if v { "true" } else { "false" });
        Ok(())
    }

    fn serialize_i8(self, v: i8) -> Result<(), Error> { self.serialize_i64(v as i64) }
    fn serialize_i16(self, v: i16) -> Result<(), Error> { self.serialize_i64(v as i64) }
    fn serialize_i32(self, v: i32) -> Result<(), Error> { self.serialize_i64(v as i64) }

    fn serialize_i64(self, v: i64) -> Result<(), Error> {
        self.output.push_str(&v.to_string());
        Ok(())
    }

    fn serialize_u8(self, v: u8) -> Result<(), Error> { self.serialize_u64(v as u64) }
    fn serialize_u16(self, v: u16) -> Result<(), Error> { self.serialize_u64(v as u64) }
    fn serialize_u32(self, v: u32) -> Result<(), Error> { self.serialize_u64(v as u64) }

    fn serialize_u64(self, v: u64) -> Result<(), Error> {
        self.output.push_str(&v.to_string());
        Ok(())
    }

    fn serialize_f32(self, v: f32) -> Result<(), Error> { self.serialize_f64(v as f64) }

    fn serialize_f64(self, v: f64) -> Result<(), Error> {
        self.output.push_str(&v.to_string());
        Ok(())
    }

    fn serialize_char(self, v: char) -> Result<(), Error> {
        self.output.push('"');
        self.output.push(v);
        self.output.push('"');
        Ok(())
    }

    fn serialize_str(self, v: &str) -> Result<(), Error> {
        self.output.push('"');
        for c in v.chars() {
            match c {
                '"' => self.output.push_str("\\\""),
                '\\' => self.output.push_str("\\\\"),
                '\n' => self.output.push_str("\\n"),
                '\r' => self.output.push_str("\\r"),
                '\t' => self.output.push_str("\\t"),
                c => self.output.push(c),
            }
        }
        self.output.push('"');
        Ok(())
    }

    fn serialize_bytes(self, _v: &[u8]) -> Result<(), Error> {
        Err(Error::Message("bytes not supported".to_string()))
    }

    fn serialize_none(self) -> Result<(), Error> {
        self.output.push_str("null");
        Ok(())
    }

    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<(), Error> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<(), Error> {
        self.output.push_str("null");
        Ok(())
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<(), Error> {
        self.serialize_unit()
    }

    fn serialize_unit_variant(self, _name: &'static str, _variant_index: u32, variant: &'static str) -> Result<(), Error> {
        self.serialize_str(variant)
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(self, _name: &'static str, value: &T) -> Result<(), Error> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(self, _name: &'static str, _variant_index: u32, _variant: &'static str, value: &T) -> Result<(), Error> {
        value.serialize(self)
    }

    fn serialize_seq(self, _len: Option<usize>) -> Result<Self::SerializeSeq, Error> {
        self.output.push('[');
        Ok(SeqSerializer { ser: self, first: true })
    }

    fn serialize_tuple(self, len: usize) -> Result<Self::SerializeTuple, Error> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_struct(self, _name: &'static str, len: usize) -> Result<Self::SerializeTupleStruct, Error> {
        self.serialize_seq(Some(len))
    }

    fn serialize_tuple_variant(self, _name: &'static str, _variant_index: u32, _variant: &'static str, len: usize) -> Result<Self::SerializeTupleVariant, Error> {
        self.serialize_seq(Some(len))
    }

    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap, Error> {
        self.output.push('{');
        if self.pretty { self.output.push('\n'); }
        self.indent += 1;
        Ok(MapSerializer { ser: self, first: true })
    }

    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct, Error> {
        self.output.push('{');
        if self.pretty { self.output.push('\n'); }
        self.indent += 1;
        Ok(StructSerializer { ser: self, first: true })
    }

    fn serialize_struct_variant(self, _name: &'static str, _variant_index: u32, _variant: &'static str, _len: usize) -> Result<Self::SerializeStructVariant, Error> {
        self.output.push('{');
        if self.pretty { self.output.push('\n'); }
        self.indent += 1;
        Ok(StructSerializer { ser: self, first: true })
    }
}

pub struct SeqSerializer<'a> {
    ser: &'a mut Serializer,
    first: bool,
}

impl<'a> SerializeSeq for SeqSerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        if !self.first { self.ser.output.push_str(", "); }
        self.first = false;
        value.serialize(&mut *self.ser)
    }

    fn end(self) -> Result<(), Error> {
        self.ser.output.push(']');
        Ok(())
    }
}

impl<'a> SerializeTuple for SeqSerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<(), Error> { SerializeSeq::end(self) }
}

impl<'a> SerializeTupleStruct for SeqSerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<(), Error> { SerializeSeq::end(self) }
}

impl<'a> SerializeTupleVariant for SeqSerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<(), Error> { SerializeSeq::end(self) }
}

pub struct MapSerializer<'a> {
    ser: &'a mut Serializer,
    first: bool,
}

impl<'a> SerializeMap for MapSerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<(), Error> {
        self.ser.write_indent();
        key.serialize(&mut *self.ser)?;
        Ok(())
    }

    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Error> {
        self.ser.output.push_str(" = ");
        value.serialize(&mut *self.ser)?;
        self.ser.write_newline();
        self.first = false;
        Ok(())
    }

    fn end(self) -> Result<(), Error> {
        self.ser.indent -= 1;
        self.ser.write_indent();
        self.ser.output.push('}');
        Ok(())
    }
}

pub struct StructSerializer<'a> {
    ser: &'a mut Serializer,
    first: bool,
}

impl<'a> SerializeStruct for StructSerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, key: &'static str, value: &T) -> Result<(), Error> {
        self.ser.write_indent();
        self.ser.output.push_str(key);
        self.ser.output.push_str(" = ");
        value.serialize(&mut *self.ser)?;
        self.ser.write_newline();
        self.first = false;
        Ok(())
    }

    fn end(self) -> Result<(), Error> {
        self.ser.indent -= 1;
        self.ser.write_indent();
        self.ser.output.push('}');
        Ok(())
    }
}

impl<'a> SerializeStructVariant for StructSerializer<'a> {
    type Ok = ();
    type Error = Error;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, key: &'static str, value: &T) -> Result<(), Error> {
        SerializeStruct::serialize_field(self, key, value)
    }

    fn end(self) -> Result<(), Error> { SerializeStruct::end(self) }
}
