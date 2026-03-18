use pyo3::prelude::*;
use pyo3::types::PyDict;
use std::path::PathBuf;
use std::sync::Arc;

mod convert;
mod types;

use convert::{py_to_value, value_to_py, values_to_py_dict};
use types::{PyBlockRef, PyDecorator, PyDiagnostic, PyDocument, PyLibraryBuilder};

/// Build ParseOptions from Python keyword arguments.
fn build_parse_options(_py: Python<'_>, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<wcl::ParseOptions> {
    let mut opts = wcl::ParseOptions::default();

    let Some(kwargs) = kwargs else {
        return Ok(opts);
    };

    if let Some(v) = kwargs.get_item("root_dir")? {
        opts.root_dir = PathBuf::from(v.extract::<String>()?);
    }
    if let Some(v) = kwargs.get_item("allow_imports")? {
        opts.allow_imports = v.extract::<bool>()?;
    }
    if let Some(v) = kwargs.get_item("max_import_depth")? {
        opts.max_import_depth = v.extract::<u32>()?;
    }
    if let Some(v) = kwargs.get_item("max_macro_depth")? {
        opts.max_macro_depth = v.extract::<u32>()?;
    }
    if let Some(v) = kwargs.get_item("max_loop_depth")? {
        opts.max_loop_depth = v.extract::<u32>()?;
    }
    if let Some(v) = kwargs.get_item("max_iterations")? {
        opts.max_iterations = v.extract::<u32>()?;
    }
    if let Some(v) = kwargs.get_item("functions")? {
        let dict = v.downcast::<PyDict>()?;
        for (name, callable) in dict.iter() {
            let name: String = name.extract()?;
            let callable: PyObject = callable.unbind();
            let func: wcl::BuiltinFn = Arc::new(move |args: &[wcl::Value]| {
                Python::with_gil(|py| {
                    let py_args: Vec<PyObject> = args
                        .iter()
                        .map(|a| value_to_py(py, a))
                        .collect::<PyResult<_>>()
                        .map_err(|e| e.to_string())?;
                    let py_list = pyo3::types::PyList::new(py, &py_args)
                        .map_err(|e| e.to_string())?;
                    let result = callable
                        .call1(py, (py_list,))
                        .map_err(|e| e.to_string())?;
                    py_to_value(py, result.bind(py)).map_err(|e| e.to_string())
                })
            });
            opts.functions.functions.insert(name, func);
        }
    }

    Ok(opts)
}

/// Parse a WCL source string and return a Document.
#[pyfunction]
#[pyo3(signature = (source, /, **kwargs))]
fn parse(py: Python<'_>, source: &str, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<PyDocument> {
    let opts = build_parse_options(py, kwargs)?;
    let doc = wcl::parse(source, opts);
    let values_cache = values_to_py_dict(py, &doc.values)?;
    Ok(PyDocument {
        doc,
        values_cache,
    })
}

/// Parse a WCL file and return a Document.
#[pyfunction]
#[pyo3(signature = (path, /, **kwargs))]
fn parse_file(py: Python<'_>, path: &str, kwargs: Option<&Bound<'_, PyDict>>) -> PyResult<PyDocument> {
    let file_path = PathBuf::from(path);
    let source = std::fs::read_to_string(&file_path)
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(format!("{}: {}", path, e)))?;

    // Set root_dir to the parent directory if not explicitly provided
    let mut opts = build_parse_options(py, kwargs)?;
    if kwargs.and_then(|kw| kw.get_item("root_dir").ok().flatten()).is_none() {
        if let Some(parent) = file_path.parent() {
            opts.root_dir = parent.to_path_buf();
        }
    }

    let doc = wcl::parse(&source, opts);
    let values_cache = values_to_py_dict(py, &doc.values)?;
    Ok(PyDocument {
        doc,
        values_cache,
    })
}

/// Install a library file into the user library directory.
#[pyfunction]
fn install_library(name: &str, content: &str) -> PyResult<String> {
    match wcl::library::install_library(name, content) {
        Ok(path) => Ok(path.to_string_lossy().to_string()),
        Err(e) => Err(pyo3::exceptions::PyIOError::new_err(e.to_string())),
    }
}

/// Remove a library file from the user library directory.
#[pyfunction]
fn uninstall_library(name: &str) -> PyResult<()> {
    wcl::library::uninstall_library(name)
        .map_err(|e| pyo3::exceptions::PyIOError::new_err(e.to_string()))
}

/// List all library files in the user library directory.
#[pyfunction]
fn list_libraries() -> PyResult<Vec<String>> {
    match wcl::library::list_libraries() {
        Ok(paths) => Ok(paths.iter().map(|p| p.to_string_lossy().to_string()).collect()),
        Err(e) => Err(pyo3::exceptions::PyIOError::new_err(e.to_string())),
    }
}

/// WCL Python bindings module.
#[pymodule]
fn _wcl_native(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(parse, m)?)?;
    m.add_function(wrap_pyfunction!(parse_file, m)?)?;
    m.add_function(wrap_pyfunction!(install_library, m)?)?;
    m.add_function(wrap_pyfunction!(uninstall_library, m)?)?;
    m.add_function(wrap_pyfunction!(list_libraries, m)?)?;
    m.add_class::<PyDocument>()?;
    m.add_class::<PyBlockRef>()?;
    m.add_class::<PyDecorator>()?;
    m.add_class::<PyDiagnostic>()?;
    m.add_class::<PyLibraryBuilder>()?;
    Ok(())
}
