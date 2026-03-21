use std::sync::Arc;
use wcl::json::{json_to_value, value_to_json};

extern "C" {
    fn host_call_function(
        name_ptr: *const u8,
        name_len: u32,
        args_ptr: *const u8,
        args_len: u32,
        result_ptr: *mut *mut u8,
        result_len: *mut u32,
    ) -> i32;
}

#[allow(unused_unsafe)]
pub fn make_builtin_fn(name: String) -> wcl::BuiltinFn {
    Arc::new(move |args: &[wcl::Value]| {
        let args_json: Vec<serde_json::Value> = args.iter().map(value_to_json).collect();
        let args_str = serde_json::Value::Array(args_json).to_string();

        let mut result_ptr: *mut u8 = std::ptr::null_mut();
        let mut result_len: u32 = 0;

        let rc = unsafe {
            host_call_function(
                name.as_ptr(),
                name.len() as u32,
                args_str.as_ptr(),
                args_str.len() as u32,
                &mut result_ptr,
                &mut result_len,
            )
        };

        if rc != 0 {
            let error = if !result_ptr.is_null() && result_len > 0 {
                let bytes =
                    unsafe { std::slice::from_raw_parts(result_ptr, result_len as usize) };
                let msg = String::from_utf8_lossy(bytes).to_string();
                unsafe {
                    super::wcl_wasm_dealloc(result_ptr, result_len);
                }
                msg
            } else {
                "host function call failed".to_string()
            };
            return Err(error);
        }

        if result_ptr.is_null() || result_len == 0 {
            return Ok(wcl::Value::Null);
        }

        let bytes = unsafe { std::slice::from_raw_parts(result_ptr, result_len as usize) };
        let result_str = String::from_utf8_lossy(bytes).to_string();
        unsafe {
            super::wcl_wasm_dealloc(result_ptr, result_len);
        }

        let json: serde_json::Value =
            serde_json::from_str(&result_str).map_err(|e| format!("invalid JSON result: {}", e))?;
        json_to_value(&json)
    })
}
