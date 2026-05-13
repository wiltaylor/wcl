//! WCL-authored custom codecs.
//!
//! Custom codecs are hosted by Rust but implemented with WCL lambdas. The
//! tokenizer receives a seekable source cursor and emits token maps. The parser
//! receives a seekable token cursor and emits WCL record values.

use crate::eval::functions::BuiltinFn;
use crate::eval::value::{BlockRef, FunctionBody, FunctionValue, LambdaAttrs, Value};
use crate::lang::ast::{BodyItem, DocItem, InlineId};
use crate::transform::error::TransformError;
use indexmap::IndexMap;
use regex::Regex;
use std::collections::HashMap;
use std::io::{Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

static CURSOR_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CustomCodecMode {
    Text,
    Bytes,
}

#[derive(Debug, Clone)]
pub struct CustomCodec {
    pub name: String,
    pub mode: CustomCodecMode,
    pub tokenizer: FunctionValue,
    pub parser: Option<FunctionValue>,
    pub parser_all: Option<FunctionValue>,
    pub encoder: FunctionValue,
    pub encoder_all: Option<FunctionValue>,
    pub helpers: HashMap<String, FunctionValue>,
}

#[derive(Debug, Default, Clone)]
pub struct CustomCodecRegistry {
    codecs: HashMap<String, CustomCodec>,
}

impl CustomCodecRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, codec: CustomCodec) -> Result<(), TransformError> {
        if super::SUPPORTED_CODECS.contains(&codec.name.as_str()) {
            return Err(TransformError::Codec(format!(
                "custom codec '{}' collides with a built-in codec",
                codec.name
            )));
        }
        if self.codecs.insert(codec.name.clone(), codec).is_some() {
            return Err(TransformError::Codec("duplicate custom codec".into()));
        }
        Ok(())
    }

    pub fn insert_standard(&mut self, codec: CustomCodec) -> Result<(), TransformError> {
        if self.codecs.insert(codec.name.clone(), codec).is_some() {
            return Err(TransformError::Codec("duplicate standard codec".into()));
        }
        Ok(())
    }

    pub fn get(&self, name: &str) -> Option<&CustomCodec> {
        self.codecs.get(name)
    }
}

pub fn standard_registry() -> Result<CustomCodecRegistry, TransformError> {
    registry_from_source(crate::standard_lib::CODECS_LIBRARY_WCL, true)
}

pub fn registry_from_source(
    source: &str,
    standard: bool,
) -> Result<CustomCodecRegistry, TransformError> {
    let doc = crate::parse(source, crate::ParseOptions::default());
    if doc.has_errors() {
        let messages = doc
            .errors()
            .into_iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(TransformError::Codec(format!(
            "failed to load standard codecs: {}",
            messages
        )));
    }
    registry_from_document(&doc, standard)
}

pub fn registry_from_document(
    doc: &crate::Document,
    standard: bool,
) -> Result<CustomCodecRegistry, TransformError> {
    let mut registry = CustomCodecRegistry::new();
    let helpers: HashMap<String, FunctionValue> = doc
        .values
        .iter()
        .filter_map(|(name, value)| match value {
            Value::Function(func) => Some((name.clone(), func.clone())),
            _ => None,
        })
        .collect();

    for item in &doc.ast.items {
        let DocItem::Body(BodyItem::Block(block)) = item else {
            continue;
        };
        if block.kind.name != "codec" {
            continue;
        }
        let Some(codec_name) = block.inline_id.as_ref().and_then(|id| match id {
            InlineId::Literal(lit) => Some(lit.value.clone()),
            InlineId::Interpolated(_) => None,
        }) else {
            return Err(TransformError::Codec(
                "codec block requires a literal inline id".into(),
            ));
        };
        let value = doc.values.get(&codec_name).ok_or_else(|| {
            TransformError::Codec(format!("codec '{}' was not evaluated", codec_name))
        })?;
        let Value::BlockRef(codec_ref) = value else {
            return Err(TransformError::Codec(format!(
                "codec '{}' did not evaluate to a block",
                codec_name
            )));
        };

        let codec = custom_codec_from_block(&codec_name, codec_ref, helpers.clone())?;
        if standard {
            registry.insert_standard(codec)?;
        } else {
            registry.insert(codec)?;
        }
    }

    Ok(registry)
}

pub fn custom_codec_from_block(
    name: &str,
    block: &BlockRef,
    helpers: HashMap<String, FunctionValue>,
) -> Result<CustomCodec, TransformError> {
    let mode = match block.attributes.get("mode") {
        Some(Value::Symbol(s)) if s == "text" => CustomCodecMode::Text,
        Some(Value::Symbol(s)) if s == "bytes" => CustomCodecMode::Bytes,
        Some(v) => {
            return Err(TransformError::Codec(format!(
                "codec '{}' mode must be :text or :bytes, got {}",
                name,
                v.type_name()
            )))
        }
        None => CustomCodecMode::Text,
    };

    let codec = CustomCodec {
        name: name.to_string(),
        mode,
        tokenizer: required_function(name, block, "tokenizer")?,
        parser: optional_function(name, block, "parser")?,
        parser_all: optional_function(name, block, "parser_all")?,
        encoder: required_function(name, block, "encoder")?,
        encoder_all: optional_function(name, block, "encoder_all")?,
        helpers,
    };
    if codec.parser.is_none() && codec.parser_all.is_none() {
        return Err(TransformError::Codec(format!(
            "codec '{}' missing required 'parser' or 'parser_all'",
            name
        )));
    }
    Ok(codec)
}

