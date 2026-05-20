//! Bundled static assets used by WCL standard tooling.

use std::path::Path;

use crate::eval::EmbeddedLibrary;

pub const CODECS_LIBRARY_WCL: &str = include_str!("std/codecs.wcl");
pub const HTML_LIBRARY_WCL: &str = include_str!("std/html.wcl");
pub const SVG_LIBRARY_WCL: &str = include_str!("std/svg.wcl");
pub const CSS_LIBRARY_WCL: &str = include_str!("std/css.wcl");
pub const WDOC_LIBRARY_WCL: &str = include_str!("std/wdoc.wcl");

/// WDoc base stylesheet.
pub const WDOC_BASE_STYLES_CSS: &str = include_str!("assets/wdoc/base_styles.css");

/// WDoc browser runtime assets.
pub const WDOC_RUNTIME_MATHJAX_CONFIG_JS: &str =
    include_str!("assets/wdoc/runtime/mathjax_config.js");
pub const WDOC_RUNTIME_THEME_JS: &str = include_str!("assets/wdoc/runtime/theme.js");
pub const WDOC_RUNTIME_PRESENTATION_JS: &str = include_str!("assets/wdoc/runtime/presentation.js");
pub const WDOC_RUNTIME_PAGE_SIGNAL_TEMPLATE_JS: &str =
    include_str!("assets/wdoc/runtime/page_signal_template.js");
pub const WDOC_RUNTIME_DIAGRAM_JS: &str = include_str!("assets/wdoc/runtime/diagram.js");

/// The WCL highlight.js grammar.
pub const WCL_HIGHLIGHTJS_GRAMMAR: &str = include_str!("assets/highlightjs/wcl.js");

/// highlight.js core library (minified).
pub const HIGHLIGHTJS_CORE: &str = include_str!("assets/highlightjs/highlight.min.js");

/// highlight.js GitHub light theme CSS (minified).
pub const HIGHLIGHTJS_THEME_LIGHT_CSS: &str = include_str!("assets/highlightjs/github.min.css");

/// highlight.js GitHub dark theme CSS (minified).
pub const HIGHLIGHTJS_THEME_DARK_CSS: &str = include_str!("assets/highlightjs/github-dark.min.css");

/// JetBrainsMono Nerd Font license.
pub const JETBRAINS_MONO_NERD_OFL: &str = include_str!("assets/fonts/OFL.txt");

/// Bundled JetBrainsMono Nerd Font assets for terminal diagrams.
pub const JETBRAINS_MONO_NERD_REGULAR: &[u8] =
    include_bytes!("assets/fonts/JetBrainsMonoNerdFontMono-Regular.ttf");
pub const JETBRAINS_MONO_NERD_BOLD: &[u8] =
    include_bytes!("assets/fonts/JetBrainsMonoNerdFontMono-Bold.ttf");
pub const JETBRAINS_MONO_NERD_ITALIC: &[u8] =
    include_bytes!("assets/fonts/JetBrainsMonoNerdFontMono-Italic.ttf");
pub const JETBRAINS_MONO_NERD_BOLD_ITALIC: &[u8] =
    include_bytes!("assets/fonts/JetBrainsMonoNerdFontMono-BoldItalic.ttf");

#[derive(Debug, Clone, Copy)]
pub struct EmbeddedTextAsset {
    pub path: &'static str,
    pub contents: &'static str,
}

#[derive(Debug, Clone, Copy)]
pub struct EmbeddedBinaryAsset {
    pub path: &'static str,
    pub contents: &'static [u8],
}

macro_rules! text_asset {
    ($path:literal, $contents:expr) => {
        EmbeddedTextAsset {
            path: $path,
            contents: $contents,
        }
    };
}

macro_rules! wcl_asset {
    ($path:literal) => {
        text_asset!($path, include_str!(concat!("std/", $path)))
    };
}

