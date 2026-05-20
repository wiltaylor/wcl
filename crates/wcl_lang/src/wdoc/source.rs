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
