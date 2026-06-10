//! Collection builtins: map, filter, fold, len, sum, range, head, tail,
//! the predicate forms (any, all, find, sort_by, min_by, max_by,
//! group_by), enumerate/slice and the string shapers (chars, repeat,
//! pad_start, pad_end), the record accessors (keys, values, merge,
//! map_values), plus the tensor constructor/accessors. Registered in
//! [`Environment::new`](crate::Environment::new).

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::builtins::{BuiltinFn, Caller, from_fn};
use crate::environment::Environment;
use crate::numeric::for_each_numeric_variant;
use crate::value::{Value, VariantPayload};

/// Register every collection builtin into `env`.
pub(crate) fn register(env: &mut Environment) {
    env.add_builtin(
        "map",
        BuiltinFn::hof(2, map_hof)
            .doc("Apply a function to every element of a list or tensor, returning the transformed collection.")
            .param("xs", "[T]", "The list or tensor to transform.")
            .param("f", "fn (T) -> U", "Function applied to each element.")
            .returns("[U]", "A new collection of the transformed elements."),
    );
    env.add_builtin(
        "filter",
        BuiltinFn::hof(2, filter_hof)
            .doc("Keep only the list elements for which the predicate returns `true`.")
            .param("xs", "[T]", "The list to filter.")
            .param(
                "pred",
                "fn (T) -> bool",
                "Predicate deciding whether to keep an element.",
            )
            .returns(
                "[T]",
                "The elements for which the predicate returned `true`.",
            ),
    );
    env.add_builtin(
        "fold",
        BuiltinFn::hof(3, fold_hof)
            .doc("Reduce a list or tensor to a single value by repeatedly combining the accumulator with each element.")
            .param("xs", "[T]", "The list or tensor to reduce.")
            .param("init", "U", "The initial accumulator value.")
            .param("f", "fn (U, T) -> U", "Combines the accumulator with the next element.")
            .returns("U", "The final accumulator value."),
    );

    env.add_builtin(
        "len",
        from_fn(len_pure)
            .doc("The number of elements in a list or tensor, or characters in a string.")
            .param("xs", "[T]", "A list, tensor, or string.")
            .returns("usize", "The number of elements (or characters)."),
    );
    env.add_builtin(
        "sum",
        from_fn(sum_pure)
            .doc("Add together every element of a non-empty homogeneous numeric list or tensor.")
            .param(
                "xs",
                "[number]",
                "A non-empty list or tensor of one numeric type.",
            )
            .returns("number", "The total, in the element's numeric type."),
    );
    env.add_builtin(
        "range",
        from_fn(range_pure)
            .doc("The half-open integer range `[start, end)` as a list.")
            .param("start", "i64", "Inclusive lower bound.")
            .param("end", "i64", "Exclusive upper bound; must be >= `start`.")
            .returns(
                "[i64]",
                "The integers from `start` up to (but excluding) `end`.",
            ),
    );
    env.add_builtin(
        "head",
        from_fn(head_pure)
            .doc("The first element of a list or tensor (`none` when empty).")
            .param("xs", "[T]", "A list or tensor.")
            .returns("T", "The first element, or `none` if empty."),
    );
    env.add_builtin(
        "tail",
        from_fn(tail_pure)
            .doc("Every element of a list or tensor except the first.")
            .param("xs", "[T]", "A list or tensor.")
            .returns("[T]", "The elements after the first."),
    );

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
        "error",
        from_fn(|msg: String| -> Result<Value, String> { Err(msg) })
            .doc("Abort evaluation with an error message.")
            .param("msg", "utf8", "The error message to report.")
            .returns("never", "Never returns — aborts evaluation."),
    );

    env.add_builtin(
        "panic",
        from_fn(|msg: String| -> Result<Value, String> { Err(msg) })
            .doc("Abort evaluation with an unrecoverable failure message.")
            .param("msg", "utf8", "The failure message to report.")
            .returns("never", "Never returns — aborts evaluation."),
    );
    env.add_builtin(
        "assert",
        from_fn(|cond: bool, msg: String| -> Result<Value, String> {
            if cond { Ok(Value::None) } else { Err(msg) }
        })
        .doc("Return `none` when `cond` is true, otherwise abort with `msg`.")
        .param("cond", "bool", "The condition that must hold.")
        .param(
            "msg",
            "utf8",
            "The error message reported when `cond` is false.",
        )
        .returns(
            "none",
            "`none` when the assertion holds (otherwise aborts).",
        ),
    );

    env.add_builtin(
        "concat",
        from_fn(|a: String, b: String| -> String { format!("{a}{b}") })
            .doc("Concatenate two strings into one.")
            .param("a", "utf8", "The left-hand string.")
            .param("b", "utf8", "The string appended after `a`.")
            .returns("utf8", "The two strings joined together."),
    );
    env.add_builtin(
        "format",
        BuiltinFn::hof(0, format_hof)
            // Variadic: keep an explicit signature since `...args` isn't a
            // structurally-expressible parameter.
            .with_signature("fn (utf8, ...args) -> utf8")
            .doc("Substitute trailing arguments into a template's `{}` placeholders (`{{`/`}}` are literal braces).")
            .param("template", "utf8", "Template string with `{}` placeholders.")
            .returns("utf8", "The template with placeholders substituted."),
    );

    env.add_builtin(
        "flatten",
        from_fn(flatten_pure)
            .doc("Concatenate a list of lists into a single list, one level deep.")
            .param(
                "xss",
                "[[T]]",
                "A list whose elements are themselves lists.",
            )
            .returns("[T]", "The inner lists concatenated, one level deep."),
    );
    env.add_builtin(
        "zip",
        from_fn(zip_pure)
            .doc("Pair up elements of two lists by index, stopping at the shorter length.")
            .param("a", "[A]", "The first list.")
            .param("b", "[B]", "The second list.")
            .returns(
                "[(A, B)]",
                "Index-paired `[a, b]` lists, up to the shorter length.",
            ),
    );

    env.add_builtin(
        "tensor_reshape",
        from_fn(tensor_reshape_pure)
            .doc("Reinterpret a tensor's data under a new shape; the element count must be unchanged.")
            .param("t", "tensor<T>", "The tensor to reshape.")
            .param("shape", "[usize]", "The new dimension sizes.")
            .returns("tensor<T>", "The same data under the new shape."),
    );

    // ── String builtins ─────────────────────────────────────────────
    env.add_builtin(
        "split",
        from_fn(|s: String, sep: String| -> Value {
            Value::List(s.split(&sep).map(|p| Value::Utf8(p.to_string())).collect())
        })
        .doc("Split a string on every occurrence of a separator into a list of pieces.")
        .param("s", "utf8", "The string to split.")
        .param("sep", "utf8", "The separator to split on.")
        .returns("[utf8]", "The pieces between separators."),
    );
    env.add_builtin(
        "join",
        from_fn(|parts: Vec<String>, sep: String| -> String { parts.join(&sep) })
            .doc("Join a list of strings into one, inserting a separator between each.")
            .param("parts", "[utf8]", "The strings to join.")
            .param("sep", "utf8", "The separator inserted between parts.")
            .returns("utf8", "The joined string."),
    );
    env.add_builtin(
        "replace",
        from_fn(|s: String, old: String, new: String| -> String { s.replace(&old, &new) })
            .doc("Replace every occurrence of a substring with another.")
            .param("s", "utf8", "The string to search.")
            .param("old", "utf8", "The substring to find.")
            .param("new", "utf8", "The replacement substring.")
            .returns("utf8", "The string with every match replaced."),
    );
    env.add_builtin(
        "contains",
        from_fn(|s: String, needle: String| -> bool { s.contains(&needle) })
            .doc("Whether a string contains a substring.")
            .param("s", "utf8", "The string to search.")
            .param("needle", "utf8", "The substring to look for.")
            .returns("bool", "`true` if the substring is present."),
    );
    env.add_builtin(
        "starts_with",
        from_fn(|s: String, prefix: String| -> bool { s.starts_with(&prefix) })
            .doc("Whether a string begins with a prefix.")
            .param("s", "utf8", "The string to test.")
            .param("prefix", "utf8", "The prefix to look for.")
            .returns("bool", "`true` if the string starts with the prefix."),
    );
    env.add_builtin(
        "ends_with",
        from_fn(|s: String, suffix: String| -> bool { s.ends_with(&suffix) })
            .doc("Whether a string ends with a suffix.")
            .param("s", "utf8", "The string to test.")
            .param("suffix", "utf8", "The suffix to look for.")
            .returns("bool", "`true` if the string ends with the suffix."),
    );
    env.add_builtin(
        "to_upper",
        from_fn(|s: String| -> String { s.to_uppercase() })
            .doc("Uppercase every character of a string.")
            .param("s", "utf8", "The string to uppercase.")
            .returns("utf8", "The uppercased string."),
    );
    env.add_builtin(
        "to_lower",
        from_fn(|s: String| -> String { s.to_lowercase() })
            .doc("Lowercase every character of a string.")
            .param("s", "utf8", "The string to lowercase.")
            .returns("utf8", "The lowercased string."),
    );
    env.add_builtin(
        "trim",
        from_fn(|s: String| -> String { s.trim().to_string() })
            .doc("Remove leading and trailing whitespace from a string.")
            .param("s", "utf8", "The string to trim.")
            .returns("utf8", "The string without leading/trailing whitespace."),
    );

    // ── List builtins ───────────────────────────────────────────────
    env.add_builtin(
        "list_contains",
        from_fn(list_contains_pure)
            .doc("Whether a list contains a value equal to `needle`.")
            .param("xs", "[T]", "The list to search.")
            .param("needle", "T", "The value to look for.")
            .returns("bool", "`true` if an equal element is present."),
    );
    env.add_builtin(
        "reverse",
        from_fn(|xs: Vec<Value>| -> Value {
            let mut xs = xs;
            xs.reverse();
            Value::List(xs)
        })
        .doc("Reverse the order of a list's elements.")
        .param("xs", "[T]", "The list to reverse.")
        .returns("[T]", "The list in reverse order."),
    );
    env.add_builtin(
        "sort",
        from_fn(sort_pure)
            .doc("Sort a list — numerically for all-numeric lists, lexicographically for all-string lists.")
            .param("xs", "[T]", "An all-numeric or all-string list.")
            .returns("[T]", "The sorted list."),
    );
    env.add_builtin(
        "sort_connected",
        from_fn(sort_connected_pure)
            .doc("Reorder a list so that items joined by edges cluster together (recursing into `children`).")
            .param("items", "[T]", "Items identified by an `id` field (possibly nested via `children`).")
            .param("edges", "[{source, destination, ...}]", "Edge records linking item ids.")
            .returns("[T]", "The reordered list, connected items adjacent."),
    );
    env.add_builtin(
        "unique",
        from_fn(|xs: Vec<Value>| -> Value {
            let mut seen = Vec::with_capacity(xs.len());
            for x in xs {
                if !seen.contains(&x) {
                    seen.push(x);
                }
            }
            Value::List(seen)
        })
        .doc("Remove duplicate elements from a list, keeping first-seen order.")
        .param("xs", "[T]", "The list to deduplicate.")
        .returns("[T]", "The list with duplicates removed."),
    );
    env.add_builtin(
        "index_of",
        from_fn(|xs: Vec<Value>, x: Value| -> i64 {
            xs.iter()
                .position(|v| v == &x)
                .map(|i| i as i64)
                .unwrap_or(-1)
        })
        .doc("The index of the first element equal to `needle`, or `-1` if absent.")
        .param("xs", "[T]", "The list to search.")
        .param("needle", "T", "The value to look for.")
        .returns("i64", "The zero-based index, or `-1` if not found."),
    );
    env.add_builtin(
        "at",
        from_fn(|xs: Vec<Value>, i: i64| -> Result<Value, String> {
            if i < 0 {
                return Err(format!("at: index {i} is negative"));
            }
            xs.into_iter()
                .nth(i as usize)
                .ok_or_else(|| format!("at: index {i} out of bounds"))
        })
        .doc("The element at a zero-based index; errors if out of bounds or negative.")
        .param("xs", "[T]", "The list to index.")
        .param("i", "i64", "The zero-based index.")
        .returns("T", "The element at `i`."),
    );
    env.add_builtin(
        "take",
        from_fn(|xs: Vec<Value>, n: i64| -> Value {
            let n = n.max(0) as usize;
            Value::List(xs.into_iter().take(n).collect())
        })
        .doc("The first `n` elements of a list (fewer if the list is shorter).")
        .param("xs", "[T]", "The list to take from.")
        .param("n", "i64", "How many leading elements to keep.")
        .returns("[T]", "The first `n` elements."),
    );
    env.add_builtin(
        "drop",
        from_fn(|xs: Vec<Value>, n: i64| -> Value {
            let n = n.max(0) as usize;
            Value::List(xs.into_iter().skip(n).collect())
        })
        .doc("Every element of a list after the first `n`.")
        .param("xs", "[T]", "The list to drop from.")
        .param("n", "i64", "How many leading elements to skip.")
        .returns("[T]", "The elements after the first `n`."),
    );

    env.add_builtin(
        "keys",
        from_fn(|r: Value| -> Result<Value, String> {
            let fields = record_fields("keys", &r)?;
            Ok(Value::List(
                fields.keys().map(|k| Value::Utf8(k.clone())).collect(),
            ))
        })
        .doc("The field names of a record, in deterministic (sorted) order.")
        .param(
            "r",
            "record",
            "A record value (or a union variant with a record body).",
        )
        .returns("[utf8]", "The field names."),
    );
    env.add_builtin(
        "values",
        from_fn(|r: Value| -> Result<Value, String> {
            let fields = record_fields("values", &r)?;
            Ok(Value::List(fields.values().cloned().collect()))
        })
        .doc("The field values of a record, in the same order as `keys`.")
        .param(
            "r",
            "record",
            "A record value (or a union variant with a record body).",
        )
        .returns("[T]", "The field values."),
    );
    env.add_builtin(
        "merge",
        from_fn(|a: Value, b: Value| -> Result<Value, String> {
            let (a_ty, a_fields) = match a {
                Value::Record { ty, fields } => (ty, fields),
                other => {
                    return Err(format!(
                        "merge: first argument must be a record, got {}",
                        other.type_name()
                    ));
                }
            };
            let b_fields = match b {
                Value::Record { fields, .. } => fields,
                other => {
                    return Err(format!(
                        "merge: second argument must be a record, got {}",
                        other.type_name()
                    ));
                }
            };
            let mut fields = a_fields;
            fields.extend(b_fields);
            Ok(Value::Record { ty: a_ty, fields })
        })
        .doc("Combine two records into one; fields of `b` win on a name clash.")
        .param("a", "record", "The base record.")
        .param("b", "record", "The overriding record.")
        .returns("record", "A record with the union of both field sets."),
    );
    env.add_builtin(
        "map_values",
        BuiltinFn::hof(2, map_values_hof)
            .doc("Apply a function to every field value of a record, keeping the keys.")
            .param("r", "record", "The record to transform.")
            .param("f", "fn (T) -> U", "Function applied to each field value.")
            .returns(
                "record",
                "A record with the same keys and transformed values.",
            ),
    );

    env.add_builtin(
        "any",
        BuiltinFn::hof(2, any_hof)
            .doc("`true` when the predicate holds for at least one element (short-circuits).")
            .param("xs", "[T]", "The list to test.")
            .param(
                "pred",
                "fn (T) -> bool",
                "Predicate applied to each element.",
            )
            .returns("bool", "`true` if any element satisfies the predicate."),
    );
    env.add_builtin(
        "all",
        BuiltinFn::hof(2, all_hof)
            .doc("`true` when the predicate holds for every element (short-circuits; `true` for an empty list).")
            .param("xs", "[T]", "The list to test.")
            .param("pred", "fn (T) -> bool", "Predicate applied to each element.")
            .returns("bool", "`true` if every element satisfies the predicate."),
    );
    env.add_builtin(
        "find",
        BuiltinFn::hof(2, find_hof)
            .doc("The first element for which the predicate returns `true`, or `none`.")
            .param("xs", "[T]", "The list to search.")
            .param(
                "pred",
                "fn (T) -> bool",
                "Predicate applied to each element.",
            )
            .returns("T", "The first matching element, or `none`."),
    );
    env.add_builtin(
        "sort_by",
        BuiltinFn::hof(2, sort_by_hof)
            .doc("Sort a list by a key function (stable). Keys must be all numeric or all strings.")
            .param("xs", "[T]", "The list to sort.")
            .param("key", "fn (T) -> K", "Maps each element to its sort key.")
            .returns("[T]", "The elements ordered by ascending key."),
    );
    env.add_builtin(
        "min_by",
        BuiltinFn::hof(2, min_by_hof)
            .doc("The element with the smallest key, or `none` for an empty list.")
            .param("xs", "[T]", "The list to search.")
            .param(
                "key",
                "fn (T) -> K",
                "Maps each element to its comparison key.",
            )
            .returns("T", "The element with the smallest key, or `none`."),
    );
    env.add_builtin(
        "max_by",
        BuiltinFn::hof(2, max_by_hof)
            .doc("The element with the largest key, or `none` for an empty list.")
            .param("xs", "[T]", "The list to search.")
            .param(
                "key",
                "fn (T) -> K",
                "Maps each element to its comparison key.",
            )
            .returns("T", "The element with the largest key, or `none`."),
    );
    env.add_builtin(
        "group_by",
        BuiltinFn::hof(2, group_by_hof)
            .doc("Group elements by a key function into `{ key, items }` records, in first-seen key order.")
            .param("xs", "[T]", "The list to group.")
            .param("key", "fn (T) -> K", "Maps each element to its group key.")
            .returns("[record]", "One `{ key, items }` record per distinct key."),
    );
    env.add_builtin(
        "enumerate",
        from_fn(|xs: Vec<Value>| -> Value {
            Value::List(
                xs.into_iter()
                    .enumerate()
                    .map(|(i, v)| Value::List(vec![Value::I64(i as i64), v]))
                    .collect(),
            )
        })
        .doc("Pair every element with its zero-based index, as `[index, element]` pairs.")
        .param("xs", "[T]", "The list to enumerate.")
        .returns("[[i64, T]]", "`[index, element]` pairs."),
    );

    env.add_builtin(
        "slice",
        from_fn(slice_pure)
            .doc("The half-open range `[start, end)` of a string's characters or a list's elements (bounds are clamped).")
            .param("xs", "utf8 | [T]", "The string or list to slice.")
            .param("start", "i64", "Inclusive start index (clamped to the length).")
            .param("end", "i64", "Exclusive end index (clamped to the length).")
            .returns("utf8 | [T]", "The sub-string / sub-list."),
    );
    env.add_builtin(
        "chars",
        from_fn(|s: String| -> Value {
            Value::List(s.chars().map(|c| Value::Utf8(c.to_string())).collect())
        })
        .doc("The characters of a string as a list of one-character strings.")
        .param("s", "utf8", "The string to split into characters.")
        .returns("[utf8]", "One string per character."),
    );
    env.add_builtin(
        "repeat",
        from_fn(|s: String, n: i64| -> String { s.repeat(n.max(0) as usize) })
            .doc("A string repeated `n` times (empty for `n <= 0`).")
            .param("s", "utf8", "The string to repeat.")
            .param("n", "i64", "How many copies to concatenate.")
            .returns("utf8", "`n` copies of `s`."),
    );
    env.add_builtin(
        "pad_start",
        from_fn(
            |s: String, width: i64, pad: String| -> Result<Value, String> {
                Ok(Value::Utf8(pad_string(s, width, &pad, true)?))
            },
        )
        .doc("Left-pad a string with a fill pattern until it is `width` characters long.")
        .param("s", "utf8", "The string to pad.")
        .param("width", "i64", "The target character count.")
        .param(
            "pad",
            "utf8",
            "The fill pattern (repeated / truncated as needed).",
        )
        .returns(
            "utf8",
            "The padded string (unchanged if already wide enough).",
        ),
    );
    env.add_builtin(
        "pad_end",
        from_fn(
            |s: String, width: i64, pad: String| -> Result<Value, String> {
                Ok(Value::Utf8(pad_string(s, width, &pad, false)?))
            },
        )
        .doc("Right-pad a string with a fill pattern until it is `width` characters long.")
        .param("s", "utf8", "The string to pad.")
        .param("width", "i64", "The target character count.")
        .param(
            "pad",
            "utf8",
            "The fill pattern (repeated / truncated as needed).",
        )
        .returns(
            "utf8",
            "The padded string (unchanged if already wide enough).",
        ),
    );
}

