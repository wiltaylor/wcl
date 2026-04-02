use indexmap::IndexMap;

/// A complete wdoc document with sections, pages, and styles.
#[derive(Debug, Clone)]
pub struct WdocDocument {
    pub name: String,
    pub title: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub sections: Vec<Section>,
    pub pages: Vec<Page>,
    pub styles: Vec<WdocStyle>,
}

/// A section in the document outline. Ordering matches declaration order.
#[derive(Debug, Clone)]
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
#[derive(Debug, Clone)]
pub struct Page {
    pub id: String,
    pub section_id: String,
    pub title: String,
    pub layout: Layout,
}

/// The layout container for a page.
#[derive(Debug, Clone)]
pub struct Layout {
    pub children: Vec<LayoutItem>,
}

/// An item inside a layout or split — either a split group or a content block.
#[derive(Debug, Clone)]
pub enum LayoutItem {
    SplitGroup(SplitGroup),
    Content(ContentBlock),
}

/// Direction of a split group.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDirection {
    /// Columns side-by-side (flex-direction: row)
    Vertical,
    /// Rows stacked (flex-direction: column)
    Horizontal,
}

/// A group of splits in a given direction (vsplit or hsplit).
#[derive(Debug, Clone)]
pub struct SplitGroup {
    pub direction: SplitDirection,
    pub splits: Vec<Split>,
}

/// A single split pane with a size percentage and nested content.
#[derive(Debug, Clone)]
pub struct Split {
    pub size_percent: f64,
    pub children: Vec<LayoutItem>,
}

/// A content block inside a split, rendered by a template function.
#[derive(Debug, Clone)]
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

/// A named style definition.
#[derive(Debug, Clone)]
pub struct WdocStyle {
    /// Style name ("default" is auto-applied to all content blocks)
    pub name: String,
    pub rules: Vec<StyleRule>,
}

/// A CSS rule targeting a specific content block kind.
#[derive(Debug, Clone)]
pub struct StyleRule {
    /// Target content block kind (e.g. "heading", "paragraph")
    pub target: String,
    /// CSS properties (name → value)
    pub properties: IndexMap<String, String>,
}
