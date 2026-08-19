//! Memoised metadata queries for page templates.
//!
//! The builtin in this module only receives the already-prepared template
//! context. It indexes the site TOC once per shared `toc` value and derives
//! current-page heading facts from authored block handles; it never asks the
//! document evaluator for another page's body.

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};

use wcl_lang::{Environment, Value, from_fn};

use crate::html::HeadingSequence;

#[derive(Default)]
/// Per-site navigation indexes, built once per build and reused by
/// every page rather than recomputed.
struct MetadataCache {
    /// One index per site, in build order.
    sites: Vec<SiteIndex>,
}

/// One site's navigation data: its contents tree, the linear reading
/// order derived from it, and the lookups a page needs to place
/// itself in both.
struct SiteIndex {
    /// The contents tree as document data.
    toc: Arc<Vec<Value>>,
    /// Pages flattened into reading order.
    reading_order: Arc<Vec<Value>>,
    /// Page name → its index in `reading_order`, for next/previous links.
    positions: HashMap<String, usize>,
    /// Page name → its ancestor chain in the contents tree, so a page can
    /// highlight its own branch.
    active_paths: HashMap<String, Arc<Vec<Value>>>,
}

impl SiteIndex {
    /// Build the reading order and lookups from a contents tree.
    fn new(toc: Arc<Vec<Value>>) -> Self {
        let mut reading_order = Vec::new();
        let mut active_paths = HashMap::new();
        index_toc(&toc, &mut Vec::new(), &mut reading_order, &mut active_paths);
        let mut positions = HashMap::new();
        for (index, entry) in reading_order.iter().enumerate() {
            if let Some(href) = record_string(entry, "href") {
                positions.entry(href.to_string()).or_insert(index);
            }
        }
        Self {
            toc,
            reading_order: Arc::new(reading_order),
            positions,
            active_paths,
        }
    }
}

/// Install the `page_metadata(ctx)` builtin into a wdoc environment.
pub(crate) fn register(env: &mut Environment) {
    let cache = Arc::new(Mutex::new(MetadataCache::default()));
    env.add_builtin(
        "page_metadata",
        from_fn(move |ctx: Value| -> Result<Value, String> {
            let fields = record_fields(&ctx, "page_metadata expects a TemplateCtx record")?;
            let page_name = string_field(fields, "page_name")?;
            let toc = list_field(fields, "toc")?;
            let content = list_field(fields, "content")?;

            let mut cache = cache.lock().unwrap_or_else(|e| e.into_inner());
            let site_idx = match cache
                .sites
                .iter()
                .position(|site| Arc::ptr_eq(&site.toc, &toc))
            {
                Some(i) => i,
                None => {
                    cache.sites.push(SiteIndex::new(toc.clone()));
                    cache.sites.len() - 1
                }
            };
            let site = &cache.sites[site_idx];
            Ok(metadata_value(site, page_name, &content))
        })
        .doc(
            "Return memoised reading order, neighbours and active TOC path plus authored \
             heading metadata for the current template page. Site metadata is indexed once \
             and no other page body is evaluated.",
        )
        .param("ctx", "TemplateCtx", "The current template context.")
        .returns("PageMetadata", "Metadata for the current page."),
    );
}

/// Assemble the metadata record one page sees: its contents, its
/// position in the reading order, and its headings.
fn metadata_value(site: &SiteIndex, page_name: &str, content: &[Value]) -> Value {
    let href = format!("{page_name}.html");
    let current_index = site.positions.get(&href).copied();
    let entry_at_offset = |offset: isize| {
        current_index
            .and_then(|i| i.checked_add_signed(offset))
            .and_then(|i| site.reading_order.get(i))
            .cloned()
            .unwrap_or(Value::None)
    };

    let mut fields = BTreeMap::new();
    fields.insert(
        "reading_order".to_string(),
        Value::List(site.reading_order.clone()),
    );
    fields.insert("previous".to_string(), entry_at_offset(-1));
    fields.insert("current".to_string(), entry_at_offset(0));
    fields.insert("current_href".to_string(), Value::Utf8(href.clone()));
    fields.insert("next".to_string(), entry_at_offset(1));
    fields.insert(
        "active_path".to_string(),
        site.active_paths
            .get(&href)
            .cloned()
            .map(Value::List)
            .unwrap_or_else(|| Value::list(Vec::new())),
    );
    fields.insert("headings".to_string(), headings_value(content));
    Value::record(vec!["PageMetadata".to_string()], fields)
}

