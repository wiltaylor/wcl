use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

use indexmap::IndexMap;

use crate::eval::value::{FunctionValue, Value};
use crate::wdoc::markup;
use crate::wdoc::model::{StyleRule, WdocStyle};

#[derive(Clone)]
struct RuntimeAssets {
    mathjax_config: String,
    theme: String,
    presentation: String,
    page_signal_template: String,
    diagram: String,
}

static RUNTIME_ASSETS: OnceLock<Result<RuntimeAssets, String>> = OnceLock::new();
static BASE_CSS: OnceLock<Result<String, String>> = OnceLock::new();

pub(crate) fn base_css() -> Result<String, String> {
    BASE_CSS.get_or_init(render_base_css_from_wcl).clone()
}

pub(crate) fn style_css(styles: &[WdocStyle]) -> Result<String, String> {
    let styles_value = Value::List(styles.iter().map(style_value).collect());
    let value = call_wdoc_function("wdoc::style_css", &[styles_value])?;
    markup::render_css(&value)
}

pub(crate) fn mathjax_config_js() -> Result<&'static str, String> {
    runtime_assets().map(|assets| assets.mathjax_config.as_str())
}

pub(crate) fn theme_runtime_js() -> Result<&'static str, String> {
    runtime_assets().map(|assets| assets.theme.as_str())
}

pub(crate) fn presentation_runtime_js() -> Result<&'static str, String> {
    runtime_assets().map(|assets| assets.presentation.as_str())
}

pub(crate) fn page_signal_runtime_js(config_json: &str) -> Result<String, String> {
    let template = runtime_assets()?.page_signal_template.as_str();
    Ok(template.replace("__WDOC_SIGNAL_CONFIG_JSON__", config_json))
}

pub(crate) fn diagram_runtime_js() -> Result<&'static str, String> {
    runtime_assets().map(|assets| assets.diagram.as_str())
}

fn runtime_assets() -> Result<&'static RuntimeAssets, String> {
    RUNTIME_ASSETS
        .get_or_init(load_runtime_assets)
        .as_ref()
        .map_err(Clone::clone)
}

fn render_base_css_from_wcl() -> Result<String, String> {
    let doc = parse_wdoc_library("stylesheet")?;
    let stylesheet = doc
        .values
        .get("__wdoc_base_styles")
        .ok_or_else(|| "bundled wdoc stylesheet '__wdoc_base_styles' was not found".to_string())?;
    markup::render_css(stylesheet)
}

fn load_runtime_assets() -> Result<RuntimeAssets, String> {
    let doc = parse_wdoc_library("runtime assets")?;
    Ok(RuntimeAssets {
        mathjax_config: runtime_string(&doc, "__wdoc_mathjax_config_js")?,
        theme: runtime_string(&doc, "__wdoc_theme_runtime_js")?,
        presentation: runtime_string(&doc, "__wdoc_presentation_runtime_js")?,
        page_signal_template: runtime_string(&doc, "__wdoc_page_signal_runtime_js_template")?,
        diagram: runtime_string(&doc, "__wdoc_diagram_runtime_js")?,
    })
}

fn parse_wdoc_library(context: &str) -> Result<crate::Document, String> {
    let doc = crate::parse(
        crate::standard_lib::WDOC_LIBRARY_WCL,
        crate::ParseOptions {
            root_dir: PathBuf::from(crate::eval::imports::EMBEDDED_LIBRARY_ROOT),
            ..Default::default()
        },
    );
    if doc.has_errors() {
        let errors = doc
            .errors()
            .into_iter()
            .map(|diagnostic| diagnostic.message.clone())
            .collect::<Vec<_>>()
            .join("; ");
        return Err(format!("failed to parse bundled wdoc {context}: {errors}"));
    }
    Ok(doc)
}

fn runtime_string(doc: &crate::Document, name: &str) -> Result<String, String> {
    doc.values
        .get(name)
        .and_then(|value| value.as_string())
        .map(str::to_string)
        .ok_or_else(|| format!("bundled wdoc runtime asset '{name}' was not found"))
}

fn call_wdoc_function(name: &str, args: &[Value]) -> Result<Value, String> {
    let doc = parse_wdoc_library("asset helpers")?;
    let helpers = function_values(&doc.values);
    let func = helpers
        .get(name)
        .ok_or_else(|| format!("bundled wdoc asset helper '{name}' was not found"))?;
    crate::call_lambda_with_env(func, args, &HashMap::new(), &helpers)
}

