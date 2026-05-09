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

#[cfg(test)]
mod tests {
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
}