pub const EMBEDDED_TEXT_ASSETS: &[EmbeddedTextAsset] = &[
    text_asset!("codecs.wcl", CODECS_LIBRARY_WCL),
    text_asset!("html.wcl", HTML_LIBRARY_WCL),
    text_asset!("svg.wcl", SVG_LIBRARY_WCL),
    text_asset!("css.wcl", CSS_LIBRARY_WCL),
    text_asset!("wdoc.wcl", WDOC_LIBRARY_WCL),
    wcl_asset!("wdoc/header.wcl"),
    wcl_asset!("wdoc/widgets/ui/button.wcl"),
    wcl_asset!("wdoc/widgets/ui/slider.wcl"),
    wcl_asset!("wdoc/widgets/ui/phone.wcl"),
    wcl_asset!("wdoc/widgets/ui/phone_landscape.wcl"),
    wcl_asset!("wdoc/widgets/ui/browser.wcl"),
    wcl_asset!("wdoc/widgets/ui/window.wcl"),
    wcl_asset!("wdoc/widgets/ui/tablet.wcl"),
    wcl_asset!("wdoc/widgets/ui/tablet_landscape.wcl"),
    wcl_asset!("wdoc/widgets/ui/input.wcl"),
    wcl_asset!("wdoc/widgets/ui/card.wcl"),
    wcl_asset!("wdoc/widgets/ui/collapsible_panel.wcl"),
    wcl_asset!("wdoc/widgets/ui/avatar.wcl"),
    wcl_asset!("wdoc/widgets/ui/toggle.wcl"),
    wcl_asset!("wdoc/widgets/ui/checkbox.wcl"),
    wcl_asset!("wdoc/widgets/ui/radio.wcl"),
    wcl_asset!("wdoc/widgets/ui/button_group.wcl"),
    wcl_asset!("wdoc/widgets/ui/textbox.wcl"),
    wcl_asset!("wdoc/widgets/ui/dropdown.wcl"),
    wcl_asset!("wdoc/widgets/ui/inline_image.wcl"),
    wcl_asset!("wdoc/widgets/ui/menubar.wcl"),
    wcl_asset!("wdoc/widgets/ui/context_menu.wcl"),
    wcl_asset!("wdoc/widgets/ui/badge.wcl"),
    wcl_asset!("wdoc/widgets/ui/navbar.wcl"),
    wcl_asset!("wdoc/widgets/ui/stat_card.wcl"),
    wcl_asset!("wdoc/widgets/ui/profile_card.wcl"),
    wcl_asset!("wdoc/widgets/ui/action_panel.wcl"),
    wcl_asset!("wdoc/widgets/ui/list_item.wcl"),
    wcl_asset!("wdoc/widgets/ui/datatable.wcl"),
    wcl_asset!("wdoc/widgets/graph/graph_node.wcl"),
    wcl_asset!("wdoc/widgets/chart/charts.wcl"),
    wcl_asset!("wdoc/widgets/terminal/button.wcl"),
    wcl_asset!("wdoc/widgets/terminal/textbox.wcl"),
    wcl_asset!("wdoc/widgets/terminal/checkbox.wcl"),
    wcl_asset!("wdoc/widgets/terminal/radio.wcl"),
    wcl_asset!("wdoc/widgets/terminal/menu.wcl"),
    wcl_asset!("wdoc/widgets/terminal/menubar.wcl"),
    wcl_asset!("wdoc/widgets/terminal/dropdown.wcl"),
    wcl_asset!("wdoc/widgets/flowchart/flowchart.wcl"),
    wcl_asset!("wdoc/widgets/flowchart/flow_process.wcl"),
    wcl_asset!("wdoc/widgets/flowchart/flow_decision.wcl"),
    wcl_asset!("wdoc/widgets/flowchart/flow_terminal.wcl"),
    wcl_asset!("wdoc/widgets/flowchart/flow_io.wcl"),
    wcl_asset!("wdoc/widgets/flowchart/flow_subprocess.wcl"),
    wcl_asset!("wdoc/widgets/c4/c4_person.wcl"),
    wcl_asset!("wdoc/widgets/c4/c4_system.wcl"),
    wcl_asset!("wdoc/widgets/c4/c4_container.wcl"),
    wcl_asset!("wdoc/widgets/c4/c4_component.wcl"),
    wcl_asset!("wdoc/widgets/c4/c4_boundary.wcl"),
    wcl_asset!("wdoc/widgets/uml/uml_class.wcl"),
    wcl_asset!("wdoc/widgets/uml/uml_actor.wcl"),
    wcl_asset!("wdoc/widgets/uml/uml_package.wcl"),
    wcl_asset!("wdoc/widgets/uml/uml_note.wcl"),
    wcl_asset!("wdoc/widgets/infra/server.wcl"),
    wcl_asset!("wdoc/widgets/infra/database.wcl"),
    wcl_asset!("wdoc/widgets/infra/cloud.wcl"),
    wcl_asset!("wdoc/widgets/infra/user.wcl"),
    text_asset!("assets/wdoc/base_styles.css", WDOC_BASE_STYLES_CSS),
    text_asset!(
        "assets/wdoc/runtime/mathjax_config.js",
        WDOC_RUNTIME_MATHJAX_CONFIG_JS
    ),
    text_asset!("assets/wdoc/runtime/theme.js", WDOC_RUNTIME_THEME_JS),
    text_asset!(
        "assets/wdoc/runtime/presentation.js",
        WDOC_RUNTIME_PRESENTATION_JS
    ),
    text_asset!(
        "assets/wdoc/runtime/page_signal_template.js",
        WDOC_RUNTIME_PAGE_SIGNAL_TEMPLATE_JS
    ),
    text_asset!("assets/wdoc/runtime/diagram.js", WDOC_RUNTIME_DIAGRAM_JS),
    text_asset!("assets/highlightjs/wcl.js", WCL_HIGHLIGHTJS_GRAMMAR),
    text_asset!("assets/highlightjs/highlight.min.js", HIGHLIGHTJS_CORE),
    text_asset!(
        "assets/highlightjs/github.min.css",
        HIGHLIGHTJS_THEME_LIGHT_CSS
    ),
    text_asset!(
        "assets/highlightjs/github-dark.min.css",
        HIGHLIGHTJS_THEME_DARK_CSS
    ),
    text_asset!("assets/fonts/OFL.txt", JETBRAINS_MONO_NERD_OFL),
];

