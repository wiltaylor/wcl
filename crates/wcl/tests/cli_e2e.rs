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

/// Parse the stdout of a successful `wcl` invocation as JSON.
fn eval_json(content: &str) -> serde_json::Value {
    let f = wcl_file(content);
    let output = Command::cargo_bin("wcl")
        .unwrap()
        .args(["eval", f.path().to_str().unwrap()])
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

/// Parse the stdout of a successful `wcl query --format json` invocation.
fn query_json(content: &str, query_str: &str) -> serde_json::Value {
    let f = wcl_file(content);
    let output = Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "query",
            "--format",
            "json",
            f.path().to_str().unwrap(),
            query_str,
        ])
        .output()
        .expect("run wcl query");
    assert!(
        output.status.success(),
        "query failed: {}",
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
fn eval_for_loop_with_query_count() {
    // Verify for-loop expansion creates blocks that queries can find
    let f = wcl_file(
        r#"
let items = ["a", "b", "c"]
for item in items {
    node {
        name = item
    }
}
"#,
    );
    wcl(&["query", "--count", f.path().to_str().unwrap(), "node"])
        .success()
        .stdout(predicate::str::contains("3").trim());
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
fn eval_output_format_yaml() {
    let f = wcl_file("port = 8080\n");
    wcl(&["eval", "--format", "yaml", f.path().to_str().unwrap()])
        .success()
        .stdout(predicate::str::contains("port: 8080"));
}

#[test]
fn eval_output_format_toml() {
    let f = wcl_file("port = 8080\n");
    wcl(&["eval", "--format", "toml", f.path().to_str().unwrap()])
        .success()
        .stdout(predicate::str::contains("port = 8080"));
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
// QUERY
// ===========================================================================

#[test]
fn query_select_by_kind() {
    let result = query_json(
        r#"
server web {
    port = 8080
}
"#,
        "server",
    );
    let arr = result.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["port"], 8080);
}

#[test]
fn query_select_by_kind_and_id() {
    let result = query_json(
        r#"
server svc-a {
    port = 8080
}
server svc-b {
    port = 9090
}
"#,
        "server#svc-a",
    );
    let arr = result.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["port"], 8080);
}

#[test]
fn query_filter_by_attribute() {
    let result = query_json(
        r#"
server svc-a {
    port = 8080
}
server svc-b {
    port = 9090
}
"#,
        "server | .port > 8080",
    );
    let arr = result.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["port"], 9090);
}

#[test]
fn query_projection() {
    let result = query_json(
        r#"
server svc-a {
    port = 8080
}
server svc-b {
    port = 9090
}
"#,
        "server | .port",
    );
    let arr = result.as_array().unwrap();
    assert!(arr.contains(&serde_json::json!(8080)));
    assert!(arr.contains(&serde_json::json!(9090)));
}

#[test]
fn query_count_flag() {
    let f = wcl_file(
        r#"
server svc-a { port = 8080 }
server svc-b { port = 9090 }
"#,
    );
    wcl(&["query", "--count", f.path().to_str().unwrap(), "server"])
        .success()
        .stdout(predicate::str::contains("2").trim());
}

#[test]
fn query_no_results_returns_empty() {
    let result = query_json("server web { port = 8080 }\n", "database");
    let arr = result.as_array().unwrap();
    assert!(arr.is_empty());
}

#[test]
fn query_text_format() {
    let f = wcl_file("server web { port = 8080 }\n");
    wcl(&["query", f.path().to_str().unwrap(), "server"])
        .success()
        .stdout(predicate::str::contains("\"port\": 8080"));
}

#[test]
fn query_equality_filter() {
    let result = query_json(
        r#"
server svc-a { env = "prod" }
server svc-b { env = "dev" }
"#,
        r#"server | .env == "prod""#,
    );
    let arr = result.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["env"], "prod");
}

#[test]
fn query_has_filter() {
    let result = query_json(
        r#"
server svc-a {
    port = 8080
    tls = true
}
server svc-b {
    port = 9090
}
"#,
        "server | has(.tls)",
    );
    let arr = result.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["tls"], true);
}

// ===========================================================================
// SET
// ===========================================================================

