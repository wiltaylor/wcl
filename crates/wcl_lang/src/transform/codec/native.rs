//! Native codecs implemented by the host runtime.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Component, Path};

use indexmap::IndexMap;

use crate::eval::value::Value;
use crate::render::shapes::{
    parse_alignment_str, render_diagram_svg, Alignment, AnchorPoint, Bounds, Connection,
    CurveStyle, Diagram, Direction, ShapeKind, ShapeNode,
};
use crate::transform::error::TransformError;

use super::CodecOptions;

pub enum OutputTarget<'a> {
    Stream(&'a mut dyn Write),
    Directory(&'a Path),
}

type NativeEncodeFn =
    for<'a> fn(&Value, &CodecOptions, OutputTarget<'a>) -> Result<usize, TransformError>;

#[derive(Clone)]
pub struct NativeCodec {
    pub name: &'static str,
    pub encode: NativeEncodeFn,
}

#[derive(Clone, Default)]
pub struct NativeCodecRegistry {
    codecs: HashMap<&'static str, NativeCodec>,
}

impl NativeCodecRegistry {
    pub fn standard() -> Self {
        let mut registry = Self::default();
        registry.insert(NativeCodec {
            name: "svg",
            encode: encode_svg,
        });
        registry.insert(NativeCodec {
            name: "html",
            encode: encode_html,
        });
        registry
    }

    pub fn insert(&mut self, codec: NativeCodec) {
        self.codecs.insert(codec.name, codec);
    }

    pub fn get(&self, name: &str) -> Option<&NativeCodec> {
        self.codecs.get(name)
    }
}

pub fn encode_native_value<'a>(
    value: &Value,
    codec: &NativeCodec,
    options: &CodecOptions,
    target: OutputTarget<'a>,
) -> Result<usize, TransformError> {
    (codec.encode)(value, options, target)
}

pub fn encode_svg_diagram_to_string(mut diagram: Diagram) -> String {
    render_diagram_svg(&mut diagram)
}

pub fn encode_html_value_to_string(value: &Value) -> Result<String, TransformError> {
    match value {
        Value::String(s) => Ok(s.clone()),
        Value::Map(map) if !is_structured_element(value) => Ok(map
            .get("html")
            .or_else(|| map.get("body"))
            .map(value_string)
            .unwrap_or_default()),
        _ => serialize_element_value(value, MarkupKind::Html),
    }
}

pub fn encode_svg_value_to_string(value: &Value) -> Result<String, TransformError> {
    if is_structured_element(value) {
        serialize_element_value(value, MarkupKind::Svg)
    } else {
        let mut diagram = diagram_from_value(value, &CodecOptions::new())?;
        Ok(render_diagram_svg(&mut diagram))
    }
}

pub fn layout_diagram_value(
    value: &Value,
    options: &CodecOptions,
) -> Result<Value, TransformError> {
    let mut diagram = diagram_from_value(value, options)?;
    let _ = render_diagram_svg(&mut diagram);
    let bounds = shape_bounds(&diagram.shapes).unwrap_or_default();
    let mut out = IndexMap::new();
    out.insert("x".to_string(), Value::Float(bounds.x));
    out.insert("y".to_string(), Value::Float(bounds.y));
    out.insert("width".to_string(), Value::Float(bounds.width));
    out.insert("height".to_string(), Value::Float(bounds.height));
    out.insert(
        "shapes".to_string(),
        Value::List(diagram.shapes.iter().map(resolved_shape_value).collect()),
    );
    Ok(Value::Map(out))
}

fn encode_svg(
    value: &Value,
    options: &CodecOptions,
    target: OutputTarget<'_>,
) -> Result<usize, TransformError> {
    let svg = if is_structured_element(value) {
        let svg = serialize_element_value(value, MarkupKind::Svg)?;
        if !svg.trim_start().starts_with("<svg") {
            return Err(TransformError::Codec(
                "svg codec expects a root svg element or a diagram map".to_string(),
            ));
        }
        svg
    } else {
        let mut diagram = diagram_from_value(value, options)?;
        render_diagram_svg(&mut diagram)
    };
    let filename = output_filename(options, "diagram.svg");
    write_text_output(&filename, &svg, target)?;
    Ok(1)
}

