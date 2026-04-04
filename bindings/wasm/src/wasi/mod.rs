mod callback;
mod registry;

use std::ffi::CStr;
use std::os::raw::c_char;
use std::path::PathBuf;
use wcl_lang::json::{block_ref_to_json, diagnostic_to_json, value_to_json, values_to_json};

// ── Memory management ────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn wcl_wasm_alloc(len: u32) -> *mut u8 {
    let layout = std::alloc::Layout::from_size_align(len as usize, 1).unwrap();
    unsafe { std::alloc::alloc(layout) }
}

#[no_mangle]
pub extern "C" fn wcl_wasm_dealloc(ptr: *mut u8, len: u32) {
    if ptr.is_null() {
        return;
    }
    let layout = std::alloc::Layout::from_size_align(len as usize, 1).unwrap();
    unsafe { std::alloc::dealloc(ptr, layout) }
}

// ── Helpers ──────────────────────────────────────────────────────────────

fn to_c_string(s: &str) -> *mut c_char {
    let bytes = s.as_bytes();
    let ptr = wcl_wasm_alloc((bytes.len() + 1) as u32);
    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr, bytes.len());
        *ptr.add(bytes.len()) = 0;
    }
    ptr as *mut c_char
}

unsafe fn read_c_str<'a>(ptr: *const c_char) -> &'a str {
    if ptr.is_null() {
        ""
    } else {
        CStr::from_ptr(ptr).to_str().unwrap_or("")
    }
}

fn build_parse_options(options_json: *const c_char) -> wcl_lang::ParseOptions {
    let mut opts = wcl_lang::ParseOptions::default();
    let json_str = unsafe { read_c_str(options_json) };
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

// ── Document lifecycle ───────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn wcl_wasm_parse(source: *const c_char, options_json: *const c_char) -> u32 {
    let source = unsafe { read_c_str(source) };
    let opts = build_parse_options(options_json);
    let doc = wcl_lang::parse(source, opts);
    registry::store(doc)
}

#[no_mangle]
pub extern "C" fn wcl_wasm_parse_with_functions(
    source: *const c_char,
    options_json: *const c_char,
    func_names_json: *const c_char,
) -> u32 {
    let source = unsafe { read_c_str(source) };
    let mut opts = build_parse_options(options_json);

    let names_str = unsafe { read_c_str(func_names_json) };
    if !names_str.is_empty() {
        if let Ok(names) = serde_json::from_str::<Vec<String>>(names_str) {
            for name in names {
                let builtin = callback::make_builtin_fn(name.clone());
                opts.functions.functions.insert(name, builtin);
            }
        }
    }

    let doc = wcl_lang::parse(source, opts);
    registry::store(doc)
}

#[no_mangle]
pub extern "C" fn wcl_wasm_document_free(handle: u32) {
    registry::remove(handle);
}

// ── Document methods ─────────────────────────────────────────────────────

#[no_mangle]
pub extern "C" fn wcl_wasm_document_values(handle: u32) -> *mut c_char {
    registry::with(handle, |doc| {
        let json = values_to_json(&doc.values);
        to_c_string(&json.to_string())
    })
    .unwrap_or(to_c_string("{}"))
}

#[no_mangle]
pub extern "C" fn wcl_wasm_document_has_errors(handle: u32) -> i32 {
    registry::with(handle, |doc| if doc.has_errors() { 1 } else { 0 }).unwrap_or(0)
}

#[no_mangle]
pub extern "C" fn wcl_wasm_document_diagnostics(handle: u32) -> *mut c_char {
    registry::with(handle, |doc| {
        let diags: Vec<serde_json::Value> =
            doc.diagnostics.iter().map(diagnostic_to_json).collect();
        to_c_string(&serde_json::Value::Array(diags).to_string())
    })
    .unwrap_or(to_c_string("[]"))
}

#[no_mangle]
pub extern "C" fn wcl_wasm_document_query(handle: u32, query: *const c_char) -> *mut c_char {
    let query_str = unsafe { read_c_str(query) };
    registry::with(handle, |doc| match doc.query(query_str) {
        Ok(value) => {
            let result = serde_json::json!({ "ok": value_to_json(&value) });
            to_c_string(&result.to_string())
        }
        Err(e) => {
            let result = serde_json::json!({ "error": e });
            to_c_string(&result.to_string())
        }
    })
    .unwrap_or_else(|| {
        let result = serde_json::json!({ "error": "invalid document handle" });
        to_c_string(&result.to_string())
    })
}

#[no_mangle]
pub extern "C" fn wcl_wasm_document_blocks(handle: u32) -> *mut c_char {
    registry::with(handle, |doc| {
        let blocks: Vec<serde_json::Value> = doc.blocks().iter().map(block_ref_to_json).collect();
        to_c_string(&serde_json::Value::Array(blocks).to_string())
    })
    .unwrap_or(to_c_string("[]"))
}

#[no_mangle]
pub extern "C" fn wcl_wasm_document_blocks_of_type(
    handle: u32,
    kind: *const c_char,
) -> *mut c_char {
    let kind_str = unsafe { read_c_str(kind) };
    registry::with(handle, |doc| {
        let blocks: Vec<serde_json::Value> = doc
            .blocks_of_type_resolved(kind_str)
            .iter()
            .map(block_ref_to_json)
            .collect();
        to_c_string(&serde_json::Value::Array(blocks).to_string())
    })
    .unwrap_or(to_c_string("[]"))
}

#[no_mangle]
pub extern "C" fn wcl_wasm_string_free(ptr: *mut c_char) {
    if ptr.is_null() {
        return;
    }
    // Find length by scanning for null terminator
    let len = unsafe { CStr::from_ptr(ptr).to_bytes().len() + 1 };
    wcl_wasm_dealloc(ptr as *mut u8, len as u32);
}