fn required_function(
    name: &str,
    block: &BlockRef,
    attr: &str,
) -> Result<FunctionValue, TransformError> {
    match block.attributes.get(attr) {
        Some(Value::Function(func)) => Ok(func.clone()),
        Some(v) => Err(TransformError::Codec(format!(
            "codec '{}' attribute '{}' must be a lambda, got {}",
            name,
            attr,
            v.type_name()
        ))),
        None => Err(TransformError::Codec(format!(
            "codec '{}' missing required '{}'",
            name, attr
        ))),
    }
}

fn optional_function(
    name: &str,
    block: &BlockRef,
    attr: &str,
) -> Result<Option<FunctionValue>, TransformError> {
    match block.attributes.get(attr) {
        Some(Value::Function(func)) => Ok(Some(func.clone())),
        Some(v) => Err(TransformError::Codec(format!(
            "codec '{}' attribute '{}' must be a lambda, got {}",
            name,
            attr,
            v.type_name()
        ))),
        None => Ok(None),
    }
}

pub fn decode_custom_records(
    mut reader: impl Read,
    codec: &CustomCodec,
) -> Result<Vec<Value>, TransformError> {
    decode_custom_records_with_options(&mut reader, codec, &super::CodecOptions::new())
}

pub fn decode_custom_records_with_options(
    mut reader: impl Read,
    codec: &CustomCodec,
    options: &super::CodecOptions,
) -> Result<Vec<Value>, TransformError> {
    let mut data = Vec::new();
    reader.read_to_end(&mut data).map_err(TransformError::Io)?;

    let tokens = tokenize(&data, codec)?;
    parse_records(tokens, codec, options)
}

pub fn encode_custom_records(
    records: &[Value],
    codec: &CustomCodec,
    options: &super::CodecOptions,
    writer: &mut dyn Write,
) -> Result<(), TransformError> {
    let options = Value::Map(options.clone());

    if let Some(encoder_all) = &codec.encoder_all {
        let value = call_codec_encoder(
            codec,
            encoder_all,
            Value::List(records.to_vec()),
            options.clone(),
            "encoder_all",
        )?;
        write_encoded_value(&value, codec, writer)?;
        writer.flush().map_err(TransformError::Io)?;
        return Ok(());
    }

    for record in records {
        let value = call_codec_encoder(
            codec,
            &codec.encoder,
            record.clone(),
            options.clone(),
            "encoder",
        )?;
        write_encoded_value(&value, codec, writer)?;
    }
    writer.flush().map_err(TransformError::Io)?;
    Ok(())
}

fn call_codec_encoder(
    codec: &CustomCodec,
    func: &FunctionValue,
    value: Value,
    options: Value,
    attr: &str,
) -> Result<Value, TransformError> {
    match func.params.len() {
        1 => call_codec_lambda(codec, func, &[value], HashMap::new()),
        2 => call_codec_lambda(codec, func, &[value, options], HashMap::new()),
        n => Err(TransformError::Codec(format!(
            "codec '{}' attribute '{}' must accept 1 or 2 arguments, got {}",
            codec.name, attr, n
        ))),
    }
}

fn write_encoded_value(
    value: &Value,
    codec: &CustomCodec,
    writer: &mut dyn Write,
) -> Result<(), TransformError> {
    match codec.mode {
        CustomCodecMode::Text => {
            let Value::String(s) = value else {
                return Err(TransformError::Codec(format!(
                    "custom codec '{}' encoder must return string in text mode, got {}",
                    codec.name,
                    value.type_name()
                )));
            };
            writer.write_all(s.as_bytes()).map_err(TransformError::Io)?;
        }
        CustomCodecMode::Bytes => {
            let bytes = value_to_bytes(value).map_err(|e| {
                TransformError::Codec(format!("custom codec '{}' encoder: {}", codec.name, e))
            })?;
            writer.write_all(&bytes).map_err(TransformError::Io)?;
        }
    }
    Ok(())
}

