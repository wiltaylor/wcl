/// The standard wdoc library WCL source.
pub const WDOC_LIBRARY_WCL: &str = include_str!(concat!(env!("OUT_DIR"), "/wdoc.wcl"));

/// The WCL highlight.js grammar.
pub const WCL_HIGHLIGHTJS_GRAMMAR: &str = include_str!("../../../extras/highlightjs/wcl.js");

/// highlight.js core library (minified).
pub const HIGHLIGHTJS_CORE: &str = include_str!("../../../extras/highlightjs/highlight.min.js");

/// highlight.js GitHub light theme CSS (minified).
pub const HIGHLIGHTJS_THEME_LIGHT_CSS: &str =
    include_str!("../../../extras/highlightjs/github.min.css");

/// highlight.js GitHub dark theme CSS (minified).
pub const HIGHLIGHTJS_THEME_DARK_CSS: &str =
    include_str!("../../../extras/highlightjs/github-dark.min.css");

/// Bundled JetBrainsMono Nerd Font assets for terminal diagrams.
pub const JETBRAINS_MONO_NERD_REGULAR: &[u8] =
    include_bytes!("../assets/fonts/JetBrainsMonoNerdFontMono-Regular.ttf");
pub const JETBRAINS_MONO_NERD_BOLD: &[u8] =
    include_bytes!("../assets/fonts/JetBrainsMonoNerdFontMono-Bold.ttf");
pub const JETBRAINS_MONO_NERD_ITALIC: &[u8] =
    include_bytes!("../assets/fonts/JetBrainsMonoNerdFontMono-Italic.ttf");
pub const JETBRAINS_MONO_NERD_BOLD_ITALIC: &[u8] =
    include_bytes!("../assets/fonts/JetBrainsMonoNerdFontMono-BoldItalic.ttf");

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::WDOC_LIBRARY_WCL;

    #[test]
    fn wireframe_widget_schemas_are_bundled() {
        for widget in [
            "checkbox",
            "radio",
            "button_group",
            "textbox",
            "dropdown",
            "inline_image",
            "menubar",
            "context_menu",
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
                WDOC_LIBRARY_WCL.contains(&format!("schema \"{widget}\"")),
                "missing schema for {widget}"
            );
            assert!(
                WDOC_LIBRARY_WCL.contains(&format!("widget_{widget}")),
                "missing template for {widget}"
            );
        }
    }

    #[test]
    fn widget_manifest_uses_categories() {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("src")
            .join("wdoc");
        let manifest = std::fs::read_to_string(manifest_dir.join("manifest.txt"))
            .expect("manifest should be readable");

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
            assert!(manifest.contains(path), "missing categorized path {path}");
            assert!(
                manifest_dir.join(path).is_file(),
                "categorized widget file should exist: {path}"
            );
        }
    }
}
