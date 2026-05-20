use indexmap::IndexMap;

use crate::{BuiltinFn, FunctionRegistry, FunctionSignature, Value};

pub fn wdoc_functions() -> FunctionRegistry {
    let mut reg = FunctionRegistry::new();
    register_layout_helpers(&mut reg);
    reg
}

/// Parse options for editor/LSP tooling that should understand WDoc files.
///
/// This registers the small set of native WDoc host functions used by the WCL
/// standard library, so `import <wdoc.wcl>` can resolve its declarations.
pub fn lsp_parse_options() -> Result<crate::ParseOptions, String> {
    Ok(crate::ParseOptions {
        functions: wdoc_functions(),
        ..Default::default()
    })
}

fn register_layout_helpers(reg: &mut FunctionRegistry) {
    for (name, align) in [
        ("wdoc::layout_stack", crate::wdoc::shapes::Alignment::Stack),
        ("wdoc::layout_flow", crate::wdoc::shapes::Alignment::Flow),
        (
            "wdoc::layout_center",
            crate::wdoc::shapes::Alignment::Center,
        ),
        ("wdoc::layout_grid", crate::wdoc::shapes::Alignment::Grid),
        (
            "wdoc::layout_layered",
            crate::wdoc::shapes::Alignment::Layered,
        ),
        ("wdoc::layout_force", crate::wdoc::shapes::Alignment::Force),
        (
            "wdoc::layout_radial",
            crate::wdoc::shapes::Alignment::Radial,
        ),
    ] {
        let builtin_name = name.to_string();
        reg.register(
            name,
            std::sync::Arc::new(move |args: &[Value]| wdoc_layout_helper(align, args)) as BuiltinFn,
            FunctionSignature {
                name: builtin_name,
                params: vec!["ctx: map".into()],
                return_type: "map".into(),
                doc: "Resolve a WDoc diagram layout request".into(),
            },
        );
    }

    reg.register(
        "wdoc::route_connections",
        std::sync::Arc::new(wdoc_route_connections_helper) as BuiltinFn,
        FunctionSignature {
            name: "wdoc::route_connections".into(),
            params: vec!["ctx: map".into()],
            return_type: "list(map)".into(),
            doc: "Route WDoc diagram connections for resolved shapes".into(),
        },
    );
}

fn wdoc_layout_helper(
    align: crate::wdoc::shapes::Alignment,
    args: &[Value],
) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("wdoc layout helper expects 1 argument".into());
    }
    let Value::Map(ctx) = &args[0] else {
        return Err("wdoc layout helper argument must be a map".into());
    };
    let mut children = value_shapes(ctx.get("shapes"))?;
    let mut connections = value_connections(ctx.get("connections"))?;
    let indices: Vec<usize> = (0..children.len()).collect();
    let parent = value_bounds(ctx.get("parent")).unwrap_or_default();
    let gap = ctx.get("gap").and_then(value_as_f64).unwrap_or(0.0);
    let options = value_string_map(ctx.get("options"));
    crate::wdoc::shapes::prepare_layout_inputs(&mut children, &connections, &parent);
    crate::wdoc::shapes::apply_builtin_layout(
        &mut children,
        &mut connections,
        crate::wdoc::shapes::LayoutRequest {
            indices: &indices,
            parent: &parent,
            scope_path: ctx
                .get("scope_path")
                .and_then(|value| value.as_string())
                .unwrap_or(""),
            align,
            gap,
            options: &options,
        },
    );
    let mut result = IndexMap::new();
    result.insert(
        "shapes".to_string(),
        Value::List(children.iter().map(shape_node_to_value).collect()),
    );
    result.insert(
        "connections".to_string(),
        Value::List(connections.iter().map(connection_to_value).collect()),
    );
    Ok(Value::Map(result))
}

