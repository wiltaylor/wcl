use criterion::{black_box, criterion_group, criterion_main, Criterion};
use wcl_core::span::FileId;

// ── Fixture strings ───────────────────────────────────────────────────────────

const SMALL: &str = r#"
name = "my-service"
port = 8080
debug = true
version = "1.0.0"
timeout = 30
"#;

const MEDIUM: &str = r#"
name = "platform"
version = "2.0.0"
debug = false
environment = "production"
region = "us-east-1"
timeout = 60
max_connections = 100
retry_count = 3
log_level = "info"
enable_metrics = true
enable_tracing = true
enable_profiling = false
cache_ttl = 300
rate_limit = 1000
max_body_size = 10485760
tls_enabled = true
cors_enabled = true
compress_enabled = true
health_check_path = "/healthz"
base_url = "https://api.example.com"
db_host = "db.example.com"
db_port = 5432
db_name = "platform_db"
db_pool_min = 5
db_pool_max = 20
redis_host = "redis.example.com"
redis_port = 6379
redis_db = 0
redis_ttl = 3600
queue_url = "amqp://mq.example.com"
queue_prefetch = 10
storage_bucket = "platform-assets"
storage_region = "us-east-1"
cdn_url = "https://cdn.example.com"
auth_issuer = "https://auth.example.com"
auth_audience = "platform-api"
jwt_expiry = 3600
refresh_expiry = 86400
admin_email = "admin@example.com"
support_email = "support@example.com"
feature_new_ui = true
feature_beta_api = false
feature_dark_mode = true
feature_analytics = true
feature_chat = false
feature_export = true
feature_import = true
feature_sso = false
feature_2fa = true
feature_audit_log = true

service api {
  port = 8080
  workers = 4
  timeout = 30
}

service worker {
  concurrency = 8
  timeout = 120
  queue = "tasks"
}

service scheduler {
  interval = 60
  timezone = "UTC"
  enabled = true
}

database primary {
  host = "db-primary.example.com"
  port = 5432
  pool_size = 20
}

database replica {
  host = "db-replica.example.com"
  port = 5432
  pool_size = 10
}

cache local {
  max_size = 1000
  ttl = 60
}

cache distributed {
  host = "redis.example.com"
  port = 6379
  ttl = 3600
}

monitoring prometheus {
  port = 9090
  scrape_interval = 15
}

logging filebeat {
  host = "logs.example.com"
  port = 5044
  index = "platform-logs"
}
"#;

/// Deeply nested arithmetic expression: ((((1 + 2) * 3) - 4) / 5) ... repeated.
const DEEP_EXPR: &str = concat!(
    "result = ",
    "((((((((((1 + 2) * 3) - 4) / 5) + 6) * 7) - 8) / 9) + 10) * 11)",
);

/// Build a large config string with `n` blocks.
fn build_large(n: usize) -> String {
    let mut s = String::with_capacity(n * 120);
    for i in 0..n {
        s.push_str(&format!(
            "service svc{i} {{\n  port = {}\n  workers = {}\n  timeout = {}\n  name = \"service-{i}\"\n  enabled = true\n}}\n",
            8000 + i,
            (i % 8) + 1,
            30 + (i % 90),
        ));
    }
    s
}

// ── Benchmarks ────────────────────────────────────────────────────────────────

fn bench_parse_small(c: &mut Criterion) {
    c.bench_function("parse/small (5 attributes)", |b| {
        b.iter(|| wcl_core::parse(black_box(SMALL), FileId(0)))
    });
}

fn bench_parse_medium(c: &mut Criterion) {
    c.bench_function("parse/medium (50 attributes + 10 blocks)", |b| {
        b.iter(|| wcl_core::parse(black_box(MEDIUM), FileId(0)))
    });
}

fn bench_parse_large(c: &mut Criterion) {
    let large = build_large(500);
    c.bench_function("parse/large (500 blocks × 5 attributes)", |b| {
        b.iter(|| wcl_core::parse(black_box(large.as_str()), FileId(0)))
    });
}

fn bench_parse_deep_expr(c: &mut Criterion) {
    c.bench_function("parse/deep nested expression", |b| {
        b.iter(|| wcl_core::parse(black_box(DEEP_EXPR), FileId(0)))
    });
}

criterion_group!(
    benches,
    bench_parse_small,
    bench_parse_medium,
    bench_parse_large,
    bench_parse_deep_expr
);
criterion_main!(benches);
