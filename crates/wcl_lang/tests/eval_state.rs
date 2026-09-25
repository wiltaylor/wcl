//! Evaluation state that outlives one expression: the depth cap that
//! keeps a long reference chain off the end of the Rust stack, the
//! per-thread cycle tracking that lets many threads read one `Document`,
//! and the connection-operand re-entry that must not poison a cached
//! value.

use std::sync::Barrier;

use wcl_lang::{Document, EvalError, Value};

/// Stack for the small-stack threads below: 2 MiB — Rust's default for a
/// spawned thread and the LSP's worker threads — in an optimised build.
/// Unoptimised frames are several times larger, so a debug test run gets
/// proportionally more; `cargo test --release` checks the real figure.
const SMALL_STACK: usize = if cfg!(debug_assertions) {
    32 << 20
} else {
    2 << 20
};

/// Run `f` on a fresh thread with [`SMALL_STACK`] bytes of stack.
fn on_small_stack<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> T {
    std::thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(f)
        .expect("spawn")
        .join()
        .expect("evaluation thread panicked")
}

/// A `@schemaless` block `cfg` holding `a0 = a1 + 1`, …, `a{n-1} = a{n} + 1`,
/// `a{n} = 0` — a chain `n` references long that never loops back.
fn chain_source(n: usize) -> String {
    let mut src = String::from("@schemaless cfg {\n");
    for i in 0..n {
        src.push_str(&format!("  a{i} = a{} + 1\n", i + 1));
    }
    src.push_str(&format!("  a{n} = 0\n}}\n"));
    src
}

fn read(doc: &Document, path: &str) -> Result<Value, EvalError> {
    doc.get(path).expect("path resolves").value()
}

#[test]
fn long_reference_chain_reports_depth_limit_instead_of_overflowing() {
    let result = on_small_stack(|| {
        let doc = Document::open(&chain_source(5000), "chain").expect("opens");
        read(&doc, "cfg.a0")
    });
    assert!(
        matches!(result, Err(EvalError::EvalDepthExceeded { max: 200, .. })),
        "{result:?}"
    );
}

#[test]
fn reference_chain_within_the_limit_evaluates_on_a_small_stack() {
    let result = on_small_stack(|| {
        let doc = Document::open(&chain_source(150), "chain").expect("opens");
        read(&doc, "cfg.a0")
    });
    assert_eq!(result, Ok(Value::I64(150)));
}

#[test]
fn deep_fn_recursion_reports_call_depth_on_a_small_stack() {
    let src = "fn down(n: i64) -> i64 if n == 0 { 0 } else { down(n - 1) }\n\
               @schemaless shallow = down(150)\n\
               @schemaless deep = down(100000)\n";
    let (shallow, deep) = on_small_stack(move || {
        let doc = Document::open(src, "rec").expect("opens");
        (read(&doc, "shallow"), read(&doc, "deep"))
    });
    assert_eq!(shallow, Ok(Value::I64(0)));
    assert!(
        matches!(deep, Err(EvalError::CallDepthExceeded { max: 200, .. })),
        "{deep:?}"
    );
}

