use indexmap::IndexMap;
use wcl_lang::Value;

use crate::model::*;

/// Render layout items to HTML through the bundled WDoc WCL layout helpers.
pub fn render_layout_items(items: &[LayoutItem], out: &mut String) {
    match render_layout_items_html(items) {
        Ok(html) => out.push_str(&html),
        Err(err) => eprintln!("wdoc: warning: layout template rendering failed: {err}"),
    }
}

fn render_layout_items_html(items: &[LayoutItem]) -> Result<String, String> {
    let functions = crate::source::wdoc_functions();
    let doc = wcl_lang::parse(
        crate::library::WDOC_LIBRARY_WCL,
        wcl_lang::ParseOptions {
            functions: functions.clone(),
            ..Default::default()
        },
    );
    if doc.has_errors() {
        return Err(format!(
            "failed to parse bundled WDoc library: {:?}",
            doc.diagnostics
        ));
    }

    let helpers = crate::source::collect_template_helpers(&doc);
    let func = helpers
        .get("wdoc::render_layout_items")
        .ok_or("missing wdoc::render_layout_items helper")?;
    let rendered = wcl_lang::call_lambda_with_env(
        func,
        &[Value::List(items.iter().map(layout_item_value).collect())],
        &functions.functions,
        &helpers,
    )?;
    match rendered {
        Value::String(html) => Ok(html),
        other => Ok(format!("{other}")),
    }
}

fn layout_item_value(item: &LayoutItem) -> Value {
    match item {
        LayoutItem::SplitGroup(group) => split_group_value(group),
        LayoutItem::Content(block) => content_block_value(block),
    }
}

fn split_group_value(group: &SplitGroup) -> Value {
    let mut map = IndexMap::new();
    map.insert(
        "item_kind".to_string(),
        Value::String("split_group".to_string()),
    );
    map.insert(
        "direction".to_string(),
        Value::String(
            match group.direction {
                SplitDirection::Vertical => "vertical",
                SplitDirection::Horizontal => "horizontal",
            }
            .to_string(),
        ),
    );
    map.insert(
        "splits".to_string(),
        Value::List(group.splits.iter().map(split_value).collect()),
    );
    Value::Map(map)
}

fn split_value(split: &Split) -> Value {
    let mut map = IndexMap::new();
    map.insert("item_kind".to_string(), Value::String("split".to_string()));
    map.insert("size_percent".to_string(), Value::Float(split.size_percent));
    map.insert(
        "children".to_string(),
        Value::List(split.children.iter().map(layout_item_value).collect()),
    );
    Value::Map(map)
}

fn content_block_value(block: &ContentBlock) -> Value {
    let mut map = IndexMap::new();
    map.insert(
        "item_kind".to_string(),
        Value::String("content".to_string()),
    );
    map.insert("kind".to_string(), Value::String(block.kind.clone()));
    map.insert(
        "css_kind".to_string(),
        Value::String(
            block
                .kind
                .rsplit("::")
                .next()
                .unwrap_or(&block.kind)
                .to_string(),
        ),
    );
    map.insert(
        "id".to_string(),
        block
            .id
            .as_ref()
            .map(|id| Value::String(id.clone()))
            .unwrap_or(Value::Null),
    );
    map.insert(
        "html".to_string(),
        Value::String(block.rendered_html.clone()),
    );
    map.insert(
        "style".to_string(),
        block
            .style
            .as_ref()
            .map(|style| Value::String(style.clone()))
            .unwrap_or(Value::Null),
    );
    Value::Map(map)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_wrapper_is_rendered_by_wcl() {
        let html = render_layout_items_html(&[LayoutItem::Content(ContentBlock {
            kind: "wdoc::paragraph".to_string(),
            id: Some("intro".to_string()),
            rendered_html: "<p class=\"wdoc-paragraph\">Hello</p>".to_string(),
            style: Some("note".to_string()),
        })])
        .expect("render layout");

        assert!(html.contains("id=\"intro\""));
        assert!(html.contains("data-wdoc-content-id=\"intro\""));
        assert!(html.contains("class=\"wdoc-style-note--paragraph\""));
        assert!(html.contains("<p class=\"wdoc-paragraph\">Hello</p>"));
    }

    #[test]
    fn split_layout_is_rendered_by_wcl() {
        let html = render_layout_items_html(&[LayoutItem::SplitGroup(SplitGroup {
            direction: SplitDirection::Vertical,
            splits: vec![Split {
                size_percent: 40.0,
                children: vec![LayoutItem::Content(ContentBlock {
                    kind: "wdoc::paragraph".to_string(),
                    id: None,
                    rendered_html: "<p class=\"wdoc-paragraph\">Pane</p>".to_string(),
                    style: None,
                })],
            }],
        })])
        .expect("render layout");

        assert!(html.contains("<div class=\"wdoc-vsplit\">"));
        assert!(html.contains("style=\"flex: 0 0 40%;\""));
        assert!(html.contains("<p class=\"wdoc-paragraph\">Pane</p>"));
    }
}
