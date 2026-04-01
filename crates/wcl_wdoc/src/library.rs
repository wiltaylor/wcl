/// The standard wdoc library WCL source.
///
/// This is embedded at compile time and written to a temp directory
/// so users can `import <wdoc.wcl>`.
pub const WDOC_LIBRARY_WCL: &str = include_str!("wdoc.wcl");

/// The WCL highlight.js grammar, embedded from the canonical source.
pub const WCL_HIGHLIGHTJS_GRAMMAR: &str = include_str!("../../../extras/highlightjs/wcl.js");