fn wdoc_route_connections_helper(args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("wdoc::route_connections() expects 1 argument".into());
    }
    let Value::Map(ctx) = &args[0] else {
        return Err("wdoc::route_connections() argument must be a map".into());
    };
    let shapes = value_shapes(ctx.get("shapes"))?;
    let connections = value_connections(ctx.get("connections"))?;
    Ok(Value::List(
        crate::wdoc::shapes::route_connection_infos(&shapes, &connections)
            .into_iter()
            .map(|route| {
                let mut map = IndexMap::new();
                map.insert("from".to_string(), Value::String(route.from_id));
                map.insert("to".to_string(), Value::String(route.to_id));
                map.insert("d".to_string(), Value::String(route.d));
                map.insert(
                    "points".to_string(),
                    Value::List(
                        route
                            .points
                            .into_iter()
                            .map(|(x, y)| Value::List(vec![Value::Float(x), Value::Float(y)]))
                            .collect(),
                    ),
                );
                map.insert("label_x".to_string(), Value::Float(route.label_x));
                map.insert("label_y".to_string(), Value::Float(route.label_y));
                Value::Map(map)
            })
            .collect(),
    ))
}

fn value_shapes(value: Option<&Value>) -> Result<Vec<crate::wdoc::shapes::ShapeNode>, String> {
    let Some(Value::List(items)) = value else {
        return Ok(Vec::new());
    };
    let mut shapes = Vec::new();
    push_shape_values(items, &mut shapes)?;
    Ok(shapes)
}

fn push_shape_values(
    items: &[Value],
    shapes: &mut Vec<crate::wdoc::shapes::ShapeNode>,
) -> Result<(), String> {
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

fn shape_from_value(
    value: &Value,
    source_order: usize,
) -> Result<crate::wdoc::shapes::ShapeNode, String> {
    use crate::wdoc::shapes::*;

    let Value::Map(map) = value else {
        return Err(format!(
            "layout shape descriptors must be maps, got {}",
            value.type_name()
        ));
    };
    let kind_name = map
        .get("kind")
        .and_then(|value| value.as_string())
        .unwrap_or("rect")
        .to_string();
    let qualified_kind = if kind_name.contains("::") {
        kind_name.clone()
    } else {
        format!("wdoc::draw::{kind_name}")
    };
    let children = map
        .get("children")
        .map(|value| match value {
            Value::List(items) => {
                let mut children = Vec::new();
                push_shape_values(items, &mut children)?;
                Ok(children)
            }
            _ => Err("layout shape children must be a list".to_string()),
        })
        .transpose()?
        .unwrap_or_default();
    let attrs = string_map(map);
    let kind = parse_shape_kind(&qualified_kind).unwrap_or(ShapeKind::Custom);
    Ok(ShapeNode {
        kind,
        kind_name: qualified_kind,
        id: map
            .get("id")
            .and_then(|value| value.as_string())
            .map(str::to_string),
        x: map.get("x").and_then(value_as_f64),
        y: map.get("y").and_then(value_as_f64),
        width: map
            .get("width")
            .or_else(|| map.get("w"))
            .and_then(value_as_f64),
        height: map
            .get("height")
            .or_else(|| map.get("h"))
            .and_then(value_as_f64),
        top: map.get("top").and_then(value_as_f64),
        bottom: map.get("bottom").and_then(value_as_f64),
        left: map.get("left").and_then(value_as_f64),
        right: map.get("right").and_then(value_as_f64),
        resolved: Bounds::default(),
        attrs,
        events: Vec::new(),
        children,
        text_block_items: Vec::new(),
        align: map
            .get("align")
            .and_then(|value| value.as_string())
            .map(parse_alignment_str)
            .unwrap_or(Alignment::None),
        gap: map.get("gap").and_then(value_as_f64).unwrap_or(0.0),
        padding: map.get("padding").and_then(value_as_f64).unwrap_or(0.0),
        z_index: map.get("z_index").and_then(value_as_f64).unwrap_or(0.0),
        source_order,
    })
}

fn value_connections(
    value: Option<&Value>,
) -> Result<Vec<crate::wdoc::shapes::Connection>, String> {
    let Some(Value::List(items)) = value else {
        return Ok(Vec::new());
    };
    items
        .iter()
        .enumerate()
        .filter_map(|(idx, item)| connection_from_value(item, idx).transpose())
        .collect()
}

fn connection_from_value(
    value: &Value,
    source_order: usize,
) -> Result<Option<crate::wdoc::shapes::Connection>, String> {
    use crate::wdoc::shapes::*;

    let Value::Map(map) = value else {
        return Err(format!(
            "layout connection descriptors must be maps, got {}",
            value.type_name()
        ));
    };
    let kind = map.get("kind").and_then(|value| value.as_string());
    if !kind.is_some_and(|kind| kind == "connection" || kind == "wdoc::draw::connection") {
        return Ok(None);
    }
    let attrs = string_map(map);
    Ok(Some(Connection {
        from_id: attrs.get("from").cloned().unwrap_or_default(),
        to_id: attrs.get("to").cloned().unwrap_or_default(),
        direction: parse_direction_str(attrs.get("direction").map(String::as_str).unwrap_or("")),
        from_anchor: parse_anchor_str(attrs.get("from_anchor").map(String::as_str).unwrap_or("")),
        to_anchor: parse_anchor_str(attrs.get("to_anchor").map(String::as_str).unwrap_or("")),
        label: attrs.get("label").cloned(),
        curve: parse_curve_str(attrs.get("curve").map(String::as_str).unwrap_or("")),
        attrs,
        z_index: map.get("z_index").and_then(value_as_f64).unwrap_or(0.0),
        source_order,
    }))
}

fn value_bounds(value: Option<&Value>) -> Option<crate::wdoc::shapes::Bounds> {
    let Some(Value::Map(map)) = value else {
        return None;
    };
    Some(crate::wdoc::shapes::Bounds {
        x: map.get("x").and_then(value_as_f64).unwrap_or(0.0),
        y: map.get("y").and_then(value_as_f64).unwrap_or(0.0),
        width: map.get("width").and_then(value_as_f64).unwrap_or(0.0),
        height: map.get("height").and_then(value_as_f64).unwrap_or(0.0),
    })
}

fn shape_node_to_value(node: &crate::wdoc::shapes::ShapeNode) -> Value {
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
        Value::String(crate::wdoc::shapes::alignment_name(node.align).to_string()),
    );
    map.insert("gap".to_string(), Value::Float(node.gap));
    map.insert("padding".to_string(), Value::Float(node.padding));
    map.insert(
        "children".to_string(),
        Value::List(node.children.iter().map(shape_node_to_value).collect()),
    );
    Value::Map(map)
}

