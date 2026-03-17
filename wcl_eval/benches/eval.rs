use criterion::{black_box, criterion_group, criterion_main, Criterion};
use wcl_core::ast::*;
use wcl_core::span::{FileId, Span};
use wcl_eval::evaluator::Evaluator;
use wcl_eval::scope::ScopeKind;

// ── Helpers ───────────────────────────────────────────────────────────────────

fn dummy_span() -> Span {
    Span::dummy()
}

fn int(n: i64) -> Expr {
    Expr::IntLit(n, dummy_span())
}

fn str_lit(s: &str) -> Expr {
    Expr::StringLit(StringLit {
        parts: vec![StringPart::Literal(s.to_string())],
        span: dummy_span(),
    })
}

fn binop(lhs: Expr, op: BinOp, rhs: Expr) -> Expr {
    Expr::BinaryOp(Box::new(lhs), op, Box::new(rhs), dummy_span())
}

/// Build `(((1 + 2) * 3) - 4) / ... ` with `depth` operations.
fn deep_arithmetic(depth: usize) -> Expr {
    let ops = [BinOp::Add, BinOp::Mul, BinOp::Sub, BinOp::Div];
    let mut expr = int(1);
    for i in 0..depth {
        let op = ops[i % ops.len()];
        // Avoid division by zero: use non-zero rhs
        let rhs_val = (i as i64 % 7) + 2;
        expr = binop(expr, op, int(rhs_val));
    }
    expr
}

/// Build a string-concatenation chain: "a" + "b" + "c" + ... (`n` parts).
fn string_concat_chain(n: usize) -> Expr {
    let mut expr = str_lit("hello");
    for i in 0..n {
        let piece = format!("-part{i}");
        expr = binop(expr, BinOp::Add, str_lit(&piece));
    }
    expr
}

/// Parse a WCL source string using wcl_core and return the Document.
fn parse(source: &str) -> Document {
    let (doc, _diags) = wcl_core::parse(source, FileId(0));
    doc
}

/// Build a WCL source with `n` blocks, each containing 3 simple attributes.
fn build_blocks_source(n: usize) -> String {
    let mut s = String::with_capacity(n * 80);
    for i in 0..n {
        s.push_str(&format!(
            "service svc{i} {{\n  port = {}\n  name = \"svc-{i}\"\n  enabled = true\n}}\n",
            8000 + i,
        ));
    }
    s
}

// ── Expression benchmarks ─────────────────────────────────────────────────────

fn bench_eval_arithmetic(c: &mut Criterion) {
    // Simple: a + b
    let expr = binop(int(123), BinOp::Add, int(456));
    c.bench_function("eval/expr arithmetic simple (a + b)", |b| {
        b.iter(|| {
            let mut ev = Evaluator::new();
            let scope = ev.scopes_mut().create_scope(ScopeKind::Module, None);
            ev.eval_expr(black_box(&expr), scope).unwrap()
        })
    });
}

fn bench_eval_arithmetic_deep(c: &mut Criterion) {
    let expr = deep_arithmetic(20);
    c.bench_function("eval/expr arithmetic deep (20 ops)", |b| {
        b.iter(|| {
            let mut ev = Evaluator::new();
            let scope = ev.scopes_mut().create_scope(ScopeKind::Module, None);
            ev.eval_expr(black_box(&expr), scope).unwrap()
        })
    });
}

fn bench_eval_string_concat(c: &mut Criterion) {
    let expr = string_concat_chain(20);
    c.bench_function("eval/expr string concat (20 parts)", |b| {
        b.iter(|| {
            let mut ev = Evaluator::new();
            let scope = ev.scopes_mut().create_scope(ScopeKind::Module, None);
            ev.eval_expr(black_box(&expr), scope).unwrap()
        })
    });
}

// ── Full-document benchmarks ──────────────────────────────────────────────────

fn bench_eval_100_blocks(c: &mut Criterion) {
    let source = build_blocks_source(100);
    let doc = parse(&source);
    c.bench_function("eval/document 100 blocks", |b| {
        b.iter(|| {
            let mut ev = Evaluator::new();
            ev.evaluate(black_box(&doc))
        })
    });
}

// ── Built-in function benchmarks ─────────────────────────────────────────────

/// Parse-then-evaluate a snippet that exercises a built-in function call.
fn bench_builtin_function(c: &mut Criterion) {
    // Use `len()` built-in on a list literal — widely supported and cheap.
    let source = r#"
items = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
count = len(items)
"#;
    let doc = parse(source);
    c.bench_function("eval/builtin len() on 10-element list", |b| {
        b.iter(|| {
            let mut ev = Evaluator::new();
            ev.evaluate(black_box(&doc))
        })
    });
}

fn bench_builtin_to_string(c: &mut Criterion) {
    let source = r#"
n = 42
s = to_string(n)
"#;
    let doc = parse(source);
    c.bench_function("eval/builtin to_string()", |b| {
        b.iter(|| {
            let mut ev = Evaluator::new();
            ev.evaluate(black_box(&doc))
        })
    });
}

criterion_group!(
    benches,
    bench_eval_arithmetic,
    bench_eval_arithmetic_deep,
    bench_eval_string_concat,
    bench_eval_100_blocks,
    bench_builtin_function,
    bench_builtin_to_string,
);
criterion_main!(benches);