/// Walk the contents tree, recording each page's reading position and
/// ancestor path.
fn index_toc(
    nodes: &[Value],
    ancestors: &mut Vec<Value>,
    reading_order: &mut Vec<Value>,
    active_paths: &mut HashMap<String, Arc<Vec<Value>>>,
) {
    for node in nodes {
        ancestors.push(node.clone());
        if let Some(href) = record_string(node, "href").filter(|href| !href.is_empty()) {
            reading_order.push(node.clone());
            active_paths.insert(href.to_string(), Arc::new(ancestors.clone()));
        }
        if let Some(children) = record_list(node, "children") {
            index_toc(children, ancestors, reading_order, active_paths);
        }
        ancestors.pop();
    }
}

#[derive(Default)]
/// Accumulator for a page's heading outline: the numbering state and
/// the headings collected so far.
struct HeadingIndex {
    /// Running section numbers, one counter per level.
    sequence: HeadingSequence,
    /// Headings collected so far, in document order.
    headings: Vec<Value>,
}

/// Build a page's heading outline as document data.
fn headings_value(handles: &[Value]) -> Value {
    let mut index = HeadingIndex::default();
    collect_headings(handles, &mut index);
    Value::list(index.headings)
}

/// Walk rendered content, appending each heading with its resolved
/// number.
fn collect_headings(handles: &[Value], index: &mut HeadingIndex) {
    for handle in handles {
        let Some(fields) = record_fields_opt(handle) else {
            continue;
        };
        let kind = fields
            .get("kind")
            .and_then(value_string)
            .unwrap_or_default();
        if let Some(level) = heading_level(kind)
            && let Some(block) = fields.get("block")
            && let Some(title) = fields
                .get("heading_text")
                .and_then(value_string)
                .or_else(|| record_string(block, "text"))
        {
            let explicit_id = record_string(block, "id").filter(|id| !id.is_empty());
            let id = index.sequence.id(title, explicit_id);
            if let Some(number) = index.sequence.number(level as u8) {
                let mut fields = BTreeMap::new();
                fields.insert("level".to_string(), Value::I64(level));
                fields.insert("id".to_string(), Value::Utf8(id));
                fields.insert("title".to_string(), Value::Utf8(title.to_string()));
                fields.insert("number".to_string(), Value::Utf8(number));
                index
                    .headings
                    .push(Value::record(vec!["OnPageHeading".to_string()], fields));
            }
        }
        if let Some(Value::List(children)) = fields.get("children") {
            collect_headings(children, index);
            // A demo intentionally renders its body twice (light and dark),
            // so its derived outline mirrors the two emitted heading runs.
            if kind == "demo" {
                collect_headings(children, index);
            }
        }
    }
}

/// The level a heading block kind stands for, or `None` when the kind
/// is not a heading.
fn heading_level(kind: &str) -> Option<i64> {
    match kind {
        "h1" | "chapter_header" => Some(1),
        "h2" => Some(2),
        "h3" => Some(3),
        "h4" => Some(4),
        "h5" => Some(5),
        "h6" => Some(6),
        _ => None,
    }
}

/// Borrow a value's record fields, erroring when it is not a record.
fn record_fields<'a>(
    value: &'a Value,
    message: &str,
) -> Result<&'a BTreeMap<String, Value>, String> {
    record_fields_opt(value).ok_or_else(|| message.to_string())
}

/// Borrow a value's record fields, or `None` when it is not a record.
fn record_fields_opt(value: &Value) -> Option<&BTreeMap<String, Value>> {
    match value {
        Value::Record { fields, .. } => Some(fields),
        _ => None,
    }
}

/// Read a list entry out of a record.
fn list_field(fields: &BTreeMap<String, Value>, name: &str) -> Result<Arc<Vec<Value>>, String> {
    match fields.get(name) {
        Some(Value::List(items)) => Ok(items.clone()),
        _ => Err(format!("page_metadata: `{name}` must be a list")),
    }
}

/// Read a string entry out of a record.
fn string_field<'a>(fields: &'a BTreeMap<String, Value>, name: &str) -> Result<&'a str, String> {
    fields
        .get(name)
        .and_then(value_string)
        .ok_or_else(|| format!("page_metadata: `{name}` must be utf8"))
}

/// Borrow a value as a string, accepting the string and identifier
/// forms.
fn value_string(value: &Value) -> Option<&str> {
    match value {
        Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) | Value::Symbol(s) => Some(s),
        _ => None,
    }
}

/// Read a string entry out of a value, when it is a record with one.
fn record_string<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    record_fields_opt(value)?.get(name).and_then(value_string)
}

/// Read a list entry out of a value, when it is a record with one.
fn record_list<'a>(value: &'a Value, name: &str) -> Option<&'a [Value]> {
    match record_fields_opt(value)?.get(name)? {
        Value::List(items) => Some(items),
        _ => None,
    }
}
