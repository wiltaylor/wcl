use std::fmt::Write;

use crate::model::ContentBlock;

/// Render a content block to HTML.
/// The block's `rendered_html` is already produced by the template function.
/// We wrap it with style classes and/or ID attributes if applicable.
pub fn render_content(block: &ContentBlock, out: &mut String) {
    let id_attr = block
        .id
        .as_ref()
        .map(|id| format!(" id=\"{id}\""))
        .unwrap_or_default();

    if let Some(style) = &block.style {
        writeln!(
            out,
            "<div{id_attr} class=\"wdoc-style-{style}--{}\">{}</div>",
            block.kind, block.rendered_html
        )
        .unwrap();
    } else if !id_attr.is_empty() {
        writeln!(out, "<div{id_attr}>{}</div>", block.rendered_html).unwrap();
    } else {
        out.push_str(&block.rendered_html);
        out.push('\n');
    }
}