fn shape_bounds(shapes: &[ShapeNode]) -> Option<Bounds> {
    let mut bounds: Option<Bounds> = None;
    for shape in shapes {
        let current = shape.resolved;
        bounds = Some(match bounds {
            Some(existing) => union_bounds(existing, current),
            None => current,
        });
        if let Some(child_bounds) = shape_bounds(&shape.children) {
            let translated = Bounds {
                x: shape.resolved.x + child_bounds.x,
                y: shape.resolved.y + child_bounds.y,
                width: child_bounds.width,
                height: child_bounds.height,
            };
            bounds = Some(match bounds {
                Some(existing) => union_bounds(existing, translated),
                None => translated,
            });
        }
    }
    bounds
}

fn union_bounds(a: Bounds, b: Bounds) -> Bounds {
    let min_x = a.x.min(b.x);
    let min_y = a.y.min(b.y);
    let max_x = (a.x + a.width).max(b.x + b.width);
    let max_y = (a.y + a.height).max(b.y + b.height);
    Bounds {
        x: min_x,
        y: min_y,
        width: max_x - min_x,
        height: max_y - min_y,
    }
}

fn resolved_shape_value(shape: &ShapeNode) -> Value {
    let mut map = IndexMap::new();
    map.insert("kind".to_string(), Value::String(shape.kind_name.clone()));
    if let Some(id) = &shape.id {
        map.insert("id".to_string(), Value::String(id.clone()));
    }
    map.insert("x".to_string(), Value::Float(shape.resolved.x));
    map.insert("y".to_string(), Value::Float(shape.resolved.y));
    map.insert("width".to_string(), Value::Float(shape.resolved.width));
    map.insert("height".to_string(), Value::Float(shape.resolved.height));
    if !shape.children.is_empty() {
        map.insert(
            "children".to_string(),
            Value::List(shape.children.iter().map(resolved_shape_value).collect()),
        );
    }
    Value::Map(map)
}

