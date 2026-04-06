//! End-to-end integration tests for the `wcl` CLI.
//!
//! Every test writes real `.wcl` files to a temp directory and invokes the
//! binary through `assert_cmd`, verifying stdout/stderr/exit-code and, where
//! applicable, the on-disk file content after mutation commands.

use assert_cmd::Command;
use predicates::prelude::*;
use std::io::Write;
use tempfile::NamedTempFile;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a temp `.wcl` file with the given content.  The file stays alive
/// as long as the returned handle is in scope.
fn wcl_file(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::with_suffix(".wcl").expect("tempfile");
    f.write_all(content.as_bytes()).expect("write");
    f.flush().expect("flush");
    f
}

/// Run `wcl <args…>` and return the captured output.
fn wcl(args: &[&str]) -> assert_cmd::assert::Assert {
    Command::cargo_bin("wcl").unwrap().args(args).assert()
}

/// Parse the stdout of a successful `wcl eval --format json` invocation.
fn eval_json(content: &str) -> serde_json::Value {
    let f = wcl_file(content);
    let output = Command::cargo_bin("wcl")
        .unwrap()
        .args(["eval", "--format", "json", f.path().to_str().unwrap()])
        .output()
        .expect("run wcl eval");
    assert!(
        output.status.success(),
        "eval failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON")
}

