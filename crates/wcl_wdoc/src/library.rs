/// The standard wdoc library WCL source.
pub const WDOC_LIBRARY_WCL: &str = include_str!("wdoc.wcl");

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