fn connection_to_value(conn: &crate::wdoc::shapes::Connection) -> Value {
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

fn layout_x(node: &crate::wdoc::shapes::ShapeNode) -> f64 {
    non_zero_or(node.resolved.x, node.x)
}

fn layout_y(node: &crate::wdoc::shapes::ShapeNode) -> f64 {
    non_zero_or(node.resolved.y, node.y)
}

fn layout_width(node: &crate::wdoc::shapes::ShapeNode) -> f64 {
    non_zero_or(node.resolved.width, node.width)
}

fn layout_height(node: &crate::wdoc::shapes::ShapeNode) -> f64 {
    non_zero_or(node.resolved.height, node.height)
}

fn non_zero_or(value: f64, fallback: Option<f64>) -> f64 {
    if value != 0.0 {
        value
    } else {
        fallback.unwrap_or(value)
    }
}

fn value_string_map(value: Option<&Value>) -> IndexMap<String, String> {
    let Some(Value::Map(map)) = value else {
        return IndexMap::new();
    };
    string_map(map)
}

fn string_map(map: &IndexMap<String, Value>) -> IndexMap<String, String> {
    map.iter()
        .filter_map(|(key, value)| match value {
            Value::String(value) => Some((key.clone(), value.clone())),
            Value::Int(value) => Some((key.clone(), value.to_string())),
            Value::BigInt(value) => Some((key.clone(), value.to_string())),
            Value::Float(value) => Some((key.clone(), value.to_string())),
            Value::Bool(value) => Some((key.clone(), value.to_string())),
            Value::Null | Value::BlockRef(_) => None,
            _ => Some((key.clone(), value.to_string())),
        })
        .collect()
}

fn value_as_f64(value: &Value) -> Option<f64> {
    match value {
        Value::Int(value) => Some(*value as f64),
        Value::BigInt(value) => value.to_string().parse().ok(),
        Value::Float(value) => Some(*value),
        Value::String(value) => value.parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lsp_parse_options_resolve_embedded_wdoc_library() {
        let options = lsp_parse_options().unwrap();
        let doc = crate::parse(
            "import <wdoc.wcl>\nuse wdoc::{icon}\nlet x = icon(\"home\")",
            options,
        );
        assert!(
            doc.diagnostics.iter().all(|d| !d.is_error()),
            "unexpected diagnostics: {:?}",
            doc.diagnostics
        );
    }
}
