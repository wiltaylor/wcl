use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::sync::Arc;

use crate::convert::{json_to_value, value_to_json};

/// C callback function type for custom WCL functions.
pub type WclCallbackFn =
    unsafe extern "C" fn(ctx: *mut c_void, args_json: *const c_char) -> *mut c_char;

/// Wrap a C callback as a `BuiltinFn` suitable for WCL's function registry.
pub fn make_builtin_fn(callback: WclCallbackFn, ctx: *mut c_void) -> wcl_lang::BuiltinFn {
    let wrapper = CallbackWrapper::new(callback, ctx);
    Arc::new(move |args: &[wcl_lang::Value]| wrapper.call(args))
}

struct CallbackWrapper {
    callback: WclCallbackFn,
    ctx: *mut c_void,
}

// Safety: The Go side guarantees the ctx handle and callback are valid across threads.
unsafe impl Send for CallbackWrapper {}
unsafe impl Sync for CallbackWrapper {}

impl CallbackWrapper {
    fn new(callback: WclCallbackFn, ctx: *mut c_void) -> Self {
        Self { callback, ctx }
    }

    fn call(&self, args: &[wcl_lang::Value]) -> Result<wcl_lang::Value, String> {
        let json_args: Vec<serde_json::Value> = args.iter().map(value_to_json).collect();
        let args_str =
            serde_json::to_string(&json_args).map_err(|e| format!("serialize args: {}", e))?;
        let c_args = CString::new(args_str).map_err(|e| format!("CString error: {}", e))?;

        let result_ptr = unsafe { (self.callback)(self.ctx, c_args.as_ptr()) };
        if result_ptr.is_null() {
            return Err("callback returned null".to_string());
        }

        let result_str = unsafe { CStr::from_ptr(result_ptr) }
            .to_str()
            .map_err(|e| format!("invalid UTF-8 from callback: {}", e))?
            .to_string();

        // Free the string allocated by the callback (Go side uses C.CString / C malloc)
        unsafe { libc_free(result_ptr as *mut c_void) };

        if let Some(msg) = result_str.strip_prefix("ERR:") {
            return Err(msg.to_string());
        }

        let json: serde_json::Value = serde_json::from_str(&result_str)
            .map_err(|e| format!("parse callback result: {}", e))?;
        json_to_value(&json)
    }
}

extern "C" {
    #[link_name = "free"]
    fn libc_free(ptr: *mut c_void);
}