fn tokenize(data: &[u8], codec: &CustomCodec) -> Result<Vec<Value>, TransformError> {
    let source = Arc::new(Mutex::new(SourceCursor::new(data)));
    let (cursor, builtins) = source_cursor_runtime(source.clone(), codec.mode);
    let mut tokens = Vec::new();

    loop {
        let before = source.lock().unwrap().pos;
        let value =
            call_codec_lambda(codec, &codec.tokenizer, &[cursor.clone()], builtins.clone())?;
        match value {
            Value::Null => {
                if !source.lock().unwrap().eof() {
                    return Err(TransformError::Codec(format!(
                        "custom codec '{}' tokenizer returned null before EOF at byte {}",
                        codec.name, before
                    )));
                }
                break;
            }
            Value::Map(map) => {
                if let Some(message) = codec_error_message(&map) {
                    return Err(TransformError::Codec(format!(
                        "custom codec '{}' tokenizer error at byte {}: {}",
                        codec.name, before, message
                    )));
                }
                validate_token(&codec.name, &map)?;
                let after = source.lock().unwrap().pos;
                if after == before {
                    return Err(TransformError::Codec(format!(
                        "custom codec '{}' tokenizer did not advance at byte {}",
                        codec.name, before
                    )));
                }
                tokens.push(Value::Map(map));
            }
            other => {
                return Err(TransformError::Codec(format!(
                    "custom codec '{}' tokenizer must return token map or null, got {}",
                    codec.name,
                    other.type_name()
                )));
            }
        }
    }

    Ok(tokens)
}

fn parse_records(
    tokens: Vec<Value>,
    codec: &CustomCodec,
    options: &super::CodecOptions,
) -> Result<Vec<Value>, TransformError> {
    let token_cursor = Arc::new(Mutex::new(TokenCursor::new(tokens)));
    let (cursor, builtins) = token_cursor_runtime(token_cursor.clone());
    let options = Value::Map(options.clone());

    if let Some(parser_all) = &codec.parser_all {
        let value = call_codec_parser(
            codec,
            parser_all,
            cursor,
            options.clone(),
            builtins,
            "parser_all",
        )?;
        if let Value::Map(map) = &value {
            if let Some(message) = codec_error_message(map) {
                return Err(TransformError::Codec(format!(
                    "custom codec '{}' parser_all error at token 0: {}",
                    codec.name, message
                )));
            }
        }
        if !token_cursor.lock().unwrap().eof() {
            return Err(TransformError::Codec(format!(
                "custom codec '{}' parser_all did not consume all tokens",
                codec.name
            )));
        }
        let Value::List(records) = value else {
            return Err(TransformError::Codec(format!(
                "custom codec '{}' parser_all must return a list of records, got {}",
                codec.name,
                value.type_name()
            )));
        };
        for record in &records {
            if let Value::Map(map) = record {
                if let Some(message) = codec_error_message(map) {
                    return Err(TransformError::Codec(format!(
                        "custom codec '{}' parser_all record error: {}",
                        codec.name, message
                    )));
                }
            }
        }
        return Ok(records);
    }

    let parser = codec.parser.as_ref().ok_or_else(|| {
        TransformError::Codec(format!(
            "custom codec '{}' missing required 'parser'",
            codec.name
        ))
    })?;
    let mut records = Vec::new();

    loop {
        let before = token_cursor.lock().unwrap().pos;
        let value = call_codec_parser(
            codec,
            parser,
            cursor.clone(),
            options.clone(),
            builtins.clone(),
            "parser",
        )?;
        match value {
            Value::Null => {
                if !token_cursor.lock().unwrap().eof() {
                    return Err(TransformError::Codec(format!(
                        "custom codec '{}' parser returned null before EOF at token {}",
                        codec.name, before
                    )));
                }
                break;
            }
            Value::Map(map) => {
                if let Some(message) = codec_error_message(&map) {
                    return Err(TransformError::Codec(format!(
                        "custom codec '{}' parser error at token {}: {}",
                        codec.name, before, message
                    )));
                }
                let after = token_cursor.lock().unwrap().pos;
                if after == before {
                    return Err(TransformError::Codec(format!(
                        "custom codec '{}' parser did not advance at token {}",
                        codec.name, before
                    )));
                }
                records.push(Value::Map(map));
            }
            other => {
                let after = token_cursor.lock().unwrap().pos;
                if after == before {
                    return Err(TransformError::Codec(format!(
                        "custom codec '{}' parser did not advance at token {}",
                        codec.name, before
                    )));
                }
                records.push(other);
            }
        }
    }

    Ok(records)
}

fn call_codec_parser(
    codec: &CustomCodec,
    func: &FunctionValue,
    cursor: Value,
    options: Value,
    builtins: HashMap<String, BuiltinFn>,
    attr: &str,
) -> Result<Value, TransformError> {
    match func.params.len() {
        1 => call_codec_lambda(codec, func, &[cursor], builtins),
        2 => call_codec_lambda(codec, func, &[cursor, options], builtins),
        n => Err(TransformError::Codec(format!(
            "codec '{}' attribute '{}' must accept 1 or 2 arguments, got {}",
            codec.name, attr, n
        ))),
    }
}

fn call_codec_lambda(
    codec: &CustomCodec,
    func: &FunctionValue,
    args: &[Value],
    builtins: HashMap<String, BuiltinFn>,
) -> Result<Value, TransformError> {
    crate::eval::evaluator::call_lambda_with_env_and_max_depth(
        func,
        args,
        &builtins,
        &codec.helpers,
        1024,
    )
        .map_err(|e| TransformError::Codec(format!("custom codec '{}': {}", codec.name, e)))
}