#[test]
fn concurrent_reads_never_report_a_false_cycle() {
    // Fields, lets and an `a = a`-style outward shadow, read from many
    // threads at once. A shared "being evaluated" flag let a thread that
    // arrived mid-evaluation read another thread's work as a cycle and
    // cache `EvalError::Cycle` for good.
    let mut src = String::from("@schemaless outer = 7\n@schemaless cfg {\n");
    src.push_str("  let base = 3\n");
    src.push_str("  outer = outer + base\n");
    for i in 0..40 {
        src.push_str(&format!("  let l{i} = f{} * 2 + base\n", i + 1));
        src.push_str(&format!("  f{i} = l{i} - f{} + outer\n", i + 1));
    }
    src.push_str("  f40 = 1\n}\n");
    let paths: Vec<String> = (0..=40)
        .map(|i| format!("cfg.f{i}"))
        .chain(["cfg.outer".to_string()])
        .collect();
    let expected: Vec<Value> = {
        let (src, paths) = (src.clone(), paths.clone());
        on_small_stack(move || {
            let doc = Document::open(&src, "t").expect("opens");
            paths
                .iter()
                .map(|p| read(&doc, p).expect("evaluates"))
                .collect()
        })
    };
    const THREADS: usize = 8;
    for round in 0..200 {
        let doc = Document::open(&src, "t").expect("opens");
        let barrier = Barrier::new(THREADS);
        std::thread::scope(|s| {
            for t in 0..THREADS {
                let (doc, barrier, paths, expected) = (&doc, &barrier, &paths, &expected);
                let worker = std::thread::Builder::new().stack_size(SMALL_STACK);
                worker
                    .spawn_scoped(s, move || {
                        barrier.wait();
                        // Each thread starts at a different field so the
                        // threads meet mid-chain.
                        for k in 0..paths.len() {
                            let i = (k + t * 5) % paths.len();
                            assert_eq!(
                                read(doc, &paths[i]).as_ref(),
                                Ok(&expected[i]),
                                "round {round}, thread {t}, {}",
                                paths[i]
                            );
                        }
                    })
                    .expect("spawn");
            }
        });
    }
}

#[test]
fn genuine_cycle_is_still_reported() {
    let doc =
        Document::open("@schemaless cfg {\n  a = b\n  b = c\n  c = a\n}\n", "t").expect("opens");
    for p in ["cfg.a", "cfg.b", "cfg.c"] {
        assert!(
            matches!(read(&doc, p), Err(EvalError::Cycle { .. })),
            "{p}: {:?}",
            read(&doc, p)
        );
    }
}

/// A block whose identifying label reads `count`, which reads the
/// `@connections` projection, which identifies blocks by their labels.
const LABEL_READS_CONNECTIONS: &str = r#"
    @block("system") type System { @inline(0) id: utf8 }
    @block("user")   type User   { @inline(0) id: utf8 }
    symbol_set RelKind { uses }
    connection PersonToSystem: User -> System : RelKind
    @document type Model {
        @children("system")          systems: list<System>
        @children("user")            users:   list<User>
        @connections(PersonToSystem) person_to_system: list<PersonToSystem>
        count: i64
        edges: i64
    }
    system "web"                  {}
    system $"sys-${count}"        {}
    user   "customer"             {}
    customer -> web :uses
    count = len(person_to_system)
    edges = len(person_to_system)
"#;

#[test]
fn value_read_during_operand_resolution_matches_a_later_read() {
    // `count` is forced while a connection operand's label is being
    // evaluated in one document, and read directly first in the other.
    // Both must agree: a value computed inside operand resolution is
    // cached, so it can't be computed under different rules there.
    let projection_first = Document::open(LABEL_READS_CONNECTIONS, "t").expect("opens");
    let edges_a = read(&projection_first, "edges");
    let count_a = read(&projection_first, "count");

    let count_first = Document::open(LABEL_READS_CONNECTIONS, "t").expect("opens");
    let count_b = read(&count_first, "count");
    let edges_b = read(&count_first, "edges");

    assert_eq!(edges_a, Ok(Value::I64(1)));
    assert_eq!(edges_b, Ok(Value::I64(1)));
    // `count` feeds a label the projection has to evaluate: a genuine
    // cycle, reported as one whichever is read first. (Which binding the
    // diagnostic names depends on where the loop was entered, as it does
    // for any field cycle.) A thread-wide "resolving an operand" flag
    // used to hide the projection from `count` instead, caching an
    // `unresolved reference` when the projection was read first.
    for count in [&count_a, &count_b] {
        assert!(matches!(count, Err(EvalError::Cycle { .. })), "{count:?}");
    }
}
