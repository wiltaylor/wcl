//! Field / map / value readers, class-attribute helpers, and HTML escaping.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use wcl_lang::{Block, Value, VariantPayload};

/// A source of named values — either a `Block`'s declared fields or a
/// variant-payload `BTreeMap`. The block path goes through a temporary
/// `Field` view, so the trait yields an owned `Value` rather than a
/// borrow; the cloned scalars/lists are cheap and live only in the
/// render path. This is the one seam that lets the `field_*` and `map_*`
/// reader families share a single body each (the `src_*` fns below).
pub(crate) trait ValueSource {
    /// Read the named field, cloning it out. `None` when absent or when
    /// evaluation failed — a renderer treats both as "not supplied".
    fn lookup(&self, name: &str) -> Option<Value>;
}

impl ValueSource for &Block<'_> {
    fn lookup(&self, name: &str) -> Option<Value> {
        self.field(name)?.value().ok().cloned()
    }
}

impl ValueSource for &BTreeMap<String, Value> {
    fn lookup(&self, name: &str) -> Option<Value> {
        self.get(name).cloned()
    }
}

// ── Generic readers over any `ValueSource` ────────────────────────
//
// Each preserves the exact coercion the old `field_*`/`map_*` pair used;
// the named wrappers further down just pin `S` to a block or a map.

/// Read a `utf8` field.
fn src_utf8<S: ValueSource>(s: S, name: &str) -> Option<String> {
    match s.lookup(name)? {
        Value::Utf8(x) | Value::Ascii(x) => Some(x),
        _ => None,
    }
}

/// Read an `identifier` field.
fn src_id<S: ValueSource>(s: S, name: &str) -> Option<String> {
    match s.lookup(name)? {
        Value::Identifier(x) | Value::Utf8(x) | Value::Ascii(x) => Some(x),
        _ => None,
    }
}

/// Read a `bool` field.
fn src_bool<S: ValueSource>(s: S, name: &str) -> Option<bool> {
    match s.lookup(name)? {
        Value::Bool(b) => Some(b),
        _ => None,
    }
}

/// Read a symbol field, without its leading colon.
fn src_symbol<S: ValueSource>(s: S, name: &str) -> Option<String> {
    match s.lookup(name)? {
        Value::Symbol(x) => Some(x),
        _ => None,
    }
}

/// Read any numeric field as `f64`.
fn src_f64<S: ValueSource>(s: S, name: &str) -> Option<f64> {
    value_as_f64(&s.lookup(name)?)
}

/// Read any integer field as `i64`.
fn src_i64<S: ValueSource>(s: S, name: &str) -> Option<i64> {
    value_as_i64(&s.lookup(name)?)
}

/// Read a `list<utf8>` field; empty when absent.
fn src_utf8_list<S: ValueSource>(s: S, name: &str) -> Vec<String> {
    match s.lookup(name) {
        Some(Value::List(items)) => items.iter().filter_map(value_as_str).collect(),
        _ => Vec::new(),
    }
}

/// Build the `class="…"` attribute from a `class` field, or an empty
/// string when it declares none.
fn src_class_attr<S: ValueSource>(s: S) -> String {
    classes_attr_from_names(&src_utf8_list(s, "class"))
}

/// The paint + identity attributes every diagram shape reads the same
/// way — `class`, `fill`, `stroke`, `id` — regardless of source. The
/// `render_*` (block) and `render_*_payload` (variant) pairs differ in
/// geometry (anchor resolution vs. pre-resolved coords) so they stay
/// split, but these reads collapse to one generic helper. A shape that
/// doesn't take a given attribute (a `line` has no fill, a `label` no
/// stroke) simply ignores that field.
pub(crate) struct ShapePaint {
    /// Pre-rendered `class="…"` attribute, or empty.
    pub(crate) class: String,
    /// Fill colour, when the shape takes one.
    pub(crate) fill: Option<String>,
    /// Stroke colour, when the shape takes one.
    pub(crate) stroke: Option<String>,
    /// Element id, when the block declares one.
    pub(crate) id: Option<String>,
}

/// Read the paint attributes every shape shares. A shape that does
/// not take one simply ignores that field.
pub(crate) fn shape_paint<S: ValueSource + Copy>(s: S) -> ShapePaint {
    ShapePaint {
        class: src_class_attr(s),
        fill: src_utf8(s, "fill"),
        stroke: src_utf8(s, "stroke"),
        id: src_id(s, "id"),
    }
}