/// Run a `fn (T) -> bool` predicate over one element, requiring a boolean.
fn call_pred(
    who: &str,
    caller: &mut dyn Caller,
    f: &crate::value::FnValue,
    elem: &Value,
) -> Result<bool, String> {
    match caller.call_fn(f, std::slice::from_ref(elem))? {
        Value::Bool(b) => Ok(b),
        other => Err(format!(
            "{who}: predicate must return bool, got {}",
            other.type_name()
        )),
    }
}

fn expect_list<'a>(who: &str, v: &'a Value) -> Result<&'a [Value], String> {
    match v {
        Value::List(items) => Ok(items),
        other => Err(format!(
            "{who}: first argument must be a list, got {}",
            other.type_name()
        )),
    }
}

fn any_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let f = expect_function("any", "second argument", &args[1])?.clone();
    for elem in expect_list("any", &args[0])? {
        if call_pred("any", caller, &f, elem)? {
            return Ok(Value::Bool(true));
        }
    }
    Ok(Value::Bool(false))
}

fn all_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let f = expect_function("all", "second argument", &args[1])?.clone();
    for elem in expect_list("all", &args[0])? {
        if !call_pred("all", caller, &f, elem)? {
            return Ok(Value::Bool(false));
        }
    }
    Ok(Value::Bool(true))
}

