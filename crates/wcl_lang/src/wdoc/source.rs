use std::collections::HashSet;
use std::path::{Path, PathBuf};

use indexmap::IndexMap;

use crate::{BlockRef, BuiltinFn, FunctionRegistry, FunctionSignature, Value};

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
    use crate::wdoc::shapes::Alignment;

    reg.register(
        "wdoc::layout_stack",
        std::sync::Arc::new(|args: &[Value]| wdoc_layout_helper(Alignment::Stack, args))
            as BuiltinFn,
        FunctionSignature {
            name: "wdoc::layout_stack".into(),
            params: vec!["ctx: map".into()],
            return_type: "map".into(),
            doc: "Resolve a WDoc stack layout request".into(),
        },
    );

    reg.register(
        "wdoc::layout_flow",
        std::sync::Arc::new(|args: &[Value]| wdoc_layout_helper(Alignment::Flow, args))
            as BuiltinFn,
        FunctionSignature {
            name: "wdoc::layout_flow".into(),
            params: vec!["ctx: map".into()],
            return_type: "map".into(),
            doc: "Resolve a WDoc flow layout request".into(),
        },
    );

    reg.register(
        "wdoc::layout_center",
        std::sync::Arc::new(|args: &[Value]| wdoc_layout_helper(Alignment::Center, args))
            as BuiltinFn,
        FunctionSignature {
            name: "wdoc::layout_center".into(),
            params: vec!["ctx: map".into()],
            return_type: "map".into(),
            doc: "Resolve a WDoc center layout request".into(),
        },
    );

    reg.register(
        "wdoc::layout_grid",
        std::sync::Arc::new(|args: &[Value]| wdoc_layout_helper(Alignment::Grid, args))
            as BuiltinFn,
        FunctionSignature {
            name: "wdoc::layout_grid".into(),
            params: vec!["ctx: map".into()],
            return_type: "map".into(),
            doc: "Resolve a WDoc grid layout request".into(),
        },
    );

    reg.register(
        "wdoc::layout_layered",
        std::sync::Arc::new(|args: &[Value]| wdoc_layout_helper(Alignment::Layered, args))
            as BuiltinFn,
        FunctionSignature {
            name: "wdoc::layout_layered".into(),
            params: vec!["ctx: map".into()],
            return_type: "map".into(),
            doc: "Resolve a WDoc layered layout request".into(),
        },
    );

    reg.register(
        "wdoc::layout_force",
        std::sync::Arc::new(|args: &[Value]| wdoc_layout_helper(Alignment::Force, args))
            as BuiltinFn,
        FunctionSignature {
            name: "wdoc::layout_force".into(),
            params: vec!["ctx: map".into()],
            return_type: "map".into(),
            doc: "Resolve a WDoc force layout request".into(),
        },
    );

    reg.register(
        "wdoc::layout_radial",
        std::sync::Arc::new(|args: &[Value]| wdoc_layout_helper(Alignment::Radial, args))
            as BuiltinFn,
        FunctionSignature {
            name: "wdoc::layout_radial".into(),
            params: vec!["ctx: map".into()],
            return_type: "map".into(),
            doc: "Resolve a WDoc radial layout request".into(),
        },
    );

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

    reg.register(
        "wdoc::primitive_diagram_value",
        std::sync::Arc::new(wdoc_primitive_diagram_value_helper) as BuiltinFn,
        FunctionSignature {
            name: "wdoc::primitive_diagram_value".into(),
            params: vec!["project: map".into(), "block: block".into()],
            return_type: "any".into(),
            doc: "Build a diagram value natively when it only uses primitive WDoc shapes".into(),
        },
    );

    reg.register(
        "wdoc::media_assets",
        std::sync::Arc::new(wdoc_media_assets_helper) as BuiltinFn,
        FunctionSignature {
            name: "wdoc::media_assets".into(),
            params: vec!["project: map".into()],
            return_type: "list(map)".into(),
            doc: "Collect WDoc local media assets from block values".into(),
        },
    );

    reg.register(
        "wdoc::project_template_name_native",
        std::sync::Arc::new(wdoc_project_template_name_helper) as BuiltinFn,
        FunctionSignature {
            name: "wdoc::project_template_name_native".into(),
            params: vec![
                "project: map".into(),
                "format: string".into(),
                "schema_name: string".into(),
            ],
            return_type: "any".into(),
            doc: "Resolve WDoc project template metadata".into(),
        },
    );

    reg.register(
        "wdoc::project_extends_name_native",
        std::sync::Arc::new(wdoc_project_extends_name_helper) as BuiltinFn,
        FunctionSignature {
            name: "wdoc::project_extends_name_native".into(),
            params: vec!["project: map".into(), "schema_name: string".into()],
            return_type: "any".into(),
            doc: "Resolve WDoc project schema extension metadata".into(),
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
    let mut children = ctx
        .get("shapes")
        .map(crate::transform::codec::native::shape_nodes_from_value)
        .transpose()
        .map_err(|err| err.to_string())?
        .unwrap_or_default();
    let mut connections = ctx
        .get("connections")
        .map(crate::transform::codec::native::connections_from_value)
        .transpose()
        .map_err(|err| err.to_string())?
        .unwrap_or_default();
    let indices: Vec<usize> = (0..children.len()).collect();
    let parent =
        crate::transform::codec::native::bounds_from_value(ctx.get("parent")).unwrap_or_default();
    let gap = ctx
        .get("gap")
        .and_then(crate::transform::codec::native::value_number)
        .unwrap_or(0.0);
    let options = ctx
        .get("options")
        .and_then(|value| match value {
            Value::Map(map) => Some(crate::transform::codec::native::string_map(map)),
            _ => None,
        })
        .unwrap_or_default();
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
        Value::List(
            children
                .iter()
                .map(crate::transform::codec::native::shape_node_to_value)
                .collect(),
        ),
    );
    result.insert(
        "connections".to_string(),
        Value::List(
            connections
                .iter()
                .map(crate::transform::codec::native::connection_to_value)
                .collect(),
        ),
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
    let shapes = ctx
        .get("shapes")
        .map(crate::transform::codec::native::shape_nodes_from_value)
        .transpose()
        .map_err(|err| err.to_string())?
        .unwrap_or_default();
    let connections = ctx
        .get("connections")
        .map(crate::transform::codec::native::connections_from_value)
        .transpose()
        .map_err(|err| err.to_string())?
        .unwrap_or_default();
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

fn wdoc_primitive_diagram_value_helper(args: &[Value]) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("wdoc::primitive_diagram_value() expects 2 arguments".into());
    }
    let Value::Map(project) = &args[0] else {
        return Err("wdoc::primitive_diagram_value() project argument must be a map".into());
    };
    let Value::BlockRef(block) = &args[1] else {
        return Err("wdoc::primitive_diagram_value() block argument must be a block".into());
    };
    if kind_leaf(&block.kind) != "diagram" {
        return Ok(Value::Null);
    }
    let shape_templates = shape_template_schemas(project);
    if !primitive_diagram_supported(block, &shape_templates) {
        return Ok(Value::Null);
    }

    let mut map = IndexMap::new();
    map.insert(
        "id".to_string(),
        block
            .id
            .as_ref()
            .map(|id| Value::String(id.clone()))
            .unwrap_or(Value::Null),
    );
    map.insert(
        "width".to_string(),
        block
            .attributes
            .get("width")
            .cloned()
            .unwrap_or(Value::Int(600)),
    );
    map.insert(
        "height".to_string(),
        block
            .attributes
            .get("height")
            .cloned()
            .unwrap_or(Value::Int(400)),
    );
    map.insert(
        "align".to_string(),
        block
            .attributes
            .get("align")
            .cloned()
            .unwrap_or_else(|| Value::String("none".to_string())),
    );
    map.insert(
        "gap".to_string(),
        block
            .attributes
            .get("gap")
            .cloned()
            .unwrap_or(Value::Int(40)),
    );
    map.insert(
        "padding".to_string(),
        block
            .attributes
            .get("padding")
            .cloned()
            .unwrap_or(Value::Int(0)),
    );
    map.insert(
        "shapes".to_string(),
        Value::List(
            child_blocks(block)
                .into_iter()
                .filter(|child| !primitive_skip_block(child, &shape_templates))
                .map(|child| primitive_shape_value(child, &shape_templates))
                .collect(),
        ),
    );
    map.insert(
        "connections".to_string(),
        Value::List(primitive_connections_in_block(block)),
    );
    Ok(Value::Map(map))
}

