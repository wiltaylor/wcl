mod convert;
mod fs;

use std::path::PathBuf;
use std::sync::Arc;
use wasm_bindgen::prelude::*;

use convert::{js_to_value, value_to_js, values_to_js};
use fs::JsFileSystem;

/// Build a result object: `{ values, hasErrors, diagnostics }`
fn build_result(doc: &wcl_lang::Document) -> JsValue {
    let obj = js_sys::Object::new();

    let values = values_to_js(&doc.values);
    js_sys::Reflect::set(&obj, &JsValue::from_str("values"), &values).unwrap();

    js_sys::Reflect::set(
        &obj,
        &JsValue::from_str("hasErrors"),
        &JsValue::from_bool(doc.has_errors()),
    )
    .unwrap();

    let diags = js_sys::Array::new();
    for d in &doc.diagnostics {
        let diag_obj = js_sys::Object::new();
        let severity = if d.is_error() { "error" } else { "warning" };
        js_sys::Reflect::set(
            &diag_obj,
            &JsValue::from_str("severity"),
            &JsValue::from_str(severity),
        )
        .unwrap();
        js_sys::Reflect::set(
            &diag_obj,
            &JsValue::from_str("message"),
            &JsValue::from_str(&d.message),
        )
        .unwrap();
        if let Some(code) = &d.code {
            js_sys::Reflect::set(
                &diag_obj,
                &JsValue::from_str("code"),
                &JsValue::from_str(code),
            )
            .unwrap();
        }
        diags.push(&diag_obj);
    }
    js_sys::Reflect::set(&obj, &JsValue::from_str("diagnostics"), &diags.into()).unwrap();

    obj.into()
}

/// Extract ParseOptions from a JS options object.
fn build_parse_options(options: Option<JsValue>) -> Result<wcl_lang::ParseOptions, JsValue> {
    let mut opts = wcl_lang::ParseOptions::default();

    let options = match options {
        Some(ref v) if !v.is_null() && !v.is_undefined() => v,
        _ => return Ok(opts),
    };

    // rootDir
    if let Ok(v) = js_sys::Reflect::get(options, &JsValue::from_str("rootDir")) {
        if let Some(s) = v.as_string() {
            opts.root_dir = PathBuf::from(s);
        }
    }

    // allowImports
    if let Ok(v) = js_sys::Reflect::get(options, &JsValue::from_str("allowImports")) {
        if let Some(b) = v.as_bool() {
            opts.allow_imports = b;
        }
    }

    // maxImportDepth
    if let Ok(v) = js_sys::Reflect::get(options, &JsValue::from_str("maxImportDepth")) {
        if let Some(n) = v.as_f64() {
            opts.max_import_depth = n as u32;
        }
    }

    // maxMacroDepth
    if let Ok(v) = js_sys::Reflect::get(options, &JsValue::from_str("maxMacroDepth")) {
        if let Some(n) = v.as_f64() {
            opts.max_macro_depth = n as u32;
        }
    }

    // maxLoopDepth
    if let Ok(v) = js_sys::Reflect::get(options, &JsValue::from_str("maxLoopDepth")) {
        if let Some(n) = v.as_f64() {
            opts.max_loop_depth = n as u32;
        }
    }

    // maxIterations
    if let Ok(v) = js_sys::Reflect::get(options, &JsValue::from_str("maxIterations")) {
        if let Some(n) = v.as_f64() {
            opts.max_iterations = n as u32;
        }
    }

    // importResolver: (path: string) => string | null
    if let Ok(v) = js_sys::Reflect::get(options, &JsValue::from_str("importResolver")) {
        if v.is_function() {
            let func = js_sys::Function::from(v);
            opts.fs = Some(Arc::new(JsFileSystem::new(func)));
        }
    }

    // files: Record<string, string> — populate an InMemoryFs
    if let Ok(v) = js_sys::Reflect::get(options, &JsValue::from_str("files")) {
        if v.is_object() && !v.is_null() && !v.is_undefined() {
            let obj = js_sys::Object::from(v);
            let keys = js_sys::Object::keys(&obj);
            let mut mem_fs = wcl_lang::InMemoryFs::new();
            for i in 0..keys.length() {
                let key = keys.get(i);
                if let Some(path) = key.as_string() {
                    if let Ok(content) = js_sys::Reflect::get(&obj, &key) {
                        if let Some(content_str) = content.as_string() {
                            mem_fs.add_file(PathBuf::from(&path), content_str);
                        }
                    }
                }
            }
            opts.fs = Some(Arc::new(mem_fs));
        }
    }

    // variables: Record<string, any>
    if let Ok(v) = js_sys::Reflect::get(options, &JsValue::from_str("variables")) {
        if v.is_object() && !v.is_null() && !v.is_undefined() {
            let obj = js_sys::Object::from(v);
            let keys = js_sys::Object::keys(&obj);
            for i in 0..keys.length() {
                let key = keys.get(i);
                if let Some(name) = key.as_string() {
                    if let Ok(val) = js_sys::Reflect::get(&obj, &key) {
                        if let Ok(wcl_val) = js_to_value(&val) {
                            opts.variables.insert(name, wcl_val);
                        }
                    }
                }
            }
        }
    }

    // libPaths: string[]
    if let Ok(v) = js_sys::Reflect::get(options, &JsValue::from_str("libPaths")) {
        if js_sys::Array::is_array(&v) {
            let arr = js_sys::Array::from(&v);
            for i in 0..arr.length() {
                if let Some(s) = arr.get(i).as_string() {
                    opts.lib_paths.push(PathBuf::from(s));
                }
            }
        }
    }

    // noDefaultLibPaths: boolean
    if let Ok(v) = js_sys::Reflect::get(options, &JsValue::from_str("noDefaultLibPaths")) {
        if let Some(b) = v.as_bool() {
            opts.no_default_lib_paths = b;
        }
    }

    // functions: Record<string, Function>
    if let Ok(v) = js_sys::Reflect::get(options, &JsValue::from_str("functions")) {
        if v.is_object() && !v.is_null() && !v.is_undefined() {
            let obj = js_sys::Object::from(v);
            let keys = js_sys::Object::keys(&obj);
            for i in 0..keys.length() {
                let key = keys.get(i);
                if let Some(name) = key.as_string() {
                    if let Ok(func_val) = js_sys::Reflect::get(&obj, &key) {
                        if func_val.is_function() {
                            let func = js_sys::Function::from(func_val);
                            let builtin: wcl_lang::BuiltinFn =
                                Arc::new(move |args: &[wcl_lang::Value]| {
                                    let js_args = js_sys::Array::new();
                                    for arg in args {
                                        js_args.push(&value_to_js(arg));
                                    }
                                    let result = func
                                        .apply(&JsValue::NULL, &js_args)
                                        .map_err(|e| format!("JS function error: {:?}", e))?;
                                    js_to_value(&result)
                                });
                            opts.functions.functions.insert(name, builtin);
                        }
                    }
                }
            }
        }
    }

    Ok(opts)
}

