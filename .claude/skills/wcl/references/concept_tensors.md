# Tensors

_Shape-carrying N-dimensional arrays — tensor<T, \[dims...\]>._

A `tensor<T, [dims...]>` is an N-dimensional array. Unlike a `list<list<...>>`, it carries an explicit shape, so its rank and per-axis sizes are visible to the type system and to any host consuming the value.

## Shape

The second type argument is a list of dimensions. Dimensions may be **fixed integers** or **symbolic names** that the host program resolves.

```wcl
weights: tensor<f64, [10, 20]>     // fixed 10 x 20 matrix
batch:   tensor<f64, [N, 3]>       // N rows of 3 floats (symbolic N)
volume:  tensor<u8,  [W, H, D]>    // three symbolic dims
```

## Construction

Build a tensor from a flat list of elements plus a shape. The data length must match the product of the dimensions.

```wcl
m = tensor([1.0, 2.0, 3.0, 4.0, 5.0, 6.0], [2, 3])   // 2x3 matrix
```

## Inspecting & reshaping

Three builtins cover the basic operations:

| Builtin | Result |
| --- | --- |
| `tensor_data(t)` | Flat `list<T>` of the underlying elements |
| `tensor_shape(t)` | `list<usize>` of the per-axis sizes |
| `tensor_reshape(t, shape)` | Same data viewed under a new shape |

```wcl
m_t = tensor_reshape(m, [3, 2])     // re-view the same numbers as 3x2
```

> [!NOTE]
> **When to reach for a tensor**
> Use a tensor when the data is genuinely rectangular and the rank matters (matrices, images, batches). For ragged or one-dimensional data, a list<T> is simpler.

## Related

- [Lists](../references/concept_lists.md)

- [Numbers](../references/concept_numbers.md)

[← Back to SKILL.md](../SKILL.md)