fn shape_template_schemas(project: &IndexMap<String, Value>) -> HashSet<String> {
    let Some(Value::Map(metadata)) = project.get("metadata") else {
        return HashSet::new();
    };
    let Some(Value::List(templates)) = metadata.get("templates") else {
        return HashSet::new();
    };
    templates
        .iter()
        .filter_map(|template| {
            let Value::Map(template) = template else {
                return None;
            };
            let format = template.get("format").and_then(Value::as_string)?;
            (format == "shape").then(|| template.get("schema").and_then(Value::as_string))?
        })
        .map(str::to_string)
        .collect()
}

fn primitive_diagram_supported(block: &BlockRef, shape_templates: &HashSet<String>) -> bool {
    child_blocks(block).into_iter().all(|child| {
        primitive_skip_block(child, shape_templates)
            || primitive_shape_supported(child, shape_templates)
                && primitive_diagram_supported(child, shape_templates)
    })
}

fn primitive_shape_supported(block: &BlockRef, shape_templates: &HashSet<String>) -> bool {
    if has_shape_template(&block.kind, shape_templates) {
        return false;
    }
    matches!(
        kind_leaf(&block.kind),
        "rect"
            | "circle"
            | "ellipse"
            | "line"
            | "path"
            | "text"
            | "text_block"
            | "image"
            | "map"
            | "group"
            | "game_layer"
    )
}