fn find_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let f = expect_function("find", "second argument", &args[1])?.clone();
    for elem in expect_list("find", &args[0])? {
        if call_pred("find", caller, &f, elem)? {
            return Ok(elem.clone());
        }
    }
    Ok(Value::None)
}

/// A sort/compare key produced by a `key` function: all-numeric or
/// all-string, mirroring `sort`'s rules.
enum SortKey {
    Num(f64),
    Str(String),
}

fn sort_key(who: &str, v: Value) -> Result<SortKey, String> {
    if v.is_numeric() {
        return Ok(SortKey::Num(v.as_f64().unwrap_or(f64::NAN)));
    }
    match v {
        Value::Utf8(s) | Value::Ascii(s) => Ok(SortKey::Str(s)),
        other => Err(format!(
            "{who}: key function must return a number or string, got {}",
            other.type_name()
        )),
    }
}

fn compare_keys(a: &SortKey, b: &SortKey) -> Result<std::cmp::Ordering, String> {
    match (a, b) {
        (SortKey::Num(x), SortKey::Num(y)) => {
            Ok(x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal))
        }
        (SortKey::Str(x), SortKey::Str(y)) => Ok(x.cmp(y)),
        _ => Err("keys must be all numeric or all strings, found mixed".to_string()),
    }
}