fn encode_html(
    value: &Value,
    options: &CodecOptions,
    target: OutputTarget<'_>,
) -> Result<usize, TransformError> {
    let html = encode_html_value_to_string(value)?;
    let filename = output_filename(options, "index.html");
    write_text_output(&filename, &html, target)?;
    Ok(1)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MarkupKind {
    Html,
    Svg,
}

fn is_structured_element(value: &Value) -> bool {
    match value {
        Value::BlockRef(_) | Value::List(_) => true,
        Value::Map(map) => {
            map.contains_key("tag")
                || map.contains_key("element")
                || map.contains_key("name")
                || matches!(map.get("kind"), Some(Value::String(kind)) if kind == "html" || kind == "document" || kind == "svg")
        }
        _ => false,
    }
}

fn serialize_element_value(value: &Value, kind: MarkupKind) -> Result<String, TransformError> {
    match value {
        Value::String(value) => Ok(escape_text(value)),
        Value::Int(_)
        | Value::BigInt(_)
        | Value::Float(_)
        | Value::Bool(_)
        | Value::Identifier(_)
        | Value::Symbol(_)
        | Value::Date(_)
        | Value::OffsetDateTime(_)
        | Value::LocalDateTime(_)
        | Value::LocalTime(_)
        | Value::Duration(_)
        | Value::Pattern(_) => Ok(escape_text(&value_string(value))),
        Value::Null => Ok(String::new()),
        Value::List(items) => {
            let mut out = String::new();
            for item in items {
                out.push_str(&serialize_element_value(item, kind)?);
            }
            Ok(out)
        }
        Value::BlockRef(block) => serialize_block_element(block, kind),
        Value::Map(map) => serialize_map_element(map, kind),
        other => Err(TransformError::Codec(format!(
            "cannot serialize {} as {:?} element",
            value_type_name(other),
            kind
        ))),
    }
}

fn serialize_block_element(
    block: &crate::eval::BlockRef,
    kind: MarkupKind,
) -> Result<String, TransformError> {
    let tag = element_tag_from_kind(&block.kind);
    let mut attrs = block.attributes.clone();
    if let Some(id) = &block.id {
        attrs
            .entry("id".to_string())
            .or_insert_with(|| Value::String(id.clone()));
    }
    serialize_element_parts(&tag, &attrs, Some(&block.children), kind)
}

fn serialize_map_element(
    map: &IndexMap<String, Value>,
    kind: MarkupKind,
) -> Result<String, TransformError> {
    let tag = map
        .get("tag")
        .or_else(|| map.get("element"))
        .or_else(|| map.get("name"))
        .or_else(|| map.get("kind"))
        .map(value_string)
        .filter(|tag| !tag.is_empty())
        .ok_or_else(|| TransformError::Codec("element map missing tag".to_string()))?;
    serialize_element_parts(&tag, map, None, kind)
}

fn serialize_element_parts(
    tag: &str,
    attrs: &IndexMap<String, Value>,
    block_children: Option<&[crate::eval::BlockRef]>,
    kind: MarkupKind,
) -> Result<String, TransformError> {
    let tag = normalize_tag(tag);
    if kind == MarkupKind::Html && tag == "text" {
        return Ok(attrs
            .get("content")
            .or_else(|| attrs.get("text"))
            .map(value_string)
            .map(|value| escape_text(&value))
            .unwrap_or_default());
    }
    if tag == "raw" {
        return Ok(attrs
            .get("content")
            .or_else(|| attrs.get("html"))
            .or_else(|| attrs.get("text"))
            .map(value_string)
            .unwrap_or_default());
    }
    if tag == "document" && kind == MarkupKind::Html {
        return Ok(format!(
            "<!doctype html>{}",
            serialize_children(attrs, block_children, kind)?
        ));
    }

    let mut out = String::new();
    out.push('<');
    out.push_str(&tag);
    let attr_string = serialize_attrs(&tag, attrs, kind)?;
    out.push_str(&attr_string);
    if kind == MarkupKind::Svg && tag == "svg" && !has_attr(attrs, "xmlns") {
        out.push_str(" xmlns=\"http://www.w3.org/2000/svg\"");
    }

    if kind == MarkupKind::Html && is_html_void_element(&tag) {
        out.push('>');
        return Ok(out);
    }

    out.push('>');
    out.push_str(&serialize_children(attrs, block_children, kind)?);
    out.push_str("</");
    out.push_str(&tag);
    out.push('>');
    Ok(out)
}

fn serialize_children(
    attrs: &IndexMap<String, Value>,
    block_children: Option<&[crate::eval::BlockRef]>,
    kind: MarkupKind,
) -> Result<String, TransformError> {
    let mut out = String::new();
    if let Some(content) = attrs.get("content").or_else(|| attrs.get("text")) {
        out.push_str(&escape_text(&value_string(content)));
    }
    if let Some(raw) = attrs.get("html").or_else(|| attrs.get("raw")) {
        out.push_str(&value_string(raw));
    }
    if let Some(Value::List(children)) = attrs.get("children") {
        for child in children {
            out.push_str(&serialize_element_value(child, kind)?);
        }
    }
    if let Some(children) = block_children {
        for child in children {
            out.push_str(&serialize_block_element(child, kind)?);
        }
    }
    Ok(out)
}

fn serialize_attrs(
    tag: &str,
    attrs: &IndexMap<String, Value>,
    kind: MarkupKind,
) -> Result<String, TransformError> {
    let mut out = String::new();
    for (name, value) in attrs {
        if is_structural_element_attr(name) {
            continue;
        }
        match value {
            Value::Null => continue,
            Value::Bool(false) => continue,
            Value::Bool(true) if kind == MarkupKind::Html && is_html_boolean_attr(name) => {
                out.push(' ');
                out.push_str(&normalize_attr_name(name));
            }
            Value::Map(map) if name == "style" => {
                let style = map
                    .iter()
                    .filter(|(_, value)| !matches!(value, Value::Null | Value::Bool(false)))
                    .map(|(key, value)| {
                        format!("{}:{}", normalize_attr_name(key), value_string(value))
                    })
                    .collect::<Vec<_>>()
                    .join(";");
                if !style.is_empty() {
                    write_attr(&mut out, "style", &style);
                }
            }
            Value::List(_) | Value::Map(_) => {
                return Err(TransformError::Codec(format!(
                    "attribute '{}' on <{}> must be scalar",
                    name, tag
                )));
            }
            other => write_attr(&mut out, &normalize_attr_name(name), &value_string(other)),
        }
    }
    Ok(out)
}

fn write_attr(out: &mut String, name: &str, value: &str) {
    out.push(' ');
    out.push_str(name);
    out.push_str("=\"");
    out.push_str(&escape_attr(value));
    out.push('"');
}

fn element_tag_from_kind(kind: &str) -> String {
    kind.rsplit("::").next().unwrap_or(kind).to_string()
}

fn normalize_tag(tag: &str) -> String {
    tag.strip_prefix("html::")
        .or_else(|| tag.strip_prefix("svg::"))
        .unwrap_or(tag)
        .replace('_', "-")
}

fn normalize_attr_name(name: &str) -> String {
    match name {
        "class_name" => "class".to_string(),
        "for_" => "for".to_string(),
        "type_" => "type".to_string(),
        _ => name.replace('_', "-"),
    }
}

fn has_attr(attrs: &IndexMap<String, Value>, name: &str) -> bool {
    attrs.keys().any(|key| normalize_attr_name(key) == name)
}

fn is_structural_element_attr(name: &str) -> bool {
    matches!(
        name,
        "tag" | "element" | "name" | "kind" | "children" | "content" | "text" | "html" | "raw"
    )
}

fn is_html_void_element(tag: &str) -> bool {
    matches!(
        tag,
        "area"
            | "base"
            | "br"
            | "col"
            | "embed"
            | "hr"
            | "img"
            | "input"
            | "link"
            | "meta"
            | "param"
            | "source"
            | "track"
            | "wbr"
    )
}

fn is_html_boolean_attr(name: &str) -> bool {
    matches!(
        normalize_attr_name(name).as_str(),
        "allowfullscreen"
            | "async"
            | "autofocus"
            | "autoplay"
            | "checked"
            | "controls"
            | "default"
            | "defer"
            | "disabled"
            | "formnovalidate"
            | "hidden"
            | "ismap"
            | "loop"
            | "multiple"
            | "muted"
            | "novalidate"
            | "open"
            | "readonly"
            | "required"
            | "reversed"
            | "selected"
    )
}

fn escape_text(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

fn escape_attr(value: &str) -> String {
    escape_text(value).replace('"', "&quot;")
}

fn value_type_name(value: &Value) -> &'static str {
    match value {
        Value::String(_) => "string",
        Value::Int(_) => "int",
        Value::BigInt(_) => "bigint",
        Value::Float(_) => "float",
        Value::Bool(_) => "bool",
        Value::Null => "null",
        Value::Identifier(_) => "identifier",
        Value::List(_) => "list",
        Value::Map(_) => "map",
        Value::Object(_) => "object",
        Value::Bytes(_) => "bytes",
        Value::MsgPackExt { .. } => "msgpack_ext",
        Value::MsgPackTimestamp { .. } => "msgpack_timestamp",
        Value::Set(_) => "set",
        Value::BlockRef(_) => "block",
        Value::Symbol(_) => "symbol",
        Value::Function(_) => "function",
        Value::Lazy(_) => "lazy",
        Value::Stream(_) => "stream",
        Value::NativeStream(_) => "native_stream",
        Value::StateHandle(_) => "state_handle",
        Value::Date(_) => "date",
        Value::OffsetDateTime(_) => "offset_datetime",
        Value::LocalDateTime(_) => "local_datetime",
        Value::LocalTime(_) => "local_time",
        Value::Duration(_) => "duration",
        Value::Pattern(_) => "pattern",
    }
}

fn output_filename(options: &CodecOptions, default: &str) -> String {
    options
        .get("filename")
        .or_else(|| options.get("path"))
        .map(value_string)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_string())
}

