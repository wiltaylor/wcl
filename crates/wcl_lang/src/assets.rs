//! Bundled static assets used by WCL standard tooling.

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
    }
}