fn keyed_elements(
    who: &str,
    caller: &mut dyn Caller,
    args: &[Value],
) -> Result<Vec<(SortKey, Value)>, String> {
    let f = expect_function(who, "second argument", &args[1])?.clone();
    let mut keyed = Vec::new();
    for elem in expect_list(who, &args[0])? {
        let key = sort_key(who, caller.call_fn(&f, std::slice::from_ref(elem))?)?;
        keyed.push((key, elem.clone()));
    }
    Ok(keyed)
}

fn sort_by_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let mut keyed = keyed_elements("sort_by", caller, args)?;
    let mut err = None;
    keyed.sort_by(|a, b| match compare_keys(&a.0, &b.0) {
        Ok(ord) => ord,
        Err(e) => {
            err.get_or_insert(e);
            std::cmp::Ordering::Equal
        }
    });
    match err {
        Some(e) => Err(format!("sort_by: {e}")),
        None => Ok(Value::List(keyed.into_iter().map(|(_, v)| v).collect())),
    }
}

fn extreme_by(
    who: &str,
    caller: &mut dyn Caller,
    args: &[Value],
    want: std::cmp::Ordering,
) -> Result<Value, String> {
    let keyed = keyed_elements(who, caller, args)?;
    let mut best: Option<(SortKey, Value)> = None;
    for (key, v) in keyed {
        best = match best {
            None => Some((key, v)),
            Some((bk, bv)) => {
                if compare_keys(&key, &bk).map_err(|e| format!("{who}: {e}"))? == want {
                    Some((key, v))
                } else {
                    Some((bk, bv))
                }
            }
        };
    }
    Ok(best.map(|(_, v)| v).unwrap_or(Value::None))
}