fn write_text_output(
    filename: &str,
    text: &str,
    target: OutputTarget<'_>,
) -> Result<(), TransformError> {
    match target {
        OutputTarget::Stream(writer) => {
            writer
                .write_all(text.as_bytes())
                .map_err(TransformError::Io)?;
            writer.flush().map_err(TransformError::Io)
        }
        OutputTarget::Directory(path) => {
            validate_relative_output_path(filename)?;
            fs::create_dir_all(path).map_err(TransformError::Io)?;
            let output_path = path.join(filename);
            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent).map_err(TransformError::Io)?;
            }
            fs::write(output_path, text).map_err(TransformError::Io)
        }
    }
}

fn validate_relative_output_path(filename: &str) -> Result<(), TransformError> {
    let path = Path::new(filename);
    if path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(TransformError::Codec(format!(
            "native codec output path must be relative and stay inside the output directory: {filename}"
        )));
    }
    Ok(())
}

fn diagram_from_value(value: &Value, options: &CodecOptions) -> Result<Diagram, TransformError> {
    let map = value_map(value, "svg codec expects a diagram map")?;
    let width = number_attr(map, "width")
        .unwrap_or_else(|| number_option(options, "width").unwrap_or(600.0));
    let height = number_attr(map, "height")
        .unwrap_or_else(|| number_option(options, "height").unwrap_or(400.0));
    let padding = number_attr(map, "padding")
        .unwrap_or_else(|| number_option(options, "padding").unwrap_or(0.0));
    let gap =
        number_attr(map, "gap").unwrap_or_else(|| number_option(options, "gap").unwrap_or(40.0));
    let align = map
        .get("align")
        .or_else(|| options.get("align"))
        .map(value_string)
        .map(|s| parse_alignment_str(&s))
        .unwrap_or(Alignment::None);
    let shapes = map
        .get("shapes")
        .map(shape_list)
        .transpose()?
        .unwrap_or_default();
    let connections = map
        .get("connections")
        .map(connection_list)
        .transpose()?
        .unwrap_or_default();
    Ok(Diagram {
        id: map.get("id").map(value_string),
        width,
        height,
        shapes,
        connections,
        classes: IndexMap::new(),
        padding,
        align,
        gap,
        options: string_map(map),
    })
}