/// Evaluate an expression against a document and parse stdout as JSON.
fn eval_expr_json(content: &str, expr: &str) -> serde_json::Value {
    let f = wcl_file(content);
    let output = Command::cargo_bin("wcl")
        .unwrap()
        .args(["eval", "--format", "json", f.path().to_str().unwrap(), expr])
        .output()
        .expect("run wcl eval");
    assert!(
        output.status.success(),
        "eval failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim()).expect("stdout should be valid JSON")
}

// ===========================================================================
// EVAL
// ===========================================================================

#[test]
fn eval_simple_attributes() {
    let json = eval_json("name = \"hello\"\nport = 8080\nenabled = true\n");
    assert_eq!(json["name"], "hello");
    assert_eq!(json["port"], 8080);
    assert_eq!(json["enabled"], true);
}

#[test]
fn eval_block_with_inline_id() {
    let json = eval_json(
        r#"
server web-prod {
    host = "0.0.0.0"
    port = 8080
}
"#,
    );
    assert_eq!(json["server"]["web-prod"]["host"], "0.0.0.0");
    assert_eq!(json["server"]["web-prod"]["port"], 8080);
}

#[test]
fn eval_block_with_label() {
    let json = eval_json(
        r#"
server primary {
    port = 443
}
"#,
    );
    assert_eq!(json["server"]["primary"]["port"], 443);
}

#[test]
fn eval_nested_blocks() {
    let json = eval_json(
        r#"
server web-prod {
    port = 8080
    logging {
        level = "info"
    }
}
"#,
    );
    assert_eq!(json["server"]["web-prod"]["logging"]["level"], "info");
}

#[test]
fn eval_let_bindings_not_in_output() {
    let json = eval_json("let x = 42\nresult = x + 1\n");
    assert!(json.get("x").is_none(), "let bindings should be erased");
    assert_eq!(json["result"], 43);
}

#[test]
fn eval_arithmetic_expressions() {
    let json = eval_json(
        r#"
let base = 10
sum = base + 5
product = base * 3
div = base / 2
modulo = base % 3
neg = -base
"#,
    );
    assert_eq!(json["sum"], 15);
    assert_eq!(json["product"], 30);
    assert_eq!(json["div"], 5);
    assert_eq!(json["modulo"], 1);
    assert_eq!(json["neg"], -10);
}

#[test]
fn eval_string_interpolation() {
    let json = eval_json(
        r#"
let name = "world"
greeting = "hello, ${name}!"
"#,
    );
    assert_eq!(json["greeting"], "hello, world!");
}

#[test]
fn eval_ternary_expression() {
    let json = eval_json(
        r#"
let flag = true
result = flag ? "yes" : "no"
"#,
    );
    assert_eq!(json["result"], "yes");
}

#[test]
fn eval_list_and_map() {
    let json = eval_json(
        r#"
tags = ["a", "b", "c"]
meta = { x = 1, y = 2 }
"#,
    );
    assert_eq!(json["tags"], serde_json::json!(["a", "b", "c"]));
    assert_eq!(json["meta"]["x"], 1);
    assert_eq!(json["meta"]["y"], 2);
}

#[test]
fn eval_builtin_functions() {
    let json = eval_json(
        r#"
u = upper("hello")
l = lower("WORLD")
n = len([1, 2, 3])
m = max(5, 10)
s = sum([1, 2, 3, 4])
"#,
    );
    assert_eq!(json["u"], "HELLO");
    assert_eq!(json["l"], "world");
    assert_eq!(json["n"], 3);
    assert_eq!(json["m"], 10);
    assert_eq!(json["s"], 10);
}

#[test]
fn eval_higher_order_functions() {
    let json = eval_json(
        r#"
doubled = map([1, 2, 3], (x) => x * 2)
evens = filter([1, 2, 3, 4, 5, 6], (x) => x % 2 == 0)
total = reduce([1, 2, 3, 4], 0, (acc, x) => acc + x)
"#,
    );
    assert_eq!(json["doubled"], serde_json::json!([2, 4, 6]));
    assert_eq!(json["evens"], serde_json::json!([2, 4, 6]));
    assert_eq!(json["total"], 10);
}

#[test]
fn eval_user_defined_function() {
    let json = eval_json(
        r#"
let double = (x) => x * 2
result = double(21)
"#,
    );
    assert_eq!(json["result"], 42);
}

#[test]
fn eval_comparison_and_logic() {
    let json = eval_json(
        r#"
a = 5 > 3
b = 5 < 3
c = true && false
d = true || false
e = !true
"#,
    );
    assert_eq!(json["a"], true);
    assert_eq!(json["b"], false);
    assert_eq!(json["c"], false);
    assert_eq!(json["d"], true);
    assert_eq!(json["e"], false);
}

#[test]
fn eval_string_functions() {
    let json = eval_json(
        r#"
trimmed = trim("  hello  ")
replaced = replace("foo bar foo", "foo", "baz")
joined = join(", ", ["a", "b", "c"])
sw = starts_with("hello world", "hello")
ew = ends_with("hello world", "world")
"#,
    );
    assert_eq!(json["trimmed"], "hello");
    assert_eq!(json["replaced"], "baz bar baz");
    assert_eq!(json["joined"], "a, b, c");
    assert_eq!(json["sw"], true);
    assert_eq!(json["ew"], true);
}

#[test]
fn eval_collection_functions() {
    let json = eval_json(
        r#"
sorted = sort([3, 1, 2])
reversed = reverse([1, 2, 3])
flat = flatten([[1, 2], [3, 4]])
merged = concat([1, 2], [3, 4])
unique = distinct([1, 2, 2, 3, 3, 3])
"#,
    );
    assert_eq!(json["sorted"], serde_json::json!([1, 2, 3]));
    assert_eq!(json["reversed"], serde_json::json!([3, 2, 1]));
    assert_eq!(json["flat"], serde_json::json!([1, 2, 3, 4]));
    assert_eq!(json["merged"], serde_json::json!([1, 2, 3, 4]));
    assert_eq!(json["unique"], serde_json::json!([1, 2, 3]));
}

#[test]
fn eval_for_loop_generates_attributes() {
    // For loops can generate top-level attributes from a list
    let json = eval_json(
        r#"
let items = [10, 20, 30]
total = sum(items)
count = len(items)
"#,
    );
    assert_eq!(json["total"], 60);
    assert_eq!(json["count"], 3);
}

#[test]
fn eval_for_loop_expands_blocks() {
    // Verify for-loop expansion creates blocks visible in the evaluated doc
    let json = eval_json(
        r#"
let items = ["a", "b", "c"]
for item in items {
    node ${item} {
        name = item
    }
}
"#,
    );
    assert!(json["node"].is_object());
    assert_eq!(json["node"].as_object().unwrap().len(), 3);
    assert_eq!(json["node"]["a"]["name"], "a");
}

#[test]
fn eval_if_else() {
    let json = eval_json(
        r#"
let debug = true
if debug {
    log_level = "debug"
}
"#,
    );
    assert_eq!(json["log_level"], "debug");
}

#[test]
fn eval_if_else_false_branch() {
    let json = eval_json(
        r#"
let debug = false
if debug {
    log_level = "debug"
} else {
    log_level = "info"
}
"#,
    );
    assert_eq!(json["log_level"], "info");
}

#[test]
fn eval_default_format_is_wcl() {
    let f = wcl_file("port = 8080\n");
    wcl(&["eval", f.path().to_str().unwrap()])
        .success()
        .stdout(predicate::str::contains("port = 8080"));
}

#[test]
fn eval_unsupported_format_fails() {
    let f = wcl_file("port = 8080\n");
    wcl(&["eval", "--format", "yaml", f.path().to_str().unwrap()])
        .failure()
        .stderr(predicate::str::contains("unsupported format"));
}

#[test]
fn eval_expression_returns_value() {
    let json = eval_expr_json("let services = [\"api\", \"web\"]\n", "services[0]");
    assert_eq!(json, "api");
}

#[test]
fn eval_expression_with_function_projection() {
    let json = eval_expr_json(
        r#"
let nums = [1, 2, 3, 4]
let summarize = (xs) => { total = sum(xs), count = len(xs) }
"#,
        "summarize(nums)",
    );
    assert_eq!(json["total"], 10);
    assert_eq!(json["count"], 4);
}

#[test]
fn eval_expression_wcl_format() {
    let f = wcl_file("let n = 42\n");
    wcl(&["eval", f.path().to_str().unwrap(), "n + 1"])
        .success()
        .stdout(predicate::str::contains("43"));
}

#[test]
fn eval_invalid_file_fails() {
    let f = wcl_file("server { port = \n");
    wcl(&["eval", f.path().to_str().unwrap()])
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn eval_nonexistent_file_fails() {
    wcl(&["eval", "/tmp/nonexistent_wcl_file_12345.wcl"])
        .failure()
        .stderr(predicate::str::contains("cannot read"));
}

// ===========================================================================
// SET
// ===========================================================================

#[test]
fn set_attribute_by_id_filter() {
    let f = wcl_file(
        r#"server svc-api {
    port = 8080
    host = "localhost"
}
"#,
    );
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["set", &path, "server | .id == \"svc-api\" ~> .port = 9090"])
        .success()
        .stdout(predicate::str::contains("set"));

    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(updated.contains("9090"));
    assert!(!updated.contains("8080"));
    assert!(updated.contains("localhost"));
}

