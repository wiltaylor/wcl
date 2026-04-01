use std::fmt::Write;

use crate::model::ContentBlock;

/// Render a content block to HTML.
/// The block's `rendered_html` is already produced by the template function.
/// We just wrap it with style classes if applicable.
pub fn render_content(block: &ContentBlock, out: &mut String) {
    if let Some(style) = &block.style {
        writeln!(
            out,
            "<div class=\"wdoc-style-{style}--{}\">{}</div>",
            block.kind, block.rendered_html
        )
        .unwrap();
    } else {
        out.push_str(&block.rendered_html);
        out.push('\n');
    }
}