pub const EMBEDDED_BINARY_ASSETS: &[EmbeddedBinaryAsset] = &[
    EmbeddedBinaryAsset {
        path: "assets/fonts/JetBrainsMonoNerdFontMono-Regular.ttf",
        contents: JETBRAINS_MONO_NERD_REGULAR,
    },
    EmbeddedBinaryAsset {
        path: "assets/fonts/JetBrainsMonoNerdFontMono-Bold.ttf",
        contents: JETBRAINS_MONO_NERD_BOLD,
    },
    EmbeddedBinaryAsset {
        path: "assets/fonts/JetBrainsMonoNerdFontMono-Italic.ttf",
        contents: JETBRAINS_MONO_NERD_ITALIC,
    },
    EmbeddedBinaryAsset {
        path: "assets/fonts/JetBrainsMonoNerdFontMono-BoldItalic.ttf",
        contents: JETBRAINS_MONO_NERD_BOLD_ITALIC,
    },
];

pub fn embedded_libraries() -> Vec<EmbeddedLibrary> {
    EMBEDDED_TEXT_ASSETS
        .iter()
        .filter(|asset| asset.path.ends_with(".wcl"))
        .map(|asset| EmbeddedLibrary {
            name: asset.path,
            path: asset.path,
            source: asset.contents,
        })
        .collect()
}

/// Return a bundled static asset by its path under the public `<WCL>:/` root.
pub fn embedded_asset_bytes(path: &Path) -> Option<&'static [u8]> {
    let path = path.to_str()?;
    EMBEDDED_BINARY_ASSETS
        .iter()
        .find(|asset| asset.path == path)
        .map(|asset| asset.contents)
        .or_else(|| {
            EMBEDDED_TEXT_ASSETS
                .iter()
                .find(|asset| asset.path == path)
                .map(|asset| asset.contents.as_bytes())
        })
}

