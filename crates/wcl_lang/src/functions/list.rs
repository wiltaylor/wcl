//! List and sequence builtins.
//!
//! Everything that walks a collection: the higher-order forms (`map`,
//! `filter`, `fold`, `any`, `all`, `find`, the `*_by` family), the
//! shape operations (`head`, `tail`, `take`, `drop`, `slice`,
//! `reverse`, `flatten`, `zip`, `enumerate`) and the orderings
//! (`sort`, `sort_by`, `sort_connected`).
//!
//! Several of these accept a tensor as well as a list, and `len` and
//! `slice` also accept a string — the doc text on each registration
//! says which.

use std::collections::{BTreeMap, HashMap, HashSet};

use super::builtin::{BuiltinFn, Caller, from_fn};
use super::expect_function;
use crate::environment::Environment;
use crate::error::ArithmeticFault;
use crate::numeric::{for_each_float_numeric_variant, for_each_integer_numeric_variant};
use crate::value::{Value, VariantPayload};

/// Register every list and sequence builtin into `env`.
pub(super) fn register(env: &mut Environment) {
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
            Value::List(std::sync::Arc::new(xs))
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
            Value::List(std::sync::Arc::new(seen))
        })
        .doc("Remove duplicate elements from a list, keeping first-seen order.")
        .param("xs", "[T]", "The list to deduplicate.")
        .returns("[T]", "The list with duplicates removed."),
    );
    env.add_builtin(
        "index_of",
        from_fn(|xs: Value, x: Value| -> i64 {
            let empty: &[Value] = &[];
            let items: &[Value] = match &xs {
                Value::List(items) => items,
                Value::Tensor { data, .. } => data,
                _ => empty,
            };
            items
                .iter()
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
        from_fn(|xs: Value, i: i64| -> Result<Value, String> {
            if i < 0 {
                return Err(format!("at: index {i} is negative"));
            }
            let items = match &xs {
                Value::List(items) => items,
                Value::Tensor { data, .. } => data,
                other => return Err(format!("at: expected list, got {}", other.type_name())),
            };
            items
                .get(i as usize)
                .cloned()
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
            Value::list(xs.into_iter().take(n).collect())
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
            Value::list(xs.into_iter().skip(n).collect())
        })
        .doc("Every element of a list after the first `n`.")
        .param("xs", "[T]", "The list to drop from.")
        .param("n", "i64", "How many leading elements to skip.")
        .returns("[T]", "The elements after the first `n`."),
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
                    .map(|(i, v)| Value::list(vec![Value::I64(i as i64), v]))
                    .collect::<Vec<_>>()
                    .into(),
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

/// Borrow `v` as a list, or fail with a message naming `who` — the
/// argument check every list builtin starts with.
fn expect_list<'a>(who: &str, v: &'a Value) -> Result<&'a [Value], String> {
    match v {
        Value::List(items) => Ok(items),
        other => Err(format!(
            "{who}: first argument must be a list, got {}",
            other.type_name()
        )),
    }
}

/// `any(xs, f)` — true when `f` holds for at least one element.
/// Short-circuits on the first match.
fn any_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let f = expect_function("any", "second argument", &args[1])?.clone();
    for elem in expect_list("any", &args[0])? {
        if call_pred("any", caller, &f, elem)? {
            return Ok(Value::Bool(true));
        }
    }
    Ok(Value::Bool(false))
}

/// `all(xs, f)` — true when `f` holds for every element. Vacuously
/// true for an empty list, and short-circuits on the first failure.
fn all_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let f = expect_function("all", "second argument", &args[1])?.clone();
    for elem in expect_list("all", &args[0])? {
        if !call_pred("all", caller, &f, elem)? {
            return Ok(Value::Bool(false));
        }
    }
    Ok(Value::Bool(true))
}

/// `find(xs, f)` — the first element satisfying `f`, else `none`.
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
    /// Numeric keys, compared numerically.
    Num(f64),
    /// String keys, compared lexicographically.
    Str(String),
}

/// Reduce a value to something orderable, or fail naming `who`.
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

/// Order two keys. Mixing numeric and string keys is an error rather
/// than an arbitrary ordering.
fn compare_keys(a: &SortKey, b: &SortKey) -> Result<std::cmp::Ordering, String> {
    match (a, b) {
        (SortKey::Num(x), SortKey::Num(y)) => {
            Ok(x.partial_cmp(y).unwrap_or(std::cmp::Ordering::Equal))
        }
        (SortKey::Str(x), SortKey::Str(y)) => Ok(x.cmp(y)),
        _ => Err("keys must be all numeric or all strings, found mixed".to_string()),
    }
}

/// Apply the key function to every element once, pairing each element
/// with its key — so a sort calls the key function n times, not n log n.
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

/// `sort_by(xs, f)` — sort by the key `f` returns. The sort is stable.
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
        None => Ok(Value::list(keyed.into_iter().map(|(_, v)| v).collect())),
    }
}

