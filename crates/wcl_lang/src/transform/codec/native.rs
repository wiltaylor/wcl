//! Native codecs implemented by the host runtime.

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::{Component, Path};

use indexmap::IndexMap;

use crate::eval::value::Value;
use crate::render::shapes::{
    parse_alignment_str, parse_shape_kind, render_diagram_svg, Alignment, AnchorPoint, Bounds,
    Connection, CurveStyle, Diagram, Direction, ShapeKind, ShapeNode,
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

pub fn encode_svg_value_to_string(
    value: &Value,
    options: &CodecOptions,
) -> Result<String, TransformError> {
    let mut diagram = diagram_from_value(value, options)?;
    Ok(render_diagram_svg(&mut diagram))
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
    let mut diagram = diagram_from_value(value, options)?;
    let svg = render_diagram_svg(&mut diagram);
    let filename = output_filename(options, "diagram.svg");
    write_text_output(&filename, &svg, target)?;
    Ok(1)
}

pub fn is_svg_diagram_value(value: &Value) -> bool {
    match value {
        Value::Map(map) => map.contains_key("shapes") || map.contains_key("connections"),
        _ => false,
    }
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

pub fn output_filename(options: &CodecOptions, default: &str) -> String {
    options
        .get("filename")
        .or_else(|| options.get("path"))
        .map(value_string)
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| default.to_string())
}

pub fn write_text_output(
    filename: &str,
    text: &str,
    target: OutputTarget<'_>,
) -> Result<(), TransformError> {
    write_bytes_output(filename, text.as_bytes(), target)
}

pub fn write_bytes_output(
    filename: &str,
    bytes: &[u8],
    target: OutputTarget<'_>,
) -> Result<(), TransformError> {
    match target {
        OutputTarget::Stream(writer) => {
            writer.write_all(bytes).map_err(TransformError::Io)?;
            writer.flush().map_err(TransformError::Io)
        }
        OutputTarget::Directory(path) => {
            validate_relative_output_path(filename)?;
            fs::create_dir_all(path).map_err(TransformError::Io)?;
            let output_path = path.join(filename);
            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent).map_err(TransformError::Io)?;
            }
            fs::write(output_path, bytes).map_err(TransformError::Io)
        }
    }
}