fn has_shape_template(kind: &str, shape_templates: &HashSet<String>) -> bool {
    shape_templates.contains(kind) || shape_templates.contains(kind_leaf(kind))
}

fn primitive_skip_block(block: &BlockRef, _shape_templates: &HashSet<String>) -> bool {
    primitive_connection_block(block)
        || matches!(
            block.kind.as_str(),
            "wdoc::draw::event"
                | "wdoc::draw::class"
                | "wdoc::draw::state"
                | "wdoc::draw::animation"
                | "wdoc::draw::keyframe"
                | "wdoc::draw::dopesheet"
        )
}

fn primitive_connection_block(block: &BlockRef) -> bool {
    block.kind == "wdoc::draw::connection"
}

fn primitive_shape_value(block: &BlockRef, shape_templates: &HashSet<String>) -> Value {
    let mut attrs = block.attributes.clone();
    attrs.insert(
        "kind".to_string(),
        Value::String(kind_leaf(&block.kind).to_string()),
    );
    attrs.insert(
        "id".to_string(),
        block
            .id
            .as_ref()
            .map(|id| Value::String(id.clone()))
            .unwrap_or(Value::Null),
    );
    attrs.insert(
        "children".to_string(),
        Value::List(
            child_blocks(block)
                .into_iter()
                .filter(|child| !primitive_skip_block(child, shape_templates))
                .map(|child| primitive_shape_value(child, shape_templates))
                .collect(),
        ),
    );
    Value::Map(attrs)
}

fn primitive_connections_in_block(block: &BlockRef) -> Vec<Value> {
    let mut connections = Vec::new();
    for child in child_blocks(block) {
        if primitive_connection_block(child) {
            connections.push(primitive_connection_value(child));
        } else {
            connections.extend(primitive_connections_in_block(child));
        }
    }
    connections
}

fn primitive_connection_value(block: &BlockRef) -> Value {
    let mut attrs = block.attributes.clone();
    attrs.insert("kind".to_string(), Value::String("connection".to_string()));
    attrs
        .entry("from".to_string())
        .or_insert_with(|| Value::String(String::new()));
    attrs
        .entry("to".to_string())
        .or_insert_with(|| Value::String(String::new()));
    Value::Map(attrs)
}