fn min_by_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    extreme_by("min_by", caller, args, std::cmp::Ordering::Less)
}

fn max_by_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    extreme_by("max_by", caller, args, std::cmp::Ordering::Greater)
}

fn group_by_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let f = expect_function("group_by", "second argument", &args[1])?.clone();
    // First-seen key order, with whole-Value key equality (so symbol /
    // bool / record keys group correctly even though they can't sort).
    let mut groups: Vec<(Value, Vec<Value>)> = Vec::new();
    for elem in expect_list("group_by", &args[0])? {
        let key = caller.call_fn(&f, std::slice::from_ref(elem))?;
        match groups.iter_mut().find(|(k, _)| k == &key) {
            Some((_, items)) => items.push(elem.clone()),
            None => groups.push((key, vec![elem.clone()])),
        }
    }
    Ok(Value::List(
        groups
            .into_iter()
            .map(|(key, items)| {
                let mut fields = BTreeMap::new();
                fields.insert("key".to_string(), key);
                fields.insert("items".to_string(), Value::List(items));
                Value::Record {
                    ty: Vec::new(),
                    fields,
                }
            })
            .collect(),
    ))
}

fn slice_pure(xs: Value, start: i64, end: i64) -> Result<Value, String> {
    let clamp = |len: usize| {
        let s = start.clamp(0, len as i64) as usize;
        let e = end.clamp(0, len as i64) as usize;
        (s, s.max(e))
    };
    match xs {
        Value::Utf8(s) | Value::Ascii(s) => {
            let chars: Vec<char> = s.chars().collect();
            let (lo, hi) = clamp(chars.len());
            Ok(Value::Utf8(chars[lo..hi].iter().collect()))
        }
        Value::List(items) => {
            let (lo, hi) = clamp(items.len());
            Ok(Value::List(items[lo..hi].to_vec()))
        }
        other => Err(format!(
            "slice: expected a string or list, got {}",
            other.type_name()
        )),
    }
}

fn pad_string(s: String, width: i64, pad: &str, at_start: bool) -> Result<String, String> {
    if pad.is_empty() {
        return Err("pad_start/pad_end: pad pattern must not be empty".to_string());
    }
    let want = width.max(0) as usize;
    let have = s.chars().count();
    if have >= want {
        return Ok(s);
    }
    let fill: String = pad.chars().cycle().take(want - have).collect();
    Ok(if at_start {
        format!("{fill}{s}")
    } else {
        format!("{s}{fill}")
    })
}

/// Borrow the field map out of a record-shaped value: a record literal /
/// projected record, or a union variant with a record body.
fn record_fields<'a>(who: &str, v: &'a Value) -> Result<&'a BTreeMap<String, Value>, String> {
    match v {
        Value::Record { fields, .. } => Ok(fields),
        Value::Variant {
            payload: VariantPayload::Record(fields),
            ..
        } => Ok(fields),
        other => Err(format!(
            "{who}: expected a record, got {}",
            other.type_name()
        )),
    }
}

fn map_values_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let f = expect_function("map_values", "second argument", &args[1])?.clone();
    let (ty, fields) = match &args[0] {
        Value::Record { ty, fields } => (ty.clone(), fields),
        Value::Variant {
            payload: VariantPayload::Record(fields),
            ..
        } => (Vec::new(), fields),
        other => {
            return Err(format!(
                "map_values: first argument must be a record, got {}",
                other.type_name()
            ));
        }
    };
    let mut out = BTreeMap::new();
    for (k, v) in fields {
        out.insert(k.clone(), caller.call_fn(&f, std::slice::from_ref(v))?);
    }
    Ok(Value::Record { ty, fields: out })
}

fn list_contains_pure(xs: Vec<Value>, needle: Value) -> bool {
    xs.iter().any(|v| v == &needle)
}

fn sort_pure(xs: Vec<Value>) -> Result<Value, String> {
    // Numeric lists sort numerically; string lists sort lexicographically.
    // Mixed-type lists and unsupported types fail loudly rather than
    // silently producing a partial ordering.
    if xs.is_empty() {
        return Ok(Value::List(xs));
    }
    let first = &xs[0];
    if first.is_numeric() && xs.iter().all(|v| v.is_numeric()) {
        let mut keyed: Vec<(f64, Value)> = xs
            .iter()
            .map(|v| (v.as_f64().unwrap_or(f64::NAN), v.clone()))
            .collect();
        keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        return Ok(Value::List(keyed.into_iter().map(|(_, v)| v).collect()));
    }
    let all_strings = xs
        .iter()
        .all(|v| matches!(v, Value::Utf8(_) | Value::Ascii(_)));
    if all_strings {
        let mut keyed: Vec<(String, Value)> = xs
            .iter()
            .map(|v| {
                let key = match v {
                    Value::Utf8(s) | Value::Ascii(s) => s.clone(),
                    _ => unreachable!(),
                };
                (key, v.clone())
            })
            .collect();
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        return Ok(Value::List(keyed.into_iter().map(|(_, v)| v).collect()));
    }
    Err(format!(
        "sort: list must be all numeric or all strings, found mixed (first element type: {})",
        first.type_name()
    ))
}

