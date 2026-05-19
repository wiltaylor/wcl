use crate::eval::value::Value;
use indexmap::IndexMap;

pub fn render_html(value: &Value) -> Result<String, String> {
    match value {
        Value::List(items) => {
            let mut map = IndexMap::new();
            map.insert("tag".to_string(), Value::String("document".to_string()));
            map.insert("children".to_string(), Value::List(items.clone()));
            render_with_codec("html", &Value::Map(map))
        }
        _ => render_with_codec("html", value),
    }
}

pub fn render_svg(value: &Value) -> Result<String, String> {
    render_with_codec("svg", value)
}

pub fn render_css(value: &Value) -> Result<String, String> {
    match value {
        Value::List(items) => render_with_codec("css", &css_stylesheet(items.clone())),
        _ => render_with_codec("css", value),
    }
}

fn render_with_codec(codec_name: &str, value: &Value) -> Result<String, String> {
    let codec_name = codec_name.to_string();
    let value = value.clone();
    std::thread::Builder::new()
        .name(format!("wdoc-{codec_name}-codec"))
        .stack_size(32 * 1024 * 1024)
        .spawn(move || {
            let registry = crate::transform::codec::custom::standard_registry()
                .map_err(|err| err.to_string())?;
            let codec = registry
                .get(&codec_name)
                .ok_or_else(|| format!("standard {codec_name} codec is not registered"))?;
            let mut out = Vec::new();
            crate::transform::codec::custom::encode_custom_value(
                &value,
                codec,
                &IndexMap::new(),
                &mut out,
            )
            .map_err(|err| err.to_string())?;
            String::from_utf8(out).map_err(|err| err.to_string())
        })
        .map_err(|err| err.to_string())?
        .join()
        .map_err(|_| "wdoc markup codec thread panicked".to_string())?
}

pub fn text(value: impl Into<String>) -> Value {
    let mut map = IndexMap::new();
    map.insert("tag".to_string(), Value::String("text".to_string()));
    map.insert("content".to_string(), Value::String(value.into()));
    Value::Map(map)
}

pub fn raw_html(value: impl Into<String>) -> Value {
    let mut map = IndexMap::new();
    map.insert("tag".to_string(), Value::String("raw".to_string()));
    map.insert("html".to_string(), Value::String(value.into()));
    Value::Map(map)
}

pub fn raw_svg(value: impl Into<String>) -> Value {
    let mut map = IndexMap::new();
    map.insert("tag".to_string(), Value::String("raw".to_string()));
    map.insert("raw".to_string(), Value::String(value.into()));
    Value::Map(map)
}

pub fn raw_css(value: impl Into<String>) -> Value {
    let mut map = IndexMap::new();
    map.insert("kind".to_string(), Value::String("raw".to_string()));
    map.insert("content".to_string(), Value::String(value.into()));
    Value::Map(map)
}

pub fn elem(tag: &str, attrs: &[(&str, Value)], children: Vec<Value>) -> Value {
    let mut map = IndexMap::new();
    map.insert("tag".to_string(), Value::String(tag.to_string()));
    for (key, value) in attrs {
        map.insert((*key).to_string(), value.clone());
    }
    if !children.is_empty() {
        map.insert("children".to_string(), Value::List(children));
    }
    Value::Map(map)
}

pub fn svg_elem(tag: &str, attrs: &[(&str, Value)], children: Vec<Value>) -> Value {
    elem(tag, attrs, children)
}

pub fn style_map(entries: &[(&str, &str)]) -> Value {
    let mut map = IndexMap::new();
    for (key, value) in entries {
        map.insert((*key).to_string(), Value::String((*value).to_string()));
    }
    Value::Map(map)
}

pub fn css_rule(selector: &str, declarations: &[(&str, &str)]) -> Value {
    let mut map = IndexMap::new();
    map.insert("kind".to_string(), Value::String("rule".to_string()));
    map.insert("selector".to_string(), Value::String(selector.to_string()));
    for (key, value) in declarations {
        map.insert((*key).to_string(), Value::String((*value).to_string()));
    }
    Value::Map(map)
}

pub fn css_stylesheet(children: Vec<Value>) -> Value {
    let mut map = IndexMap::new();
    map.insert("kind".to_string(), Value::String("stylesheet".to_string()));
    map.insert("children".to_string(), Value::List(children));
    Value::Map(map)
}

pub fn css_at(kind: &str, attrs: &[(&str, Value)], children: Vec<Value>) -> Value {
    let mut map = IndexMap::new();
    map.insert("kind".to_string(), Value::String(kind.to_string()));
    for (key, value) in attrs {
        map.insert((*key).to_string(), value.clone());
    }
    if !children.is_empty() {
        map.insert("children".to_string(), Value::List(children));
    }
    Value::Map(map)
}

pub fn s(value: impl Into<String>) -> Value {
    Value::String(value.into())
}

pub fn b(value: bool) -> Value {
    Value::Bool(value)
}

pub fn i(value: i64) -> Value {
    Value::Int(value)
}