// ── Shared helpers (source-agnostic) ──────────────────────────────

/// Append ` name="value"` when the value is present; a no-op
/// otherwise, so callers need no conditional.
pub(crate) fn append_attr(out: &mut String, name: &str, value: Option<&str>) {
    if let Some(v) = value {
        write!(out, " {name}=\"{}\"", escape_html(v)).expect("write to String");
    }
}

/// The block's first label as a string, whether written as a string
/// or an identifier.
pub(crate) fn label_string(block: &Block<'_>) -> Option<String> {
    let labels = block.labels().ok()?;
    value_as_string(labels.into_iter().next()?)
}

/// Read a value as a string, accepting the string and identifier
/// forms.
pub(crate) fn value_as_string(v: Value) -> Option<String> {
    match v {
        Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) | Value::Symbol(s) => Some(s),
        other => Some(other.to_string()),
    }
}

/// Build a `class="…"` attribute from class names; empty when the
/// list is.
pub(crate) fn classes_attr_from_names(names: &[String]) -> String {
    if names.is_empty() {
        return String::new();
    }
    let joined = names
        .iter()
        .map(|s| escape_html(s))
        .collect::<Vec<_>>()
        .join(" ");
    format!(" class=\"{joined}\"")
}

// ── Block-side accessors ──────────────────────────────────────────

/// Build the `class="…"` attribute from a block's `class` field.
pub(crate) fn class_attr(block: &Block<'_>) -> String {
    src_class_attr(block)
}

/// Read a field that may be computed (e.g. a table's `rows = map(…)`).
/// An *absent* field is `None` so the caller can fall back (the table
/// renderers re-parse pipe rows); a *present* field whose expression
/// fails to evaluate is a genuine authoring error — it records a fatal
/// lower diagnostic instead of silently degrading to the fallback.
pub(crate) fn computed_field(block: &Block<'_>, name: &str) -> Option<Value> {
    match block.field(name)?.value() {
        Ok(v) => Some(v.clone()),
        Err(e) => {
            super::record_lower_error(block, e.clone());
            None
        }
    }
}

/// Read a `utf8` field off a block.
pub(crate) fn field_utf8(block: &Block<'_>, name: &str) -> Option<String> {
    src_utf8(block, name)
}

/// Read an `identifier` field off a block.
pub(crate) fn field_id(block: &Block<'_>, name: &str) -> Option<String> {
    src_id(block, name)
}

/// Read a `bool` field off a block.
pub(crate) fn field_bool(block: &Block<'_>, name: &str) -> Option<bool> {
    src_bool(block, name)
}

/// Read a symbol field off a block.
pub(crate) fn field_symbol(block: &Block<'_>, name: &str) -> Option<String> {
    src_symbol(block, name)
}

/// Read a numeric field off a block as `f64`.
pub(crate) fn field_f64(block: &Block<'_>, name: &str) -> Option<f64> {
    if let Some(v) = src_f64(block, name) {
        return Some(v);
    }
    // Fall back to a schema-declared default (`name = 0.0` inline
    // form or `@default(...)` decorator). This is what lets a
    // layered child render at (x=0, y=0) without forcing every
    // user to write x = 0.0 themselves. Block-only — variant
    // payloads carry already-resolved geometry.
    value_as_f64(&block.schema()?.field(name)?.default_value()?)
}

/// Read an integer field off a block as `i64`.
pub(crate) fn field_i64(block: &Block<'_>, name: &str) -> Option<i64> {
    src_i64(block, name)
}

/// Read a numeric list field off a block; empty when absent.
pub(crate) fn field_f64_list(block: &Block<'_>, name: &str) -> Vec<f64> {
    let Some(field) = block.field(name) else {
        return Vec::new();
    };
    let Ok(value) = field.value() else {
        return Vec::new();
    };
    let Value::List(items) = value else {
        return Vec::new();
    };
    items.iter().filter_map(value_as_f64).collect()
}

/// Read a `list<utf8>` field off a block; empty when absent.
pub(crate) fn field_utf8_list(block: &Block<'_>, name: &str) -> Vec<String> {
    src_utf8_list(block, name)
}

/// Read a `list<symbol>` field, distinguishing "field absent or
/// none" (returns `None`, callers apply their own default) from
/// "explicitly empty list" (returns `Some(vec![])`).
pub(crate) fn field_symbol_list_opt(block: &Block<'_>, name: &str) -> Option<Vec<String>> {
    let field = block.field(name)?;
    let value = field.value().ok()?;
    let Value::List(items) = value else {
        return None;
    };
    Some(
        items
            .iter()
            .filter_map(|v| match v {
                Value::Symbol(s) => Some(s.clone()),
                _ => None,
            })
            .collect(),
    )
}