fn shape_list(value: &Value) -> Result<Vec<ShapeNode>, TransformError> {
    let Value::List(items) = value else {
        return Err(TransformError::Codec(
            "diagram shapes must be a list".into(),
        ));
    };
    items
        .iter()
        .enumerate()
        .map(|(i, item)| shape_from_value(item, i))
        .collect()
}

fn shape_from_value(value: &Value, source_order: usize) -> Result<ShapeNode, TransformError> {
    let map = value_map(value, "diagram shape must be a map")?;
    let kind_name = map
        .get("kind")
        .map(value_string)
        .unwrap_or_else(|| "rect".into());
    let kind = shape_kind(&kind_name);
    let children = map
        .get("children")
        .map(shape_list)
        .transpose()?
        .unwrap_or_default();
    let align = map
        .get("align")
        .map(value_string)
        .map(|s| parse_alignment_str(&s))
        .unwrap_or(Alignment::None);
    Ok(ShapeNode {
        kind,
        kind_name,
        id: map.get("id").map(value_string),
        x: number_attr(map, "x"),
        y: number_attr(map, "y"),
        width: number_attr(map, "width").or_else(|| number_attr(map, "w")),
        height: number_attr(map, "height").or_else(|| number_attr(map, "h")),
        top: number_attr(map, "top"),
        bottom: number_attr(map, "bottom"),
        left: number_attr(map, "left"),
        right: number_attr(map, "right"),
        resolved: Bounds::default(),
        attrs: string_map(map),
        events: Vec::new(),
        children,
        text_block_items: Vec::new(),
        align,
        gap: number_attr(map, "gap").unwrap_or(0.0),
        padding: number_attr(map, "padding").unwrap_or(0.0),
        z_index: number_attr(map, "z_index").unwrap_or(0.0),
        source_order,
    })
}

fn connection_list(value: &Value) -> Result<Vec<Connection>, TransformError> {
    let Value::List(items) = value else {
        return Err(TransformError::Codec(
            "diagram connections must be a list".into(),
        ));
    };
    items
        .iter()
        .enumerate()
        .map(connection_from_value)
        .collect()
}

