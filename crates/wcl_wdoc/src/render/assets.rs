use std::fmt::Write;

use crate::model::{StyleRule, WdocStyle};

/// Base CSS for wdoc HTML output.
pub const BASE_CSS: &str = r#"/* wdoc base styles */
*, *::before, *::after { box-sizing: border-box; margin: 0; padding: 0; }

/* Light theme (default) */
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
    --color-code-bg: #f6f8fa;
    --color-code-border: #e1e4e8;
    --color-toggle-bg: #e0e0e0;
    --color-toggle-knob: #ffffff;
    --color-table-border: #d0d7de;
    --color-table-header-bg: #f0f3f6;
    --color-table-stripe: #f6f8fa;
}

/* Dark theme */
[data-theme="dark"] {
    --color-bg: #0d1117;
    --color-text: #e6edf3;
    --color-nav-bg: #161b22;
    --color-nav-border: #30363d;
    --color-link: #58a6ff;
    --color-nav-hover: #1f2937;
    --color-nav-active: #1c3a5f;
    --color-code-bg: #161b22;
    --color-code-border: #30363d;
    --color-toggle-bg: #30363d;
    --color-toggle-knob: #e6edf3;
    --color-table-border: #30363d;
    --color-table-header-bg: #161b22;
    --color-table-stripe: #0d1117;
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
    display: flex;
    flex-direction: column;
}
.wdoc-nav-title {
    font-size: 1.1rem;
    font-weight: 700;
    padding: 0 1.25rem 1rem;
    border-bottom: 1px solid var(--color-nav-border);
    margin-bottom: 0.75rem;
}
.wdoc-nav ul { list-style: none; flex: 1; }
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

/* Theme toggle */
.wdoc-theme-toggle {
    padding: 0.75rem 1.25rem;
    border-top: 1px solid var(--color-nav-border);
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.8rem;
    color: var(--color-text);
    cursor: pointer;
    user-select: none;
}
.wdoc-theme-toggle-track {
    width: 36px;
    height: 20px;
    background: var(--color-toggle-bg);
    border-radius: 10px;
    position: relative;
    transition: background 0.2s;
}
.wdoc-theme-toggle-knob {
    width: 16px;
    height: 16px;
    background: var(--color-toggle-knob);
    border-radius: 50%;
    position: absolute;
    top: 2px;
    left: 2px;
    transition: transform 0.2s;
}
[data-theme="dark"] .wdoc-theme-toggle-knob {
    transform: translateX(16px);
}
.wdoc-theme-icon { font-size: 1rem; }

/* Main content area — centered in the space right of the nav */
.wdoc-content {
    margin-left: var(--nav-width);
    max-width: var(--content-max-width);
    padding: 2rem 2.5rem;
    /* Center horizontally in remaining viewport space */
    margin-right: auto;
    margin-left: calc(var(--nav-width) + max(0px, (100vw - var(--nav-width) - var(--content-max-width)) / 2));
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
.wdoc-paragraph ul, .wdoc-paragraph ol,
.wdoc-callout-body ul, .wdoc-callout-body ol { padding-left: 0; margin-bottom: 0.5rem; list-style-position: inside; }
.wdoc-paragraph li, .wdoc-callout-body li { margin-bottom: 0.25rem; }

/* Code blocks */
.wdoc-code {
    background: var(--color-code-bg);
    border: 1px solid var(--color-code-border);
    border-radius: 6px;
    padding: 1rem;
    margin-bottom: 1rem;
    overflow-x: auto;
    font-size: 0.875rem;
    line-height: 1.5;
}
.wdoc-code code {
    font-family: "SFMono-Regular", Consolas, "Liberation Mono", Menlo, monospace;
    background: none;
    padding: 0;
}
/* Override highlight.js background to match our code box */
.wdoc-code .hljs { background: transparent; padding: 0; }

/* Diagrams */
.wdoc-diagram {
    margin-bottom: 1rem;
    text-align: center;
}
.wdoc-diagram svg {
    max-width: 100%;
    height: auto;
}

/* Callout blocks */
.wdoc-callout {
    border-left: 4px solid var(--color-nav-border);
    border-radius: 6px;
    padding: 1rem 1.25rem;
    margin-bottom: 1rem;
    background: var(--color-code-bg);
}
.wdoc-callout-header {
    font-weight: 600;
    margin-bottom: 0.5rem;
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 1rem;
}
.wdoc-callout-body > *:last-child { margin-bottom: 0; }

/* Clickable diagram shapes */
.wdoc-diagram svg a { cursor: pointer; }
.wdoc-diagram svg a:hover rect,
.wdoc-diagram svg a:hover circle,
.wdoc-diagram svg a:hover ellipse { opacity: 0.85; }

/* Images */
.wdoc-image {
    max-width: 100%;
    height: auto;
    border-radius: 6px;
    margin-bottom: 1rem;
    display: block;
}

/* Tables */
.wdoc-table {
    width: 100%;
    border-collapse: collapse;
    margin-bottom: 1rem;
    font-size: 0.9rem;
}
.wdoc-table caption {
    caption-side: top;
    text-align: left;
    font-weight: 600;
    padding-bottom: 0.5rem;
    color: var(--color-text);
}
.wdoc-table th, .wdoc-table td {
    border: 1px solid var(--color-table-border);
    padding: 0.5rem 0.75rem;
    text-align: left;
}
.wdoc-table th {
    background: var(--color-table-header-bg);
    font-weight: 600;
}
.wdoc-table tbody tr:nth-child(even) {
    background: var(--color-table-stripe);
}

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
    writeln!(css, "{selector} {{").unwrap();
    for (prop, val) in &rule.properties {
        // Convert WCL underscores to CSS hyphens (font_family → font-family)
        let css_prop = prop.replace('_', "-");
        writeln!(css, "    {css_prop}: {val};").unwrap();
    }
    css.push_str("}\n");
}
