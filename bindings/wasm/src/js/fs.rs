use std::path::{Path, PathBuf};
use wcl::FileSystem;

/// Normalize a path by resolving `.` and `..` without touching the filesystem.
fn normalize_path(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                components.pop();
            }
            Component::CurDir => {}
            other => {
                components.push(other);
            }
        }
    }
    components.iter().collect()
}

/// A filesystem backed by a JavaScript callback function.
///
/// The JS function receives a path string and should return:
/// - A string with the file contents if the file exists
/// - `null` or `undefined` if the file does not exist
pub struct JsFileSystem {
    resolver: js_sys::Function,
}

impl JsFileSystem {
    pub fn new(resolver: js_sys::Function) -> Self {
        JsFileSystem { resolver }
    }
}

// SAFETY: WASM is single-threaded; there is no concurrent access.
unsafe impl Send for JsFileSystem {}
unsafe impl Sync for JsFileSystem {}

impl FileSystem for JsFileSystem {
    fn read_file(&self, path: &Path) -> Result<String, String> {
        let path_str = wasm_bindgen::JsValue::from_str(&path.to_string_lossy());
        let result = self
            .resolver
            .call1(&wasm_bindgen::JsValue::NULL, &path_str)
            .map_err(|e| format!("import resolver error: {:?}", e))?;

        if result.is_null() || result.is_undefined() {
            Err(format!("file not found: {}", path.display()))
        } else {
            result
                .as_string()
                .ok_or_else(|| "import resolver must return a string or null".to_string())
        }
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, String> {
        Ok(normalize_path(path))
    }

    fn exists(&self, path: &Path) -> bool {
        let path_str = wasm_bindgen::JsValue::from_str(&path.to_string_lossy());
        match self.resolver.call1(&wasm_bindgen::JsValue::NULL, &path_str) {
            Ok(result) => !result.is_null() && !result.is_undefined(),
            Err(_) => false,
        }
    }

    fn glob(&self, _pattern: &Path) -> Result<Vec<PathBuf>, String> {
        Err("glob imports are not supported in WASM contexts".to_string())
    }
}