/// `sort_connected(items, edges)` — reorder a list so that items
/// participating in the same connections cluster together. Each item
/// is identified by an `id` field (Record or Variant payload-Record).
/// Each edge is a record with `source` / `destination` id values
/// (extra fields are ignored — the projector's `kind` and any custom
/// fields flow through untouched). Items without an `id` are kept in
/// their relative source order at the end.
///
/// **Recursive lift.** Items with a `children` field that is a
/// `list` are sorted in place: edges whose both endpoints live
/// inside a child subtree drive that subtree's order, and edges
/// whose endpoints straddle two children of this level cause those
/// children to be ordered next to each other.
fn sort_connected_pure(items_v: Value, edges_v: Value) -> Result<Value, String> {
    let items = match items_v {
        Value::List(xs) => xs,
        other => {
            return Err(format!(
                "sort_connected: first argument must be a list, got {}",
                other.type_name()
            ));
        }
    };
    let edges = match edges_v {
        Value::List(xs) => xs,
        other => {
            return Err(format!(
                "sort_connected: second argument must be a list of edges, got {}",
                other.type_name()
            ));
        }
    };
    Ok(Value::List(sort_connected_level(items, &edges)))
}

fn sort_connected_level(items: Vec<Value>, edges: &[Value]) -> Vec<Value> {
    if items.is_empty() {
        return items;
    }
    let descendants: Vec<HashSet<String>> = items.iter().map(collect_descendant_ids).collect();

    // Build adjacency by lifting each edge to whichever pair of
    // top-level items contain its endpoints. Self-loops (both
    // endpoints in the same subtree) drive the recursive sort, not
    // this level's adjacency.
    let mut adj: HashMap<usize, HashSet<usize>> = HashMap::new();
    for edge in edges {
        let Some((src, dst)) = edge_endpoint_pair(edge) else {
            continue;
        };
        let src_idx = find_subtree(&descendants, &src);
        let dst_idx = find_subtree(&descendants, &dst);
        if let (Some(si), Some(di)) = (src_idx, dst_idx)
            && si != di
        {
            adj.entry(si).or_default().insert(di);
            adj.entry(di).or_default().insert(si);
        }
    }

    let order = greedy_bfs_order(items.len(), &adj);

    // Recurse into each ordered item's children, scoping edges to
    // that subtree.
    order
        .into_iter()
        .map(|i| {
            let sub_edges: Vec<Value> = edges
                .iter()
                .filter(|e| {
                    edge_endpoint_pair(e)
                        .map(|(s, d)| descendants[i].contains(&s) && descendants[i].contains(&d))
                        .unwrap_or(false)
                })
                .cloned()
                .collect();
            recurse_sort_children(items[i].clone(), &sub_edges)
        })
        .collect()
}

/// Greedy BFS ordering: start from the highest-degree node, emit its
/// neighbours by descending degree, repeat for any remaining
/// component. Items with no edges keep their relative source order
/// at the end.
fn greedy_bfs_order(n: usize, adj: &HashMap<usize, HashSet<usize>>) -> Vec<usize> {
    let degree = |i: usize| adj.get(&i).map(|s| s.len()).unwrap_or(0);
    let mut visited = vec![false; n];
    let mut order: Vec<usize> = Vec::with_capacity(n);
    let mut seeds: Vec<usize> = (0..n).filter(|i| degree(*i) > 0).collect();
    seeds.sort_by(|a, b| degree(*b).cmp(&degree(*a)).then(a.cmp(b)));
    for start in seeds {
        if visited[start] {
            continue;
        }
        let mut queue: Vec<usize> = vec![start];
        let mut head = 0;
        while head < queue.len() {
            let cur = queue[head];
            head += 1;
            if visited[cur] {
                continue;
            }
            visited[cur] = true;
            order.push(cur);
            if let Some(neighbors) = adj.get(&cur) {
                let mut neigh: Vec<usize> =
                    neighbors.iter().copied().filter(|i| !visited[*i]).collect();
                neigh.sort_by(|a, b| degree(*b).cmp(&degree(*a)).then(a.cmp(b)));
                queue.extend(neigh);
            }
        }
    }
    // Append untouched items in their original order.
    for (i, seen) in visited.iter().enumerate() {
        if !seen {
            order.push(i);
        }
    }
    order
}

fn recurse_sort_children(item: Value, edges: &[Value]) -> Value {
    match item {
        Value::Record { ty, mut fields } => {
            if let Some(Value::List(children)) = fields.get("children").cloned() {
                let sorted = sort_connected_level(children, edges);
                fields.insert("children".to_string(), Value::List(sorted));
            }
            Value::Record { ty, fields }
        }
        Value::Variant {
            union,
            variant,
            payload: VariantPayload::Record(mut map),
        } => {
            if let Some(Value::List(children)) = map.get("children").cloned() {
                let sorted = sort_connected_level(children, edges);
                map.insert("children".to_string(), Value::List(sorted));
            }
            Value::Variant {
                union,
                variant,
                payload: VariantPayload::Record(map),
            }
        }
        other => other,
    }
}

fn collect_descendant_ids(item: &Value) -> HashSet<String> {
    let mut out = HashSet::new();
    if let Some(id) = item_id(item) {
        out.insert(id);
    }
    if let Some(children) = item_children(item) {
        for c in children {
            out.extend(collect_descendant_ids(c));
        }
    }
    out
}

fn item_id(v: &Value) -> Option<String> {
    let map = item_fields(v)?;
    extract_id_string(map.get("id")?)
}

fn item_children(v: &Value) -> Option<&[Value]> {
    let map = item_fields(v)?;
    match map.get("children")? {
        Value::List(xs) => Some(xs),
        _ => None,
    }
}