pub fn validate_relative_output_path(filename: &str) -> Result<(), TransformError> {
    let path = Path::new(filename);
    if filename.is_empty()
        || path.is_absolute()
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
        .map(shape_nodes_from_value)
        .transpose()?
        .unwrap_or_default();
    let connections = map
        .get("connections")
        .map(connections_from_value)
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

pub fn shape_nodes_from_value(value: &Value) -> Result<Vec<ShapeNode>, TransformError> {
    let Value::List(items) = value else {
        return Err(TransformError::Codec(
            "diagram shapes must be a list".into(),
        ));
    };
    let mut shapes = Vec::new();
    push_shape_values(items, &mut shapes)?;
    Ok(shapes)
}

fn push_shape_values(items: &[Value], shapes: &mut Vec<ShapeNode>) -> Result<(), TransformError> {
    for item in items {
        match item {
            Value::List(nested) => push_shape_values(nested, shapes)?,
            other => {
                let source_order = shapes.len();
                shapes.push(shape_from_value(other, source_order)?);
            }
        }
    }
    Ok(())
}

fn shape_from_value(value: &Value, source_order: usize) -> Result<ShapeNode, TransformError> {
    let map = value_map(value, &shape_map_error(value))?;
    let kind_name = map
        .get("kind")
        .map(value_string)
        .unwrap_or_else(|| "rect".into());
    let kind = shape_kind(&kind_name);
    let children = map
        .get("children")
        .map(shape_nodes_from_value)
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

fn shape_map_error(value: &Value) -> String {
    let preview = value.to_string();
    let preview = if preview.len() > 160 {
        format!("{}...", &preview[..160])
    } else {
        preview
    };
    format!(
        "diagram shape must be a map, got {}: {}",
        value.type_name(),
        preview
    )
}

pub fn connections_from_value(value: &Value) -> Result<Vec<Connection>, TransformError> {
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
    parse_shape_kind(kind)
        .or_else(|| parse_shape_kind(&format!("wdoc::draw::{kind}")))
        .unwrap_or(ShapeKind::Custom)
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
        Value::Object(object) => Ok(&object.fields),
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

pub fn bounds_from_value(value: Option<&Value>) -> Option<Bounds> {
    let Some(value) = value else {
        return None;
    };
    let map = value_map(value, "").ok()?;
    Some(Bounds {
        x: number_attr(map, "x").unwrap_or(0.0),
        y: number_attr(map, "y").unwrap_or(0.0),
        width: number_attr(map, "width").unwrap_or(0.0),
        height: number_attr(map, "height").unwrap_or(0.0),
    })
}

pub fn value_number(value: &Value) -> Option<f64> {
    match value {
        Value::Int(n) => Some(*n as f64),
        Value::BigInt(n) => n.to_string().parse().ok(),
        Value::Float(n) => Some(*n),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

pub fn string_map(map: &IndexMap<String, Value>) -> IndexMap<String, String> {
    map.iter()
        .filter(|(key, _)| {
            key.as_str() != "shapes" && key.as_str() != "connections" && key.as_str() != "children"
        })
        .map(|(key, value)| (key.clone(), value_string(value)))
        .collect()
}

pub fn shape_node_to_value(node: &ShapeNode) -> Value {
    let mut map: IndexMap<String, Value> = node
        .attrs
        .iter()
        .map(|(key, value)| (key.clone(), Value::String(value.clone())))
        .collect();
    map.insert("kind".to_string(), Value::String(node.kind_name.clone()));
    if let Some(id) = &node.id {
        map.insert("id".to_string(), Value::String(id.clone()));
    }
    map.insert("x".to_string(), Value::Float(layout_x(node)));
    map.insert("y".to_string(), Value::Float(layout_y(node)));
    map.insert("width".to_string(), Value::Float(layout_width(node)));
    map.insert("height".to_string(), Value::Float(layout_height(node)));
    map.insert(
        "align".to_string(),
        Value::String(crate::render::shapes::alignment_name(node.align).to_string()),
    );
    map.insert("gap".to_string(), Value::Float(node.gap));
    map.insert("padding".to_string(), Value::Float(node.padding));
    map.insert(
        "children".to_string(),
        Value::List(node.children.iter().map(shape_node_to_value).collect()),
    );
    Value::Map(map)
}

pub fn connection_to_value(conn: &Connection) -> Value {
    let mut map: IndexMap<String, Value> = conn
        .attrs
        .iter()
        .map(|(key, value)| (key.clone(), Value::String(value.clone())))
        .collect();
    map.insert("kind".to_string(), Value::String("connection".to_string()));
    map.insert("from".to_string(), Value::String(conn.from_id.clone()));
    map.insert("to".to_string(), Value::String(conn.to_id.clone()));
    if let Some(label) = &conn.label {
        map.insert("label".to_string(), Value::String(label.clone()));
    }
    Value::Map(map)
}

fn layout_x(node: &ShapeNode) -> f64 {
    non_zero_or(node.resolved.x, node.x)
}

fn layout_y(node: &ShapeNode) -> f64 {
    non_zero_or(node.resolved.y, node.y)
}

fn layout_width(node: &ShapeNode) -> f64 {
    non_zero_or(node.resolved.width, node.width)
}

fn layout_height(node: &ShapeNode) -> f64 {
    non_zero_or(node.resolved.height, node.height)
}

fn non_zero_or(value: f64, fallback: Option<f64>) -> f64 {
    if value != 0.0 {
        value
    } else {
        fallback.unwrap_or(value)
    }
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
    fn svg_codec_flattens_nested_shape_lists() {
        let mut rect = IndexMap::new();
        rect.insert("kind".to_string(), Value::String("rect".to_string()));
        rect.insert("id".to_string(), Value::String("box".to_string()));
        rect.insert("width".to_string(), Value::Int(80));
        rect.insert("height".to_string(), Value::Int(40));

        let mut text = IndexMap::new();
        text.insert("kind".to_string(), Value::String("text".to_string()));
        text.insert("content".to_string(), Value::String("Hello".to_string()));
        text.insert("width".to_string(), Value::Int(80));
        text.insert("height".to_string(), Value::Int(20));

        let mut diagram = IndexMap::new();
        diagram.insert("width".to_string(), Value::Int(120));
        diagram.insert("height".to_string(), Value::Int(80));
        diagram.insert(
            "shapes".to_string(),
            Value::List(vec![Value::Map(rect), Value::List(vec![Value::Map(text)])]),
        );

        let svg = encode_svg_value_to_string(&Value::Map(diagram), &CodecOptions::new())
            .expect("encode svg");

        assert!(svg.contains("<rect"));
        assert!(svg.contains("Hello"));
    }

    #[test]
    fn standard_registry_does_not_include_wdoc_html_codec() {
        let registry = NativeCodecRegistry::standard();
        assert!(registry.get("wdoc-html").is_none());
    }
}