/// Return a bundled UTF-8 static asset by its path under the public `<WCL>:/` root.
pub fn embedded_asset_text(path: &Path) -> Option<&'static str> {
    let path = path.to_str()?;
    EMBEDDED_TEXT_ASSETS
        .iter()
        .find(|asset| asset.path == path)
        .map(|asset| asset.contents)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn bundled_assets_are_present() {
        assert!(WCL_HIGHLIGHTJS_GRAMMAR.contains("WCL"));
        assert!(HIGHLIGHTJS_CORE.contains("highlight"));
        assert!(HIGHLIGHTJS_THEME_LIGHT_CSS.contains(".hljs"));
        assert!(HIGHLIGHTJS_THEME_DARK_CSS.contains(".hljs"));
        assert!(WDOC_BASE_STYLES_CSS.contains(".wdoc-content"));
        assert!(WDOC_RUNTIME_THEME_JS.contains("wdoc-theme"));
        assert!(WDOC_RUNTIME_DIAGRAM_JS.contains("__wdocDiagramRuntimeInit"));
        assert!(JETBRAINS_MONO_NERD_OFL.contains("SIL OPEN FONT LICENSE"));
        assert!(JETBRAINS_MONO_NERD_REGULAR.len() > 1024);
        assert!(JETBRAINS_MONO_NERD_BOLD.len() > 1024);
        assert!(JETBRAINS_MONO_NERD_ITALIC.len() > 1024);
        assert!(JETBRAINS_MONO_NERD_BOLD_ITALIC.len() > 1024);
        assert!(embedded_asset_text(Path::new("assets/highlightjs/wcl.js"))
            .expect("wcl highlight asset")
            .contains("WCL"));
        assert!(
            embedded_asset_text(Path::new("assets/wdoc/base_styles.css"))
                .expect("wdoc base styles asset")
                .contains(".wdoc-content")
        );
        assert!(
            embedded_asset_text(Path::new("assets/wdoc/runtime/diagram.js"))
                .expect("wdoc diagram runtime asset")
                .contains("__wdocDiagramRuntimeInit")
        );
        assert!(
            embedded_asset_bytes(Path::new(
                "assets/fonts/JetBrainsMonoNerdFontMono-Regular.ttf"
            ))
            .expect("font asset")
            .len()
                > 1024
        );
    }

    #[test]
    fn embedded_libraries_are_backed_by_text_assets() {
        let text_paths = EMBEDDED_TEXT_ASSETS
            .iter()
            .map(|asset| asset.path)
            .collect::<HashSet<_>>();
        let libraries = embedded_libraries();

        assert!(libraries.iter().any(|library| library.name == "wdoc.wcl"));
        assert!(libraries
            .iter()
            .any(|library| library.name == "wdoc/header.wcl"));

        for library in libraries {
            assert_eq!(library.name, library.path);
            assert!(
                text_paths.contains(library.path),
                "embedded library '{}' should be in the central text asset registry",
                library.path
            );
            assert_eq!(
                embedded_asset_text(Path::new(library.path)),
                Some(library.source)
            );
        }
    }

    #[test]
    fn embedded_asset_registry_covers_text_and_binary_paths() {
        assert!(embedded_asset_text(Path::new("wdoc/header.wcl"))
            .expect("wdoc header")
            .contains("namespace wdoc"));
        assert!(embedded_asset_text(Path::new("assets/highlightjs/wcl.js"))
            .expect("wcl highlight asset")
            .contains("WCL"));
        assert!(
            embedded_asset_bytes(Path::new(
                "assets/fonts/JetBrainsMonoNerdFontMono-Regular.ttf"
            ))
            .expect("font asset")
            .len()
                > 1024
        );
        assert!(embedded_asset_text(Path::new(
            "assets/fonts/JetBrainsMonoNerdFontMono-Regular.ttf"
        ))
        .is_none());
        assert!(embedded_asset_bytes(Path::new("missing.asset")).is_none());
    }

    #[test]
    fn embedded_asset_paths_are_unique() {
        let mut paths = HashSet::new();
        for asset in EMBEDDED_TEXT_ASSETS {
            assert!(
                paths.insert(asset.path),
                "duplicate text asset {}",
                asset.path
            );
        }
        for asset in EMBEDDED_BINARY_ASSETS {
            assert!(
                paths.insert(asset.path),
                "duplicate embedded asset {}",
                asset.path
            );
        }
    }
}