#[test]
fn set_string_value_by_id() {
    let f = wcl_file(
        r#"server svc-api {
    host = "old.example.com"
}
"#,
    );
    let path = f.path().to_str().unwrap().to_string();

    wcl(&[
        "set",
        &path,
        "server | .id == \"svc-api\" ~> .host = \"new.example.com\"",
    ])
    .success();

    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(updated.contains("new.example.com"));
}

#[test]
fn set_creates_missing_attribute() {
    let f = wcl_file("server web { port = 8080 }\n");
    let path = f.path().to_str().unwrap().to_string();

    wcl(&[
        "set",
        &path,
        "server | .id == \"web\" ~> .timeout = \"30s\"",
    ])
    .success();

    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(updated.contains("timeout"));
    assert!(updated.contains("\"30s\""));
}

#[test]
fn set_no_match_fails() {
    let f = wcl_file("server web { port = 8080 }\n");
    wcl(&[
        "set",
        f.path().to_str().unwrap(),
        "database | .id == \"db\" ~> .port = 5432",
    ])
    .failure()
    .stderr(predicate::str::contains("matched no blocks"));
}

#[test]
fn set_multi_match_updates_all() {
    let f = wcl_file(
        r#"server a { port = 8080 env = "prod" }
server b { port = 9090 env = "prod" }
server c { port = 7070 env = "dev" }
"#,
    );
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["set", &path, "server | .env == \"prod\" ~> .replicas = 4"]).success();

    let updated = std::fs::read_to_string(&path).unwrap();
    let count = updated.matches("replicas = 4").count();
    assert_eq!(count, 2, "two prod servers should get replicas");
}