/// Shared implementation of `min_by` and `max_by`, differing only in
/// which ordering wins.
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

/// `min_by(xs, f)` — the element with the smallest key, or `none` for
/// an empty list.
fn min_by_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    extreme_by("min_by", caller, args, std::cmp::Ordering::Less)
}

/// `max_by(xs, f)` — the element with the largest key, or `none` for
/// an empty list.
fn max_by_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    extreme_by("max_by", caller, args, std::cmp::Ordering::Greater)
}

/// `group_by(xs, f)` — bucket elements by the key `f` returns,
/// preserving each bucket's original order.
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
                fields.insert("items".to_string(), Value::List(std::sync::Arc::new(items)));
                Value::Record {
                    ty: Vec::new(),
                    fields: std::sync::Arc::new(fields),
                }
            })
            .collect::<Vec<_>>()
            .into(),
    ))
}

/// `slice(xs, start, end)` over a list or string. Indices are clamped
/// rather than erroring, and negative indices count from the end.
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
            Ok(Value::List(std::sync::Arc::new(items[lo..hi].to_vec())))
        }
        other => Err(format!(
            "slice: expected a string or list, got {}",
            other.type_name()
        )),
    }
}

/// `contains(xs, needle)` by value equality.
fn list_contains_pure(xs: Value, needle: Value) -> bool {
    match &xs {
        Value::List(items) => items.iter().any(|v| v == &needle),
        Value::Tensor { data, .. } => data.iter().any(|v| v == &needle),
        _ => false,
    }
}

/// `sort(xs)` using each element's own natural order.
fn sort_pure(xs: Vec<Value>) -> Result<Value, String> {
    // Numeric lists sort numerically; string lists sort lexicographically.
    // Mixed-type lists and unsupported types fail loudly rather than
    // silently producing a partial ordering.
    if xs.is_empty() {
        return Ok(Value::List(std::sync::Arc::new(xs)));
    }
    let first = &xs[0];
    if first.is_numeric() && xs.iter().all(|v| v.is_numeric()) {
        let mut keyed: Vec<(f64, Value)> = xs
            .iter()
            .map(|v| (v.as_f64().unwrap_or(f64::NAN), v.clone()))
            .collect();
        keyed.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        return Ok(Value::list(keyed.into_iter().map(|(_, v)| v).collect()));
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
        return Ok(Value::list(keyed.into_iter().map(|(_, v)| v).collect()));
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
    Ok(Value::list(sort_connected_level(
        std::sync::Arc::unwrap_or_clone(items),
        &edges,
    )))
}

/// Order one level of a tree so that connected siblings sit together,
/// following `edges`. The workhorse of the connection-aware sort.
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

/// Apply the connection-aware sort to every level beneath `item`.
fn recurse_sort_children(item: Value, edges: &[Value]) -> Value {
    match item {
        Value::Record { ty, fields } => {
            let mut fields = std::sync::Arc::unwrap_or_clone(fields);
            if let Some(Value::List(children)) = fields.get("children").cloned() {
                let sorted = sort_connected_level(std::sync::Arc::unwrap_or_clone(children), edges);
                fields.insert("children".to_string(), Value::list(sorted));
            }
            Value::record(ty, fields)
        }
        Value::Variant {
            union,
            variant,
            payload: VariantPayload::Record(map),
        } => {
            let mut map = std::sync::Arc::unwrap_or_clone(map);
            if let Some(Value::List(children)) = map.get("children").cloned() {
                let sorted = sort_connected_level(std::sync::Arc::unwrap_or_clone(children), edges);
                map.insert("children".to_string(), Value::list(sorted));
            }
            Value::Variant {
                union,
                variant,
                payload: VariantPayload::Record(std::sync::Arc::new(map)),
            }
        }
        other => other,
    }
}

/// Every id in `item`'s subtree, itself included — used to decide
/// which subtree an edge endpoint falls in.
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

/// The `id` field of a tree item, if it has one.
fn item_id(v: &Value) -> Option<String> {
    let map = item_fields(v)?;
    extract_id_string(map.get("id")?)
}

/// The `children` list of a tree item, if it has one.
fn item_children(v: &Value) -> Option<&[Value]> {
    let map = item_fields(v)?;
    match map.get("children")? {
        Value::List(xs) => Some(xs),
        _ => None,
    }
}

/// Borrow a value's record fields, or `None` if it is not a record.
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

/// The `(source, destination)` ids of an edge record.
fn edge_endpoint_pair(v: &Value) -> Option<(String, String)> {
    let fields = item_fields(v)?;
    let src = extract_id_string(fields.get("source")?)?;
    let dst = extract_id_string(fields.get("destination")?)?;
    Some((src, dst))
}

/// Read a value as an id string, accepting either a string or an
/// identifier.
fn extract_id_string(v: &Value) -> Option<String> {
    match v {
        Value::Identifier(s) | Value::Utf8(s) | Value::Ascii(s) => Some(s.clone()),
        _ => None,
    }
}

/// Index of the subtree containing `target`, or `None` when no
/// subtree does.
fn find_subtree(descendants: &[HashSet<String>], target: &str) -> Option<usize> {
    descendants.iter().position(|s| s.contains(target))
}

// ── Higher-order ─────────────────────────────────────────────────────

/// `map(xs, f)` — apply `f` to every element.
fn map_hof(caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let f = expect_function("map", "second argument", &args[1])?.clone();
    match &args[0] {
        Value::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for elem in items.iter() {
                out.push(caller.call_fn(&f, std::slice::from_ref(elem))?);
            }
            Ok(Value::list(out))
        }
        Value::Tensor { shape, data } => {
            let shape = shape.clone();
            let mut out = Vec::with_capacity(data.len());
            for elem in data.iter() {
                out.push(caller.call_fn(&f, std::slice::from_ref(elem))?);
            }
            Ok(Value::Tensor {
                shape,
                data: std::sync::Arc::new(out),
            })
        }
        other => Err(format!(
            "map: first argument must be a list or tensor, got {}",
            other.type_name()
        )),
    }
}