fn validate_token(name: &str, map: &IndexMap<String, Value>) -> Result<(), TransformError> {
    match map.get("kind") {
        Some(Value::Symbol(_)) => {}
        Some(v) => {
            return Err(TransformError::Codec(format!(
                "custom codec '{}' token kind must be a symbol, got {}",
                name,
                v.type_name()
            )));
        }
        None => {
            return Err(TransformError::Codec(format!(
                "custom codec '{}' token missing required field 'kind'",
                name
            )));
        }
    }
    for field in ["start", "end"] {
        match map.get(field) {
            Some(Value::Int(_)) => {}
            Some(v) => {
                return Err(TransformError::Codec(format!(
                    "custom codec '{}' token field '{}' must be int, got {}",
                    name,
                    field,
                    v.type_name()
                )));
            }
            None => {
                return Err(TransformError::Codec(format!(
                    "custom codec '{}' token missing required field '{}'",
                    name, field
                )));
            }
        }
    }
    if let Some(v) = map.get("text") {
        if !matches!(v, Value::String(_)) {
            return Err(TransformError::Codec(format!(
                "custom codec '{}' token field 'text' must be string, got {}",
                name,
                v.type_name()
            )));
        }
    }
    Ok(())
}

fn codec_error_message(map: &IndexMap<String, Value>) -> Option<String> {
    match (map.get("__wcl_codec_error"), map.get("message")) {
        (Some(Value::Bool(true)), Some(Value::String(s))) => Some(s.clone()),
        (Some(Value::Bool(true)), _) => Some("codec error".into()),
        _ => None,
    }
}

#[derive(Debug)]
struct SourceCursor {
    data: Vec<u8>,
    pos: usize,
}

impl SourceCursor {
    fn new(data: &[u8]) -> Self {
        Self {
            data: data.to_vec(),
            pos: 0,
        }
    }

    fn eof(&self) -> bool {
        self.pos >= self.data.len()
    }

    fn remaining(&self) -> &[u8] {
        &self.data[self.pos.min(self.data.len())..]
    }

    fn peek(&self, n: usize, mode: CustomCodecMode) -> Result<Value, String> {
        let end = self.pos.saturating_add(n).min(self.data.len());
        bytes_to_mode_value(&self.data[self.pos..end], mode)
    }

    fn take(&mut self, n: usize, mode: CustomCodecMode) -> Result<Value, String> {
        let end = self.pos.saturating_add(n).min(self.data.len());
        let value = bytes_to_mode_value(&self.data[self.pos..end], mode)?;
        self.pos = end;
        Ok(value)
    }

    fn text_remaining(&self, name: &str) -> Result<&str, String> {
        std::str::from_utf8(self.remaining()).map_err(|e| format!("{name}() needs UTF-8 text: {e}"))
    }
}

#[derive(Debug)]
struct TokenCursor {
    tokens: Vec<Value>,
    pos: usize,
}