#[test]
fn set_preserves_other_content() {
    let f = wcl_file(
        r#"server svc-api {
    host = "localhost"
    port = 8080
    debug = true
}
"#,
    );
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["set", &path, "server | .id == \"svc-api\" ~> .port = 9090"]).success();

    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(updated.contains("localhost"));
    assert!(updated.contains("debug"));
    assert!(updated.contains("9090"));
}

#[test]
fn set_result_still_parses() {
    let f = wcl_file(
        r#"server svc-api {
    port = 8080
}
"#,
    );
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["set", &path, "server | .id == \"svc-api\" ~> .port = 9090"]).success();

    wcl(&["validate", &path]).success();
}

// ===========================================================================
// ADD
// ===========================================================================

#[test]
fn add_top_level_attribute() {
    let f = wcl_file("name = \"test\"\n");
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["add", &path, "region = \"us-east\""])
        .success()
        .stdout(predicate::str::contains("added"));

    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(updated.contains("region = \"us-east\""));
    assert!(updated.contains("name = \"test\""));
}

#[test]
fn add_top_level_block() {
    let f = wcl_file("name = \"test\"\n");
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["add", &path, "server web {\n    port = 8080\n}"]).success();

    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(updated.contains("server web"));
    assert!(updated.contains("port = 8080"));
}

#[test]
fn add_attribute_to_matched_block() {
    let f = wcl_file("server api {\n    port = 8080\n}\n");
    let path = f.path().to_str().unwrap().to_string();

    wcl(&[
        "add",
        &path,
        "server | .id == \"api\" ~> host = \"localhost\"",
    ])
    .success();

    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(updated.contains("host = \"localhost\""));
    assert!(updated.contains("port = 8080"));
}

#[test]
fn add_child_block_to_matched_block() {
    let f = wcl_file("server api {\n    port = 8080\n}\n");
    let path = f.path().to_str().unwrap().to_string();

    wcl(&[
        "add",
        &path,
        "server | .id == \"api\" ~> tls { enabled = true }",
    ])
    .success();

    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(updated.contains("tls {"));
    assert!(updated.contains("enabled = true"));
}

#[test]
fn add_invalid_fragment_fails() {
    let f = wcl_file("server api {\n    port = 8080\n}\n");
    wcl(&[
        "add",
        f.path().to_str().unwrap(),
        "this is not valid wcl @@@",
    ])
    .failure()
    .stderr(predicate::str::contains("invalid WCL fragment").or(predicate::str::contains("error")));
}

#[test]
fn add_result_still_parses() {
    let f = wcl_file("server api {\n    port = 8080\n}\n");
    let path = f.path().to_str().unwrap().to_string();

    wcl(&[
        "add",
        &path,
        "server | .id == \"api\" ~> host = \"localhost\"",
    ])
    .success();
    wcl(&["validate", &path]).success();
}

#[test]
fn add_to_invalid_file_fails() {
    let f = wcl_file("server { port = \n");
    wcl(&["add", f.path().to_str().unwrap(), "x = 1"])
        .failure()
        .stderr(predicate::str::contains("error"));
}

// ===========================================================================
// REMOVE
// ===========================================================================