// ── Map-side accessors (for variant payloads) ─────────────────────

/// Build a `class="…"` attribute from a record's `class` entry.
pub(crate) fn class_attr_from_map(map: &BTreeMap<String, Value>) -> String {
    src_class_attr(map)
}

/// Read a `utf8` entry out of a record.
pub(crate) fn map_utf8(map: &BTreeMap<String, Value>, name: &str) -> Option<String> {
    src_utf8(map, name)
}

/// Read an `identifier` entry out of a record.
pub(crate) fn map_id(map: &BTreeMap<String, Value>, name: &str) -> Option<String> {
    src_id(map, name)
}

/// Read a numeric entry out of a record as `f64`.
pub(crate) fn map_f64(map: &BTreeMap<String, Value>, name: &str) -> Option<f64> {
    src_f64(map, name)
}

/// Read a `list<utf8>` entry out of a record; empty when absent.
pub(crate) fn map_utf8_list(map: &BTreeMap<String, Value>, name: &str) -> Vec<String> {
    src_utf8_list(map, name)
}

// ── Value-coercion helpers ────────────────────────────────────────

/// Read any numeric value as `f64`.
pub(crate) fn value_as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::F64(n) => Some(*n),
        Value::F32(n) => Some(*n as f64),
        Value::I64(n) => Some(*n as f64),
        Value::I32(n) => Some(*n as f64),
        _ => None,
    }
}

/// Read any integer value as `i64`.
pub(crate) fn value_as_i64(v: &Value) -> Option<i64> {
    match v {
        Value::I64(n) => Some(*n),
        Value::I32(n) => Some(*n as i64),
        Value::U32(n) => Some(*n as i64),
        Value::U64(n) => Some(*n as i64),
        _ => None,
    }
}

/// Read a value as a string, for the readers that borrow rather than
/// consume.
pub(crate) fn value_as_str(v: &Value) -> Option<String> {
    match v {
        Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) | Value::Symbol(s) => {
            Some(s.clone())
        }
        _ => None,
    }
}

// ── Variant / fundamental readers (shared by the PDF + Markdown backends) ──

/// Destructure a `Value::Variant` with a record payload into `(kind, map)`,
/// where `kind` is the snake-cased fundamental name (`Paragraph` → `paragraph`).
pub(crate) fn as_record_variant(value: &Value) -> Option<(String, &BTreeMap<String, Value>)> {
    let Value::Variant {
        variant, payload, ..
    } = value
    else {
        return None;
    };
    let VariantPayload::Record(map) = payload else {
        return None;
    };
    Some((super::kind_for_variant(variant), map))
}

/// Read a list-typed field from a payload map (empty slice when absent).
pub(crate) fn map_list<'a>(map: &'a BTreeMap<String, Value>, name: &str) -> &'a [Value] {
    match map.get(name) {
        Some(Value::List(items)) => items,
        _ => &[],
    }
}

/// Plain text of a table-cell value.
pub(crate) fn cell_text(v: &Value) -> String {
    match v {
        Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) | Value::Symbol(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        other => other.as_f64().map_or(String::new(), |f| {
            if f.fract() == 0.0 {
                format!("{}", f as i64)
            } else {
                format!("{f}")
            }
        }),
    }
}

/// The concatenated raw text of an element's inline children (so the inline
/// engine runs once over the whole paragraph, and headings get their text
/// without re-running the emphasis engine).
pub(crate) fn gather_inline_text(children: &[Value]) -> String {
    let mut s = String::new();
    for c in children {
        if let Some((kind, map)) = as_record_variant(c) {
            match kind.as_str() {
                "inline" => s.push_str(&map_utf8(map, "text").unwrap_or_default()),
                "paragraph" => s.push_str(&map_utf8_list(map, "spans").join("")),
                "element" => s.push_str(&gather_inline_text(map_list(map, "children"))),
                _ => {}
            }
        }
    }
    s
}

