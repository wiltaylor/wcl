use std::path::PathBuf;

use crate::wdoc::model::*;
use crate::Value;

/// Render layout items to HTML through the bundled WDoc WCL layout helpers.
pub fn render_layout_items(items: &[LayoutItem], out: &mut String) {
    match render_layout_items_html(items) {
        Ok(html) => out.push_str(&html),
        Err(err) => eprintln!("wdoc: warning: layout template rendering failed: {err}"),
    }
}

fn render_layout_items_html(items: &[LayoutItem]) -> Result<String, String> {
    let functions = crate::wdoc::source::wdoc_functions();
    let doc = crate::parse(
        crate::standard_lib::WDOC_LIBRARY_WCL,
        crate::ParseOptions {
            root_dir: PathBuf::from(crate::eval::imports::EMBEDDED_LIBRARY_ROOT),
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

    let helpers = crate::wdoc::source::collect_template_helpers(&doc);
    let func = helpers
        .get("wdoc::render_layout_items")
        .ok_or("missing wdoc::render_layout_items helper")?;
    let rendered = crate::call_lambda_with_env(
        func,
        &[crate::wdoc::model::layout_items_to_value(items)],
        &functions.functions,
        &helpers,
    )?;
    match rendered {
        Value::String(html) => Ok(html),
        other => Ok(format!("{other}")),
    }
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
