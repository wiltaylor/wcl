#![allow(clippy::not_unsafe_ptr_arg_deref)]

mod callback;
mod convert;

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::path::PathBuf;

use convert::{block_ref_to_json, diagnostic_to_json, values_to_json};

pub use callback::WclCallbackFn;

/// Opaque document handle. Use `wcl_ffi_document_free` to release.
pub type WclDocument = c_void;

thread_local! {
    static LAST_ERROR: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

fn set_last_error(msg: String) {
    LAST_ERROR.with(|e| *e.borrow_mut() = Some(msg));
}

fn clear_last_error() {
    LAST_ERROR.with(|e| *e.borrow_mut() = None);
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn to_c_string(s: &str) -> *mut c_char {
    CString::new(s).unwrap_or_default().into_raw()
}

fn ok_json(value: serde_json::Value) -> *mut c_char {
    let result = serde_json::json!({ "ok": value });
    to_c_string(&result.to_string())
}

fn err_json(msg: &str) -> *mut c_char {
    let result = serde_json::json!({ "error": msg });
    to_c_string(&result.to_string())
}

unsafe fn c_str_to_str<'a>(ptr: *const c_char) -> &'a str {
    if ptr.is_null() {
        ""
    } else {
        CStr::from_ptr(ptr).to_str().unwrap_or("")
    }
}

fn as_doc(ptr: *const WclDocument) -> &'static wcl_lang::Document {
    unsafe { &*(ptr as *const wcl_lang::Document) }
}

fn build_parse_options(options_json: *const c_char) -> wcl_lang::ParseOptions {
    let mut opts = wcl_lang::ParseOptions::default();
    let json_str = unsafe { c_str_to_str(options_json) };
    if json_str.is_empty() {
        return opts;
    }
    let Ok(json) = serde_json::from_str::<serde_json::Value>(json_str) else {
        return opts;
    };
    if let Some(s) = json.get("rootDir").and_then(|v| v.as_str()) {
        opts.root_dir = PathBuf::from(s);
    }
    if let Some(b) = json.get("allowImports").and_then(|v| v.as_bool()) {
        opts.allow_imports = b;
    }
    if let Some(n) = json.get("maxImportDepth").and_then(|v| v.as_u64()) {
        opts.max_import_depth = n as u32;
    }
    if let Some(n) = json.get("maxMacroDepth").and_then(|v| v.as_u64()) {
        opts.max_macro_depth = n as u32;
    }
    if let Some(n) = json.get("maxLoopDepth").and_then(|v| v.as_u64()) {
        opts.max_loop_depth = n as u32;
    }
    if let Some(n) = json.get("maxIterations").and_then(|v| v.as_u64()) {
        opts.max_iterations = n as u32;
    }
    if let Some(vars) = json.get("variables").and_then(|v| v.as_object()) {
        for (key, val) in vars {
            if let Ok(wcl_val) = wcl_lang::json::json_to_value(val) {
                opts.variables.insert(key.clone(), wcl_val);
            }
        }
    }
    opts
}

fn doc_into_ptr(doc: wcl_lang::Document) -> *mut WclDocument {
    Box::into_raw(Box::new(doc)) as *mut WclDocument
}

// ── Document lifecycle ───────────────────────────────────────────────────

/// Parse a WCL source string and return an opaque Document pointer.
///
/// `options_json` is an optional JSON string with parse options (may be null).
/// The caller must free the returned document with `wcl_ffi_document_free`.
#[no_mangle]
pub extern "C" fn wcl_ffi_parse(
    source: *const c_char,
    options_json: *const c_char,
) -> *mut WclDocument {
    let source = unsafe { c_str_to_str(source) };
    let opts = build_parse_options(options_json);
    let doc = wcl_lang::parse(source, opts);
    doc_into_ptr(doc)
}

/// Parse a WCL file and return an opaque Document pointer.
///
/// Returns null on I/O failure; call `wcl_ffi_last_error` to get the message.
/// Sets root_dir to the file's parent directory if not specified in options.
/// The caller must free the returned document with `wcl_ffi_document_free`.
#[no_mangle]
pub extern "C" fn wcl_ffi_parse_file(
    path: *const c_char,
    options_json: *const c_char,
) -> *mut WclDocument {
    clear_last_error();
    let path_str = unsafe { c_str_to_str(path) };
    let file_path = PathBuf::from(path_str);

    let source = match std::fs::read_to_string(&file_path) {
        Ok(s) => s,
        Err(e) => {
            set_last_error(format!("{}: {}", path_str, e));
            return std::ptr::null_mut();
        }
    };

    let mut opts = build_parse_options(options_json);

    let json_str = unsafe { c_str_to_str(options_json) };
    let has_root_dir = !json_str.is_empty()
        && serde_json::from_str::<serde_json::Value>(json_str)
            .ok()
            .and_then(|j| j.get("rootDir").cloned())
            .is_some();

    if !has_root_dir {
        if let Some(parent) = file_path.parent() {
            opts.root_dir = parent.to_path_buf();
        }
    }

    let doc = wcl_lang::parse(&source, opts);
    doc_into_ptr(doc)
}

/// Get the last error message from a failed FFI call.
///
/// Returns null if no error. Caller must free with `wcl_ffi_string_free`.
#[no_mangle]
pub extern "C" fn wcl_ffi_last_error() -> *mut c_char {
    LAST_ERROR.with(|e| match e.borrow().as_deref() {
        Some(msg) => to_c_string(msg),
        None => std::ptr::null_mut(),
    })
}

/// Free a Document previously returned by `wcl_ffi_parse`.
///
/// Safe to call with null. Must not be called twice on the same pointer.
#[no_mangle]
pub extern "C" fn wcl_ffi_document_free(doc: *mut WclDocument) {
    if !doc.is_null() {
        unsafe {
            drop(Box::from_raw(doc as *mut wcl_lang::Document));
        }
    }
}

// ── Document methods ─────────────────────────────────────────────────────

/// Get the evaluated values as a JSON string.
///
/// Caller must free with `wcl_ffi_string_free`.
#[no_mangle]
pub extern "C" fn wcl_ffi_document_values(doc: *const WclDocument) -> *mut c_char {
    let doc = as_doc(doc);
    let json = values_to_json(&doc.values);
    to_c_string(&json.to_string())
}

/// Check if the document has any errors.
#[no_mangle]
pub extern "C" fn wcl_ffi_document_has_errors(doc: *const WclDocument) -> bool {
    as_doc(doc).has_errors()
}

/// Get error diagnostics as a JSON array string.
///
/// Caller must free with `wcl_ffi_string_free`.
#[no_mangle]
pub extern "C" fn wcl_ffi_document_errors(doc: *const WclDocument) -> *mut c_char {
    let doc = as_doc(doc);
    let errors: Vec<serde_json::Value> = doc
        .diagnostics
        .iter()
        .filter(|d| d.is_error())
        .map(diagnostic_to_json)
        .collect();
    to_c_string(&serde_json::Value::Array(errors).to_string())
}

/// Get all diagnostics as a JSON array string.
///
/// Caller must free with `wcl_ffi_string_free`.
#[no_mangle]
pub extern "C" fn wcl_ffi_document_diagnostics(doc: *const WclDocument) -> *mut c_char {
    let doc = as_doc(doc);
    let diags: Vec<serde_json::Value> = doc.diagnostics.iter().map(diagnostic_to_json).collect();
    to_c_string(&serde_json::Value::Array(diags).to_string())
}

/// Execute a query against the document.
///
/// Returns JSON: `{"ok": <value>}` or `{"error": "message"}`.
/// Caller must free with `wcl_ffi_string_free`.
#[no_mangle]
pub extern "C" fn wcl_ffi_document_query(
    doc: *const WclDocument,
    query: *const c_char,
) -> *mut c_char {
    let doc = as_doc(doc);
    let query_str = unsafe { c_str_to_str(query) };
    match doc.query(query_str) {
        Ok(value) => ok_json(convert::value_to_json(&value)),
        Err(e) => err_json(&e),
    }
}

/// Get all blocks as a JSON array string.
///
/// Caller must free with `wcl_ffi_string_free`.
#[no_mangle]
pub extern "C" fn wcl_ffi_document_blocks(doc: *const WclDocument) -> *mut c_char {
    let doc = as_doc(doc);
    let blocks: Vec<serde_json::Value> = doc.blocks().iter().map(block_ref_to_json).collect();
    to_c_string(&serde_json::Value::Array(blocks).to_string())
}

/// Get blocks of a specific type as a JSON array string.
///
/// Caller must free with `wcl_ffi_string_free`.
#[no_mangle]
pub extern "C" fn wcl_ffi_document_blocks_of_type(
    doc: *const WclDocument,
    kind: *const c_char,
) -> *mut c_char {
    let doc = as_doc(doc);
    let kind_str = unsafe { c_str_to_str(kind) };
    let blocks: Vec<serde_json::Value> = doc
        .blocks_of_type_resolved(kind_str)
        .iter()
        .map(block_ref_to_json)
        .collect();
    to_c_string(&serde_json::Value::Array(blocks).to_string())
}

// ── Parse with custom functions ──────────────────────────────────────────

/// Parse a WCL source string with custom callback functions.
///
/// - `func_names`: array of C strings (function names)
/// - `func_callbacks`: array of C callback function pointers
/// - `func_contexts`: array of opaque context pointers (one per callback)
/// - `func_count`: number of functions
///
/// Returns an opaque Document pointer. Caller must free with `wcl_ffi_document_free`.
#[no_mangle]
pub extern "C" fn wcl_ffi_parse_with_functions(
    source: *const c_char,
    options_json: *const c_char,
    func_names: *const *const c_char,
    func_callbacks: *const WclCallbackFn,
    func_contexts: *const usize,
    func_count: usize,
) -> *mut WclDocument {
    let source = unsafe { c_str_to_str(source) };
    let mut opts = build_parse_options(options_json);

    for i in 0..func_count {
        let name = unsafe { c_str_to_str(*func_names.add(i)) }.to_string();
        let cb = unsafe { *func_callbacks.add(i) };
        let ctx = unsafe { *func_contexts.add(i) } as *mut c_void;
        let builtin = callback::make_builtin_fn(cb, ctx);
        opts.functions.functions.insert(name, builtin);
    }

    let doc = wcl_lang::parse(source, opts);
    doc_into_ptr(doc)
}

// ── Library management ───────────────────────────────────────────────────

/// List installed libraries. Returns JSON: `{"ok": ["path1", ...]}` or `{"error": "..."}`.
///
/// Caller must free with `wcl_ffi_string_free`.
#[no_mangle]
pub extern "C" fn wcl_ffi_list_libraries() -> *mut c_char {
    match wcl_lang::library::list_libraries() {
        Ok(paths) => {
            let names: Vec<String> = paths
                .iter()
                .map(|p| p.to_string_lossy().to_string())
                .collect();
            ok_json(serde_json::json!(names))
        }
        Err(e) => err_json(&e.to_string()),
    }
}

// ── Function calling ─────────────────────────────────────────────────────

/// Call an exported function by name.
///
/// `args_json` is a JSON array of arguments. Returns a JSON string with
/// the result, or an `ERR:message` string on failure.
#[no_mangle]
pub extern "C" fn wcl_ffi_call_function(
    doc: *mut WclDocument,
    name: *const c_char,
    args_json: *const c_char,
) -> *mut c_char {
    clear_last_error();
    let doc = unsafe { &*(doc as *const wcl_lang::Document) };
    let name = unsafe { CStr::from_ptr(name) }.to_str().unwrap_or("");
    let args_str = if args_json.is_null() {
        "[]"
    } else {
        unsafe { CStr::from_ptr(args_json) }
            .to_str()
            .unwrap_or("[]")
    };

    let args: Vec<wcl_lang::Value> = match serde_json::from_str::<Vec<serde_json::Value>>(args_str)
    {
        Ok(json_args) => {
            let mut vals = Vec::new();
            for jv in &json_args {
                match convert::json_to_value(jv) {
                    Ok(v) => vals.push(v),
                    Err(e) => {
                        let msg = format!("ERR:arg conversion: {}", e);
                        return CString::new(msg).unwrap().into_raw();
                    }
                }
            }
            vals
        }
        Err(e) => {
            let msg = format!("ERR:invalid args JSON: {}", e);
            return CString::new(msg).unwrap().into_raw();
        }
    };

    match doc.call_function(name, &args) {
        Ok(result) => {
            let json = wcl_lang::json::value_to_json(&result);
            let s = serde_json::to_string(&json).unwrap_or_else(|_| "null".to_string());
            CString::new(s).unwrap().into_raw()
        }
        Err(e) => {
            let msg = format!("ERR:{}", e);
            CString::new(msg).unwrap().into_raw()
        }
    }
}

/// List exported functions as a JSON array of objects with `name` and `params` fields.
#[no_mangle]
pub extern "C" fn wcl_ffi_list_functions(doc: *mut WclDocument) -> *mut c_char {
    clear_last_error();
    let doc = unsafe { &*(doc as *const wcl_lang::Document) };

    let names = doc.exported_function_names();
    let mut entries = Vec::new();
    for name in &names {
        let params = doc
            .values
            .get(*name)
            .and_then(|v| match v {
                wcl_lang::Value::Function(f) => Some(&f.params),
                _ => None,
            })
            .cloned()
            .unwrap_or_default();
        entries.push(serde_json::json!({
            "name": name,
            "params": params,
        }));
    }

    let json = serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string());
    CString::new(json).unwrap().into_raw()
}

