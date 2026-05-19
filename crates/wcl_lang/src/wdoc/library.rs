use std::path::PathBuf;
use std::sync::OnceLock;

/// The standard wdoc library WCL source.
pub const WDOC_LIBRARY_WCL: &str = crate::standard_lib::WDOC_LIBRARY_WCL;

/// The WCL highlight.js grammar.
pub const WCL_HIGHLIGHTJS_GRAMMAR: &str = crate::assets::WCL_HIGHLIGHTJS_GRAMMAR;

/// highlight.js core library (minified).
pub const HIGHLIGHTJS_CORE: &str = crate::assets::HIGHLIGHTJS_CORE;

/// highlight.js GitHub light theme CSS (minified).
pub const HIGHLIGHTJS_THEME_LIGHT_CSS: &str = crate::assets::HIGHLIGHTJS_THEME_LIGHT_CSS;

/// highlight.js GitHub dark theme CSS (minified).
pub const HIGHLIGHTJS_THEME_DARK_CSS: &str = crate::assets::HIGHLIGHTJS_THEME_DARK_CSS;

/// Bundled JetBrainsMono Nerd Font assets for terminal diagrams.
pub const JETBRAINS_MONO_NERD_REGULAR: &[u8] = crate::assets::JETBRAINS_MONO_NERD_REGULAR;
pub const JETBRAINS_MONO_NERD_BOLD: &[u8] = crate::assets::JETBRAINS_MONO_NERD_BOLD;
pub const JETBRAINS_MONO_NERD_ITALIC: &[u8] = crate::assets::JETBRAINS_MONO_NERD_ITALIC;
pub const JETBRAINS_MONO_NERD_BOLD_ITALIC: &[u8] = crate::assets::JETBRAINS_MONO_NERD_BOLD_ITALIC;
pub const JETBRAINS_MONO_NERD_OFL: &str = crate::assets::JETBRAINS_MONO_NERD_OFL;

#[derive(Clone)]
struct RuntimeAssets {
    mathjax_config: String,
    theme: String,
    presentation: String,
    page_signal_template: String,
    diagram: String,
}

static RUNTIME_ASSETS: OnceLock<Result<RuntimeAssets, String>> = OnceLock::new();

pub fn mathjax_config_js() -> Result<&'static str, String> {
    runtime_assets().map(|assets| assets.mathjax_config.as_str())
}

pub fn theme_runtime_js() -> Result<&'static str, String> {
    runtime_assets().map(|assets| assets.theme.as_str())
}

pub fn presentation_runtime_js() -> Result<&'static str, String> {
    runtime_assets().map(|assets| assets.presentation.as_str())
}

pub fn page_signal_runtime_js(config_json: &str) -> Result<String, String> {
    let template = runtime_assets()?.page_signal_template.as_str();
    Ok(template.replace("__WDOC_SIGNAL_CONFIG_JSON__", config_json))
}

pub fn diagram_runtime_js() -> Result<&'static str, String> {
    runtime_assets().map(|assets| assets.diagram.as_str())
}

fn runtime_assets() -> Result<&'static RuntimeAssets, String> {
    RUNTIME_ASSETS
        .get_or_init(load_runtime_assets)
        .as_ref()
        .map_err(Clone::clone)
}

fn load_runtime_assets() -> Result<RuntimeAssets, String> {
    let doc = crate::parse(
        WDOC_LIBRARY_WCL,
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
        return Err(format!(
            "failed to parse bundled wdoc runtime assets: {errors}"
        ));
    }

    Ok(RuntimeAssets {
        mathjax_config: runtime_string(&doc, "__wdoc_mathjax_config_js")?,
        theme: runtime_string(&doc, "__wdoc_theme_runtime_js")?,
        presentation: runtime_string(&doc, "__wdoc_presentation_runtime_js")?,
        page_signal_template: runtime_string(&doc, "__wdoc_page_signal_runtime_js_template")?,
        diagram: runtime_string(&doc, "__wdoc_diagram_runtime_js")?,
    })
}

fn runtime_string(doc: &crate::Document, name: &str) -> Result<String, String> {
    doc.values
        .get(name)
        .and_then(|value| value.as_string())
        .map(str::to_string)
        .ok_or_else(|| format!("bundled wdoc runtime asset '{name}' was not found"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wireframe_widget_schemas_are_reachable_from_bundled_entrypoint() {
        let doc = crate::parse(
            WDOC_LIBRARY_WCL,
            crate::ParseOptions {
                root_dir: PathBuf::from(crate::eval::imports::EMBEDDED_LIBRARY_ROOT),
                ..Default::default()
            },
        );
        assert!(
            !doc.has_errors(),
            "unexpected diagnostics: {:?}",
            doc.errors()
        );

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
}