fn child_blocks(block: &BlockRef) -> Vec<&BlockRef> {
    block
        .attributes
        .values()
        .filter_map(|value| match value {
            Value::BlockRef(child) => Some(child),
            _ => None,
        })
        .chain(block.children.iter())
        .collect()
}

fn kind_leaf(kind: &str) -> &str {
    kind.rsplit("::").next().unwrap_or(kind)
}

fn wdoc_media_assets_helper(args: &[Value]) -> Result<Value, String> {
    if args.len() != 1 {
        return Err("wdoc::media_assets() expects 1 argument".into());
    }
    let Value::Map(project) = &args[0] else {
        return Err("wdoc::media_assets() project argument must be a map".into());
    };
    let source_dirs = project_source_dirs(project);
    let mut seen = HashSet::new();
    let mut assets = Vec::new();
    if let Some(Value::Map(values)) = project.get("values") {
        for value in values.values() {
            if let Value::BlockRef(block) = value {
                collect_media_assets(block, &source_dirs, &mut seen, &mut assets);
            }
        }
    }
    Ok(Value::List(assets))
}

fn project_source_dirs(project: &IndexMap<String, Value>) -> Vec<PathBuf> {
    let Some(Value::List(items)) = project.get("source_dirs") else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| item.as_string().map(PathBuf::from))
        .collect()
}

fn collect_media_assets(
    block: &BlockRef,
    source_dirs: &[PathBuf],
    seen: &mut HashSet<(String, String)>,
    assets: &mut Vec<Value>,
) {
    if media_asset_kind(&block.kind) {
        if let Some(src) = block.attributes.get("src").and_then(Value::as_string) {
            let resolved = resolve_asset_source(src, source_dirs);
            let key = (src.to_string(), resolved.clone());
            if seen.insert(key) {
                let mut asset = IndexMap::new();
                asset.insert("path".to_string(), Value::String(src.to_string()));
                asset.insert("src".to_string(), Value::String(resolved));
                assets.push(Value::Map(asset));
            }
        }
    }
    for child in child_blocks(block) {
        collect_media_assets(child, source_dirs, seen, assets);
    }
}

fn media_asset_kind(kind: &str) -> bool {
    matches!(
        kind,
        "wdoc::image"
            | "wdoc::draw::image"
            | "wdoc::draw::map"
            | "wdoc::draw::sprite"
            | "wdoc::draw::dopesheet"
            | "wdoc::draw::dopesheet_view"
            | "wdoc::draw::tilemap"
    )
}

fn resolve_asset_source(src: &str, source_dirs: &[PathBuf]) -> String {
    let src_path = Path::new(src);
    if src_path.is_absolute() {
        return src.to_string();
    }
    source_dirs
        .iter()
        .map(|dir| dir.join(src_path))
        .find(|candidate| candidate.exists())
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| src.to_string())
}

fn wdoc_project_template_name_helper(args: &[Value]) -> Result<Value, String> {
    if args.len() != 3 {
        return Err("wdoc::project_template_name_native() expects 3 arguments".into());
    }
    let Value::Map(project) = &args[0] else {
        return Err("wdoc::project_template_name_native() project argument must be a map".into());
    };
    let Some(format) = args[1].as_string() else {
        return Err("wdoc::project_template_name_native() format argument must be a string".into());
    };
    let Some(schema_name) = args[2].as_string() else {
        return Err(
            "wdoc::project_template_name_native() schema_name argument must be a string".into(),
        );
    };
    if let Some(function) = find_metadata_template(project, format, schema_name) {
        return Ok(Value::String(function));
    }
    if format == "shape" {
        if let Some(name) = builtin_shape_template_name(project, schema_name) {
            return Ok(Value::String(name));
        }
    }
    Ok(Value::Null)
}

