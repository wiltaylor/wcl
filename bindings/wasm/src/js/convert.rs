use js_sys::{Array, Object, Reflect, Set};
use wasm_bindgen::JsValue;
use wcl::Value;

/// Convert a WCL Value to a JsValue.
pub fn value_to_js(value: &Value) -> JsValue {
    match value {
        Value::String(s) | Value::Identifier(s) => JsValue::from_str(s),
        Value::Int(i) => JsValue::from_f64(*i as f64),
        Value::Float(f) => JsValue::from_f64(*f),
        Value::Bool(b) => JsValue::from_bool(*b),
        Value::Null => JsValue::NULL,
        Value::List(items) => {
            let arr = Array::new();
            for item in items {
                arr.push(&value_to_js(item));
            }
            arr.into()
        }
        Value::Map(map) => {
            let obj = Object::new();
            for (k, v) in map {
                Reflect::set(&obj, &JsValue::from_str(k), &value_to_js(v)).unwrap();
            }
            obj.into()
        }
        Value::Set(items) => {
            let set = Set::new(&JsValue::UNDEFINED);
            for item in items {
                set.add(&value_to_js(item));
            }
            set.into()
        }
        Value::BlockRef(br) => {
            let obj = Object::new();
            Reflect::set(
                &obj,
                &JsValue::from_str("kind"),
                &JsValue::from_str(&br.kind),
            )
            .unwrap();
            if let Some(id) = &br.id {
                Reflect::set(&obj, &JsValue::from_str("id"), &JsValue::from_str(id)).unwrap();
            }
            if !br.attributes.is_empty() {
                let attrs = Object::new();
                for (k, v) in &br.attributes {
                    Reflect::set(&attrs, &JsValue::from_str(k), &value_to_js(v)).unwrap();
                }
                Reflect::set(&obj, &JsValue::from_str("attributes"), &attrs.into()).unwrap();
            }
            if !br.children.is_empty() {
                let children = Array::new();
                for child in &br.children {
                    children.push(&value_to_js(&Value::BlockRef(child.clone())));
                }
                Reflect::set(&obj, &JsValue::from_str("children"), &children.into()).unwrap();
            }
            obj.into()
        }
        Value::Function(_) => JsValue::NULL,
    }
}

/// Convert a JsValue to a WCL Value.
pub fn js_to_value(val: &JsValue) -> Result<Value, String> {
    if val.is_null() || val.is_undefined() {
        return Ok(Value::Null);
    }
    if let Some(b) = val.as_bool() {
        return Ok(Value::Bool(b));
    }
    if let Some(f) = val.as_f64() {
        // Represent integers as Int when possible
        if f.fract() == 0.0 && f >= i64::MIN as f64 && f <= i64::MAX as f64 {
            return Ok(Value::Int(f as i64));
        }
        return Ok(Value::Float(f));
    }
    if let Some(s) = val.as_string() {
        return Ok(Value::String(s));
    }
    if Array::is_array(val) {
        let arr = Array::from(val);
        let items: Result<Vec<Value>, String> = arr.iter().map(|item| js_to_value(&item)).collect();
        return Ok(Value::List(items?));
    }
    if val.is_object() {
        let obj = Object::from(val.clone());
        let keys = Object::keys(&obj);
        let mut map = indexmap::IndexMap::new();
        for i in 0..keys.length() {
            let key = keys.get(i).as_string().unwrap_or_default();
            let value = Reflect::get(val, &JsValue::from_str(&key))
                .map_err(|e| format!("failed to get property '{}': {:?}", key, e))?;
            map.insert(key, js_to_value(&value)?);
        }
        return Ok(Value::Map(map));
    }
    Err(format!("cannot convert JS value to WCL value: {:?}", val))
}

/// Convert an IndexMap of WCL Values to a plain JS object.
pub fn values_to_js(values: &indexmap::IndexMap<String, Value>) -> JsValue {
    let obj = Object::new();
    for (k, v) in values {
        Reflect::set(&obj, &JsValue::from_str(k), &value_to_js(v)).unwrap();
    }
    obj.into()
}
