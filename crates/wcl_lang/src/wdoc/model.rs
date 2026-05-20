use indexmap::IndexMap;
use serde::{Deserialize, Serialize};

use crate::eval::value::Value;

/// A complete wdoc document with sections, pages, and styles.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WdocDocument {
    pub name: String,
    pub title: String,
    pub template: WdocTemplate,
    pub version: Option<String>,
    pub author: Option<String>,
    pub site: SiteConfig,
    pub sections: Vec<Section>,
    pub pages: Vec<Page>,
    pub styles: Vec<WdocStyle>,
    pub extra_css: String,
}

/// A section in the document outline. Ordering matches declaration order.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Section {
    /// Fully-qualified dotted path (e.g. "my-docs.getting-started.installation")
    pub id: String,
    /// Short ID (the block's inline ID, e.g. "installation")
    pub short_id: String,
    /// Display title (from inline arg)
    pub title: String,
    pub children: Vec<Section>,
}

/// A page of content, belonging to a section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Page {
    pub id: String,
    pub section_id: String,
    pub title: String,
    pub template: Option<WdocTemplate>,
    pub path: Option<String>,
    pub date: Option<String>,
    pub draft: bool,
    pub weight: Option<i64>,
    pub summary: Option<String>,
    pub tags: Vec<String>,
    pub categories: Vec<String>,
    pub params: IndexMap<String, String>,
    pub layout: Layout,
    pub signals: Vec<WdocSignal>,
    pub bindings: Vec<WdocBinding>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SiteConfig {
    pub header_html: Option<String>,
    pub nav_html: Option<String>,
    pub footer_html: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WdocTemplate {
    Book,
    Site,
    Presentation,
}

impl WdocTemplate {
    pub const DEFAULT: Self = Self::Book;

    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "book" => Ok(Self::Book),
            "site" => Ok(Self::Site),
            "presentation" => Ok(Self::Presentation),
            other => Err(format!(
                "unknown wdoc template '{other}' (supported: book, site, presentation)"
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Book => "book",
            Self::Site => "site",
            Self::Presentation => "presentation",
        }
    }
}

/// The layout container for a page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Layout {
    pub children: Vec<LayoutItem>,
}

/// An item inside a layout or split — either a split group or a content block.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "item_kind", rename_all = "snake_case")]
pub enum LayoutItem {
    SplitGroup(SplitGroup),
    Content(ContentBlock),
}

/// Direction of a split group.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SplitDirection {
    /// Columns side-by-side (flex-direction: row)
    Vertical,
    /// Rows stacked (flex-direction: column)
    Horizontal,
}

/// A group of splits in a given direction (vsplit or hsplit).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SplitGroup {
    pub direction: SplitDirection,
    pub splits: Vec<Split>,
}

/// A single split pane with a size percentage and nested content.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Split {
    pub size_percent: f64,
    pub children: Vec<LayoutItem>,
}

/// A content block inside a split, rendered by a template function.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContentBlock {
    /// Block kind (e.g. "heading", "paragraph", "image", "code")
    pub kind: String,
    /// Optional block ID for anchor linking
    pub id: Option<String>,
    /// HTML output from the template function
    pub rendered_html: String,
    /// Style class from @style decorator
    pub style: Option<String>,
}

/// A page-scoped dynamic value available to wdoc runtime bindings.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WdocSignal {
    pub name: String,
    pub initial: serde_json::Value,
    pub type_name: Option<String>,
}

/// A declarative runtime binding from a signal value to an HTML/SVG property.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WdocBinding {
    pub name: Option<String>,
    pub signal: String,
    pub target: String,
    pub property: String,
    pub path: Option<String>,
    pub format: Option<String>,
}

/// A named style definition.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WdocStyle {
    /// Style name ("default" is auto-applied to all content blocks)
    pub name: String,
    pub rules: Vec<StyleRule>,
}

/// A CSS rule targeting a specific content block kind.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StyleRule {
    /// Target content block kind (e.g. "heading", "paragraph")
    pub target: String,
    /// CSS properties (name → value)
    pub properties: IndexMap<String, String>,
}

pub fn document_to_value(doc: &WdocDocument) -> Result<Value, String> {
    let json = serde_json::to_value(doc)
        .map_err(|e| format!("failed to convert wdoc document to value: {e}"))?;
    Ok(crate::json::json_value_to_wcl(&json))
}

pub fn document_from_value(value: &Value) -> Result<WdocDocument, String> {
    let json = crate::json::value_to_json(value);
    serde_json::from_value(json).map_err(|e| format!("failed to decode wdoc document value: {e}"))
}