fn function_values(values: &IndexMap<String, Value>) -> HashMap<String, FunctionValue> {
    values
        .iter()
        .filter_map(|(name, value)| match value {
            Value::Function(func) => Some((name.clone(), func.clone())),
            _ => None,
        })
        .collect()
}

fn style_value(style: &WdocStyle) -> Value {
    let mut map = IndexMap::new();
    map.insert("name".to_string(), Value::String(style.name.clone()));
    map.insert(
        "rules".to_string(),
        Value::List(style.rules.iter().map(style_rule_value).collect()),
    );
    Value::Map(map)
}

fn style_rule_value(rule: &StyleRule) -> Value {
    let mut map = IndexMap::new();
    map.insert("target".to_string(), Value::String(rule.target.clone()));
    map.insert(
        "properties".to_string(),
        Value::Map(
            rule.properties
                .iter()
                .map(|(key, value)| (key.clone(), Value::String(value.clone())))
                .collect(),
        ),
    );
    Value::Map(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base_css_renders_from_bundled_wcl_stylesheet() {
        let css = base_css().expect("base css");

        assert!(css.contains(":root"));
        assert!(css.contains("@font-face {"));
        assert!(css.contains("@keyframes wdoc-terminal-blink"));
        assert!(css.contains("@media (max-width: 768px)"));
        assert!(css.contains(".wdoc-content"));
    }

    #[test]
    fn style_css_renders_with_wcl_helper() {
        let css = style_css(&[WdocStyle {
            name: "note".to_string(),
            rules: vec![StyleRule {
                target: "wdoc::paragraph".to_string(),
                properties: IndexMap::from([("color".to_string(), "red".to_string())]),
            }],
        }])
        .expect("style css");

        assert!(css.contains(".wdoc-style-note--paragraph"));
        assert!(css.contains("color: red;"));
    }

    #[test]
    fn runtime_assets_are_exposed_by_bundled_entrypoint() {
        assert!(theme_runtime_js()
            .expect("theme runtime")
            .contains("wdoc-theme"));
        assert!(presentation_runtime_js()
            .expect("presentation runtime")
            .contains("data-wdoc-slide-right"));
        assert!(mathjax_config_js()
            .expect("mathjax config")
            .contains("MathJax"));
        assert!(diagram_runtime_js()
            .expect("diagram runtime")
            .contains("__wdocDiagramRuntimeInit"));
        assert!(page_signal_runtime_js("{\"signals\":[],\"bindings\":[]}")
            .expect("page signal runtime")
            .contains("__wdocPageSignalsInit"));
    }

    #[test]
    fn wireframe_widget_schemas_are_reachable_from_bundled_entrypoint() {
        let doc = parse_wdoc_library("test").expect("wdoc library");

        for widget in [
            "checkbox",
            "radio",
            "slider",
            "button_group",
            "textbox",
            "dropdown",
            "inline_image",
            "menubar",
            "context_menu",
            "stat_card",
            "profile_card",
            "action_panel",
            "list_item",
            "window",
            "tablet",
            "phone_landscape",
            "tablet_landscape",
            "graph_node",
            "pie_chart",
            "bar_chart",
            "line_chart",
        ] {
            assert!(
                doc.schemas
                    .get_schema(&format!("wdoc::draw::{widget}"), None)
                    .is_some(),
                "missing schema for {widget}"
            );
            assert!(
                doc.values.contains_key(&format!("wdoc::widget_{widget}")),
                "missing template for {widget}"
            );
        }
    }

    #[test]
    fn widget_sources_use_categories() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|path| path.parent())
            .expect("workspace root")
            .join("crates/wcl_lang/src/std/wdoc");
        for path in [
            "widgets/ui/button.wcl",
            "widgets/graph/graph_node.wcl",
            "widgets/chart/charts.wcl",
            "widgets/flowchart/flowchart.wcl",
            "widgets/flowchart/flow_process.wcl",
            "widgets/c4/c4_person.wcl",
            "widgets/uml/uml_class.wcl",
            "widgets/infra/server.wcl",
        ] {
            assert!(
                root.join(path).is_file(),
                "categorized widget file should exist: {path}"
            );
        }
    }
}