/// Parse a WCL source string and return a document object.
///
/// Returns `{ values, hasErrors, diagnostics }`.
#[wasm_bindgen]
pub fn parse(source: &str, options: Option<JsValue>) -> Result<JsValue, JsValue> {
    let opts = build_parse_options(options)?;
    let doc = wcl_lang::parse(source, opts);
    Ok(build_result(&doc))
}

/// Parse a WCL source string and return just the values object.
///
/// Returns a plain object with the evaluated values.
/// Throws if there are parse errors.
#[wasm_bindgen(js_name = "parseValues")]
pub fn parse_values(source: &str, options: Option<JsValue>) -> Result<JsValue, JsValue> {
    let opts = build_parse_options(options)?;
    let doc = wcl_lang::parse(source, opts);
    if doc.has_errors() {
        let messages: Vec<String> = doc.errors().iter().map(|d| d.message.clone()).collect();
        return Err(JsValue::from_str(&messages.join("; ")));
    }
    Ok(values_to_js(&doc.values))
}

/// Parse a WCL source string and execute a query against it.
///
/// Returns the query result as a JS value.
#[wasm_bindgen]
pub fn query(source: &str, query_str: &str, options: Option<JsValue>) -> Result<JsValue, JsValue> {
    let opts = build_parse_options(options)?;
    let doc = wcl_lang::parse(source, opts);
    if doc.has_errors() {
        let messages: Vec<String> = doc.errors().iter().map(|d| d.message.clone()).collect();
        return Err(JsValue::from_str(&messages.join("; ")));
    }
    let result = doc.query(query_str).map_err(|e| JsValue::from_str(&e))?;
    Ok(value_to_js(&result))
}
