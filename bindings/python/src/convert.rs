use pyo3::prelude::*;
use pyo3::types::{PyBool, PyDict, PyFloat, PyInt, PyList, PySet, PyString};
use wcl::Value;

use crate::types::{PyBlockRef, PyDecorator};

/// Convert a WCL Value to a Python object.
pub fn value_to_py(py: Python<'_>, value: &Value) -> PyResult<PyObject> {
    match value {
        Value::String(s) => Ok(s.into_pyobject(py)?.into_any().unbind()),
        Value::Int(i) => Ok(i.into_pyobject(py)?.into_any().unbind()),
        Value::Float(f) => Ok(f.into_pyobject(py)?.into_any().unbind()),
        Value::Bool(b) => Ok((*b)
            .into_pyobject(py)
            .unwrap()
            .to_owned()
            .into_any()
            .unbind()),
        Value::Null => Ok(py.None()),
        Value::Identifier(s) => Ok(s.into_pyobject(py)?.into_any().unbind()),
        Value::List(items) => {
            let py_items: Vec<PyObject> = items
                .iter()
                .map(|v| value_to_py(py, v))
                .collect::<PyResult<_>>()?;
            Ok(PyList::new(py, &py_items)?.into_any().unbind())
        }
        Value::Map(map) => {
            let dict = PyDict::new(py);
            for (k, v) in map {
                dict.set_item(k, value_to_py(py, v)?)?;
            }
            Ok(dict.into_any().unbind())
        }
        Value::Set(items) => {
            let py_items: Vec<PyObject> = items
                .iter()
                .map(|v| value_to_py(py, v))
                .collect::<PyResult<_>>()?;
            match PySet::new(py, &py_items) {
                Ok(s) => Ok(s.into_any().unbind()),
                Err(_) => {
                    // Fallback to list if items are unhashable
                    Ok(PyList::new(py, &py_items)?.into_any().unbind())
                }
            }
        }
        Value::BlockRef(br) => {
            let py_br = PyBlockRef::from_rust(py, br)?;
            Ok(py_br.into_pyobject(py)?.into_any().unbind())
        }
        Value::Function(_) => Ok(py.None()),
    }
}

/// Convert a Python object to a WCL Value.
#[allow(clippy::only_used_in_recursion)]
pub fn py_to_value(py: Python<'_>, obj: &Bound<'_, pyo3::PyAny>) -> PyResult<Value> {
    if obj.is_none() {
        return Ok(Value::Null);
    }
    // Check bool before int because bool is a subclass of int in Python
    if obj.is_instance_of::<PyBool>() {
        return Ok(Value::Bool(obj.extract::<bool>()?));
    }
    if obj.is_instance_of::<PyInt>() {
        return Ok(Value::Int(obj.extract::<i64>()?));
    }
    if obj.is_instance_of::<PyFloat>() {
        return Ok(Value::Float(obj.extract::<f64>()?));
    }
    if obj.is_instance_of::<PyString>() {
        return Ok(Value::String(obj.extract::<String>()?));
    }
    if obj.is_instance_of::<PyList>() {
        let list = obj.downcast::<PyList>()?;
        let items: Vec<Value> = list
            .iter()
            .map(|item| py_to_value(py, &item))
            .collect::<PyResult<_>>()?;
        return Ok(Value::List(items));
    }
    if obj.is_instance_of::<PyDict>() {
        let dict = obj.downcast::<PyDict>()?;
        let mut map = indexmap::IndexMap::new();
        for (k, v) in dict.iter() {
            let key: String = k.extract()?;
            map.insert(key, py_to_value(py, &v)?);
        }
        return Ok(Value::Map(map));
    }
    if obj.is_instance_of::<PySet>() {
        let set = obj.downcast::<PySet>()?;
        let items: Vec<Value> = set
            .iter()
            .map(|item| py_to_value(py, &item))
            .collect::<PyResult<_>>()?;
        return Ok(Value::Set(items));
    }
    Err(pyo3::exceptions::PyTypeError::new_err(format!(
        "cannot convert {} to WCL value",
        obj.get_type().name()?
    )))
}

/// Convert an IndexMap of WCL Values to a Python dict.
pub fn values_to_py_dict(
    py: Python<'_>,
    values: &indexmap::IndexMap<String, Value>,
) -> PyResult<PyObject> {
    let dict = PyDict::new(py);
    for (k, v) in values {
        dict.set_item(k, value_to_py(py, v)?)?;
    }
    Ok(dict.into_any().unbind())
}

/// Convert a DecoratorValue to a PyDecorator.
pub fn decorator_to_py(py: Python<'_>, dec: &wcl::DecoratorValue) -> PyResult<PyDecorator> {
    let args_dict = PyDict::new(py);
    for (k, v) in &dec.args {
        args_dict.set_item(k, value_to_py(py, v)?)?;
    }
    Ok(PyDecorator {
        name: dec.name.clone(),
        args: args_dict.into_any().unbind(),
    })
}