impl TokenCursor {
    fn new(tokens: Vec<Value>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn eof(&self) -> bool {
        self.pos >= self.tokens.len()
    }
}

fn source_cursor_runtime(
    cursor: Arc<Mutex<SourceCursor>>,
    mode: CustomCodecMode,
) -> (Value, HashMap<String, BuiltinFn>) {
    let id = CURSOR_ID.fetch_add(1, Ordering::Relaxed);
    let prefix = format!("__wcl_source_cursor_{id}");
    let mut map = IndexMap::new();
    let arities = [
        ("pos", 0),
        ("len", 0),
        ("eof", 0),
        ("seek", 1),
        ("seek_to", 1),
        ("peek", 1),
        ("take", 1),
        ("match", 1),
        ("peek_match", 1),
        ("take_match", 1),
        ("take_until", 1),
    ];
    for (name, arity) in arities {
        map.insert(
            name.to_string(),
            builtin_value(format!("{prefix}_{name}"), arity),
        );
    }

    let mut builtins: HashMap<String, BuiltinFn> = HashMap::new();
    {
        let c = cursor.clone();
        builtins.insert(
            format!("{prefix}_pos"),
            Arc::new(move |_| Ok(Value::Int(c.lock().unwrap().pos as i64))),
        );
    }
    {
        let c = cursor.clone();
        builtins.insert(
            format!("{prefix}_len"),
            Arc::new(move |_| Ok(Value::Int(c.lock().unwrap().data.len() as i64))),
        );
    }
    {
        let c = cursor.clone();
        builtins.insert(
            format!("{prefix}_eof"),
            Arc::new(move |_| Ok(Value::Bool(c.lock().unwrap().eof()))),
        );
    }
    {
        let c = cursor.clone();
        builtins.insert(
            format!("{prefix}_seek"),
            Arc::new(move |args| {
                let delta = expect_int(args, 0, "seek")?;
                let mut c = c.lock().unwrap();
                c.pos = clamp_pos(c.pos as i64 + delta, c.data.len());
                Ok(Value::Int(c.pos as i64))
            }),
        );
    }
    {
        let c = cursor.clone();
        builtins.insert(
            format!("{prefix}_seek_to"),
            Arc::new(move |args| {
                let pos = expect_int(args, 0, "seek_to")?;
                let mut c = c.lock().unwrap();
                c.pos = clamp_pos(pos, c.data.len());
                Ok(Value::Int(c.pos as i64))
            }),
        );
    }
    {
        let c = cursor.clone();
        builtins.insert(
            format!("{prefix}_peek"),
            Arc::new(move |args| {
                let n = expect_nonnegative_usize(args, 0, "peek")?;
                c.lock().unwrap().peek(n, mode)
            }),
        );
    }
    {
        let c = cursor.clone();
        builtins.insert(
            format!("{prefix}_take"),
            Arc::new(move |args| {
                let n = expect_nonnegative_usize(args, 0, "take")?;
                c.lock().unwrap().take(n, mode)
            }),
        );
    }
    {
        let c = cursor.clone();
        builtins.insert(
            format!("{prefix}_match"),
            Arc::new(move |args| {
                let needle = args
                    .first()
                    .ok_or_else(|| "match() expects 1 argument".to_string())?;
                let c = c.lock().unwrap();
                source_matches(&c, mode, needle).map(Value::Bool)
            }),
        );
    }
    {
        let c = cursor.clone();
        builtins.insert(
            format!("{prefix}_peek_match"),
            Arc::new(move |args| {
                let pattern = expect_pattern(args, 0, "peek_match")?;
                let c = c.lock().unwrap();
                source_regex_match(&c, pattern, "peek_match")
                    .map(|m| m.map(Value::String).unwrap_or(Value::Null))
            }),
        );
    }
    {
        let c = cursor.clone();
        builtins.insert(
            format!("{prefix}_take_match"),
            Arc::new(move |args| {
                let pattern = expect_pattern(args, 0, "take_match")?;
                let mut c = c.lock().unwrap();
                let Some(matched) = source_regex_match(&c, pattern, "take_match")? else {
                    return Ok(Value::Null);
                };
                c.pos = c.pos.saturating_add(matched.len()).min(c.data.len());
                Ok(Value::String(matched))
            }),
        );
    }
    {
        let c = cursor;
        builtins.insert(
            format!("{prefix}_take_until"),
            Arc::new(move |args| {
                let pattern = expect_pattern(args, 0, "take_until")?;
                let mut c = c.lock().unwrap();
                let haystack = c.text_remaining("take_until")?;
                let re = Regex::new(pattern).map_err(|e| format!("invalid pattern: {e}"))?;
                let end = re
                    .find(haystack)
                    .map(|m| m.start())
                    .unwrap_or(haystack.len());
                let value = haystack[..end].to_string();
                c.pos = c.pos.saturating_add(value.len()).min(c.data.len());
                Ok(Value::String(value))
            }),
        );
    }

    (Value::Map(map), builtins)
}

fn token_cursor_runtime(cursor: Arc<Mutex<TokenCursor>>) -> (Value, HashMap<String, BuiltinFn>) {
    let id = CURSOR_ID.fetch_add(1, Ordering::Relaxed);
    let prefix = format!("__wcl_token_cursor_{id}");
    let mut map = IndexMap::new();
    let arities = [
        ("pos", 0),
        ("len", 0),
        ("eof", 0),
        ("seek", 1),
        ("seek_to", 1),
        ("peek", 1),
        ("take", 1),
        ("match_kind", 1),
        ("peek_kind", 0),
        ("take_kind", 1),
        ("expect_kind", 2),
    ];
    for (name, arity) in arities {
        map.insert(
            name.to_string(),
            builtin_value(format!("{prefix}_{name}"), arity),
        );
    }

    let mut builtins: HashMap<String, BuiltinFn> = HashMap::new();
    {
        let c = cursor.clone();
        builtins.insert(
            format!("{prefix}_pos"),
            Arc::new(move |_| Ok(Value::Int(c.lock().unwrap().pos as i64))),
        );
    }
    {
        let c = cursor.clone();
        builtins.insert(
            format!("{prefix}_len"),
            Arc::new(move |_| Ok(Value::Int(c.lock().unwrap().tokens.len() as i64))),
        );
    }
    {
        let c = cursor.clone();
        builtins.insert(
            format!("{prefix}_eof"),
            Arc::new(move |_| Ok(Value::Bool(c.lock().unwrap().eof()))),
        );
    }
    {
        let c = cursor.clone();
        builtins.insert(
            format!("{prefix}_seek"),
            Arc::new(move |args| {
                let delta = expect_int(args, 0, "seek")?;
                let mut c = c.lock().unwrap();
                c.pos = clamp_pos(c.pos as i64 + delta, c.tokens.len());
                Ok(Value::Int(c.pos as i64))
            }),
        );
    }
    {
        let c = cursor.clone();
        builtins.insert(
            format!("{prefix}_seek_to"),
            Arc::new(move |args| {
                let pos = expect_int(args, 0, "seek_to")?;
                let mut c = c.lock().unwrap();
                c.pos = clamp_pos(pos, c.tokens.len());
                Ok(Value::Int(c.pos as i64))
            }),
        );
    }
    {
        let c = cursor.clone();
        builtins.insert(
            format!("{prefix}_peek"),
            Arc::new(move |args| {
                let n = expect_nonnegative_usize(args, 0, "peek")?;
                let c = c.lock().unwrap();
                Ok(tokens_slice_value(&c.tokens, c.pos, n))
            }),
        );
    }
    {
        let c = cursor.clone();
        builtins.insert(
            format!("{prefix}_take"),
            Arc::new(move |args| {
                let n = expect_nonnegative_usize(args, 0, "take")?;
                let mut c = c.lock().unwrap();
                let value = tokens_slice_value(&c.tokens, c.pos, n);
                c.pos = c.pos.saturating_add(n).min(c.tokens.len());
                Ok(value)
            }),
        );
    }
    {
        let c = cursor.clone();
        builtins.insert(
            format!("{prefix}_match_kind"),
            Arc::new(move |args| {
                let expected = match args.first() {
                    Some(Value::Symbol(s)) => s,
                    Some(v) => {
                        return Err(format!(
                            "match_kind() expects symbol, got {}",
                            v.type_name()
                        ))
                    }
                    None => return Err("match_kind() expects 1 argument".into()),
                };
                let c = c.lock().unwrap();
                let matched = c
                    .tokens
                    .get(c.pos)
                    .and_then(Value::as_map)
                    .and_then(|m| m.get("kind"))
                    .and_then(Value::as_symbol)
                    .map(|kind| kind == expected)
                    .unwrap_or(false);
                Ok(Value::Bool(matched))
            }),
        );
    }
    {
        let c = cursor.clone();
        builtins.insert(
            format!("{prefix}_peek_kind"),
            Arc::new(move |_| {
                let c = c.lock().unwrap();
                Ok(token_kind_at(&c, c.pos)
                    .map(Value::Symbol)
                    .unwrap_or(Value::Null))
            }),
        );
    }
    {
        let c = cursor.clone();
        builtins.insert(
            format!("{prefix}_take_kind"),
            Arc::new(move |args| {
                let expected = expect_symbol(args, 0, "take_kind")?;
                let mut c = c.lock().unwrap();
                if token_kind_at(&c, c.pos).as_deref() != Some(expected) {
                    return Ok(Value::Null);
                }
                let value = tokens_slice_value(&c.tokens, c.pos, 1);
                c.pos = c.pos.saturating_add(1).min(c.tokens.len());
                Ok(value)
            }),
        );
    }
    {
        let c = cursor;
        builtins.insert(
            format!("{prefix}_expect_kind"),
            Arc::new(move |args| {
                let expected = expect_symbol(args, 0, "expect_kind")?;
                let message = match args.get(1) {
                    Some(Value::String(s)) => s.clone(),
                    Some(v) => {
                        return Err(format!(
                            "expect_kind() argument 2 must be string, got {}",
                            v.type_name()
                        ))
                    }
                    None => return Err("expect_kind() expects argument 2".into()),
                };
                let mut c = c.lock().unwrap();
                if token_kind_at(&c, c.pos).as_deref() != Some(expected) {
                    return Ok(codec_error_value(message));
                }
                let value = tokens_slice_value(&c.tokens, c.pos, 1);
                c.pos = c.pos.saturating_add(1).min(c.tokens.len());
                Ok(value)
            }),
        );
    }

    (Value::Map(map), builtins)
}

fn builtin_value(name: String, arity: usize) -> Value {
    Value::Function(FunctionValue {
        params: (0..arity).map(|i| format!("arg{i}")).collect(),
        body: FunctionBody::Builtin(name),
        closure_scope: None,
        decorators: Vec::new(),
        lambda_attrs: LambdaAttrs::default(),
        param_types: vec![],
        return_type: None,
    })
}

fn bytes_to_mode_value(bytes: &[u8], mode: CustomCodecMode) -> Result<Value, String> {
    match mode {
        CustomCodecMode::Text => String::from_utf8(bytes.to_vec())
            .map(Value::String)
            .map_err(|e| format!("cursor read split invalid UTF-8: {}", e)),
        CustomCodecMode::Bytes => Ok(Value::List(
            bytes.iter().map(|b| Value::Int(i64::from(*b))).collect(),
        )),
    }
}

fn value_to_bytes(value: &Value) -> Result<Vec<u8>, String> {
    let Value::List(items) = value else {
        return Err(format!("expected list(i64), got {}", value.type_name()));
    };
    let mut bytes = Vec::with_capacity(items.len());
    for item in items {
        let Value::Int(i) = item else {
            return Err(format!("expected byte int, got {}", item.type_name()));
        };
        if !(0..=255).contains(i) {
            return Err(format!("byte value out of range: {}", i));
        }
        bytes.push(*i as u8);
    }
    Ok(bytes)
}

fn expect_int(args: &[Value], index: usize, name: &str) -> Result<i64, String> {
    match args.get(index) {
        Some(Value::Int(i)) => Ok(*i),
        Some(v) => Err(format!("{}() expects int, got {}", name, v.type_name())),
        None => Err(format!("{}() expects argument {}", name, index + 1)),
    }
}

fn expect_nonnegative_usize(args: &[Value], index: usize, name: &str) -> Result<usize, String> {
    let n = expect_int(args, index, name)?;
    if n < 0 {
        return Err(format!("{}() length must be non-negative", name));
    }
    Ok(n as usize)
}

fn expect_pattern<'a>(args: &'a [Value], index: usize, name: &str) -> Result<&'a str, String> {
    match args.get(index) {
        Some(Value::Pattern(pattern)) => Ok(pattern),
        Some(Value::String(pattern)) => Ok(pattern),
        Some(v) => Err(format!(
            "{name}() expects string or pattern, got {}",
            v.type_name()
        )),
        None => Err(format!("{name}() expects argument {}", index + 1)),
    }
}