fn item_fields(v: &Value) -> Option<&BTreeMap<String, Value>> {
    match v {
        Value::Record { fields, .. } => Some(fields),
        Value::Variant {
            payload: VariantPayload::Record(map),
            ..
        } => Some(map),
        _ => None,
    }
}

fn edge_endpoint_pair(v: &Value) -> Option<(String, String)> {
    let fields = item_fields(v)?;
    let src = extract_id_string(fields.get("source")?)?;
    let dst = extract_id_string(fields.get("destination")?)?;
    Some((src, dst))
}

fn extract_id_string(v: &Value) -> Option<String> {
    match v {
        Value::Identifier(s) | Value::Utf8(s) | Value::Ascii(s) => Some(s.clone()),
        _ => None,
    }
}

fn find_subtree(descendants: &[HashSet<String>], target: &str) -> Option<usize> {
    descendants.iter().position(|s| s.contains(target))
}

// ── Higher-order ─────────────────────────────────────────────────────

/// Borrow a `&FnValue` from `value`, producing a uniform diagnostic
/// `"{builtin}: {what} must be a function, got {ty}"` on mismatch.
/// `what` is interpolated as-is, so prefer phrases like
/// `"second argument"` (the callsite reads naturally with or without
/// "the").
fn expect_function<'a>(
    builtin: &str,
    what: &str,
    value: &'a Value,
) -> Result<&'a crate::value::FnValue, String> {
    match value {
        Value::Function(fv) => Ok(fv),
        other => Err(format!(
            "{builtin}: {what} must be a function, got {}",
            other.type_name()
        )),
    }
}

fn map_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let f = expect_function("map", "second argument", &args[1])?.clone();
    match &args[0] {
        Value::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for elem in items {
                out.push(caller.call_fn(&f, std::slice::from_ref(elem))?);
            }
            Ok(Value::List(out))
        }
        Value::Tensor { shape, data } => {
            let shape = shape.clone();
            let mut out = Vec::with_capacity(data.len());
            for elem in data {
                out.push(caller.call_fn(&f, std::slice::from_ref(elem))?);
            }
            Ok(Value::Tensor { shape, data: out })
        }
        other => Err(format!(
            "map: first argument must be a list or tensor, got {}",
            other.type_name()
        )),
    }
}

fn filter_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let items = match &args[0] {
        Value::List(items) => items,
        Value::Tensor { .. } => {
            return Err(
                "filter: tensors are not supported (would invalidate shape); use map or convert to a list"
                    .into(),
            );
        }
        other => {
            return Err(format!(
                "filter: first argument must be a list, got {}",
                other.type_name()
            ));
        }
    };
    let f = expect_function("filter", "second argument", &args[1])?.clone();
    let mut out = Vec::new();
    for elem in items {
        match caller.call_fn(&f, std::slice::from_ref(elem))? {
            Value::Bool(true) => out.push(elem.clone()),
            Value::Bool(false) => {}
            other => {
                return Err(format!(
                    "filter: predicate must return bool, got {}",
                    other.type_name()
                ));
            }
        }
    }
    Ok(Value::List(out))
}

fn fold_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let items = match &args[0] {
        Value::List(items) => items.clone(),
        Value::Tensor { data, .. } => data.clone(),
        other => {
            return Err(format!(
                "fold: first argument must be a list or tensor, got {}",
                other.type_name()
            ));
        }
    };
    let f = expect_function("fold", "third argument", &args[2])?.clone();
    let mut acc = args[1].clone();
    for elem in items {
        acc = caller.call_fn(&f, &[acc, elem])?;
    }
    Ok(acc)
}

// ── Pure ─────────────────────────────────────────────────────────────

fn len_pure(v: Value) -> Result<i64, String> {
    match v {
        Value::List(items) => Ok(items.len() as i64),
        Value::Tensor { data, .. } => Ok(data.len() as i64),
        Value::Utf8(s) | Value::Ascii(s) => Ok(s.chars().count() as i64),
        other => Err(format!(
            "len: expected list, tensor, or string, got {}",
            other.type_name()
        )),
    }
}

fn range_pure(start: i64, end: i64) -> Result<Vec<i64>, String> {
    if end < start {
        return Err(format!(
            "range: end ({end}) must be greater than or equal to start ({start})"
        ));
    }
    Ok((start..end).collect())
}

fn head_pure(v: Value) -> Result<Value, String> {
    match v {
        Value::List(items) => Ok(items.into_iter().next().unwrap_or(Value::None)),
        Value::Tensor { data, .. } => Ok(data.into_iter().next().unwrap_or(Value::None)),
        other => Err(format!(
            "head: expected list or tensor, got {}",
            other.type_name()
        )),
    }
}

fn tail_pure(v: Value) -> Result<Vec<Value>, String> {
    match v {
        Value::List(items) => {
            if items.is_empty() {
                Ok(Vec::new())
            } else {
                Ok(items.into_iter().skip(1).collect())
            }
        }
        Value::Tensor { data, .. } => Ok(data.into_iter().skip(1).collect()),
        other => Err(format!(
            "tail: expected list or tensor, got {}",
            other.type_name()
        )),
    }
}

/// `sum(list)` — reduces a non-empty homogeneous numeric list. Picks the
/// variant of the first element; every subsequent element must match.
fn sum_pure(v: Value) -> Result<Value, String> {
    let items = match v {
        Value::List(items) => items,
        Value::Tensor { data, .. } => data,
        other => {
            return Err(format!(
                "sum: expected list or tensor, got {}",
                other.type_name()
            ));
        }
    };
    let first = items.first().ok_or("sum: empty list".to_string())?;

    macro_rules! sum_variant {
        ($t:ty, $variant:ident) => {
            if let Value::$variant(first_v) = first {
                let mut acc: $t = *first_v;
                for elem in items.iter().skip(1) {
                    match elem {
                        Value::$variant(n) => {
                            acc += *n;
                        }
                        other => {
                            return Err(format!(
                                "sum: mixed element types ({} and {})",
                                first.type_name(),
                                other.type_name()
                            ));
                        }
                    }
                }
                return Ok(Value::$variant(acc));
            }
        };
    }
    for_each_numeric_variant!(sum_variant);
    Err(format!(
        "sum: list elements must be numeric, got {}",
        first.type_name()
    ))
}

