use wcl_lang::{Block, Value};

pub(crate) fn render_page(name: &str, blocks: impl Iterator<Item = String>) -> String {
    let mut body = String::new();
    for b in blocks {
        body.push_str(&b);
        body.push('\n');
    }
    format!(
        "<!DOCTYPE html>\n\
         <html>\n\
         <head><meta charset=\"utf-8\"><title>{title}</title></head>\n\
         <body>\n\
         {body}</body>\n\
         </html>\n",
        title = escape_html(name),
        body = body,
    )
}

pub(crate) fn render_block(block: &Block<'_>) -> Option<String> {
    let kind = block.kind();
    let text = block_text(block)?;
    let escaped = escape_html(&text);
    Some(match kind {
        "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "p" => format!("<{kind}>{escaped}</{kind}>"),
        _ => return None,
    })
}

fn block_text(block: &Block<'_>) -> Option<String> {
    let labels = block.labels().ok()?;
    let first = labels.into_iter().next()?;
    match first {
        Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) | Value::Symbol(s) => Some(s),
        other => Some(other.to_string()),
    }
}

pub(crate) fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            _ => out.push(c),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::escape_html;

    #[test]
    fn escapes_html_specials() {
        assert_eq!(
            escape_html("<a href=\"x\">hi & 'bye'</a>"),
            "&lt;a href=&quot;x&quot;&gt;hi &amp; &#39;bye&#39;&lt;/a&gt;"
        );
    }
}
