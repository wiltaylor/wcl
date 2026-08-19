//! Name-resolution / schema-validation scaling bench.
//!
//! Models the shape that made `wcl wdoc build` superlinear
//! (PERF-wdoc-build-scaling.md): a namespaced imported schema with many
//! declarations, plus many block instances whose schema lookups, type
//! references, and field evaluations all run the name resolver. Before
//! the `ref_registry` cache, every resolution rebuilt the full
//! declared-FQN set, so doubling both axes scaled worse than linearly;
//! with the cache the two sizes should scale roughly with (blocks ×
//! fields).

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

/// A `namespace lib` schema with `types` block kinds (each extending a
/// shared interface) and a `@document` declaring a child slot per kind.
fn schema(types: usize) -> String {
    let mut s = String::from("namespace lib\n\ninterface Common { id: identifier? }\n");
    for i in 0..types {
        s.push_str(&format!(
            "@block(\"kind{i}\")\ntype Kind{i} extends Common {{\n  @inline(0) name: utf8\n  weight: i64\n  tag: utf8?\n}}\n",
        ));
    }
    s.push_str("@document\ntype Model {\n");
    for i in 0..types {
        s.push_str(&format!("  @children(\"kind{i}\") k{i}: list<Kind{i}>\n"));
    }
    s.push_str("}\n");
    s
}

/// A root document importing the schema and instantiating `blocks`
/// blocks spread across the kinds.
fn root(types: usize, blocks: usize) -> String {
    let mut s = String::from("import <schema.wcl>\n");
    for b in 0..blocks {
        let k = b % types;
        s.push_str(&format!(
            "kind{k} \"item{b}\" {{\n  weight = {b}\n  tag = \"t{b}\"\n}}\n"
        ));
    }
    s
}

/// Build and open a synthetic document of the requested size.
fn open(types: usize, blocks: usize) -> wcl_lang::Document {
    let mut reg = wcl_lang::Registry::new();
    reg.register("schema.wcl", schema(types));
    let loader = reg.loader(wcl_lang::disk_loader());
    wcl_lang::Document::open_at_with_loader(
        &root(types, blocks),
        "bench.wcl",
        None,
        &wcl_lang::Environment::new(),
        loader,
    )
    .expect("open ok")
}

/// Validate the schema and force every block field — the wdoc-build
/// shape: lots of kind lookups + type-reference resolutions.
fn force(doc: &wcl_lang::Document) -> usize {
    let mut touched = doc.schema_errors().len();
    for block in doc.blocks() {
        for f in block.fields() {
            if f.value().is_ok() {
                touched += 1;
            }
        }
    }
    touched
}

/// Measure name resolution across several document sizes.
fn bench_resolve(c: &mut Criterion) {
    // Two sizes, 4x apart on both axes: superlinear resolution shows up
    // as a much-worse-than-16x ratio between them.
    for (types, blocks) in [(25, 100), (100, 400)] {
        c.bench_function(&format!("resolve_{types}_types_{blocks}_blocks"), |b| {
            b.iter(|| {
                let doc = open(black_box(types), black_box(blocks));
                black_box(force(&doc));
            })
        });
    }
}

criterion_group!(benches, bench_resolve);
criterion_main!(benches);
