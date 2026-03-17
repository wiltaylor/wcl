use criterion::{black_box, criterion_group, criterion_main, Criterion};
use wcl::{parse, ParseOptions};

// ── Fixture strings ───────────────────────────────────────────────────────────

const SMALL: &str = r#"
name = "my-service"
port = 8080
debug = true
version = "1.0.0"
timeout = 30
"#;

const REALISTIC: &str = r#"
let base_port = 8000
let env = "production"

app {
  name = "platform"
  version = "3.1.0"
  environment = env
  debug = false
}

database primary {
  host = "db-primary.example.com"
  port = 5432
  name = "platform_prod"
  pool_min = 5
  pool_max = 20
  ssl = true
}

database replica {
  host = "db-replica.example.com"
  port = 5432
  name = "platform_prod"
  pool_min = 2
  pool_max = 10
  ssl = true
}

cache redis {
  host = "redis.example.com"
  port = 6379
  db = 0
  ttl = 3600
}

service api {
  port = base_port
  workers = 4
  timeout = 30
  max_body = 10485760
}

service worker {
  concurrency = 8
  timeout = 120
}

monitoring {
  enabled = true
  port = 9090
  scrape_interval = 15
  retention = "30d"
}

logging {
  level = "info"
  format = "json"
  output = "stdout"
}
"#;

/// Build a config with `n` service blocks.
fn build_many_blocks(n: usize) -> String {
    let mut s = String::with_capacity(n * 100);
    for i in 0..n {
        s.push_str(&format!(
            "service svc{i} {{\n  port = {}\n  name = \"service-{i}\"\n  enabled = true\n  timeout = {}\n}}\n",
            8000 + i,
            30 + (i % 90),
        ));
    }
    s
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// ParseOptions with imports disabled (no filesystem access in benchmarks).
fn bench_options() -> ParseOptions {
    ParseOptions {
        allow_imports: false,
        ..ParseOptions::default()
    }
}

// ── Benchmarks ────────────────────────────────────────────────────────────────

fn bench_e2e_small(c: &mut Criterion) {
    let opts = bench_options();
    c.bench_function("e2e/small (5 attributes)", |b| {
        b.iter(|| {
            parse(black_box(SMALL), opts.clone())
        })
    });
}

fn bench_e2e_realistic(c: &mut Criterion) {
    let opts = bench_options();
    c.bench_function("e2e/realistic config (let bindings + blocks)", |b| {
        b.iter(|| {
            parse(black_box(REALISTIC), opts.clone())
        })
    });
}

fn bench_e2e_many_blocks_50(c: &mut Criterion) {
    let source = build_many_blocks(50);
    let opts = bench_options();
    c.bench_function("e2e/50 blocks full pipeline", |b| {
        b.iter(|| {
            parse(black_box(source.as_str()), opts.clone())
        })
    });
}

fn bench_e2e_many_blocks_200(c: &mut Criterion) {
    let source = build_many_blocks(200);
    let opts = bench_options();
    c.bench_function("e2e/200 blocks full pipeline", |b| {
        b.iter(|| {
            parse(black_box(source.as_str()), opts.clone())
        })
    });
}

criterion_group!(
    benches,
    bench_e2e_small,
    bench_e2e_realistic,
    bench_e2e_many_blocks_50,
    bench_e2e_many_blocks_200,
);
criterion_main!(benches);
