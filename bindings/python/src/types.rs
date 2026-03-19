use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::convert::{decorator_to_py, value_to_py, values_to_py_dict};

/// A parsed and evaluated WCL document.
#[pyclass(name = "Document")]
pub struct PyDocument {
    pub(crate) doc: wcl::Document,
    pub(crate) values_cache: PyObject,
}

#[pymethods]
impl PyDocument {
    /// The evaluated values as a Python dict.
    #[getter]
    fn values(&self, py: Python<'_>) -> PyObject {
        self.values_cache.clone_ref(py)
    }

    /// Whether the document has any errors.
    #[getter]
    fn has_errors(&self) -> bool {
        self.doc.has_errors()
    }

    /// Only error diagnostics.
    #[getter]
    fn errors(&self) -> Vec<PyDiagnostic> {
        self.doc
            .diagnostics
            .iter()
            .filter(|d| d.is_error())
            .map(PyDiagnostic::from_rust)
            .collect()
    }

    /// All diagnostics.
    #[getter]
    fn diagnostics(&self) -> Vec<PyDiagnostic> {
        self.doc
            .diagnostics
            .iter()
            .map(PyDiagnostic::from_rust)
            .collect()
    }

    /// Execute a query against this document.
    fn query(&self, py: Python<'_>, query_str: &str) -> PyResult<PyObject> {
        match self.doc.query(query_str) {
            Ok(value) => value_to_py(py, &value),
            Err(e) => Err(pyo3::exceptions::PyValueError::new_err(e)),
        }
    }

    /// Get all blocks as BlockRef objects.
    fn blocks(&self, py: Python<'_>) -> PyResult<Vec<PyBlockRef>> {
        self.doc
            .blocks()
            .iter()
            .map(|br| PyBlockRef::from_rust(py, br))
            .collect()
    }

    /// Get blocks of a specific type.
    fn blocks_of_type(&self, py: Python<'_>, kind: &str) -> PyResult<Vec<PyBlockRef>> {
        self.doc
            .blocks_of_type_resolved(kind)
            .iter()
            .map(|br| PyBlockRef::from_rust(py, br))
            .collect()
    }
}

/// A reference to a WCL block with its attributes.
#[pyclass(name = "BlockRef")]
pub struct PyBlockRef {
    #[pyo3(get)]
    pub kind: String,
    #[pyo3(get)]
    pub id: Option<String>,
    #[pyo3(get)]
    pub labels: Vec<String>,
    pub(crate) attributes: PyObject,
    pub(crate) children_data: Vec<wcl::BlockRef>,
    pub(crate) decorators_data: Vec<wcl::DecoratorValue>,
}

#[pymethods]
impl PyBlockRef {
    #[getter]
    fn attributes(&self, py: Python<'_>) -> PyObject {
        self.attributes.clone_ref(py)
    }

    #[getter]
    fn children(&self, py: Python<'_>) -> PyResult<Vec<PyBlockRef>> {
        self.children_data
            .iter()
            .map(|c| PyBlockRef::from_rust(py, c))
            .collect()
    }

    #[getter]
    fn decorators(&self, py: Python<'_>) -> PyResult<Vec<PyDecorator>> {
        self.decorators_data
            .iter()
            .map(|d| decorator_to_py(py, d))
            .collect()
    }

    /// Check if this block has a decorator with the given name.
    fn has_decorator(&self, name: &str) -> bool {
        self.decorators_data.iter().any(|d| d.name == name)
    }

    /// Get an attribute value by key, or None if not found.
    fn get(&self, py: Python<'_>, key: &str) -> PyResult<PyObject> {
        let dict = self.attributes.bind(py);
        let dict = dict.downcast::<PyDict>()?;
        match dict.get_item(key)? {
            Some(v) => Ok(v.into_any().unbind()),
            None => Ok(py.None()),
        }
    }

    fn __repr__(&self) -> String {
        match &self.id {
            Some(id) => format!("BlockRef({} {})", self.kind, id),
            None => format!("BlockRef({})", self.kind),
        }
    }
}

impl PyBlockRef {
    pub fn from_rust(py: Python<'_>, br: &wcl::BlockRef) -> PyResult<Self> {
        let attributes = values_to_py_dict(py, &br.attributes)?;
        Ok(PyBlockRef {
            kind: br.kind.clone(),
            id: br.id.clone(),
            labels: br.labels.clone(),
            attributes,
            children_data: br.children.clone(),
            decorators_data: br.decorators.clone(),
        })
    }
}

/// A WCL decorator with name and arguments.
#[pyclass(name = "Decorator")]
pub struct PyDecorator {
    #[pyo3(get)]
    pub name: String,
    pub(crate) args: PyObject,
}

#[pymethods]
impl PyDecorator {
    #[getter]
    fn args(&self, py: Python<'_>) -> PyObject {
        self.args.clone_ref(py)
    }

    fn __repr__(&self) -> String {
        format!("Decorator(@{})", self.name)
    }
}

/// A WCL diagnostic (error, warning, etc.).
#[pyclass(name = "Diagnostic")]
#[derive(Clone)]
pub struct PyDiagnostic {
    #[pyo3(get)]
    pub severity: String,
    #[pyo3(get)]
    pub message: String,
    #[pyo3(get)]
    pub code: Option<String>,
}

impl PyDiagnostic {
    pub fn from_rust(d: &wcl::Diagnostic) -> Self {
        PyDiagnostic {
            severity: match d.severity {
                wcl::Severity::Error => "error".to_string(),
                wcl::Severity::Warning => "warning".to_string(),
                wcl::Severity::Info => "info".to_string(),
                wcl::Severity::Hint => "hint".to_string(),
            },
            message: d.message.clone(),
            code: d.code.clone(),
        }
    }
}

#[pymethods]
impl PyDiagnostic {
    fn __repr__(&self) -> String {
        match &self.code {
            Some(code) => format!("Diagnostic({}: [{}] {})", self.severity, code, self.message),
            None => format!("Diagnostic({}: {})", self.severity, self.message),
        }
    }
}

/// Builder for constructing WCL library files.
#[pyclass(name = "LibraryBuilder")]
pub struct PyLibraryBuilder {
    inner: wcl::library::LibraryBuilder,
}

#[pymethods]
impl PyLibraryBuilder {
    #[new]
    fn new(name: &str) -> Self {
        PyLibraryBuilder {
            inner: wcl::library::LibraryBuilder::new(name),
        }
    }

    /// Add raw WCL schema text.
    fn add_schema_text(&mut self, schema: &str) {
        self.inner.add_schema_text(schema);
    }

    /// Add a function stub declaration.
    #[pyo3(signature = (name, params, return_type=None, doc=None))]
    fn add_function_stub(
        &mut self,
        name: &str,
        params: Vec<(String, String)>,
        return_type: Option<String>,
        doc: Option<String>,
    ) {
        self.inner
            .add_function_stub(wcl::library::FunctionStub {
                name: name.to_string(),
                params,
                return_type,
                doc,
            });
    }

    /// Build the library content as a WCL string.
    fn build(&self) -> String {
        self.inner.build()
    }

    /// Install the library to the user library directory.
    fn install(&self) -> PyResult<String> {
        match self.inner.install() {
            Ok(path) => Ok(path.to_string_lossy().to_string()),
            Err(e) => Err(pyo3::exceptions::PyIOError::new_err(e.to_string())),
        }
    }
}
