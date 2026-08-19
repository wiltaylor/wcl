//! Tensor builtins: the `tensor` constructor, its `tensor_data` /
//! `tensor_shape` accessors, and `tensor_reshape`.
//!
//! A tensor is a flat element list plus the shape it is read under, so
//! every operation here is a shape check away from being a list
//! operation.

use super::builtin::from_fn;
use crate::environment::Environment;
use crate::value::Value;

/// Register every tensor builtin into `env`.
pub(super) fn register(env: &mut Environment) {
    env.add_builtin(
        "tensor",
        from_fn(tensor_pure)
            .doc("Build a tensor from flat row-major data and a shape; the data length must equal the product of the dimensions.")
            .param("data", "[number]", "Flat, row-major element data.")
            .param("shape", "[usize]", "The dimension sizes.")
            .returns("tensor<T>", "The constructed tensor."),
    );
    env.add_builtin(
        "tensor_data",
        from_fn(tensor_data_pure)
            .doc("The flat row-major element data of a tensor as a list.")
            .param("t", "tensor<T>", "The tensor to read.")
            .returns("[T]", "The tensor's flat, row-major element data."),
    );
    env.add_builtin(
        "tensor_shape",
        from_fn(tensor_shape_pure)
            .doc("The dimension sizes of a tensor as a list.")
            .param("t", "tensor<T>", "The tensor to read.")
            .returns("[usize]", "The tensor's dimension sizes."),
    );
    env.add_builtin(
        "tensor_reshape",
        from_fn(tensor_reshape_pure)
            .doc("Reinterpret a tensor's data under a new shape; the element count must be unchanged.")
            .param("t", "tensor<T>", "The tensor to reshape.")
            .param("shape", "[usize]", "The new dimension sizes.")
            .returns("tensor<T>", "The same data under the new shape."),
    );
}

/// Convert a `Value::List` of integer shape entries into `(dims, product)`,
/// rejecting empty shapes (when `allow_empty` is false), non-integer
/// entries, and shape products that overflow `u64`. `builtin` is
/// interpolated into every error message so the source-level builtin
/// name (`tensor`, `tensor_reshape`) appears in diagnostics.
fn validate_tensor_shape(
    builtin: &str,
    shape_vals: &[Value],
    allow_empty: bool,
) -> Result<(Vec<u64>, u64), String> {
    if !allow_empty && shape_vals.is_empty() {
        return Err(format!("{builtin}: shape must have at least one dimension"));
    }
    let mut dims: Vec<u64> = Vec::with_capacity(shape_vals.len());
    for s in shape_vals {
        let d = s.as_u64().ok_or_else(|| {
            format!(
                "{builtin}: shape entries must be non-negative integers, got {}",
                s.type_name()
            )
        })?;
        dims.push(d);
    }
    let mut product: u64 = 1;
    for d in &dims {
        product = product
            .checked_mul(*d)
            .ok_or_else(|| format!("{builtin}: shape product overflows u64"))?;
    }
    Ok((dims, product))
}

/// `tensor(data, shape)` — build a tensor, checking that the element
/// count matches the shape's product.
fn tensor_pure(data: Value, shape: Value) -> Result<Value, String> {
    let data = match data {
        Value::List(items) => items,
        other => {
            return Err(format!(
                "tensor: first argument must be a list, got {}",
                other.type_name()
            ));
        }
    };
    let shape_vals = match shape {
        Value::List(items) => items,
        other => {
            return Err(format!(
                "tensor: second argument must be a list of dimensions, got {}",
                other.type_name()
            ));
        }
    };
    let (dims, expected) = validate_tensor_shape("tensor", &shape_vals, false)?;
    if (data.len() as u64) != expected {
        return Err(format!(
            "tensor: data length {} does not match shape product {expected}",
            data.len(),
        ));
    }
    Ok(Value::Tensor { shape: dims, data })
}

/// `tensor_data(t)` — the elements in row-major order.
fn tensor_data_pure(v: Value) -> Result<Vec<Value>, String> {
    match v {
        Value::Tensor { data, .. } => Ok(std::sync::Arc::unwrap_or_clone(data)),
        other => Err(format!(
            "tensor_data: expected tensor, got {}",
            other.type_name()
        )),
    }
}

/// `tensor_shape(t)` — the extent of each dimension.
fn tensor_shape_pure(v: Value) -> Result<Vec<i64>, String> {
    match v {
        Value::Tensor { shape, .. } => Ok(shape.into_iter().map(|d| d as i64).collect()),
        other => Err(format!(
            "tensor_shape: expected tensor, got {}",
            other.type_name()
        )),
    }
}

/// `tensor_reshape(t, shape)` — reinterpret the same elements under a
/// new shape of the same total size.
fn tensor_reshape_pure(t: Value, new_shape: Value) -> Result<Value, String> {
    let (data, _old_shape) = match t {
        Value::Tensor { shape, data } => (data, shape),
        other => {
            return Err(format!(
                "tensor_reshape: expected tensor, got {}",
                other.type_name()
            ));
        }
    };
    let shape_vals = match new_shape {
        Value::List(items) => items,
        other => {
            return Err(format!(
                "tensor_reshape: shape must be a list of u64, got {}",
                other.type_name()
            ));
        }
    };
    let (dims, expected) = validate_tensor_shape("tensor_reshape", &shape_vals, true)?;
    if (data.len() as u64) != expected {
        return Err(format!(
            "tensor_reshape: data length {} does not match new shape product {expected}",
            data.len(),
        ));
    }
    Ok(Value::Tensor { shape: dims, data })
}