#[test]
fn remove_block_by_id() {
    let f = wcl_file(
        r#"server svc-api {
    port = 8080
}

server svc-old {
    port = 3000
}
"#,
    );
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["remove", &path, "server | .id == \"svc-old\""])
        .success()
        .stdout(predicate::str::contains("removed"));

    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(!updated.contains("svc-old"));
    assert!(updated.contains("svc-api"));
    assert!(updated.contains("8080"));
}

#[test]
fn remove_attribute_from_block() {
    let f = wcl_file(
        r#"server svc-api {
    port = 8080
    debug = true
    host = "localhost"
}
"#,
    );
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["remove", &path, "server | .id == \"svc-api\" ~> .debug"])
        .success()
        .stdout(predicate::str::contains("removed"));

    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(!updated.contains("debug"));
    assert!(updated.contains("port"));
    assert!(updated.contains("host"));
}

#[test]
fn remove_multi_match_block() {
    let f = wcl_file(
        r#"server a { env = "dev" }
server b { env = "prod" }
server c { env = "dev" }
"#,
    );
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["remove", &path, "server | .env == \"dev\""]).success();

    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(!updated.contains("server a"));
    assert!(!updated.contains("server c"));
    assert!(updated.contains("server b"));
}

#[test]
fn remove_result_still_parses() {
    let f = wcl_file(
        r#"server svc-api {
    port = 8080
    debug = true
}
"#,
    );
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["remove", &path, "server | .id == \"svc-api\" ~> .debug"]).success();
    wcl(&["validate", &path]).success();
}

#[test]
fn remove_no_match_fails() {
    let f = wcl_file("server svc-api { port = 8080 }\n");
    wcl(&[
        "remove",
        f.path().to_str().unwrap(),
        "server | .id == \"missing\"",
    ])
    .failure()
    .stderr(predicate::str::contains("matched no blocks"));
}

// ===========================================================================
// SET + EVAL round-trip
// ===========================================================================

#[test]
fn set_then_eval_reflects_change() {
    let f = wcl_file(
        r#"server svc-api {
    port = 8080
    host = "localhost"
}
"#,
    );
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["set", &path, "server | .id == \"svc-api\" ~> .port = 9090"]).success();

    // Eval and verify
    let output = Command::cargo_bin("wcl")
        .unwrap()
        .args(["eval", "--format", "json", &path])
        .output()
        .expect("run eval");
    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap();
    assert_eq!(json["server"]["svc-api"]["port"], 9090);
    assert_eq!(json["server"]["svc-api"]["host"], "localhost");
}

// ===========================================================================
// ADD + EVAL round-trip
// ===========================================================================

#[test]
fn add_then_eval_shows_new_block() {
    let f = wcl_file("server svc-api {\n    port = 8080\n}\n");
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["add", &path, "server svc-new {\n    port = 7070\n}"]).success();

    // Eval and verify both blocks exist
    let output = Command::cargo_bin("wcl")
        .unwrap()
        .args(["eval", "--format", "json", &path])
        .output()
        .expect("run eval");
    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap();
    assert!(
        json["server"]["svc-api"].is_object(),
        "original block should exist"
    );
    assert!(
        json["server"]["svc-new"].is_object(),
        "new block should exist"
    );
}

// ===========================================================================
// REMOVE + EVAL round-trip
// ===========================================================================

#[test]
fn remove_then_eval_block_gone() {
    let f = wcl_file(
        r#"server svc-api {
    port = 8080
}
server svc-old {
    port = 3000
}
"#,
    );
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["remove", &path, "server | .id == \"svc-old\""]).success();

    let output = Command::cargo_bin("wcl")
        .unwrap()
        .args(["eval", "--format", "json", &path])
        .output()
        .expect("run eval");
    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap();
    assert!(
        json["server"]["svc-api"].is_object(),
        "kept block should exist"
    );
    assert!(
        json["server"]["svc-old"].is_null(),
        "removed block should be gone"
    );
}

