use std::fmt::Write;

use crate::model::{StyleRule, WdocStyle};

/// Base CSS for wdoc HTML output.
pub const BASE_CSS: &str = r#"/* wdoc base styles */
*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }

:root {
    --nav-width: 260px;
    --content-max-width: 960px;
    --font-body: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
    --font-heading: var(--font-body);
    --color-bg: #ffffff;
    --color-text: #1a1a1a;
    --color-nav-bg: #f5f5f5;
    --color-nav-border: #e0e0e0;
    --color-link: #0366d6;
    --color-nav-hover: #e8e8e8;
    --color-nav-active: #dbeafe;
}

html { font-size: 16px; }
body {
    font-family: var(--font-body);
    color: var(--color-text);
    background: var(--color-bg);
    line-height: 1.6;
    display: flex;
    min-height: 100vh;
}

/* Navigation sidebar */
.wdoc-nav {
    width: var(--nav-width);
    min-width: var(--nav-width);
    background: var(--color-nav-bg);
    border-right: 1px solid var(--color-nav-border);
    padding: 1.5rem 0;
    overflow-y: auto;
    position: fixed;
    top: 0;
    left: 0;
    bottom: 0;
}
.wdoc-nav-title {
    font-size: 1.1rem;
    font-weight: 700;
    padding: 0 1.25rem 1rem;
    border-bottom: 1px solid var(--color-nav-border);
    margin-bottom: 0.75rem;
}
.wdoc-nav ul { list-style: none; }
.wdoc-nav li a {
    display: block;
    padding: 0.35rem 1.25rem;
    color: var(--color-text);
    text-decoration: none;
    font-size: 0.9rem;
}
.wdoc-nav li a:hover { background: var(--color-nav-hover); }
.wdoc-nav li a.active { background: var(--color-nav-active); font-weight: 600; }
.wdoc-nav li ul { padding-left: 1rem; }

/* Main content area */
.wdoc-content {
    margin-left: var(--nav-width);
    flex: 1;
    padding: 2rem 2.5rem;
    max-width: var(--content-max-width);
}

/* Splits (flexbox layout) */
.wdoc-vsplit { display: flex; flex-direction: row; gap: 1.5rem; width: 100%; }
.wdoc-hsplit { display: flex; flex-direction: column; gap: 1.5rem; width: 100%; }
.wdoc-split { min-width: 0; }

/* Content blocks */
.wdoc-heading { margin-top: 1.5rem; margin-bottom: 0.5rem; font-family: var(--font-heading); }
h1.wdoc-heading { font-size: 2rem; margin-top: 0; }
h2.wdoc-heading { font-size: 1.5rem; }
h3.wdoc-heading { font-size: 1.25rem; }
h4.wdoc-heading { font-size: 1.1rem; }
h5.wdoc-heading { font-size: 1rem; }
h6.wdoc-heading { font-size: 0.9rem; }

.wdoc-paragraph { margin-bottom: 1rem; }

a { color: var(--color-link); text-decoration: none; }
a:hover { text-decoration: underline; }

/* Responsive */
@media (max-width: 768px) {
    .wdoc-nav { display: none; }
    .wdoc-content { margin-left: 0; padding: 1rem; }
    .wdoc-vsplit { flex-direction: column; }
    .wdoc-split { flex: 1 1 auto !important; }
}
"#;

/// Generate CSS from wdoc-style definitions.
pub fn generate_style_css(styles: &[WdocStyle]) -> String {
    let mut css = String::new();

    for style in styles {
        for rule in &style.rules {
            // Strip "for_" prefix from target (e.g., "for_heading" → "heading")
            let target = rule.target.strip_prefix("for_").unwrap_or(&rule.target);

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
    writeln!(css, "{selector} {{").unwrap();
    for (prop, val) in &rule.properties {
        // Convert WCL underscores to CSS hyphens (font_family → font-family)
        let css_prop = prop.replace('_', "-");
        writeln!(css, "    {css_prop}: {val};").unwrap();
    }
    css.push_str("}\n");
}
