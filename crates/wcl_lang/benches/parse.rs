use criterion::{Criterion, criterion_group, criterion_main};
use std::hint::black_box;

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

fn bench_parse(c: &mut Criterion) {
    let src = fixture(100);
    c.bench_function("parse_100_blocks", |b| {
        b.iter(|| {
            let doc = wcl_lang::Document::open(black_box(&src), "bench").expect("open ok");
            black_box(doc);
        })
    });
}

criterion_group!(benches, bench_parse);
criterion_main!(benches);