fn expect_symbol<'a>(args: &'a [Value], index: usize, name: &str) -> Result<&'a str, String> {
    match args.get(index) {
        Some(Value::Symbol(symbol)) => Ok(symbol),
        Some(v) => Err(format!("{name}() expects symbol, got {}", v.type_name())),
        None => Err(format!("{name}() expects argument {}", index + 1)),
    }
}

fn clamp_pos(pos: i64, len: usize) -> usize {
    pos.clamp(0, len as i64) as usize
}

fn source_matches(
    cursor: &SourceCursor,
    mode: CustomCodecMode,
    needle: &Value,
) -> Result<bool, String> {
    match needle {
        Value::String(s) => Ok(cursor.remaining().starts_with(s.as_bytes())),
        Value::List(_) => {
            let bytes = value_to_bytes(needle)?;
            Ok(cursor.remaining().starts_with(&bytes))
        }
        Value::Pattern(pattern) => {
            if mode != CustomCodecMode::Text {
                return Ok(false);
            }
            let haystack = std::str::from_utf8(cursor.remaining())
                .map_err(|e| format!("match() needs UTF-8 text: {}", e))?;
            let re = Regex::new(pattern).map_err(|e| format!("invalid pattern: {}", e))?;
            Ok(re.find(haystack).map(|m| m.start() == 0).unwrap_or(false))
        }
        other => Err(format!(
            "match() expects string, byte list, or pattern, got {}",
            other.type_name()
        )),
    }
}

