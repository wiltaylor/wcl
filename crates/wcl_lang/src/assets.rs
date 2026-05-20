//! Bundled static assets used by WCL standard tooling.

use std::path::Path;

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

/// Return a bundled static asset by its path under the public `<WCL>:/` root.
pub fn embedded_asset_bytes(path: &Path) -> Option<&'static [u8]> {
    match path.to_str()? {
        "assets/highlightjs/wcl.js" => Some(WCL_HIGHLIGHTJS_GRAMMAR.as_bytes()),
        "assets/highlightjs/highlight.min.js" => Some(HIGHLIGHTJS_CORE.as_bytes()),
        "assets/highlightjs/github.min.css" => Some(HIGHLIGHTJS_THEME_LIGHT_CSS.as_bytes()),
        "assets/highlightjs/github-dark.min.css" => Some(HIGHLIGHTJS_THEME_DARK_CSS.as_bytes()),
        "assets/fonts/OFL.txt" => Some(JETBRAINS_MONO_NERD_OFL.as_bytes()),
        "assets/fonts/JetBrainsMonoNerdFontMono-Regular.ttf" => Some(JETBRAINS_MONO_NERD_REGULAR),
        "assets/fonts/JetBrainsMonoNerdFontMono-Bold.ttf" => Some(JETBRAINS_MONO_NERD_BOLD),
        "assets/fonts/JetBrainsMonoNerdFontMono-Italic.ttf" => Some(JETBRAINS_MONO_NERD_ITALIC),
        "assets/fonts/JetBrainsMonoNerdFontMono-BoldItalic.ttf" => {
            Some(JETBRAINS_MONO_NERD_BOLD_ITALIC)
        }
        _ => None,
    }
}

/// Return a bundled UTF-8 static asset by its path under the public `<WCL>:/` root.
pub fn embedded_asset_text(path: &Path) -> Option<&'static str> {
    match path.to_str()? {
        "assets/highlightjs/wcl.js" => Some(WCL_HIGHLIGHTJS_GRAMMAR),
        "assets/highlightjs/highlight.min.js" => Some(HIGHLIGHTJS_CORE),
        "assets/highlightjs/github.min.css" => Some(HIGHLIGHTJS_THEME_LIGHT_CSS),
        "assets/highlightjs/github-dark.min.css" => Some(HIGHLIGHTJS_THEME_DARK_CSS),
        "assets/fonts/OFL.txt" => Some(JETBRAINS_MONO_NERD_OFL),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_assets_are_present() {
        assert!(WCL_HIGHLIGHTJS_GRAMMAR.contains("WCL"));
        assert!(HIGHLIGHTJS_CORE.contains("highlight"));
        assert!(HIGHLIGHTJS_THEME_LIGHT_CSS.contains(".hljs"));
        assert!(HIGHLIGHTJS_THEME_DARK_CSS.contains(".hljs"));
        assert!(JETBRAINS_MONO_NERD_OFL.contains("SIL OPEN FONT LICENSE"));
        assert!(JETBRAINS_MONO_NERD_REGULAR.len() > 1024);
        assert!(JETBRAINS_MONO_NERD_BOLD.len() > 1024);
        assert!(JETBRAINS_MONO_NERD_ITALIC.len() > 1024);
        assert!(JETBRAINS_MONO_NERD_BOLD_ITALIC.len() > 1024);
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
    }
}