fn connection_from_value(
    (source_order, value): (usize, &Value),
) -> Result<Connection, TransformError> {
    let map = value_map(value, "diagram connection must be a map")?;
    Ok(Connection {
        from_id: required_string(map, "from")?,
        to_id: required_string(map, "to")?,
        direction: match map.get("direction").map(value_string).as_deref() {
            Some("none") => Direction::None,
            Some("from") => Direction::From,
            Some("both") => Direction::Both,
            _ => Direction::To,
        },
        from_anchor: anchor_attr(map, "from_anchor"),
        to_anchor: anchor_attr(map, "to_anchor"),
        label: map.get("label").map(value_string),
        curve: match map.get("curve").map(value_string).as_deref() {
            Some("bezier") => CurveStyle::Bezier,
            _ => CurveStyle::Straight,
        },
        attrs: string_map(map),
        z_index: number_attr(map, "z_index").unwrap_or(0.0),
        source_order,
    })
}

fn shape_kind(kind: &str) -> ShapeKind {
    match kind {
        "circle" => ShapeKind::Circle,
        "ellipse" => ShapeKind::Ellipse,
        "line" => ShapeKind::Line,
        "path" => ShapeKind::Path,
        "text" => ShapeKind::Text,
        "text_block" => ShapeKind::TextBlock,
        "inline_svg" => ShapeKind::InlineSvg,
        "icon" => ShapeKind::Icon,
        "image" => ShapeKind::Image,
        "map" => ShapeKind::Map,
        "sprite" => ShapeKind::Sprite,
        "dopesheet_view" => ShapeKind::DopesheetView,
        "tilemap" => ShapeKind::Tilemap,
        "game_layer" => ShapeKind::GameLayer,
        "group" => ShapeKind::Group,
        "rect" => ShapeKind::Rect,
        _ => ShapeKind::Custom,
    }
}

fn anchor_attr(map: &IndexMap<String, Value>, key: &str) -> AnchorPoint {
    match map.get(key).map(value_string).as_deref() {
        Some("top") => AnchorPoint::Top,
        Some("bottom") => AnchorPoint::Bottom,
        Some("left") => AnchorPoint::Left,
        Some("right") => AnchorPoint::Right,
        Some("center") => AnchorPoint::Center,
        _ => AnchorPoint::Auto,
    }
}

fn value_map<'a>(
    value: &'a Value,
    message: &str,
) -> Result<&'a IndexMap<String, Value>, TransformError> {
    match value {
        Value::Map(map) => Ok(map),
        _ => Err(TransformError::Codec(message.into())),
    }
}

fn required_string(map: &IndexMap<String, Value>, key: &str) -> Result<String, TransformError> {
    map.get(key)
        .map(value_string)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| TransformError::Codec(format!("connection missing required '{key}'")))
}

fn number_attr(map: &IndexMap<String, Value>, key: &str) -> Option<f64> {
    map.get(key).and_then(value_number)
}

fn number_option(map: &CodecOptions, key: &str) -> Option<f64> {
    map.get(key).and_then(value_number)
}