/// `filter(xs, f)` — keep the elements for which `f` holds.
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
    for elem in items.iter() {
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
    Ok(Value::List(std::sync::Arc::new(out)))
}

/// `fold(xs, init, f)` — left fold over the list.
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
    for elem in std::sync::Arc::unwrap_or_clone(items) {
        acc = caller.call_fn(&f, &[acc, elem])?;
    }
    Ok(acc)
}

// ── Pure ─────────────────────────────────────────────────────────────

/// `len(v)` for a list, string, record or tensor.
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

/// `range(start, end)` — the half-open integer range.
fn range_pure(start: i64, end: i64) -> Result<Vec<i64>, String> {
    if end < start {
        return Err(format!(
            "range: end ({end}) must be greater than or equal to start ({start})"
        ));
    }
    Ok((start..end).collect())
}

/// `head(xs)` — the first element, or `none` for an empty list.
fn head_pure(v: Value) -> Result<Value, String> {
    match v {
        Value::List(items) => Ok(items.first().cloned().unwrap_or(Value::None)),
        Value::Tensor { data, .. } => Ok(data.first().cloned().unwrap_or(Value::None)),
        other => Err(format!(
            "head: expected list or tensor, got {}",
            other.type_name()
        )),
    }
}

/// `tail(xs)` — every element but the first; empty for an empty list.
fn tail_pure(v: Value) -> Result<Vec<Value>, String> {
    match v {
        Value::List(items) => Ok(items.get(1..).map(<[Value]>::to_vec).unwrap_or_default()),
        Value::Tensor { data, .. } => Ok(data.get(1..).map(<[Value]>::to_vec).unwrap_or_default()),
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

    // The accumulator keeps the first element's own variant, so a narrow
    // type can run out of room part-way through the list. `$add` is what
    // each half of the numeric ladder does about that: integers report the
    // overflow, floats let IEEE saturate to `inf`.
    macro_rules! sum_variant_with {
        ($t:ty, $variant:ident, $add:expr) => {
            if let Value::$variant(first_v) = first {
                let mut acc: $t = *first_v;
                for elem in items.iter().skip(1) {
                    match elem {
                        Value::$variant(n) => {
                            acc = $add(acc, *n)?;
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
    macro_rules! sum_int {
        ($t:ty, $variant:ident) => {
            sum_variant_with!($t, $variant, |acc: $t, n: $t| acc
                .checked_add(n)
                .ok_or_else(|| {
                    // A builtin answers with a plain `String`, so this
                    // can't be an `EvalError` — but it renders through the
                    // same fault, so `sum([127i8, 1i8])` reads like the
                    // `127i8 + 1i8` it is.
                    format!("sum: cannot {}", ArithmeticFault::overflow(stringify!($t)))
                }));
        };
    }
    macro_rules! sum_float {
        ($t:ty, $variant:ident) => {
            sum_variant_with!($t, $variant, |acc: $t, n: $t| Ok::<$t, String>(acc + n));
        };
    }
    for_each_integer_numeric_variant!(sum_int);
    for_each_float_numeric_variant!(sum_float);
    Err(format!(
        "sum: list elements must be numeric, got {}",
        first.type_name()
    ))
}

// ── Tensor primitives ────────────────────────────────────────────────

/// `flatten(xs)` — concatenate one level of nested lists.
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
    for inner in std::sync::Arc::unwrap_or_clone(items) {
        match inner {
            Value::List(xs) => out.extend(std::sync::Arc::unwrap_or_clone(xs)),
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

/// `zip(a, b)` — pair elements positionally, stopping at the shorter
/// list.
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
        out.push(Value::List(std::sync::Arc::new(vec![
            av[i].clone(),
            bv[i].clone(),
        ])));
    }
    Ok(out)
}
