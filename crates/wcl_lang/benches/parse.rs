//! Benchmarks: parse throughput against synthetic documents.

use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

/// Generate a document source with `n` repeated blocks.
fn fixture(n: usize) -> String {
    let mut s = String::with_capacity(n * 128);
    for i in 0..n {
        s.push_str(&format!(
            "service \"svc{i}\" {{\n  port = {}\n  enabled = true\n  metadata {{\n    region = \"us-east-1\"\n    tier = \"prod\"\n  }}\n}}\n",
            8000 + i
        ));
    }
    s
}

/// Measure parse throughput across several document sizes.
fn bench_parse(c: &mut Criterion) {
    let src = fixture(100);
    c.bench_function("parse_100_blocks", |b| {
        b.iter(|| {
            let doc = wcl_lang::Document::open(black_box(&src), "bench").expect("open ok");
            black_box(doc);
        })
    });
}

/// Generate `n` fields each holding a two-slot interpolated string.
fn interpolation_fixture(n: usize) -> String {
    let mut s = String::from("x = 1\n");
    for i in 0..n {
        s.push_str(&format!("v{i} = $\"v ${{x}} and ${{x}}\"\n"));
    }
    s
}

/// Measure parsing a file dense with `${}` slots. Two sizes, 4x apart:
/// slot parsing that costs O(offset) per slot shows up as a
/// much-worse-than-4x ratio between them.
fn bench_parse_interpolations(c: &mut Criterion) {
    for n in [1_000, 4_000] {
        let src = interpolation_fixture(n);
        c.bench_function(&format!("parse_{n}_interpolated_fields"), |b| {
            b.iter(|| {
                let doc = wcl_lang::parse_for_edit(black_box(&src), "bench").expect("parse ok");
                black_box(doc);
            })
        });
    }
}

criterion_group!(benches, bench_parse, bench_parse_interpolations);
criterion_main!(benches);