fn wdoc_project_extends_name_helper(args: &[Value]) -> Result<Value, String> {
    if args.len() != 2 {
        return Err("wdoc::project_extends_name_native() expects 2 arguments".into());
    }
    let Value::Map(project) = &args[0] else {
        return Err("wdoc::project_extends_name_native() project argument must be a map".into());
    };
    let Some(schema_name) = args[1].as_string() else {
        return Err(
            "wdoc::project_extends_name_native() schema_name argument must be a string".into(),
        );
    };
    if let Some(base) = find_metadata_extends(project, schema_name) {
        return Ok(Value::String(base));
    }
    Ok(Value::Null)
}

fn find_metadata_template(
    project: &IndexMap<String, Value>,
    format: &str,
    schema_name: &str,
) -> Option<String> {
    let Some(Value::Map(metadata)) = project.get("metadata") else {
        return None;
    };
    let Some(Value::List(templates)) = metadata.get("templates") else {
        return None;
    };
    templates.iter().find_map(|template| {
        let Value::Map(template) = template else {
            return None;
        };
        let item_format = template.get("format")?.as_string()?;
        let schema = template.get("schema")?.as_string()?;
        if item_format == format && project_schema_matches(schema, schema_name) {
            template.get("fn_name")?.as_string().map(str::to_string)
        } else {
            None
        }
    })
}

fn find_metadata_extends(project: &IndexMap<String, Value>, schema_name: &str) -> Option<String> {
    let Some(Value::Map(metadata)) = project.get("metadata") else {
        return None;
    };
    let Some(Value::List(extends)) = metadata.get("extends") else {
        return None;
    };
    extends.iter().find_map(|extends| {
        let Value::Map(extends) = extends else {
            return None;
        };
        let schema = extends.get("schema")?.as_string()?;
        if project_schema_matches(schema, schema_name) {
            extends.get("base")?.as_string().map(str::to_string)
        } else {
            None
        }
    })
}

fn project_schema_matches(registered: &str, requested: &str) -> bool {
    registered == requested || registered == kind_leaf(requested)
}

fn builtin_shape_template_name(
    project: &IndexMap<String, Value>,
    schema_name: &str,
) -> Option<String> {
    let values = match project.get("values") {
        Some(Value::Map(values)) => values,
        _ => return None,
    };
    let leaf = kind_leaf(schema_name);
    let widget_name = format!("wdoc::widget_{leaf}");
    if values.contains_key(&widget_name) {
        return Some(widget_name);
    }
    if let Some(terminal_leaf) = leaf.strip_prefix("terminal_") {
        let terminal_name = format!("wdoc::terminal_widget_{terminal_leaf}");
        if values.contains_key(&terminal_name) {
            return Some(terminal_name);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_builtin_accepts_svg_codec_shape_values() {
        let functions = wdoc_functions();
        let layout = functions
            .functions
            .get("wdoc::layout_center")
            .expect("layout builtin");

        let mut shape = IndexMap::new();
        shape.insert("kind".to_string(), Value::String("rect".to_string()));
        shape.insert("id".to_string(), Value::String("box".to_string()));
        shape.insert("width".to_string(), Value::Int(80));
        shape.insert("height".to_string(), Value::Int(40));

        let mut parent = IndexMap::new();
        parent.insert("x".to_string(), Value::Float(0.0));
        parent.insert("y".to_string(), Value::Float(0.0));
        parent.insert("width".to_string(), Value::Float(200.0));
        parent.insert("height".to_string(), Value::Float(120.0));

        let mut ctx = IndexMap::new();
        ctx.insert("shapes".to_string(), Value::List(vec![Value::Map(shape)]));
        ctx.insert("connections".to_string(), Value::List(vec![]));
        ctx.insert("parent".to_string(), Value::Map(parent));

        let result = layout(&[Value::Map(ctx)]).expect("layout result");
        let Value::Map(result) = result else {
            panic!("layout result should be a map");
        };
        let Some(Value::List(shapes)) = result.get("shapes") else {
            panic!("layout result should include shapes");
        };
        let Some(Value::Map(shape)) = shapes.first() else {
            panic!("layout result should include a shape map");
        };
        assert_eq!(shape.get("x"), Some(&Value::Float(60.0)));
        assert_eq!(shape.get("y"), Some(&Value::Float(40.0)));
    }

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
