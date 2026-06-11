//! Evaluator scaling bench: per-invocation resolution costs.
//!
//! Models the shape that made `wcl wdoc build` superlinear after the
//! let-memoisation fixes (PERF-wdoc-let-memoisation.md): a named user fn
//! invoked from inside a block-field `map(...)` closure, whose every call
//! resolves builtin callees and root lets through the scope chain, and
//! whose helpers re-reference document-scale projections (`@connections`,
//! union `@children`). Before the resolution/projection caches, each call
//! paid O(document) — doubling both axes scaled far worse than the data;
//! with the caches the two sizes should scale roughly with
//! (connections × blocks).

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

/// A `namespace lib` schema: `types` block kinds with per-kind field
/// names (so union variants dispatch unambiguously), a connection, a
/// two-variant union over the first two kinds, and a `@document` with a
/// child slot per kind plus union-children / connections projections.
fn schema(types: usize) -> String {
    let mut s = String::from("namespace lib\n\nsymbol_set RelKind { default uses }\n");
    for i in 0..types {
        s.push_str(&format!(
            "@block(\"kind{i}\")\ntype Kind{i} {{\n  @inline(0) name: utf8\n  f{i}: i64\n}}\n",
        ));
    }
    s.push_str("connection Rel: Kind0 -> Kind0 : RelKind\n");
    s.push_str("union Entry { E0 { f0: i64 } E1 { f1: i64 } }\n");
    s.push_str("@block(\"tbl\")\ntype Tbl {\n  @inline(0) name: utf8\n  out: list<i64>\n}\n");
    s.push_str("@document\ntype Model {\n");
    for i in 0..types {
        s.push_str(&format!("  @children(\"kind{i}\") k{i}: list<Kind{i}>\n"));
    }
    s.push_str("  @children(\"tbl\") tbls: list<Tbl>\n");
    s.push_str("  @children(Entry) entries: list<Entry>\n");
    s.push_str("  @connections(Rel) rels: list<Rel>\n");
    s.push_str("  probe: i64\n");
    s.push_str("}\n");
    s
}

/// A root document importing the schema: `blocks` blocks spread across
/// the kinds, `conns` connection statements between kind0 items, the
/// wad-shaped root lets, a field re-referencing the union projection,
/// and a block whose field calls the named fn from a `map` closure.
fn root(types: usize, blocks: usize, conns: usize) -> String {
    let mut s = String::from("import <schema.wcl>\n");
    for b in 0..blocks {
        let k = b % types;
        s.push_str(&format!("kind{k} \"item{b}\" {{\n  f{k} = {b}\n}}\n"));
    }
    // Connection operands must be kind0 blocks: items at multiples of
    // `types`. Cycle through them (duplicates are fine for cost).
    let kind0: Vec<usize> = (0..blocks).step_by(types).collect();
    for j in 0..conns {
        let a = kind0[j % kind0.len()];
        let b = kind0[(j + 1) % kind0.len()];
        s.push_str(&format!("item{a} -> item{b}\n"));
    }
    s.push_str(
        "let recs = flatten([map(rels, fn(r: Rel) -> utf8 r.source), map(rels, fn(r: Rel) -> utf8 r.destination)])\n\
         let dests = fn(id: utf8) -> list<utf8> map(filter(recs, fn(r: utf8) -> bool r == id), fn(r: utf8) -> utf8 r)\n\
         probe = len(map(recs, fn(r: utf8) -> i64 len(entries)))\n\
         tbl \"t0\" {\n  out = map(recs, fn(s: utf8) -> i64 len(dests(s)) + len(rels))\n}\n",
    );
    s
}

fn open(types: usize, blocks: usize, conns: usize) -> wcl_lang::Document {
    let mut reg = wcl_lang::Registry::new();
    reg.register("schema.wcl", schema(types));
    let loader = reg.loader(wcl_lang::disk_loader());
    wcl_lang::Document::open_at_with_loader(
        &root(types, blocks, conns),
        "bench.wcl",
        None,
        &wcl_lang::Environment::new(),
        loader,
    )
    .expect("open ok")
}

/// Force the two pathological fields: `tbl.out` (named fn invoked per
/// element from a block-field closure, re-referencing `rels`) and
/// `probe` (per-element re-reference of the union `entries` projection).
fn force(doc: &wcl_lang::Document) -> usize {
    let mut touched = 0;
    let tbl = doc.block("tbl").expect("tbl block");
    let out = tbl.field("out").expect("out field");
    if out.value().is_ok() {
        touched += 1;
    }
    if doc.field("probe").expect("probe field").value().is_ok() {
        touched += 1;
    }
    touched
}

fn bench_eval_scale(c: &mut Criterion) {
    // Two sizes, 2x apart on every axis: per-call O(document) resolution
    // shows up as a much-worse-than-4x ratio between them.
    for (types, blocks, conns) in [(30, 60, 40), (60, 120, 80)] {
        c.bench_function(
            &format!("eval_scale_{types}_types_{blocks}_blocks_{conns}_conns"),
            |b| {
                b.iter(|| {
                    let doc = open(black_box(types), black_box(blocks), black_box(conns));
                    black_box(force(&doc));
                })
            },
        );
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = bench_eval_scale
}
criterion_main!(benches);