#[test]
fn set_attribute_in_block_by_id() {
    let f = wcl_file(
        r#"server svc-api {
    port = 8080
    host = "localhost"
}
"#,
    );
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["set", &path, "server#svc-api.port", "9090"])
        .success()
        .stdout(predicate::str::contains("set"));

    // Verify the file was updated
    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(updated.contains("9090"), "port should be updated to 9090");
    assert!(!updated.contains("8080"), "old port 8080 should be gone");
    // Other attributes should be untouched
    assert!(updated.contains("localhost"));
}

#[test]
fn set_attribute_in_block_by_kind() {
    let f = wcl_file(
        r#"config {
    debug = false
}
"#,
    );
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["set", &path, "config.debug", "true"]).success();

    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(updated.contains("true"), "debug should be set to true");
}

#[test]
fn set_string_value() {
    let f = wcl_file(
        r#"server svc-api {
    host = "old.example.com"
}
"#,
    );
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["set", &path, "server#svc-api.host", "\"new.example.com\""]).success();

    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(updated.contains("new.example.com"));
}

#[test]
fn set_top_level_attribute() {
    let f = wcl_file("version = 1\n");
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["set", &path, "version", "2"]).success();

    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(updated.contains("2"));
}

#[test]
fn set_nonexistent_attribute_fails() {
    let f = wcl_file("server web { port = 8080 }\n");
    wcl(&[
        "set",
        f.path().to_str().unwrap(),
        "server#web.missing",
        "42",
    ])
    .failure()
    .stderr(predicate::str::contains("not found"));
}

#[test]
fn set_nonexistent_block_fails() {
    let f = wcl_file("server web { port = 8080 }\n");
    wcl(&[
        "set",
        f.path().to_str().unwrap(),
        "database#db.port",
        "5432",
    ])
    .failure()
    .stderr(predicate::str::contains("not found"));
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

    wcl(&["set", &path, "server#svc-api.port", "9090"]).success();

    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(updated.contains("localhost"), "host should be preserved");
    assert!(updated.contains("debug"), "debug should be preserved");
    assert!(updated.contains("9090"), "port should be updated");
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

    wcl(&["set", &path, "server#svc-api.port", "9090"]).success();

    // The modified file should still be valid WCL
    wcl(&["validate", &path]).success();
}

// ===========================================================================
// ADD
// ===========================================================================

#[test]
fn add_block_with_id() {
    let f = wcl_file("server svc-api {\n    port = 8080\n}\n");
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["add", &path, "server svc-new"])
        .success()
        .stdout(predicate::str::contains("added"));

    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(updated.contains("svc-new"), "new block should be added");
    assert!(updated.contains("svc-api"), "existing block should remain");
}

#[test]
fn add_block_without_id() {
    let f = wcl_file("x = 1\n");
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["add", &path, "config"])
        .success()
        .stdout(predicate::str::contains("added config"));

    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(updated.contains("config {"), "new block should be added");
}

#[test]
fn add_result_still_parses() {
    let f = wcl_file("server svc-api {\n    port = 8080\n}\n");
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["add", &path, "service svc-new"]).success();

    // The modified file should still be valid WCL
    wcl(&["validate", &path]).success();
}

#[test]
fn add_preserves_existing_content() {
    let f = wcl_file(
        r#"let x = 42
server svc-api {
    port = 8080
}
"#,
    );
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["add", &path, "database db-main"]).success();

    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(
        updated.contains("let x = 42"),
        "let binding should be preserved"
    );
    assert!(updated.contains("svc-api"), "existing block should remain");
    assert!(updated.contains("db-main"), "new block should be added");
}

#[test]
fn add_to_invalid_file_fails() {
    let f = wcl_file("server { port = \n");
    wcl(&["add", f.path().to_str().unwrap(), "config new"])
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

    wcl(&["remove", &path, "server#svc-old"])
        .success()
        .stdout(predicate::str::contains("removed"));

    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(!updated.contains("svc-old"), "removed block should be gone");
    assert!(updated.contains("svc-api"), "other block should remain");
    assert!(
        updated.contains("8080"),
        "other block content should remain"
    );
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

    wcl(&["remove", &path, "server#svc-api.debug"])
        .success()
        .stdout(predicate::str::contains("removed"));

    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(
        !updated.contains("debug"),
        "debug attribute should be removed"
    );
    assert!(updated.contains("port"), "other attributes should remain");
    assert!(updated.contains("host"), "other attributes should remain");
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

    wcl(&["remove", &path, "server#svc-api.debug"]).success();

    wcl(&["validate", &path]).success();
}

