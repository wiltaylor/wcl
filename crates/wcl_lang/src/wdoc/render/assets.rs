use crate::eval::value::Value;
use crate::wdoc::markup;
use crate::wdoc::model::{StyleRule, WdocStyle};
use indexmap::IndexMap;
use std::sync::OnceLock;

static WCL_BASE_STYLES_CACHE: OnceLock<Result<String, String>> = OnceLock::new();

/// Render the bundled WCL-authored base stylesheet.
pub fn base_css() -> Result<String, String> {
    WCL_BASE_STYLES_CACHE
        .get_or_init(render_base_css_from_wcl)
        .clone()
}

fn render_base_css_from_wcl() -> Result<String, String> {
    let doc = crate::parse(
        crate::standard_lib::WDOC_LIBRARY_WCL,
        crate::ParseOptions {
            root_dir: std::path::PathBuf::from(crate::eval::imports::EMBEDDED_LIBRARY_ROOT),
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
        return Err(format!("failed to parse bundled wdoc stylesheet: {errors}"));
    }

    let stylesheet = doc
        .values
        .get("__wdoc_base_styles")
        .ok_or_else(|| "bundled wdoc stylesheet '__wdoc_base_styles' was not found".to_string())?;
    markup::render_css(stylesheet)
}

/// Generate CSS from wdoc-style definitions.
pub fn generate_style_css(styles: &[WdocStyle]) -> String {
    let mut css = String::new();

    for style in styles {
        for rule in &style.rules {
            // Extract leaf name from target (e.g., "wdoc::heading" → "heading")
            let target = rule.target.rsplit("::").next().unwrap_or(&rule.target);

            if style.name == "default" {
                write_rule(&mut css, &format!(".wdoc-{target}"), rule);
            } else {
                write_rule(
                    &mut css,
                    &format!(".wdoc-style-{}--{target}", style.name),
                    rule,
                );
            }
        }
    }

    css
}

fn write_rule(css: &mut String, selector: &str, rule: &StyleRule) {
    if rule.properties.is_empty() {
        return;
    }
    let mut map = IndexMap::new();
    map.insert("kind".to_string(), Value::String("rule".to_string()));
    map.insert("selector".to_string(), Value::String(selector.to_string()));
    for (prop, val) in &rule.properties {
        map.insert(prop.clone(), Value::String(val.clone()));
    }
    let rendered =
        markup::render_css(&Value::Map(map)).expect("wdoc style rule should serialize as CSS");
    css.push_str(&rendered);
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
}