fn source_regex_match(
    cursor: &SourceCursor,
    pattern: &str,
    name: &str,
) -> Result<Option<String>, String> {
    let haystack = cursor.text_remaining(name)?;
    let re = Regex::new(pattern).map_err(|e| format!("invalid pattern: {e}"))?;
    Ok(re
        .find(haystack)
        .filter(|m| m.start() == 0)
        .map(|m| m.as_str().to_string()))
}

fn token_kind_at(cursor: &TokenCursor, pos: usize) -> Option<String> {
    cursor
        .tokens
        .get(pos)
        .and_then(Value::as_map)
        .and_then(|m| m.get("kind"))
        .and_then(Value::as_symbol)
        .map(str::to_string)
}

fn codec_error_value(message: String) -> Value {
    let mut map = IndexMap::new();
    map.insert("__wcl_codec_error".into(), Value::Bool(true));
    map.insert("message".into(), Value::String(message));
    Value::Map(map)
}

fn tokens_slice_value(tokens: &[Value], pos: usize, n: usize) -> Value {
    if n == 0 || pos >= tokens.len() {
        return Value::Null;
    }
    let end = pos.saturating_add(n).min(tokens.len());
    let slice = tokens[pos..end].to_vec();
    if n == 1 {
        slice.into_iter().next().unwrap_or(Value::Null)
    } else {
        Value::List(slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_with(
        source: &str,
        input: &str,
        codec_name: &str,
    ) -> Result<Vec<Value>, TransformError> {
        let registry = registry_from_source(source, false)?;
        let codec = registry.get(codec_name).expect("codec registered");
        decode_custom_records(input.as_bytes(), codec)
    }

    #[test]
    fn parser_can_return_scalar_records() {
        let source = r#"
codec chars {
    mode = :text
    tokenizer = cursor => cursor.eof() ? null : {
        let start = cursor.pos()
        let ch = cursor.take(1)
        { kind = :char, text = ch, start = start, end = cursor.pos(), value = ch }
    }
    parser = tokens => tokens.eof() ? null : {
        let token = tokens.take(1)
        token.value
    }
    encoder = record => record
}
"#;

        let records = decode_with(source, "ab", "chars").unwrap();
        assert_eq!(
            records,
            vec![Value::String("a".into()), Value::String("b".into())]
        );
    }

    #[test]
    fn parser_all_parses_full_stream_once() {
        let source = r#"
codec pairs {
    mode = :text
    tokenizer = cursor => cursor.eof() ? null : {
        let start = cursor.pos()
        let ch = cursor.take(1)
        { kind = :char, text = ch, start = start, end = cursor.pos(), value = ch }
    }
    parser_all = tokens => {
        let first = tokens.take(1)
        let second = tokens.take(1)
        [first.value + second.value]
    }
    encoder = record => record
}
"#;

        let records = decode_with(source, "ab", "pairs").unwrap();
        assert_eq!(records, vec![Value::String("ab".into())]);
    }

    #[test]
    fn parser_all_must_consume_all_tokens() {
        let source = r#"
codec incomplete {
    mode = :text
    tokenizer = cursor => cursor.eof() ? null : {
        let start = cursor.pos()
        let ch = cursor.take(1)
        { kind = :char, text = ch, start = start, end = cursor.pos(), value = ch }
    }
    parser_all = tokens => []
    encoder = record => record
}
"#;

        let err = decode_with(source, "x", "incomplete").unwrap_err();
        assert!(err
            .to_string()
            .contains("parser_all did not consume all tokens"));
    }

    #[test]
    fn error_key_is_valid_record_data() {
        let source = r#"
codec keyed {
    mode = :text
    tokenizer = cursor => cursor.eof() ? null : {
        let start = cursor.pos()
        let ch = cursor.take(1)
        { kind = :char, text = ch, start = start, end = cursor.pos(), value = ch }
    }
    parser = tokens => tokens.eof() ? null : {
        let token = tokens.take(1)
        { error = token.value }
    }
    encoder = record => record.error
}
"#;

        let records = decode_with(source, "x", "keyed").unwrap();
        let Value::Map(map) = &records[0] else {
            panic!("expected map");
        };
        assert_eq!(map.get("error"), Some(&Value::String("x".into())));
    }

    #[test]
    fn reserved_codec_error_shape_fails_parser() {
        let source = r#"
codec failing {
    mode = :text
    tokenizer = cursor => cursor.eof() ? null : {
        let start = cursor.pos()
        let ch = cursor.take(1)
        { kind = :char, text = ch, start = start, end = cursor.pos(), value = ch }
    }
    parser = tokens => tokens.eof() ? null : {
        let _token = tokens.take(1)
        { __wcl_codec_error = true message = "boom" }
    }
    encoder = record => ""
}
"#;

        let err = decode_with(source, "x", "failing").unwrap_err();
        assert!(err.to_string().contains("boom"));
    }

    #[test]
    fn source_cursor_regex_helpers_match_and_advance() {
        let source = r#"
codec words {
    mode = :text
    tokenizer = cursor => cursor.eof() ? null : {
        let start = cursor.pos()
        let prefix = cursor.peek_match("[A-Za-z]+")
        prefix == null ? {
            let skipped = cursor.take_until("[A-Za-z]+")
            { kind = :skip, text = skipped, start = start, end = cursor.pos() }
        } : {
            let word = cursor.take_match("[A-Za-z]+")
            { kind = :word, text = word, start = start, end = cursor.pos(), value = word }
        }
    }
    parser = tokens => tokens.eof() ? null : {
        let token = tokens.take(1)
        token.kind == :word ? token.value : ""
    }
    encoder = record => record
}
"#;

        let records = decode_with(source, "--alpha beta", "words").unwrap();
        assert_eq!(
            records,
            vec![
                Value::String("".into()),
                Value::String("alpha".into()),
                Value::String("".into()),
                Value::String("beta".into())
            ]
        );
    }

    #[test]
    fn token_cursor_kind_helpers_take_and_expect() {
        let source = r#"
codec simple {
    mode = :text
    tokenizer = cursor => cursor.eof() ? null : {
        let start = cursor.pos()
        let ch = cursor.take(1)
        { kind = ch == "," ? :comma : :word, text = ch, start = start, end = cursor.pos(), value = ch }
    }
    parser_all = tokens => {
        let first_kind = tokens.peek_kind()
        let first = tokens.take_kind(:word)
        let comma = tokens.expect_kind(:comma, "expected comma")
        (type_of(comma) == "map" && map_has(comma, "__wcl_codec_error")) ? comma : {
            let second = tokens.take_kind(:word)
            [first_kind, first.value, comma.kind, second.value]
        }
    }
    encoder = record => to_string(record)
}
"#;

        let records = decode_with(source, "a,b", "simple").unwrap();
        assert_eq!(
            records,
            vec![
                Value::Symbol("word".into()),
                Value::String("a".into()),
                Value::Symbol("comma".into()),
                Value::String("b".into())
            ]
        );

        let err = decode_with(source, "ab", "simple").unwrap_err();
        assert!(err.to_string().contains("expected comma"));
    }
}