#[test]
fn remove_attr_then_eval_attr_gone() {
    let f = wcl_file(
        r#"server svc-api {
    port = 8080
    debug = true
}
"#,
    );
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["remove", &path, "server | .id == \"svc-api\" ~> .debug"]).success();

    let output = Command::cargo_bin("wcl")
        .unwrap()
        .args(["eval", "--format", "json", &path])
        .output()
        .expect("run eval");
    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap();
    assert_eq!(json["server"]["svc-api"]["port"], 8080);
    assert!(
        json["server"]["svc-api"]["debug"].is_null(),
        "debug should be gone"
    );
}

// ===========================================================================
// VALIDATE
// ===========================================================================

#[test]
fn validate_with_schema_valid() {
    let f = wcl_file(
        r#"
schema "server" {
    port: i64
    host: string
}

server web {
    port = 8080
    host = "localhost"
}
"#,
    );
    wcl(&["validate", f.path().to_str().unwrap()]).success();
}

#[test]
fn validate_with_schema_type_mismatch() {
    let f = wcl_file(
        r#"
schema "server" {
    port: i64
}

server web {
    port = "not_a_number"
}
"#,
    );
    wcl(&["validate", f.path().to_str().unwrap()])
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn validate_with_schema_missing_required() {
    let f = wcl_file(
        r#"
schema "server" {
    port: i64
    host: string
}

server web {
    port = 8080
}
"#,
    );
    wcl(&["validate", f.path().to_str().unwrap()])
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn validate_schema_optional_field() {
    let f = wcl_file(
        r#"
schema "server" {
    port: i64
    host: string @optional
}

server web {
    port = 8080
}
"#,
    );
    wcl(&["validate", f.path().to_str().unwrap()]).success();
}

// ===========================================================================
// FMT
// ===========================================================================

#[test]
fn fmt_outputs_to_stdout() {
    let f = wcl_file("  x   =   1  \n");
    wcl(&["fmt", f.path().to_str().unwrap()])
        .success()
        .stdout(predicate::str::contains("x = 1"));
}

#[test]
fn fmt_write_modifies_file() {
    let f = wcl_file("  x   =   1  \n");
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["fmt", "--write", &path]).success();

    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(updated.contains("x = 1"), "file should be formatted");
}

#[test]
fn fmt_check_returns_success_when_formatted() {
    // Format first, then check
    let f = wcl_file("x = 1\n");
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["fmt", "--write", &path]).success();
    wcl(&["fmt", "--check", &path]).success();
}

// ===========================================================================
// Multi-step workflows
// ===========================================================================

#[test]
fn workflow_add_set_eval() {
    let f = wcl_file("server svc-api {\n    port = 8080\n}\n");
    let path = f.path().to_str().unwrap().to_string();

    // Add a new service with a port
    wcl(&["add", &path, "server svc-worker {\n    port = 3000\n}"]).success();

    // Validate the file
    wcl(&["validate", &path]).success();

    // Eval and check both blocks
    let output = Command::cargo_bin("wcl")
        .unwrap()
        .args(["eval", "--format", "json", &path])
        .output()
        .expect("run eval");
    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap();
    assert_eq!(json["server"]["svc-api"]["port"], 8080);
    assert_eq!(json["server"]["svc-worker"]["port"], 3000);
}

#[test]
fn workflow_set_validate_eval() {
    let f = wcl_file(
        r#"
schema "server" {
    port: i64
    host: string @optional
}

server svc-api {
    port = 8080
    host = "localhost"
}
"#,
    );
    let path = f.path().to_str().unwrap().to_string();

    // Update the port
    wcl(&["set", &path, "server | .id == \"svc-api\" ~> .port = 9090"]).success();

    // Should still validate
    wcl(&["validate", &path]).success();

    // Eval should reflect the new value
    let output = Command::cargo_bin("wcl")
        .unwrap()
        .args(["eval", "--format", "json", &path])
        .output()
        .expect("run eval");
    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap();
    assert_eq!(json["server"]["svc-api"]["port"], 9090);
}