// ── Memory management ────────────────────────────────────────────────────

/// Free a string previously returned by any `wcl_ffi_*` function.
///
/// Safe to call with null.
#[no_mangle]
pub extern "C" fn wcl_ffi_string_free(s: *mut c_char) {
    if !s.is_null() {
        unsafe {
            drop(CString::from_raw(s));
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::CString;

    fn c(s: &str) -> CString {
        CString::new(s).unwrap()
    }

    #[test]
    fn test_parse_and_values() {
        let source = c("x = 42\ny = \"hello\"");
        let doc = wcl_ffi_parse(source.as_ptr(), std::ptr::null());
        assert!(!doc.is_null());
        assert!(!wcl_ffi_document_has_errors(doc));

        let values_ptr = wcl_ffi_document_values(doc);
        let values_str = unsafe { CStr::from_ptr(values_ptr) }.to_str().unwrap();
        let values: serde_json::Value = serde_json::from_str(values_str).unwrap();
        assert_eq!(values["x"], 42);
        assert_eq!(values["y"], "hello");

        wcl_ffi_string_free(values_ptr);
        wcl_ffi_document_free(doc);
    }

    #[test]
    fn test_parse_with_errors() {
        let source = c("x = @invalid");
        let doc = wcl_ffi_parse(source.as_ptr(), std::ptr::null());
        assert!(!doc.is_null());
        assert!(wcl_ffi_document_has_errors(doc));

        let errors_ptr = wcl_ffi_document_errors(doc);
        let errors_str = unsafe { CStr::from_ptr(errors_ptr) }.to_str().unwrap();
        let errors: Vec<serde_json::Value> = serde_json::from_str(errors_str).unwrap();
        assert!(!errors.is_empty());
        assert_eq!(errors[0]["severity"], "error");

        wcl_ffi_string_free(errors_ptr);
        wcl_ffi_document_free(doc);
    }

    #[test]
    fn test_parse_with_options() {
        let source = c("x = 42");
        let opts = c(r#"{"maxImportDepth": 10}"#);
        let doc = wcl_ffi_parse(source.as_ptr(), opts.as_ptr());
        assert!(!doc.is_null());
        assert!(!wcl_ffi_document_has_errors(doc));
        wcl_ffi_document_free(doc);
    }

    #[test]
    fn test_query() {
        let source = c("service { port = 8080 }\nservice { port = 9090 }");
        let doc = wcl_ffi_parse(source.as_ptr(), std::ptr::null());
        assert!(!wcl_ffi_document_has_errors(doc));

        let query = c("service | .port");
        let result_ptr = wcl_ffi_document_query(doc, query.as_ptr());
        let result_str = unsafe { CStr::from_ptr(result_ptr) }.to_str().unwrap();
        let result: serde_json::Value = serde_json::from_str(result_str).unwrap();
        assert_eq!(result["ok"], serde_json::json!([8080, 9090]));

        wcl_ffi_string_free(result_ptr);
        wcl_ffi_document_free(doc);
    }

    #[test]
    fn test_blocks() {
        let source = c("server { port = 80 }\nclient { timeout = 30 }");
        let doc = wcl_ffi_parse(source.as_ptr(), std::ptr::null());

        let blocks_ptr = wcl_ffi_document_blocks(doc);
        let blocks_str = unsafe { CStr::from_ptr(blocks_ptr) }.to_str().unwrap();
        let blocks: Vec<serde_json::Value> = serde_json::from_str(blocks_str).unwrap();
        assert_eq!(blocks.len(), 2);

        let kind = c("server");
        let server_blocks_ptr = wcl_ffi_document_blocks_of_type(doc, kind.as_ptr());
        let server_str = unsafe { CStr::from_ptr(server_blocks_ptr) }
            .to_str()
            .unwrap();
        let server_blocks: Vec<serde_json::Value> = serde_json::from_str(server_str).unwrap();
        assert_eq!(server_blocks.len(), 1);
        assert_eq!(server_blocks[0]["kind"], "server");

        wcl_ffi_string_free(blocks_ptr);
        wcl_ffi_string_free(server_blocks_ptr);
        wcl_ffi_document_free(doc);
    }

    #[test]
    fn test_diagnostics() {
        let source = c("x = 42");
        let doc = wcl_ffi_parse(source.as_ptr(), std::ptr::null());

        let diags_ptr = wcl_ffi_document_diagnostics(doc);
        let diags_str = unsafe { CStr::from_ptr(diags_ptr) }.to_str().unwrap();
        let _diags: Vec<serde_json::Value> = serde_json::from_str(diags_str).unwrap();

        wcl_ffi_string_free(diags_ptr);
        wcl_ffi_document_free(doc);
    }

    #[test]
    fn test_parse_with_variables() {
        let source = c("port = PORT");
        let opts = c(r#"{"variables":{"PORT":8080}}"#);
        let doc = wcl_ffi_parse(source.as_ptr(), opts.as_ptr());
        assert!(!doc.is_null());
        assert!(!wcl_ffi_document_has_errors(doc));

        let values_ptr = wcl_ffi_document_values(doc);
        let values_str = unsafe { CStr::from_ptr(values_ptr) }.to_str().unwrap();
        let values: serde_json::Value = serde_json::from_str(values_str).unwrap();
        assert_eq!(values["port"], 8080);

        wcl_ffi_string_free(values_ptr);
        wcl_ffi_document_free(doc);
    }

    #[test]
    fn test_variables_override_let() {
        let source = c("let x = 2\nresult = x");
        let opts = c(r#"{"variables":{"x":99}}"#);
        let doc = wcl_ffi_parse(source.as_ptr(), opts.as_ptr());
        assert!(!doc.is_null());
        assert!(!wcl_ffi_document_has_errors(doc));

        let values_ptr = wcl_ffi_document_values(doc);
        let values_str = unsafe { CStr::from_ptr(values_ptr) }.to_str().unwrap();
        let values: serde_json::Value = serde_json::from_str(values_str).unwrap();
        assert_eq!(values["result"], 99);

        wcl_ffi_string_free(values_ptr);
        wcl_ffi_document_free(doc);
    }

    #[test]
    fn test_document_free_null() {
        wcl_ffi_document_free(std::ptr::null_mut());
    }

    #[test]
    fn test_string_free_null() {
        wcl_ffi_string_free(std::ptr::null_mut());
    }
}
