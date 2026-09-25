//! Resource limits on untrusted input: expression depth and the size of
//! what a builtin may build. Each case must end in a diagnostic, never
//! a stack overflow, an allocation abort or a hang.

use wcl_lang::Document;

/// The smallest stack the library runs on (LSP and test worker
/// threads). Every depth case runs here so a regression aborts the
/// test binary rather than passing on the 8 MiB main thread.
const SMALL_STACK: usize = 2 * 1024 * 1024;

fn on_small_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(f)
        .expect("spawn")
        .join()
        .expect("no panic");
}

/// Open `src`, force every top-level field, format it and drop it all.
/// Returns the parse error, if any.
fn exercise(src: &str) -> Option<String> {
    match Document::open(src, "limits") {
        Ok(doc) => {
            for field in doc.fields() {
                let _ = field.value();
            }
        }
        Err(e) => return Some(e.to_string()),
    }
    let ast = wcl_lang::parse_for_edit(src, "limits").expect("parses for edit");
    let printed = wcl_lang::format::to_source(&ast);
    wcl_lang::parse_for_edit(&printed, "limits").expect("printed output reparses");
    None
}

fn assert_too_deep(src: String) {
    on_small_stack(move || {
        let err = exercise(&src).expect("depth cap fires");
        assert!(err.contains("expression too deep"), "got: {err}");
    });
}

fn assert_ok(src: String) {
    on_small_stack(move || {
        if let Some(err) = exercise(&src) {
            panic!("expected a parse, got: {err}");
        }
    });
}

#[test]
fn long_operator_chain_errors_instead_of_overflowing() {
    assert_too_deep(format!("@schemaless x = 1{}\n", "+1".repeat(20_000)));
    assert_too_deep(format!("@schemaless x = 1{}\n", "+1".repeat(200_000)));
}

#[test]
fn long_member_chain_errors_instead_of_overflowing() {
    assert_too_deep(format!("@schemaless x = a{}\n", ".b".repeat(20_000)));
}

#[test]
fn long_call_chain_errors_instead_of_overflowing() {
    assert_too_deep(format!("@schemaless x = f{}\n", "()".repeat(20_000)));
}

#[test]
fn long_else_if_chain_errors_instead_of_overflowing() {
    let src = format!(
        "@schemaless x = if false {{ 0 }}{} else {{ 1 }}\n",
        " else if false { 0 }".repeat(20_000)
    );
    on_small_stack(move || {
        let err = exercise(&src).expect("a cap fires");
        assert!(
            err.contains("expression too deep") || err.contains("nesting too deep"),
            "got: {err}"
        );
    });
}

#[test]
fn nested_interpolation_errors_instead_of_overflowing() {
    // Each `${…}` slot used to start a fresh parser with its own
    // recursion count, so nesting was unbounded. Unoptimised parse
    // frames at the 128-level nesting cap outgrow 2 MiB, so this one
    // runs on a larger stack; the point is that the cap now fires.
    let n = 5_000;
    let src = format!(
        "@schemaless x = {}1{}\n",
        "$\"${".repeat(n),
        "}\"".repeat(n)
    );
    let err = std::thread::Builder::new()
        .stack_size(16 * 1024 * 1024)
        .spawn(move || wcl_lang::parse_for_edit(&src, "limits").map(|_| ()))
        .expect("spawn")
        .join()
        .expect("no panic")
        .expect_err("a cap fires");
    assert!(err.to_string().contains("too deep"), "got: {err}");
}

// The deepest trees the parser accepts, in each shape, must still
// evaluate, format and drop on a 2 MiB stack.
const AT_CAP: usize = 250;

#[test]
fn operator_chain_at_the_cap_runs_on_a_small_stack() {
    assert_ok(format!("@schemaless x = 1{}\n", "+1".repeat(AT_CAP)));
}

#[test]
fn member_chain_at_the_cap_runs_on_a_small_stack() {
    assert_ok(format!(
        "@schemaless r = {{ b: 1 }}\n@schemaless x = r{}\n",
        ".b".repeat(AT_CAP)
    ));
}

#[test]
fn call_chain_at_the_cap_runs_on_a_small_stack() {
    assert_ok(format!(
        "@schemaless f = fn() -> any f\n@schemaless x = f{}\n",
        "()".repeat(AT_CAP)
    ));
}

#[test]
fn else_if_chain_at_the_cap_runs_on_a_small_stack() {
    assert_ok(format!(
        "@schemaless x = if false {{ 0 }}{} else {{ 1 }}\n",
        // Each `else if` link is a level of parse recursion too, so
        // the chain stops at the nesting cap (128) before the depth cap.
        " else if false { 0 }".repeat(100)
    ));
}

#[test]
fn ordinary_long_expressions_still_parse() {
    // A long but realistic chain stays well inside the cap.
    assert_ok(format!("@schemaless x = 1{}\n", " + 1".repeat(200)));
    assert_ok(format!("@schemaless x = \"a\"{}\n", " + \"b\"".repeat(200)));
}
