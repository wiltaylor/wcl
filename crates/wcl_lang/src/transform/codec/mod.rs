//! Codec system — format adapters that bridge between wire formats and WCL values.
//!
//! Each codec provides a Decoder (read) and Encoder (write) that operate on
//! the unified Event stream.

pub mod binary_codec;
pub mod csv_codec;
pub mod hcl_codec;
pub mod json;
pub mod msgpack;
pub mod text_codec;
pub mod toml_codec;
pub mod xml;
pub mod yaml;

use crate::transform::error::TransformError;
use crate::transform::event::Event;
use indexmap::IndexMap;
use std::io::{Read, Write};

use crate::eval::value::Value;

/// A decoder reads from an input source and produces Events.
pub trait Decoder {
    /// Read the next event from the input.
    fn next_event(&mut self) -> Result<Event, TransformError>;
}

/// An encoder consumes Events and writes to an output sink.
pub trait Encoder {
    /// Write an event to the output.
    fn write_event(&mut self, event: &Event) -> Result<(), TransformError>;
    /// Flush any buffered output and finalize.
    fn finish(&mut self) -> Result<(), TransformError>;
}

/// Configuration options for a codec.
pub type CodecOptions = IndexMap<String, Value>;

/// Factory function for creating a decoder.
type DecoderFactory = Box<dyn Fn(Box<dyn Read>, &CodecOptions) -> Box<dyn Decoder>>;
/// Factory function for creating an encoder.
type EncoderFactory = Box<dyn Fn(Box<dyn Write>, &CodecOptions) -> Box<dyn Encoder>>;

/// Registry of available codecs.
pub struct CodecRegistry {
    decoders: IndexMap<String, DecoderFactory>,
    encoders: IndexMap<String, EncoderFactory>,
}

impl CodecRegistry {
    pub fn new() -> Self {
        let mut reg = Self {
            decoders: IndexMap::new(),
            encoders: IndexMap::new(),
        };
        reg.register_builtins();
        reg
    }

    fn register_builtins(&mut self) {
        // JSON
        self.decoders.insert(
            "json".into(),
            Box::new(|reader, _opts| Box::new(json::JsonDecoder::new(reader))),
        );
        self.encoders.insert(
            "json".into(),
            Box::new(|writer, opts| {
                let pretty = opts
                    .get("pretty")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                Box::new(json::JsonEncoder::new(writer, pretty))
            }),
        );
    }

    /// Create a decoder for the named codec.
    pub fn decoder(
        &self,
        name: &str,
        reader: Box<dyn Read>,
        options: &CodecOptions,
    ) -> Result<Box<dyn Decoder>, TransformError> {
        let factory = self
            .decoders
            .get(name)
            .ok_or_else(|| TransformError::UnknownCodec(name.to_string()))?;
        Ok(factory(reader, options))
    }

    /// Create an encoder for the named codec.
    pub fn encoder(
        &self,
        name: &str,
        writer: Box<dyn Write>,
        options: &CodecOptions,
    ) -> Result<Box<dyn Encoder>, TransformError> {
        let factory = self
            .encoders
            .get(name)
            .ok_or_else(|| TransformError::UnknownCodec(name.to_string()))?;
        Ok(factory(writer, options))
    }

    /// List available codec names.
    pub fn available(&self) -> Vec<&str> {
        // Include all codecs supported by the execute() function
        SUPPORTED_CODECS.to_vec()
    }
}

impl Default for CodecRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// All codec names supported by the transform engine.
pub const SUPPORTED_CODECS: &[&str] = &[
    "json", "yaml", "csv", "toml", "hcl", "xml", "msgpack", "binary", "text",
];

/// Decode an entire input into a list of records (each record is a Value::Map).
pub fn decode_all(decoder: &mut dyn Decoder) -> Result<Vec<Value>, TransformError> {
    let mut records = Vec::new();
    let mut current_map: Option<IndexMap<String, Value>> = None;
    let mut in_sequence = false;

    loop {
        match decoder.next_event()? {
            Event::Eof => break,
            Event::EnterSeq(_) => {
                in_sequence = true;
            }
            Event::ExitSeq => {
                in_sequence = false;
            }
            Event::EnterMap(_) => {
                current_map = Some(IndexMap::new());
            }
            Event::ExitMap => {
                if let Some(map) = current_map.take() {
                    records.push(Value::Map(map));
                }
            }
            Event::Scalar(key, value) => {
                if let Some(ref mut map) = current_map {
                    if let Some(k) = key {
                        map.insert(k, value);
                    }
                } else if !in_sequence {
                    // Top-level scalar — wrap in a single-record
                    let mut m = IndexMap::new();
                    if let Some(k) = key {
                        m.insert(k, value);
                    }
                    records.push(Value::Map(m));
                }
            }
        }
    }

    Ok(records)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codec_registry_has_all_codecs() {
        let reg = CodecRegistry::new();
        let avail = reg.available();
        for codec in SUPPORTED_CODECS {
            assert!(avail.contains(codec), "missing codec: {}", codec);
        }
    }
}