fn value_number(value: &Value) -> Option<f64> {
    match value {
        Value::Int(n) => Some(*n as f64),
        Value::Float(n) => Some(*n),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn string_map(map: &IndexMap<String, Value>) -> IndexMap<String, String> {
    map.iter()
        .filter(|(key, _)| {
            key.as_str() != "shapes" && key.as_str() != "connections" && key.as_str() != "children"
        })
        .map(|(key, value)| (key.clone(), value_string(value)))
        .collect()
}

fn value_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        Value::Symbol(s) => s.clone(),
        Value::Identifier(s) => s.clone(),
        Value::Int(n) => n.to_string(),
        Value::Float(n) => n.to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Span;

    fn block(
        kind: &str,
        id: Option<&str>,
        attrs: IndexMap<String, Value>,
        children: Vec<crate::eval::BlockRef>,
    ) -> crate::eval::BlockRef {
        crate::eval::BlockRef {
            kind: kind.to_string(),
            id: id.map(str::to_string),
            qualified_id: id.map(str::to_string),
            attributes: attrs,
            children,
            decorators: Vec::new(),
            span: Span::dummy(),
        }
    }

    #[test]
    fn svg_codec_encodes_diagram_value_to_stream() {
        let mut shape = IndexMap::new();
        shape.insert("kind".to_string(), Value::String("rect".to_string()));
        shape.insert("id".to_string(), Value::String("box".to_string()));
        shape.insert("width".to_string(), Value::Int(80));
        shape.insert("height".to_string(), Value::Int(40));
        shape.insert("fill".to_string(), Value::String("#fff".to_string()));

        let mut diagram = IndexMap::new();
        diagram.insert("width".to_string(), Value::Int(120));
        diagram.insert("height".to_string(), Value::Int(80));
        diagram.insert("shapes".to_string(), Value::List(vec![Value::Map(shape)]));

        let registry = NativeCodecRegistry::standard();
        let codec = registry.get("svg").expect("svg codec");
        let mut out = Vec::new();
        let written = encode_native_value(
            &Value::Map(diagram),
            codec,
            &CodecOptions::new(),
            OutputTarget::Stream(&mut out),
        )
        .expect("encode svg");

        let svg = String::from_utf8(out).expect("utf8");
        assert_eq!(written, 1);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("width=\"120\""));
        assert!(svg.contains("height=\"80\""));
    }

    #[test]
    fn html_codec_can_write_to_directory_target() {
        let dir = std::env::temp_dir().join(format!(
            "wcl-native-html-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let codec = NativeCodecRegistry::standard()
            .get("html")
            .expect("html codec")
            .clone();
        let mut options = CodecOptions::new();
        options.insert(
            "filename".to_string(),
            Value::String("page.html".to_string()),
        );

        encode_native_value(
            &Value::String("<!doctype html><html></html>".to_string()),
            &codec,
            &options,
            OutputTarget::Directory(&dir),
        )
        .expect("encode html");

        let html = std::fs::read_to_string(dir.join("page.html")).expect("read html");
        assert!(html.contains("<html>"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn html_codec_serializes_structured_element_tree() {
        let mut h1_attrs = IndexMap::new();
        h1_attrs.insert(
            "content".to_string(),
            Value::String("Hello <world>".to_string()),
        );
        let h1 = block("html::h1", None, h1_attrs, vec![]);

        let mut input_attrs = IndexMap::new();
        input_attrs.insert("disabled".to_string(), Value::Bool(true));
        input_attrs.insert("class_name".to_string(), Value::String("field".to_string()));
        let input = block("html::input", None, input_attrs, vec![]);

        let root = block("html::div", Some("app"), IndexMap::new(), vec![h1, input]);
        let codec = NativeCodecRegistry::standard().get("html").unwrap().clone();
        let mut out = Vec::new();

        encode_native_value(
            &Value::BlockRef(root),
            &codec,
            &CodecOptions::new(),
            OutputTarget::Stream(&mut out),
        )
        .expect("encode html");

        let html = String::from_utf8(out).unwrap();
        assert!(html.contains("<div id=\"app\">"));
        assert!(html.contains("<h1>Hello &lt;world&gt;</h1>"));
        assert!(html.contains("<input disabled class=\"field\">"));
    }

    #[test]
    fn svg_codec_serializes_structured_svg_root() {
        let mut rect_attrs = IndexMap::new();
        rect_attrs.insert("x".to_string(), Value::Int(4));
        rect_attrs.insert("y".to_string(), Value::Int(6));
        rect_attrs.insert("stroke_width".to_string(), Value::Int(2));
        let rect = block("svg::rect", None, rect_attrs, vec![]);

        let mut svg_attrs = IndexMap::new();
        svg_attrs.insert("width".to_string(), Value::Int(120));
        svg_attrs.insert("height".to_string(), Value::Int(80));
        let svg_root = block("svg::svg", None, svg_attrs, vec![rect]);
        let codec = NativeCodecRegistry::standard().get("svg").unwrap().clone();
        let mut out = Vec::new();

        encode_native_value(
            &Value::BlockRef(svg_root),
            &codec,
            &CodecOptions::new(),
            OutputTarget::Stream(&mut out),
        )
        .expect("encode svg");

        let svg = String::from_utf8(out).unwrap();
        assert!(
            svg.contains("<svg width=\"120\" height=\"80\" xmlns=\"http://www.w3.org/2000/svg\">")
        );
        assert!(svg.contains("<rect x=\"4\" y=\"6\" stroke-width=\"2\"></rect>"));
    }
}
