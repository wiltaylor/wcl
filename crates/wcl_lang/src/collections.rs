//! Collection builtins: map, filter, fold, len, sum, range, head, tail,
//! plus the tensor constructor/accessors. Registered in
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
