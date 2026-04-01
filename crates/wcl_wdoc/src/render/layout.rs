use std::fmt::Write;

use crate::model::*;
use crate::render::content::render_content;

/// Render layout items to HTML.
pub fn render_layout_items(items: &[LayoutItem], out: &mut String) {
    for item in items {
        match item {
            LayoutItem::SplitGroup(group) => render_split_group(group, out),
            LayoutItem::Content(block) => render_content(block, out),
        }
    }
}

fn render_split_group(group: &SplitGroup, out: &mut String) {
    let dir_class = match group.direction {
        SplitDirection::Vertical => "wdoc-vsplit",
        SplitDirection::Horizontal => "wdoc-hsplit",
    };
    writeln!(out, "<div class=\"{dir_class}\">").unwrap();
    for split in &group.splits {
        render_split(split, out);
    }
    out.push_str("</div>\n");
}

fn render_split(split: &Split, out: &mut String) {
    writeln!(
        out,
        "<div class=\"wdoc-split\" style=\"flex: 0 0 {size}%;\">",
        size = split.size_percent
    )
    .unwrap();
    render_layout_items(&split.children, out);
    out.push_str("</div>\n");
}