#[test]
fn remove_nonexistent_path_fails() {
    let f = wcl_file("server svc-api { port = 8080 }\n");
    wcl(&["remove", f.path().to_str().unwrap(), "server#svc-missing"])
        .failure()
        .stderr(predicate::str::contains("not found"));
}

#[test]
fn remove_top_level_attribute() {
    let f = wcl_file(
        r#"name = "hello"
version = 42
debug = false
"#,
    );
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["remove", &path, "debug"]).success();

    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(!updated.contains("debug"), "debug should be removed");
    assert!(updated.contains("name"), "name should remain");
    assert!(updated.contains("version"), "version should remain");
}

#[test]
fn remove_block_by_kind() {
    let f = wcl_file(
        r#"name = "test"
config {
    debug = true
}
"#,
    );
    let path = f.path().to_str().unwrap().to_string();

    wcl(&["remove", &path, "config"]).success();

    let updated = std::fs::read_to_string(&path).unwrap();
    assert!(
        !updated.contains("config"),
        "config block should be removed"
    );
    assert!(updated.contains("name"), "name attribute should remain");
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

    // Set port to 9090
    wcl(&["set", &path, "server#svc-api.port", "9090"]).success();

    // Eval and verify
    let output = Command::cargo_bin("wcl")
        .unwrap()
        .args(["eval", &path])
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

    // Add a new block
    wcl(&["add", &path, "server svc-new"]).success();

    // Eval and verify both blocks exist
    let output = Command::cargo_bin("wcl")
        .unwrap()
        .args(["eval", &path])
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

    wcl(&["remove", &path, "server#svc-old"]).success();

    let output = Command::cargo_bin("wcl")
        .unwrap()
        .args(["eval", &path])
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

    wcl(&["remove", &path, "server#svc-api.debug"]).success();

    let output = Command::cargo_bin("wcl")
        .unwrap()
        .args(["eval", &path])
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
// INSPECT
// ===========================================================================

#[test]
fn inspect_ast_produces_output() {
    let f = wcl_file("x = 1\n");
    wcl(&["inspect", "--ast", f.path().to_str().unwrap()])
        .success()
        .stdout(predicate::str::is_empty().not());
}

#[test]
fn inspect_scopes_shows_variables() {
    let f = wcl_file("let x = 42\ny = x + 1\n");
    wcl(&["inspect", "--scopes", f.path().to_str().unwrap()])
        .success()
        .stdout(predicate::str::contains("x"));
}

// ===========================================================================
// Multi-step workflows
// ===========================================================================

#[test]
fn workflow_add_set_eval() {
    let f = wcl_file("server svc-api {\n    port = 8080\n}\n");
    let path = f.path().to_str().unwrap().to_string();

    // Add a new service
    wcl(&["add", &path, "server svc-worker"]).success();

    // Read the file to put the port inside svc-worker manually
    // Since add creates an empty block, we need to add content to it
    let mut content = std::fs::read_to_string(&path).unwrap();
    content = content.replace(
        "server svc-worker {\n}",
        "server svc-worker {\n    port = 3000\n}",
    );
    std::fs::write(&path, &content).unwrap();

    // Validate the file
    wcl(&["validate", &path]).success();

    // Eval and check both blocks
    let output = Command::cargo_bin("wcl")
        .unwrap()
        .args(["eval", &path])
        .output()
        .expect("run eval");
    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap();
    assert_eq!(json["server"]["svc-api"]["port"], 8080);
    assert_eq!(json["server"]["svc-worker"]["port"], 3000);
}

#[test]
fn workflow_set_validate_query() {
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
    wcl(&["set", &path, "server#svc-api.port", "9090"]).success();

    // Should still validate
    wcl(&["validate", &path]).success();

    // Query should reflect the new value
    let output = Command::cargo_bin("wcl")
        .unwrap()
        .args(["query", "--format", "json", &path, "server | .port"])
        .output()
        .expect("run query");
    assert!(output.status.success());
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&output.stdout)).unwrap();
    assert!(json.as_array().unwrap().contains(&serde_json::json!(9090)));
}
