/// The standard wdoc library WCL source.
pub const WDOC_LIBRARY_WCL: &str = wcl_lang::standard_lib::WDOC_LIBRARY_WCL;

/// The WCL highlight.js grammar.
pub const WCL_HIGHLIGHTJS_GRAMMAR: &str = wcl_lang::assets::WCL_HIGHLIGHTJS_GRAMMAR;

/// highlight.js core library (minified).
pub const HIGHLIGHTJS_CORE: &str = wcl_lang::assets::HIGHLIGHTJS_CORE;

/// highlight.js GitHub light theme CSS (minified).
pub const HIGHLIGHTJS_THEME_LIGHT_CSS: &str = wcl_lang::assets::HIGHLIGHTJS_THEME_LIGHT_CSS;

/// highlight.js GitHub dark theme CSS (minified).
pub const HIGHLIGHTJS_THEME_DARK_CSS: &str = wcl_lang::assets::HIGHLIGHTJS_THEME_DARK_CSS;

/// Bundled JetBrainsMono Nerd Font assets for terminal diagrams.
pub const JETBRAINS_MONO_NERD_REGULAR: &[u8] = wcl_lang::assets::JETBRAINS_MONO_NERD_REGULAR;
pub const JETBRAINS_MONO_NERD_BOLD: &[u8] = wcl_lang::assets::JETBRAINS_MONO_NERD_BOLD;
pub const JETBRAINS_MONO_NERD_ITALIC: &[u8] = wcl_lang::assets::JETBRAINS_MONO_NERD_ITALIC;
pub const JETBRAINS_MONO_NERD_BOLD_ITALIC: &[u8] =
    wcl_lang::assets::JETBRAINS_MONO_NERD_BOLD_ITALIC;
pub const JETBRAINS_MONO_NERD_OFL: &str = wcl_lang::assets::JETBRAINS_MONO_NERD_OFL;

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::WDOC_LIBRARY_WCL;

    #[test]
    fn wireframe_widget_schemas_are_reachable_from_bundled_entrypoint() {
        let doc = wcl_lang::parse(
            WDOC_LIBRARY_WCL,
            wcl_lang::ParseOptions {
                root_dir: PathBuf::from(wcl_lang::eval::imports::EMBEDDED_LIBRARY_ROOT),
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
}
