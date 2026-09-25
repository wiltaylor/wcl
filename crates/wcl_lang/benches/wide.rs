//! Wide-body scaling bench: name lookup in bodies with many items.
//!
//! Models two shapes that were quadratic before the per-body name index
//! and the field-provenance index: a block whose every field references
//! a sibling (each reference scanned the whole body), and a `@document`
//! schema validating many top-level fields (each field searched the
//! whole document for its source, then the whole schema for its
//! declaration). Each case runs at two sizes 4x apart, so per-reference
//! O(body) work shows up as a much-worse-than-4x ratio between them.

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

/// One schemaless block of `n` fields, each referencing a sibling.
fn wide_block(n: usize) -> String {
    let mut s = String::from("@schemaless blk {\n  x = 1\n");
    for i in 0..n {
        s.push_str(&format!("  a{i} = x + 1\n"));
    }
    s.push_str("}\n");
    s
}

/// A `@document` schema declaring `n` fields, and those `n` fields.
fn wide_document(n: usize) -> String {
    let mut s = String::from("@document type Root {\n");
    for i in 0..n {
        s.push_str(&format!("  f{i}: i64\n"));
    }
    s.push_str("}\n");
    for i in 0..n {
        s.push_str(&format!("f{i} = {i}\n"));
    }
    s
}

/// Force every field of the wide block.
fn force_block(doc: &wcl_lang::Document) -> usize {
    let blk = doc.block("blk").expect("blk block");
    blk.fields().filter(|f| f.value().is_ok()).count()
}

/// Measure evaluating a wide block and validating a wide document.
fn bench_wide(c: &mut Criterion) {
    for n in [1_000, 4_000] {
        let src = wide_block(n);
        c.bench_function(&format!("wide_block_{n}_fields"), |b| {
            b.iter(|| {
                let doc = wcl_lang::Document::open(black_box(&src), "bench").expect("open ok");
                black_box(force_block(&doc));
            })
        });
    }
    for n in [1_000, 4_000] {
        let src = wide_document(n);
        c.bench_function(&format!("wide_document_{n}_fields"), |b| {
            b.iter(|| {
                let doc = wcl_lang::Document::open(black_box(&src), "bench").expect("open ok");
                black_box(doc.schema_errors().len());
            })
        });
    }
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = bench_wide
}
criterion_main!(benches);