// ── Tensor primitives ────────────────────────────────────────────────

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

fn tensor_data_pure(v: Value) -> Result<Vec<Value>, String> {
    match v {
        Value::Tensor { data, .. } => Ok(data),
        other => Err(format!(
            "tensor_data: expected tensor, got {}",
            other.type_name()
        )),
    }
}

fn tensor_shape_pure(v: Value) -> Result<Vec<i64>, String> {
    match v {
        Value::Tensor { shape, .. } => Ok(shape.into_iter().map(|d| d as i64).collect()),
        other => Err(format!(
            "tensor_shape: expected tensor, got {}",
            other.type_name()
        )),
    }
}

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

fn flatten_pure(v: Value) -> Result<Vec<Value>, String> {
    let items = match v {
        Value::List(items) => items,
        other => {
            return Err(format!(
                "flatten: expected list of lists, got {}",
                other.type_name()
            ));
        }
    };
    let mut out: Vec<Value> = Vec::new();
    for inner in items {
        match inner {
            Value::List(xs) => out.extend(xs),
            other => {
                return Err(format!(
                    "flatten: outer list element must be a list, got {}",
                    other.type_name()
                ));
            }
        }
    }
    Ok(out)
}

fn zip_pure(a: Value, b: Value) -> Result<Vec<Value>, String> {
    let av = match a {
        Value::List(xs) => xs,
        other => {
            return Err(format!(
                "zip: first arg must be a list, got {}",
                other.type_name()
            ));
        }
    };
    let bv = match b {
        Value::List(xs) => xs,
        other => {
            return Err(format!(
                "zip: second arg must be a list, got {}",
                other.type_name()
            ));
        }
    };
    let n = av.len().min(bv.len());
    let mut out: Vec<Value> = Vec::with_capacity(n);
    for i in 0..n {
        out.push(Value::List(vec![av[i].clone(), bv[i].clone()]));
    }
    Ok(out)
}

/// `format(template, ...args)` — `{}` positional substitution.
fn format_hof(_caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let Some((template_v, rest)) = args.split_first() else {
        return Err("format: missing template argument".into());
    };
    let template = match template_v {
        Value::Utf8(s) | Value::Ascii(s) => s.clone(),
        other => {
            return Err(format!(
                "format: template must be a string, got {}",
                other.type_name()
            ));
        }
    };
    let mut out = String::with_capacity(template.len());
    let mut idx = 0usize;
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            if chars.peek() == Some(&'{') {
                chars.next();
                out.push('{');
                continue;
            }
            // Expect `}`.
            if chars.next() != Some('}') {
                return Err("format: expected '{}' placeholder".into());
            }
            let Some(arg) = rest.get(idx) else {
                return Err(format!(
                    "format: not enough arguments for placeholder #{idx}"
                ));
            };
            idx += 1;
            out.push_str(&format_value(arg));
        } else if c == '}' {
            if chars.peek() == Some(&'}') {
                chars.next();
                out.push('}');
                continue;
            }
            return Err("format: unmatched '}' in template".into());
        } else {
            out.push(c);
        }
    }
    if idx < rest.len() {
        return Err(format!(
            "format: {} extra arguments after template",
            rest.len() - idx
        ));
    }
    Ok(Value::Utf8(out))
}

/// Render a `Value` for inclusion in a `format` substitution. Stays
/// compact and predictable — host CLIs render richer forms.
pub(crate) fn format_value(v: &Value) -> String {
    match v {
        Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) => s.clone(),
        Value::Symbol(s) => format!(":{s}"),
        Value::Bool(b) => b.to_string(),
        Value::None => "none".to_string(),
        Value::I8(n) => n.to_string(),
        Value::I16(n) => n.to_string(),
        Value::I32(n) => n.to_string(),
        Value::I64(n) => n.to_string(),
        Value::I128(n) => n.to_string(),
        Value::Isize(n) => n.to_string(),
        Value::U8(n) => n.to_string(),
        Value::U16(n) => n.to_string(),
        Value::U32(n) => n.to_string(),
        Value::U64(n) => n.to_string(),
        Value::U128(n) => n.to_string(),
        Value::Usize(n) => n.to_string(),
        Value::F32(n) => n.to_string(),
        Value::F64(n) => n.to_string(),
        Value::Utf16(units) => String::from_utf16_lossy(units),
        Value::Utf32(chars) => chars.iter().collect(),
        Value::List(items) => {
            let parts: Vec<String> = items.iter().map(format_value).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Tensor { shape, data } => {
            let dims: Vec<String> = shape.iter().map(u64::to_string).collect();
            let elems: Vec<String> = data.iter().map(format_value).collect();
            format!("tensor[{}]({})", dims.join("x"), elems.join(", "))
        }
        Value::Variant {
            union,
            variant,
            payload,
        } => {
            use crate::value::VariantPayload;
            let path = format!("{}::{}", union.join("."), variant);
            match payload {
                VariantPayload::Unit => path,
                VariantPayload::Positional(v) => format!("{path}({})", format_value(v)),
                VariantPayload::Record(map) => {
                    let parts: Vec<String> = map
                        .iter()
                        .map(|(k, v)| format!("{k}: {}", format_value(v)))
                        .collect();
                    format!("{path} {{ {} }}", parts.join(", "))
                }
            }
        }
        Value::Function(_) => "<fn>".to_string(),
        Value::Record { ty, fields } => {
            let parts: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{k}: {}", format_value(v)))
                .collect();
            format!("{} {{ {} }}", ty.join("."), parts.join(", "))
        }
        Value::DataPath { kind, segments } => {
            format!("&{}<{kind}>", segments.join("."))
        }
    }
}
