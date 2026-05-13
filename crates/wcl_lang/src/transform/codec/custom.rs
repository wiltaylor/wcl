//! WCL-authored custom codecs.
//!
//! Custom codecs are hosted by Rust but implemented with WCL lambdas. The
//! tokenizer receives a seekable source cursor and emits token maps. The parser
//! receives a seekable token cursor and emits record maps.

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
    pub parser: FunctionValue,
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

    Ok(CustomCodec {
        name: name.to_string(),
        mode,
        tokenizer: required_function(name, block, "tokenizer")?,
        parser: required_function(name, block, "parser")?,
        encoder: required_function(name, block, "encoder")?,
        encoder_all: optional_function(name, block, "encoder_all")?,
        helpers,
    })
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
    let mut data = Vec::new();
    reader.read_to_end(&mut data).map_err(TransformError::Io)?;

    let tokens = tokenize(&data, codec)?;
    parse_records(tokens, codec)
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
                if let Some(message) = error_message(&map) {
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

fn parse_records(tokens: Vec<Value>, codec: &CustomCodec) -> Result<Vec<Value>, TransformError> {
    let token_cursor = Arc::new(Mutex::new(TokenCursor::new(tokens)));
    let (cursor, builtins) = token_cursor_runtime(token_cursor.clone());
    let mut records = Vec::new();

    loop {
        let before = token_cursor.lock().unwrap().pos;
        let value = call_codec_lambda(codec, &codec.parser, &[cursor.clone()], builtins.clone())?;
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
                if let Some(message) = error_message(&map) {
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
                return Err(TransformError::Codec(format!(
                    "custom codec '{}' parser must return record map or null, got {}",
                    codec.name,
                    other.type_name()
                )));
            }
        }
    }

    Ok(records)
}

fn call_codec_lambda(
    codec: &CustomCodec,
    func: &FunctionValue,
    args: &[Value],
    builtins: HashMap<String, BuiltinFn>,
) -> Result<Value, TransformError> {
    crate::eval::evaluator::call_lambda_with_env(func, args, &builtins, &codec.helpers)
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

fn error_message(map: &IndexMap<String, Value>) -> Option<String> {
    match map.get("error") {
        Some(Value::String(s)) => Some(s.clone()),
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
        let c = cursor;
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
        let c = cursor;
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