/// Escape the five characters that would otherwise be read as markup.
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
    use super::*;
    use wcl_lang::Document;

    /// Build a tiny doc whose single `marker` block carries the given
    /// fields, then hand its block to `f`. Lets the accessor tests drive
    /// the real block-view path rather than a hand-rolled `Value`.
    fn with_block(body: &str, f: impl FnOnce(&Block<'_>)) {
        let src = format!(
            "@schemaless\n@document\ntype Root {{}}\n\
             @block(\"marker\")\ntype Marker {{\n  \
             text: utf8?  flag: bool?  count: i64?  ratio: f64?  \
             tag: symbol?  names: list<utf8>?\n}}\n\
             marker {{\n{body}\n}}\n"
        );
        let doc = Document::open(&src, "test.wcl").expect("doc parses");
        let block = doc.blocks().next().expect("one block");
        f(&block);
    }

    #[test]
    fn block_and_map_readers_agree() {
        with_block(
            "  text = \"hi\"\n  flag = true\n  count = 3\n  ratio = 1.5\n  tag = :sym\n  names = [\"a\", \"b\"]",
            |block| {
                // Mirror the block's fields into a payload map and assert
                // every reader pair produces the same result.
                let mut map: BTreeMap<String, Value> = BTreeMap::new();
                map.insert("text".into(), Value::Utf8("hi".into()));
                map.insert("ratio".into(), Value::F64(1.5));
                map.insert(
                    "names".into(),
                    Value::List(std::sync::Arc::new(vec![
                        Value::Utf8("a".into()),
                        Value::Utf8("b".into()),
                    ])),
                );

                // String / float / list readers exist on both sources and
                // share the `src_*` body — assert they agree.
                assert_eq!(field_utf8(block, "text"), map_utf8(&map, "text"));
                assert_eq!(field_f64(block, "ratio"), map_f64(&map, "ratio"));
                assert_eq!(
                    field_utf8_list(block, "names"),
                    map_utf8_list(&map, "names")
                );
                // Bool / i64 / symbol readers are block-only; check they
                // coerce through the same generic path.
                assert_eq!(field_bool(block, "flag"), Some(true));
                assert_eq!(field_i64(block, "count"), Some(3));
                assert_eq!(field_symbol(block, "tag"), Some("sym".to_string()));
            },
        );
    }

    #[test]
    fn field_f64_falls_back_to_schema_default_block_only() {
        // The schema declares `size = 7.0` as an inline default; the block
        // omits it. The block reader must surface the default; a payload
        // map with no entry must not (it carries resolved geometry).
        let src = "@schemaless\n@document\ntype Root {}\n@block(\"marker\")\ntype Marker { size = 7.0 }\nmarker {}\n";
        let doc = Document::open(src, "test.wcl").expect("doc parses");
        let block = doc.blocks().next().expect("one block");
        assert_eq!(field_f64(&block, "size"), Some(7.0));

        let empty: BTreeMap<String, Value> = BTreeMap::new();
        assert_eq!(map_f64(&empty, "size"), None);
    }

    #[test]
    fn utf8_list_drops_none_entries() {
        // `class: ["base", if cond { "extra" }]` — an untaken else-less
        // `if` contributes a `none` element, and every consumer of a
        // `list<utf8>` field must drop it rather than render it.
        with_block("  names = [\"base\", if false { \"extra\" }]", |block| {
            assert_eq!(field_utf8_list(block, "names"), vec!["base".to_string()]);
        });
    }

    #[test]
    fn class_attr_is_absent_when_every_entry_is_none() {
        // A list whose entries are ALL `none` must emit no attribute —
        // `class=""` would be a different rendering, and the whole point
        // of the inline conditional is that it costs nothing when untaken.
        //
        // Both blocks share one fixture shape, so the `taken` case is what
        // proves the `untaken` empty string is the DROP and not a field the
        // reader failed to reach.
        let src = "@schemaless\n@document\ntype Root {}\n@block(\"marker\")\n\
                   type Marker { class: list<utf8>? }\n\
                   marker untaken { class = [if false { \"extra\" }] }\n\
                   marker taken   { class = [if true  { \"extra\" }] }\n";
        let doc = Document::open(src, "test.wcl").expect("doc parses");
        let mut blocks = doc.blocks();
        let untaken = blocks.next().expect("untaken block");
        let taken = blocks.next().expect("taken block");
        assert_eq!(class_attr(&untaken), "");
        assert_eq!(class_attr(&taken), " class=\"extra\"");
    }

    #[test]
    fn classes_attr_from_names_omits_the_attribute_for_an_empty_list() {
        assert_eq!(classes_attr_from_names(&[]), "");
        assert_eq!(
            classes_attr_from_names(&["a".into(), "b".into()]),
            " class=\"a b\""
        );
    }
}
