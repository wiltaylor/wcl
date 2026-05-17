//! End-to-end integration tests for the `wcl` CLI.
//!
//! Every test writes real `.wcl` files to a temp directory and invokes the
//! binary through `assert_cmd`, verifying stdout/stderr/exit-code and, where
//! applicable, the on-disk file content after mutation commands.

use assert_cmd::Command;
use predicates::prelude::*;
use std::io::Write;
use tempfile::{tempdir, NamedTempFile};

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

fn sample_elf64_le() -> Vec<u8> {
    let mut data = vec![0u8; 0x100 + 64 * 3];
    data[0..4].copy_from_slice(&[0x7f, b'E', b'L', b'F']);
    data[4] = 2;
    data[5] = 1;
    data[6] = 1;
    data[16..18].copy_from_slice(&3u16.to_le_bytes());
    data[18..20].copy_from_slice(&62u16.to_le_bytes());
    data[20..24].copy_from_slice(&1u32.to_le_bytes());
    data[24..32].copy_from_slice(&0x401000u64.to_le_bytes());
    data[40..48].copy_from_slice(&0x100u64.to_le_bytes());
    data[52..54].copy_from_slice(&64u16.to_le_bytes());
    data[58..60].copy_from_slice(&64u16.to_le_bytes());
    data[60..62].copy_from_slice(&3u16.to_le_bytes());
    data[62..64].copy_from_slice(&2u16.to_le_bytes());

    data[0x80..0x84].copy_from_slice(&[1, 2, 3, 4]);
    data[0x90..0xa1].copy_from_slice(b"\0.text\0.shstrtab\0");

    let text = 0x100 + 64;
    data[text..text + 4].copy_from_slice(&1u32.to_le_bytes());
    data[text + 4..text + 8].copy_from_slice(&1u32.to_le_bytes());
    data[text + 8..text + 16].copy_from_slice(&6u64.to_le_bytes());
    data[text + 16..text + 24].copy_from_slice(&0x401000u64.to_le_bytes());
    data[text + 24..text + 32].copy_from_slice(&0x80u64.to_le_bytes());
    data[text + 32..text + 40].copy_from_slice(&4u64.to_le_bytes());
    data[text + 48..text + 56].copy_from_slice(&16u64.to_le_bytes());

    let names = 0x100 + 64 * 2;
    data[names..names + 4].copy_from_slice(&7u32.to_le_bytes());
    data[names + 4..names + 8].copy_from_slice(&3u32.to_le_bytes());
    data[names + 24..names + 32].copy_from_slice(&0x90u64.to_le_bytes());
    data[names + 32..names + 40].copy_from_slice(&17u64.to_le_bytes());
    data[names + 48..names + 56].copy_from_slice(&1u64.to_le_bytes());

    data
}

fn run_transform_stdout(
    transform: &std::path::Path,
    name: &str,
    input: &std::path::Path,
) -> String {
    let output = Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "transform",
            "run",
            name,
            "-f",
            transform.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
        ])
        .output()
        .expect("run wcl transform");
    assert!(
        output.status.success(),
        "transform failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
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
fn eval_direct_ref_preserves_id_in_json() {
    let json = eval_json(
        r#"
system wad {
    name = "WAD"
}

app main {
    system = wad
}
"#,
    );
    assert_eq!(json["app"]["main"]["system"]["id"], "wad");
    assert_eq!(json["app"]["main"]["system"]["name"], "WAD");
}

#[test]
fn eval_let_bindings_not_in_output() {
    let json = eval_json("let x = 42\nresult = x + 1\n");
    assert!(json.get("x").is_none(), "let bindings should be erased");
    assert_eq!(json["result"], 43);
}

#[test]
fn eval_import_codec_json_can_be_filtered() {
    let dir = tempdir().expect("tempdir");
    let config = dir.path().join("config.wcl");
    let data = dir.path().join("data.json");

    std::fs::write(
        &data,
        r#"
// comments are accepted by the WCL JSON codec
[
  {"name": "alice", "active": true},
  {"name": "bob", "active": false}
]
"#,
    )
    .expect("write data");
    std::fs::write(
        &config,
        r#"
rows = import_codec("data.json", "json", {})
active = filter(rows, row => row.active)
names = map(active, row => { name = row.name })
"#,
    )
    .expect("write config");

    let output = Command::cargo_bin("wcl")
        .unwrap()
        .args(["eval", "--format", "json", config.to_str().unwrap()])
        .output()
        .expect("run wcl eval");
    assert!(
        output.status.success(),
        "eval failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("stdout should be valid JSON");
    assert_eq!(json["names"], serde_json::json!([{"name": "alice"}]));
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
// TRANSFORMS
// ===========================================================================

#[test]
fn transform_custom_text_codec_decodes_with_symbol_tokens() {
    let dir = tempdir().expect("tempdir");
    let transform = dir.path().join("custom.wcl");
    let input = dir.path().join("input.txt");

    std::fs::write(&input, "ab").expect("write input");
    std::fs::write(
        &transform,
        r#"
codec chars {
    mode = :text

    tokenizer = cursor => cursor.eof() ? null : {
        let start = cursor.pos()
        let ch = cursor.take(1)
        { kind = :char, text = ch, start = start, end = cursor.pos(), value = ch }
    }

    parser = tokens => tokens.eof() ? null : {
        let t = tokens.take(1)
        { text = t.text, kind = t.kind }
    }

    encoder = record => record.text
}

transform chars-to-json {
    input = "codec::chars"
    output = "codec::json"

    map {
        text = in.text
        kind = in.kind
    }
}
"#,
    )
    .expect("write transform");

    Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "transform",
            "run",
            "chars-to-json",
            "-f",
            transform.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""kind":"char""#))
        .stdout(predicate::str::contains(r#""text":"a""#))
        .stdout(predicate::str::contains(r#""text":"b""#));
}

#[test]
fn transform_custom_text_codec_encodes_records() {
    let dir = tempdir().expect("tempdir");
    let transform = dir.path().join("custom.wcl");
    let input = dir.path().join("input.json");

    std::fs::write(&input, r#"[{"text":"x"},{"text":"y"}]"#).expect("write input");
    std::fs::write(
        &transform,
        r#"
codec chars {
    mode = :text
    tokenizer = cursor => null
    parser = tokens => null
    encoder = record => record.text
}

transform json-to-chars {
    input = "codec::json"
    output = "codec::chars"

    map {
        text = in.text
    }
}
"#,
    )
    .expect("write transform");

    Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "transform",
            "run",
            "json-to-chars",
            "-f",
            transform.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("xy"));
}

#[test]
fn transform_custom_byte_codec_decodes_records() {
    let dir = tempdir().expect("tempdir");
    let transform = dir.path().join("bytes.wcl");
    let input = dir.path().join("input.bin");

    std::fs::write(&input, [65_u8, 66_u8]).expect("write input");
    std::fs::write(
        &transform,
        r#"
codec bytes-one {
    mode = :bytes

    tokenizer = cursor => cursor.eof() ? null : {
        let start = cursor.pos()
        let b = cursor.take(1)
        { kind = :byte, start = start, end = cursor.pos(), value = b[0] }
    }

    parser = tokens => tokens.eof() ? null : {
        let t = tokens.take(1)
        { value = t.value, kind = t.kind }
    }

    encoder = record => [record.value]
}

transform bytes-to-json {
    input = "codec::bytes-one"
    output = "codec::json"

    map {
        value = in.value
        kind = in.kind
    }
}
"#,
    )
    .expect("write transform");

    Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "transform",
            "run",
            "bytes-to-json",
            "-f",
            transform.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""kind":"byte""#))
        .stdout(predicate::str::contains(r#""value":65"#))
        .stdout(predicate::str::contains(r#""value":66"#));
}

#[test]
fn transform_standard_elf_codec_decodes_section_streams() {
    let dir = tempdir().expect("tempdir");
    let transform = dir.path().join("elf.wcl");
    let input = dir.path().join("bash");

    std::fs::write(&input, sample_elf64_le()).expect("write input");
    std::fs::write(
        &transform,
        r#"
transform elf-to-json {
    input = "codec::elf"
    output = "codec::json"

    run = file => map(file.sections, section => {
        name = section.name
        type = section.type_name
        size = section.size
        first_chunk = section.name == ".text" ? section.data.next() : []
    })
}
"#,
    )
    .expect("write transform");

    Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "transform",
            "run",
            "elf-to-json",
            "-f",
            transform.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""name":".text""#))
        .stdout(predicate::str::contains(r#""first_chunk":[1,2,3,4]"#));
}

#[test]
fn transform_html_and_svg_codecs_encode_structured_elements() {
    let dir = tempdir().expect("tempdir");
    let input = dir.path().join("input.json");
    std::fs::write(&input, r#"[{"title":"Structured output"}]"#).expect("write input");

    let transform = dir.path().join("markup.wcl");
    std::fs::write(
        &transform,
        r##"
transform json-to-html {
    input = "codec::json"
    output = "codec::html"
    run = rows => {
        tag = "html"
        children = [
            { tag = "head", children = [{ tag = "title", content = "Structured output" }] },
            { tag = "body", children = [{ tag = "main", class_name = "page", children = [
                { tag = "h1", content = "Structured output" },
                { tag = "input", type_ = "checkbox", checked = true }
            ] }] }
        ]
    }
}

transform json-to-svg {
    input = "codec::json"
    output = "codec::svg"
    run = rows => {
        tag = "svg"
        width = 120
        height = 48
        viewBox = "0 0 120 48"
        children = [
            { tag = "rect", x = 1, y = 1, width = 118, height = 46, rx = 6, fill = "#f8fafc", stroke = "#2563eb" },
            { tag = "text", x = 12, y = 29, content = "Structured output", fill = "#0f172a" }
        ]
    }
}
"##,
    )
    .expect("write transform");

    let html = run_transform_stdout(&transform, "json-to-html", &input);
    assert!(html.contains("<main class=\"page\">"));
    assert!(html.contains("<h1>Structured output</h1>"));
    assert!(html.contains("<input type=\"checkbox\" checked>"));

    let svg = run_transform_stdout(&transform, "json-to-svg", &input);
    assert!(svg.contains("<svg"));
    assert!(svg.contains("xmlns=\"http://www.w3.org/2000/svg\""));
    assert!(svg.contains("viewBox=\"0 0 120 48\""));
    assert!(svg.contains("<rect"));
    assert!(svg.contains(">Structured output</text>"));
}

#[test]
fn transform_css_codec_encodes_structured_stylesheet() {
    let dir = tempdir().expect("tempdir");
    let input = dir.path().join("input.json");
    std::fs::write(&input, r#"[{"title":"Structured output"}]"#).expect("write input");

    let transform = dir.path().join("styles.wcl");
    std::fs::write(
        &transform,
        r##"
transform json-to-css {
    input = "codec::json"
    output = "codec::css"
    run = rows => {
        kind = "stylesheet"
        children = [
            {
                kind = "rule"
                selector = ".card"
                font_family = "system-ui"
                margin_block = "1rem"
                _webkit_appearance = "none"
                vars = map_set({}, "--card-accent", "#2563eb")
            },
            {
                kind = "media"
                query = "(min-width: 48rem)"
                children = [
                    { kind = "rule", selector = ".card", display = "grid" }
                ]
            },
            {
                kind = "keyframes"
                name = "fade"
                frames = [
                    { selector = "from", opacity = 0 },
                    { selector = "to", opacity = 1 }
                ]
            }
        ]
    }
}
"##,
    )
    .expect("write transform");

    let css = run_transform_stdout(&transform, "json-to-css", &input);
    assert!(css.contains(".card {\n"));
    assert!(css.contains("  --card-accent: #2563eb;"));
    assert!(css.contains("  font-family: system-ui;"));
    assert!(css.contains("  margin-block: 1rem;"));
    assert!(css.contains("  -webkit-appearance: none;"));
    assert!(css.contains("@media (min-width: 48rem) {"));
    assert!(css.contains("    display: grid;"));
    assert!(css.contains("@keyframes fade {"));
    assert!(css.contains("    opacity: 1;"));
}

#[test]
fn transform_decode_only_custom_codec_decodes_records() {
    let dir = tempdir().expect("tempdir");
    let transform = dir.path().join("decode_only.wcl");
    let input = dir.path().join("input.txt");

    std::fs::write(&input, "ab").expect("write input");
    std::fs::write(
        &transform,
        r#"
codec chars {
    mode = :text

    tokenizer = cursor => cursor.eof() ? null : {
        let start = cursor.pos()
        let ch = cursor.take(1)
        { kind = :char, text = ch, start = start, end = cursor.pos(), value = ch }
    }

    parser = tokens => tokens.eof() ? null : {
        let t = tokens.take(1)
        { text = t.text, kind = t.kind }
    }
}

transform chars-to-json {
    input = "codec::chars"
    output = "codec::json"

    map {
        text = in.text
        kind = in.kind
    }
}
"#,
    )
    .expect("write transform");

    Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "transform",
            "run",
            "chars-to-json",
            "-f",
            transform.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(r#""kind":"char""#))
        .stdout(predicate::str::contains(r#""text":"a""#))
        .stdout(predicate::str::contains(r#""text":"b""#));
}

#[test]
fn transform_decode_only_custom_codec_rejects_output_use() {
    let dir = tempdir().expect("tempdir");
    let transform = dir.path().join("decode_only_output.wcl");
    let input = dir.path().join("input.json");

    std::fs::write(&input, r#"[{"text":"x"}]"#).expect("write input");
    std::fs::write(
        &transform,
        r#"
codec chars {
    mode = :text
    tokenizer = cursor => null
    parser = tokens => null
}

transform json-to-chars {
    input = "codec::json"
    output = "codec::chars"

    map {
        text = in.text
    }
}
"#,
    )
    .expect("write transform");

    Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "transform",
            "run",
            "json-to-chars",
            "-f",
            transform.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not support encoding"));
}

#[test]
fn transform_encode_only_custom_codec_encodes_records() {
    let dir = tempdir().expect("tempdir");
    let transform = dir.path().join("encode_only.wcl");
    let input = dir.path().join("input.json");

    std::fs::write(&input, r#"[{"text":"x"},{"text":"y"}]"#).expect("write input");
    std::fs::write(
        &transform,
        r#"
codec chars {
    mode = :text
    encoder = record => record.text
}

transform json-to-chars {
    input = "codec::json"
    output = "codec::chars"

    map {
        text = in.text
    }
}
"#,
    )
    .expect("write transform");

    Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "transform",
            "run",
            "json-to-chars",
            "-f",
            transform.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("xy"));
}

#[test]
fn transform_encode_only_custom_codec_rejects_input_use() {
    let dir = tempdir().expect("tempdir");
    let transform = dir.path().join("encode_only_input.wcl");
    let input = dir.path().join("input.txt");

    std::fs::write(&input, "xy").expect("write input");
    std::fs::write(
        &transform,
        r#"
codec chars {
    mode = :text
    encoder = record => record.text
}

transform chars-to-json {
    input = "codec::chars"
    output = "codec::json"

    map {
        text = in.text
    }
}
"#,
    )
    .expect("write transform");

    Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "transform",
            "run",
            "chars-to-json",
            "-f",
            transform.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("does not support decoding"));
}

#[test]
fn transform_custom_codec_rejects_string_token_kind() {
    let dir = tempdir().expect("tempdir");
    let transform = dir.path().join("bad.wcl");
    let input = dir.path().join("input.txt");

    std::fs::write(&input, "x").expect("write input");
    std::fs::write(
        &transform,
        r#"
codec bad {
    mode = :text

    tokenizer = cursor => cursor.eof() ? null : {
        let start = cursor.pos()
        let ch = cursor.take(1)
        { kind = "char", text = ch, start = start, end = cursor.pos() }
    }

    parser = tokens => null
    encoder = record => record.text
}

transform bad-to-json {
    input = "codec::bad"
    output = "codec::json"
    map { text = in.text }
}
"#,
    )
    .expect("write transform");

    Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "transform",
            "run",
            "bad-to-json",
            "-f",
            transform.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("token kind must be a symbol"));
}

#[test]
fn transform_json_codec_accepts_comments() {
    let dir = tempdir().expect("tempdir");
    let transform = dir.path().join("json.wcl");
    let input = dir.path().join("input.json");

    std::fs::write(
        &input,
        r#"
// leading comment
[
    {"name":"Alice", /* inline block */ "age":30},
    {"name":"Bob", "age":25} // trailing item comment
]
"#,
    )
    .expect("write input");
    std::fs::write(
        &transform,
        r#"
transform json-copy {
    input = "codec::json"
    output = "codec::json"

    map {
        name = in.name
        age = in.age
    }
}
"#,
    )
    .expect("write transform");

    let stdout = run_transform_stdout(&transform, "json-copy", &input);
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid json output");
    assert_eq!(
        json,
        serde_json::json!([
            {"name": "Alice", "age": 30},
            {"name": "Bob", "age": 25}
        ])
    );
}

#[test]
fn transform_json_codec_pretty_prints_when_enabled() {
    let dir = tempdir().expect("tempdir");
    let transform = dir.path().join("json.wcl");
    let input = dir.path().join("input.json");

    std::fs::write(&input, r#"[{"name":"Alice","meta":{"age":30}}]"#).expect("write input");
    std::fs::write(
        &transform,
        r#"
transform json-copy {
    input = "codec::json"
    output = "codec::json"
    pretty = true

    map {
        name = in.name
        meta = in.meta
    }
}
"#,
    )
    .expect("write transform");

    let stdout = run_transform_stdout(&transform, "json-copy", &input);
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid json output");
    assert_eq!(
        json,
        serde_json::json!([{"name": "Alice", "meta": {"age": 30}}])
    );
    assert!(stdout.contains("[\n"));
    assert!(stdout.contains("  {\n"));
    assert!(stdout.contains("    \"name\": \"Alice\""));
    assert!(stdout.contains("    \"meta\": {\n"));
    assert!(stdout.contains("      \"age\": 30"));
}

#[test]
fn transform_json_codec_defaults_to_compact_output() {
    let dir = tempdir().expect("tempdir");
    let transform = dir.path().join("json.wcl");
    let input = dir.path().join("input.json");

    std::fs::write(&input, r#"[{"name":"Alice","age":30}]"#).expect("write input");
    std::fs::write(
        &transform,
        r#"
transform json-copy {
    input = "codec::json"
    output = "codec::json"

    map {
        name = in.name
        age = in.age
    }
}
"#,
    )
    .expect("write transform");

    let stdout = run_transform_stdout(&transform, "json-copy", &input);
    assert_eq!(stdout, r#"[{"name":"Alice","age":30}]"#.to_string() + "\n");
}

#[test]
fn transform_xml_codec_decodes_and_encodes_standard_codec() {
    let dir = tempdir().expect("tempdir");
    let transform = dir.path().join("xml.wcl");
    let input = dir.path().join("input.xml");

    std::fs::write(
        &input,
        r#"<person id="p1"><name>Alice &amp; Bob</name></person>"#,
    )
    .expect("write input");
    std::fs::write(
        &transform,
        r#"
transform transform_xml {
    input = "codec::xml"
    output = "codec::xml"
    root_name = "summary"

    map {
        person = in.name
        name = in.children[0].text
    }
}
"#,
    )
    .expect("write transform");

    let stdout = run_transform_stdout(&transform, "transform_xml", &input);
    assert_eq!(
        stdout,
        "<summary><person>person</person><name>Alice &amp; Bob</name></summary>\n"
    );
}

#[test]
fn transform_msgpack_codec_uses_standard_wcl_codec() {
    let dir = tempdir().expect("tempdir");
    let transform = dir.path().join("msgpack.wcl");
    let input = dir.path().join("input.msgpack");

    std::fs::write(
        &input,
        [
            0x91, 0x82, 0xa4, b'n', b'a', b'm', b'e', 0xa3, b'A', b'd', b'a', 0xa4, b'p', b'o',
            b'r', b't', 0xcd, 0x1f, 0x90,
        ],
    )
    .expect("write input");
    std::fs::write(
        &transform,
        r#"
transform transform_msgpack {
    input = "codec::msgpack"
    output = "codec::json"

    map {
        name = in.name
        port = in.port
    }
}
"#,
    )
    .expect("write transform");

    let stdout = run_transform_stdout(&transform, "transform_msgpack", &input);
    assert_eq!(stdout, r#"[{"name":"Ada","port":8080}]"#.to_string() + "\n");
}

#[test]
fn transform_json_codec_preserves_comment_markers_inside_strings() {
    let dir = tempdir().expect("tempdir");
    let transform = dir.path().join("json.wcl");
    let input = dir.path().join("input.json");

    std::fs::write(&input, r#"[{"text":"// not a comment /* also not */"}]"#).expect("write input");
    std::fs::write(
        &transform,
        r#"
transform json-copy {
    input = "codec::json"
    output = "codec::json"

    map {
        text = in.text
    }
}
"#,
    )
    .expect("write transform");

    let stdout = run_transform_stdout(&transform, "json-copy", &input);
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid json output");
    assert_eq!(
        json,
        serde_json::json!([{"text": "// not a comment /* also not */"}])
    );
}

#[test]
fn transform_json_codec_rejects_unterminated_block_comment() {
    let dir = tempdir().expect("tempdir");
    let transform = dir.path().join("json.wcl");
    let input = dir.path().join("input.json");

    std::fs::write(&input, r#"[{"name":"Alice"} /* no close"#).expect("write input");
    std::fs::write(
        &transform,
        r#"
transform json-copy {
    input = "codec::json"
    output = "codec::json"

    map {
        name = in.name
    }
}
"#,
    )
    .expect("write transform");

    Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "transform",
            "run",
            "json-copy",
            "-f",
            transform.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("unterminated block comment"));
}

#[test]
fn transform_yaml_codec_uses_standard_wcl_codec() {
    let dir = tempdir().expect("tempdir");
    let transform = dir.path().join("yaml.wcl");
    let input = dir.path().join("input.yaml");

    std::fs::write(
        &input,
        "- name: Alice\n  active: true\n- name: Bob\n  active: false\n",
    )
    .expect("write input");
    std::fs::write(
        &transform,
        r#"
transform yaml-copy {
    input = "codec::yaml"
    output = "codec::yaml"

    map {
        name = in.name
        active = in.active
    }
}
"#,
    )
    .expect("write transform");

    let stdout = run_transform_stdout(&transform, "yaml-copy", &input);
    let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(&stdout).expect("valid yaml output");
    let json = serde_json::to_value(yaml).expect("yaml to json");
    assert_eq!(
        json,
        serde_json::json!([
            {"name": "Alice", "active": true},
            {"name": "Bob", "active": false}
        ])
    );
}

#[test]
fn transform_toml_codec_uses_standard_wcl_codec() {
    let dir = tempdir().expect("tempdir");
    let transform = dir.path().join("toml.wcl");
    let input = dir.path().join("input.toml");

    std::fs::write(
        &input,
        "[[items]]\nname = \"Alice\"\nactive = true\n\n[[items]]\nname = \"Bob\"\nactive = false\n",
    )
    .expect("write input");
    std::fs::write(
        &transform,
        r#"
transform toml-copy {
    input = "codec::toml"
    output = "codec::toml"

    map {
        name = in.name
        active = in.active
    }
}
"#,
    )
    .expect("write transform");

    let stdout = run_transform_stdout(&transform, "toml-copy", &input);
    let toml: toml::Value = toml::from_str(&stdout).expect("valid toml output");
    let records = toml
        .get("records")
        .and_then(toml::Value::as_array)
        .expect("records array");
    assert_eq!(records.len(), 2);
    assert_eq!(
        records[0].get("name").and_then(toml::Value::as_str),
        Some("Alice")
    );
    assert_eq!(
        records[0].get("active").and_then(toml::Value::as_bool),
        Some(true)
    );
    assert_eq!(
        records[1].get("name").and_then(toml::Value::as_str),
        Some("Bob")
    );
    assert_eq!(
        records[1].get("active").and_then(toml::Value::as_bool),
        Some(false)
    );
}

#[test]
fn transform_csv_codec_uses_standard_wcl_codec() {
    let dir = tempdir().expect("tempdir");
    let transform = dir.path().join("csv.wcl");
    let input = dir.path().join("input.csv");

    std::fs::write(
        &input,
        "name,note\nAlice,\"hello, \"\"world\"\"\"\nBob,\"line 1\nline 2\"\n",
    )
    .expect("write input");
    std::fs::write(
        &transform,
        r#"
transform csv-copy {
    input = "codec::csv"
    output = "codec::csv"

    map {
        name = in.name
        note = in.note
    }
}
"#,
    )
    .expect("write transform");

    let stdout = run_transform_stdout(&transform, "csv-copy", &input);
    assert_eq!(
        stdout,
        "name,note\nAlice,\"hello, \"\"world\"\"\"\nBob,\"line 1\nline 2\"\n"
    );
}

#[test]
fn transform_hcl_codec_uses_standard_wcl_codec() {
    let dir = tempdir().expect("tempdir");
    let transform = dir.path().join("hcl.wcl");
    let input = dir.path().join("input.hcl");

    std::fs::write(
        &input,
        r#"
name = "root"
count = 1 + 2 * 3
server "web" "prod" {
  host = "localhost"
}
"#,
    )
    .expect("write input");
    std::fs::write(
        &transform,
        r#"
transform hcl-copy {
    input = "codec::hcl"
    output = "codec::json"

    map {
        name = in.name
        count = in.count
        server = in.server
    }
}
"#,
    )
    .expect("write transform");

    let stdout = run_transform_stdout(&transform, "hcl-copy", &input);
    let json: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid json output");
    assert_eq!(
        json,
        serde_json::json!([
            {
                "name": "root",
                "count": 7,
                "server": {"web": {"prod": {"host": "localhost"}}}
            }
        ])
    );
}

#[test]
fn transform_imported_codecs_library_does_not_duplicate_json() {
    let dir = tempdir().expect("tempdir");
    let lib_dir = dir.path().join("lib");
    let transform = dir.path().join("json.wcl");
    let input = dir.path().join("input.json");

    std::fs::create_dir_all(&lib_dir).expect("create lib");
    std::fs::write(
        lib_dir.join("codecs.wcl"),
        wcl::standard_lib::CODECS_LIBRARY_WCL,
    )
    .expect("write codecs");
    std::fs::write(&input, r#"[{"name":"Alice"}]"#).expect("write input");
    std::fs::write(
        &transform,
        r#"
import <codecs.wcl>

transform json-copy {
    input = "codec::json"
    output = "codec::json"

    map {
        name = in.name
    }
}
"#,
    )
    .expect("write transform");

    let output = Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "transform",
            "run",
            "json-copy",
            "-f",
            transform.to_str().unwrap(),
            "--input",
            input.to_str().unwrap(),
            "--lib-path",
            lib_dir.to_str().unwrap(),
        ])
        .output()
        .expect("run transform");
    assert!(
        output.status.success(),
        "transform failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid json output");
    assert_eq!(json, serde_json::json!([{"name": "Alice"}]));
}

#[test]
fn install_library_writes_codecs_file() {
    let dir = tempdir().expect("tempdir");
    let data_home = dir.path().join("data");

    Command::cargo_bin("wcl")
        .unwrap()
        .env("XDG_DATA_HOME", &data_home)
        .args(["install-library"])
        .assert()
        .success()
        .stdout(predicate::str::contains("installed standard library"));

    let codecs = data_home.join("wcl/lib/codecs.wcl");
    let source = std::fs::read_to_string(codecs).expect("read codecs");
    assert!(source.contains("codec json"));
    assert!(data_home.join("wcl/lib/html.wcl").exists());
    assert!(data_home.join("wcl/lib/svg.wcl").exists());
    assert!(data_home.join("wcl/lib/css.wcl").exists());
}

// ===========================================================================
// WDOC
// ===========================================================================

#[test]
fn wdoc_build_renders_drawing_image_and_copies_asset() {
    let dir = tempdir().expect("tempdir");
    let pages_dir = dir.path().join("pages");
    let images_dir = pages_dir.join("images/deep");
    std::fs::create_dir_all(&images_dir).expect("create images dir");
    std::fs::write(images_dir.join("hero.png"), [0x89, b'P', b'N', b'G']).expect("write image");

    let site = r#"
import <wdoc.wcl>
import "./pages/page.wcl"
use wdoc::{doc, section}

doc my_docs {
    title = "Image Docs"
    section overview "Overview" {}
}
"#;

    let page = r#"
import <wdoc.wcl>
use wdoc::{page, layout}
use wdoc::draw::{diagram, image}

page home {
    section = "my_docs.overview"
    title = "Home"

    layout {
        diagram hero_diagram {
            width = 200
            height = 120

            image hero {
                x = 20
                y = 10
                width = 160
                height = 90
                src = "images/deep/hero.png"
                alt = "Hero image"
                fit = "cover"
                rx = 6
                ry = 6
            }
        }
    }
}
"#;
    let input = dir.path().join("site.wcl");
    std::fs::write(&input, site).expect("write wdoc file");
    std::fs::write(pages_dir.join("page.wcl"), page).expect("write page file");
    let output = dir.path().join("out");

    Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "wdoc",
            "build",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    let html = std::fs::read_to_string(output.join("home.html")).expect("read rendered page");
    assert!(html.contains("<image href=\"images/deep/hero.png\""));
    assert!(html.contains("preserveAspectRatio=\"xMidYMid slice\""));
    assert!(html.contains("role=\"img\" aria-label=\"Hero image\""));
    assert!(output.join("images/deep/hero.png").exists());
}

#[test]
fn wdoc_build_embeds_local_inline_svg_from_import_dir() {
    let dir = tempdir().expect("tempdir");
    let pages_dir = dir.path().join("pages");
    let icons_dir = pages_dir.join("icons");
    std::fs::create_dir_all(&icons_dir).expect("create icons dir");
    std::fs::write(
        icons_dir.join("logo.svg"),
        r#"<svg viewBox="0 0 10 10"><script>alert(1)</script><path class="logo-path" onclick="bad()" d="M0 0L10 10"/></svg>"#,
    )
    .expect("write svg");

    let site = r#"
import <wdoc.wcl>
import "./pages/page.wcl"
use wdoc::{doc, section}

doc my_docs {
    title = "Inline SVG Docs"
    section overview "Overview" {}
}
"#;

    let page = r#"
import <wdoc.wcl>
use wdoc::{page, layout}
use wdoc::draw::{diagram, inline_svg}

page home {
    section = "my_docs.overview"
    title = "Home"

    layout {
        diagram logo_diagram {
            width = 80
            height = 80

            inline_svg logo {
                x = 10
                y = 12
                width = 40
                height = 40
                src = "icons/logo.svg"
                class = "brand-logo"
                fill = "currentColor"
            }
        }
    }
}
"#;
    let input = dir.path().join("site.wcl");
    std::fs::write(&input, site).expect("write wdoc file");
    std::fs::write(pages_dir.join("page.wcl"), page).expect("write page file");
    let output = dir.path().join("out");

    Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "wdoc",
            "build",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    let html = std::fs::read_to_string(output.join("home.html")).expect("read rendered page");
    assert!(html.contains("class=\"brand-logo\""));
    assert!(html.contains("<path class=\"logo-path\" d=\"M0 0L10 10\""));
    assert!(html.contains("fill=\"currentColor\""));
    assert!(!html.contains("alert(1)"));
    assert!(!html.contains("onclick"));
}

#[test]
fn wdoc_build_registers_design_system_diagram_css_once() {
    let dir = tempdir().expect("tempdir");
    let site = r#"
import <wdoc.wcl>
use wdoc::{doc, section, page, layout}
use wdoc::draw::{diagram, rect}

doc my_docs {
    title = "Design Docs"
    section overview "Overview" {}
}

page home {
    section = "my_docs.overview"
    title = "Home"

    layout {
        diagram first {
            width = 80
            height = 40
            design_system = "wad_interface"
            css = ".ui-button-bg { fill: red; }"

            rect bg {
                x = 0
                y = 0
                width = 40
                height = 20
                class = "ui-button-bg"
            }
        }

        diagram second {
            width = 80
            height = 40
            design_system = "wad_interface"
            css = ".ui-button-bg { fill: red; }"

            rect bg {
                x = 0
                y = 0
                width = 40
                height = 20
                class = "ui-button-bg"
            }
        }
    }
}
"#;
    let input = dir.path().join("site.wcl");
    std::fs::write(&input, site).expect("write wdoc file");
    let output = dir.path().join("out");

    Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "wdoc",
            "build",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    let html = std::fs::read_to_string(output.join("home.html")).expect("read rendered page");
    let css = std::fs::read_to_string(output.join("styles.css")).expect("read styles");
    assert_eq!(html.matches("class=\"wad-ds-wad_interface\"").count(), 2);
    assert!(!html.contains("<style>"));
    assert_eq!(
        css.matches(".wad-ds-wad_interface .ui-button-bg").count(),
        1
    );
}

#[test]
fn wdoc_build_registers_imported_css_fragment_once() {
    let dir = tempdir().expect("tempdir");
    let fragments_dir = dir.path().join("fragments");
    std::fs::create_dir_all(&fragments_dir).expect("create fragments dir");

    let site = r#"
import <wdoc.wcl>
import "./fragments/css.wcl"
use wdoc::{doc, section, page, layout}
use wdoc::draw::{diagram, rect}

doc my_docs {
    title = "Design Docs"
    section overview "Overview" {}
}

page home {
    section = "my_docs.overview"
    title = "Home"

    layout {
        diagram first {
            width = 80
            height = 40
            design_system = "wad_interface"

            rect bg {
                x = 0
                y = 0
                width = 40
                height = 20
                class = "token-swatch"
            }
        }

        diagram second {
            width = 80
            height = 40
            design_system = "wad_interface"

            rect bg {
                x = 0
                y = 0
                width = 40
                height = 20
                class = "token-swatch"
            }
        }
    }
}
"#;
    let css_fragment = r#"
import <wdoc.wcl>
use wdoc::{css_fragment}

css_fragment wad_interface_tokens {
    scope = "wad_interface"
    css = ".token-swatch { fill: var(--wad-token-frost); }"
}

css_fragment wad_interface_tokens_duplicate {
    scope = "wad_interface"
    css = ".token-swatch { fill: var(--wad-token-frost); }"
}
"#;
    let input = dir.path().join("site.wcl");
    std::fs::write(&input, site).expect("write wdoc file");
    std::fs::write(fragments_dir.join("css.wcl"), css_fragment).expect("write fragment file");
    let output = dir.path().join("out");

    Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "wdoc",
            "build",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    let html = std::fs::read_to_string(output.join("home.html")).expect("read rendered page");
    let css = std::fs::read_to_string(output.join("styles.css")).expect("read styles");
    assert_eq!(html.matches("class=\"wad-ds-wad_interface\"").count(), 2);
    assert!(!html.contains("<style>"));
    assert_eq!(
        css.matches(".wad-ds-wad_interface .token-swatch").count(),
        1
    );
}

#[test]
fn wdoc_build_registers_imported_font_asset_and_global_css() {
    let dir = tempdir().expect("tempdir");
    let theme_dir = dir.path().join("theme");
    std::fs::create_dir_all(theme_dir.join("fonts")).expect("create theme font dir");
    std::fs::write(theme_dir.join("fonts/Inter-Regular.woff2"), [0, 1, 2, 3]).expect("write font");

    let site = r#"
import <wdoc.wcl>
import "./theme/fonts.wcl"
use wdoc::{doc, section, page, layout}
use wdoc::draw::{diagram, text}

doc my_docs {
    title = "Design Docs"
    section overview "Overview" {}
}

page home {
    section = "my_docs.overview"
    title = "Home"

    layout {
        diagram first {
            width = 160
            height = 60

            text label {
                x = 8
                y = 24
                content = "Hello"
                font_family = "\"Inter\", system-ui, sans-serif"
                font_size = 14
            }
        }
    }
}
"#;
    let fonts = r#"
import <wdoc.wcl>
use wdoc::{font_asset, global_css}

font_asset inter_regular {
    family = "Inter"
    src = "fonts/Inter-Regular.woff2"
    weight = "400"
    style = "normal"
    display = "swap"
}

font_asset inter_regular_duplicate {
    family = "Inter"
    src = "fonts/Inter-Regular.woff2"
    weight = "400"
    style = "normal"
    display = "swap"
}

global_css app_fonts {
    css = ":root { --font-body: \"Inter\", system-ui, sans-serif; }"
}
"#;
    let input = dir.path().join("site.wcl");
    std::fs::write(&input, site).expect("write wdoc file");
    std::fs::write(theme_dir.join("fonts.wcl"), fonts).expect("write fonts file");
    let output = dir.path().join("out");

    Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "wdoc",
            "build",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    let html = std::fs::read_to_string(output.join("home.html")).expect("read rendered page");
    let css = std::fs::read_to_string(output.join("styles.css")).expect("read styles");
    assert_eq!(css.matches("font-family: \"Inter\";").count(), 1);
    assert!(css.contains("font-family: \"Inter\";"));
    assert!(css.contains("src: url(\"fonts/Inter-Regular.woff2\") format(\"woff2\");"));
    assert!(css.contains(":root { --font-body: \"Inter\", system-ui, sans-serif; }"));
    assert!(!css.contains(".wad-ds-wad_interface :root"));
    assert!(html.contains("font-family=\"&quot;Inter&quot;, system-ui, sans-serif\""));
    assert!(output.join("fonts/Inter-Regular.woff2").exists());
}

#[test]
fn wdoc_build_expands_imported_macro_inside_diagram_body() {
    let dir = tempdir().expect("tempdir");
    let pages_dir = dir.path().join("pages");
    std::fs::create_dir_all(&pages_dir).expect("create pages dir");

    let site = r#"
import <wdoc.wcl>
import "./pages/page.wcl"
use wdoc::{doc, section}

doc my_docs {
    title = "Macro Docs"
    section overview "Overview" {}
}
"#;

    let renderers = r##"
import <wdoc.wcl>
use wdoc::draw::{group, rect, text}

export macro action_button(label_text, xpos) {
    group button {
        x = xpos
        y = 20
        width = 120
        height = 36

        rect bg {
            x = 0
            y = 0
            width = 120
            height = 36
            rx = 6
            fill = "#5E81AC"
        }

        text caption {
            x = 0
            y = 0
            width = 120
            height = 36
            content = label_text
            fill = "#fff"
        }
    }
}
"##;

    let page = r#"
import <wdoc.wcl>
import "./renderers.wcl"
use wdoc::{page, layout}
use wdoc::draw::{diagram}

page home {
    section = "my_docs.overview"
    title = "Home"

    layout {
        let button_text = "Preview"

        diagram macro_diagram {
            width = 180
            height = 80

            action_button(button_text, 30)
        }
    }
}
"#;
    let input = dir.path().join("site.wcl");
    std::fs::write(&input, site).expect("write site file");
    std::fs::write(pages_dir.join("renderers.wcl"), renderers).expect("write renderer file");
    std::fs::write(pages_dir.join("page.wcl"), page).expect("write page file");
    let output = dir.path().join("out");

    Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "wdoc",
            "build",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    let html = std::fs::read_to_string(output.join("home.html")).expect("read rendered page");
    assert!(html.contains("<g transform=\"translate(30,20)\""));
    assert!(html.contains(">Preview</text>"));
    assert!(html.contains("fill=\"#5E81AC\""));
}

#[test]
fn wdoc_build_expands_imported_partial_macros_inside_diagram_body() {
    let dir = tempdir().expect("tempdir");
    let pages_dir = dir.path().join("pages");
    std::fs::create_dir_all(&pages_dir).expect("create pages dir");

    let site = r#"
import <wdoc.wcl>
import "./pages/page.wcl"
use wdoc::{doc, section}

doc my_docs {
    title = "Partial Macro Docs"
    section overview "Overview" {}
}
"#;

    let renderer_a = r##"
import <wdoc.wcl>
use wdoc::draw::{rect, text}

export partial macro render_example(label_text) {
    if label_text == "Preview" {
        rect first {
            x = 10
            y = 12
            width = 70
            height = 30
            fill = "#BF616A"
        }

        text first_label {
            x = 16
            y = 18
            width = 60
            height = 20
            content = label_text
            fill = "#fff"
        }
    }
}
"##;

    let renderer_b = r##"
import <wdoc.wcl>
use wdoc::draw::{rect, text}

export partial macro render_example(label_text) {
    rect second {
        x = 90
        y = 12
        width = 70
        height = 30
        fill = "#A3BE8C"
    }

    text second_label {
        x = 96
        y = 18
        width = 60
        height = 20
        content = label_text
        fill = "#111"
    }
}
"##;

    let page = r#"
import <wdoc.wcl>
import "./renderer_a.wcl"
import "./renderer_b.wcl"
use wdoc::{page, layout}
use wdoc::draw::{diagram}

page home {
    section = "my_docs.overview"
    title = "Home"

    layout {
        let button_text = "Preview"

        diagram macro_diagram {
            width = 180
            height = 70

            render_example(button_text)
        }
    }
}
"#;
    let input = dir.path().join("site.wcl");
    std::fs::write(&input, site).expect("write site file");
    std::fs::write(pages_dir.join("renderer_a.wcl"), renderer_a).expect("write renderer a file");
    std::fs::write(pages_dir.join("renderer_b.wcl"), renderer_b).expect("write renderer b file");
    std::fs::write(pages_dir.join("page.wcl"), page).expect("write page file");
    let output = dir.path().join("out");

    Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "wdoc",
            "build",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    let html = std::fs::read_to_string(output.join("home.html")).expect("read rendered page");
    let first = html.find("fill=\"#BF616A\"").expect("first fragment");
    let second = html.find("fill=\"#A3BE8C\"").expect("second fragment");
    assert!(first < second);
    assert!(html.matches(">Preview</text>").count() >= 2);
}

#[test]
fn wdoc_build_binds_for_iterator_args_for_imported_partial_macro_body_loops() {
    let dir = tempdir().expect("tempdir");

    let site = r#"
import <wdoc.wcl>
use wdoc::{doc, page, section, layout}
use wdoc::draw::{diagram, rect}

import "./page.wcl"
import "./control.wcl"

let components = [{ id = "a" }]
let items = [{ id = "i", component = "a" }]

doc d {
    title = "D"
    section s "S" {}
}
"#;

    let page = r#"
for component in components {
    page p-${component.id} {
        section = "d.s"
        title = "P"

        layout {
            diagram demo {
                width = 50
                height = 50

                render(component, items)
            }
        }
    }
}
"#;

    let control = r#"
partial macro render(component, items) {
    for item in filter(items, item => item.component == component.id) {
        rect r-${item.id} {
            x = 1
            y = 1
            width = 10
            height = 10
            fill = "red"
        }
    }
}
"#;

    let input = dir.path().join("main.wcl");
    std::fs::write(&input, site).expect("write site file");
    std::fs::write(dir.path().join("page.wcl"), page).expect("write page file");
    std::fs::write(dir.path().join("control.wcl"), control).expect("write control file");
    let output = dir.path().join("out");

    Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "wdoc",
            "build",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    let html = std::fs::read_to_string(output.join("p-a.html")).expect("read rendered page");
    assert!(html.contains("<rect"));
    assert!(html.contains("fill=\"red\""));
}

#[test]
fn wdoc_build_keeps_partial_macro_params_bound_in_globbed_nested_documents() {
    let dir = tempdir().expect("tempdir");
    let pages_dir = dir.path().join("pages");
    let controls_dir = dir.path().join("controls");
    std::fs::create_dir_all(&pages_dir).expect("create pages dir");
    std::fs::create_dir_all(&controls_dir).expect("create controls dir");

    let site = r#"
import <wdoc.wcl>
use wdoc::{doc, page, section, layout}
use wdoc::draw::{diagram, rect, text}

schema "before_import" { name: string }
before_import alpha { name = "before" }

import "./pages/page.wcl"
import "./controls/*.wcl"

let components = [{ id = "ds_button", label = "Button" }]
let example_preview_height = 50
let ui_component_example_element_models = [
    { id = "first", component = "ds_button", x = 1 },
    { id = "skip", component = "other", x = 20 },
]

schema "after_import" { name: string }
after_import omega { name = "after" }

doc d {
    title = "D"
    section s "S" {}
}
"#;

    let page = r#"
for component in components {
    page p-${component.id} {
        section = "d.s"
        title = "P"

        layout {
            diagram demo {
                width = 90
                height = example_preview_height

                wad_render_ui_component_examples(
                    component,
                    example_preview_height,
                    ui_component_example_element_models
                )
            }
        }
    }
}
"#;

    let control = r#"
partial macro wad_render_ui_component_examples(component, example_preview_height, ui_component_example_element_models) {
    if to_string(component.id) == "ds_button" {
        let labels = map(ui_component_example_element_models, item => item.id)

        for example_element in filter(
            ui_component_example_element_models,
            item => item.component == to_string(component.id)
        ) {
            rect example-${component.id}-${example_element.id} {
                x = example_element.x
                y = 1
                width = 10
                height = example_preview_height - 40
                fill = "red"
            }

            text label-${example_element.id} {
                x = 1
                y = 20
                content = to_string(component.label) + " " + to_string(labels[0])
                font_size = 10
            }
        }
    }
}
"#;

    let input = dir.path().join("main.wcl");
    std::fs::write(&input, site).expect("write site file");
    std::fs::write(pages_dir.join("page.wcl"), page).expect("write page file");
    std::fs::write(controls_dir.join("button.wcl"), control).expect("write control file");
    let output = dir.path().join("out");

    Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "wdoc",
            "build",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    let html = std::fs::read_to_string(output.join("p-ds_button.html")).expect("read page");
    assert!(html.contains("<rect"));
    assert!(html.contains("fill=\"red\""));
    assert!(html.contains(">Button first</text>"));
}

#[test]
fn wdoc_build_preserves_block_ref_partial_macro_args_from_query_loops() {
    let dir = tempdir().expect("tempdir");
    let pages_dir = dir.path().join("pages");
    let controls_dir = dir.path().join("controls");
    std::fs::create_dir_all(&pages_dir).expect("create pages dir");
    std::fs::create_dir_all(&controls_dir).expect("create controls dir");

    let site = r#"
import <wdoc.wcl>
use wdoc::{doc, page, section, layout}
use wdoc::draw::{diagram, rect, text}

import "./pages/page.wcl"
import "./controls/*.wcl"

schema "UiComponent" {
    label: string
}

UiComponent ds_button {
    label = "Button"
}

let example_preview_height = 50
let ui_component_example_element_models = [{ id = "first", component = "ds_button" }]

doc d {
    title = "D"
    section s "S" {}
}
"#;

    let page = r#"
for component in (..UiComponent) {
    page p-${component.id} {
        section = "d.s"
        title = "P"

        layout {
            diagram demo {
                width = 90
                height = example_preview_height

                wad_render_ui_component_examples(
                    component,
                    example_preview_height,
                    ui_component_example_element_models
                )
            }
        }
    }
}
"#;

    let control = r#"
partial macro wad_render_ui_component_examples(component, example_preview_height, ui_component_example_element_models) {
    if to_string(component.id) == "ds_button" {
        for example_element in filter(
            ui_component_example_element_models,
            item => item.component == to_string(component.id)
        ) {
            rect r-${example_element.id} {
                x = 1
                y = 1
                width = 10
                height = example_preview_height - 40
                fill = "red"
            }

            text label-${example_element.id} {
                x = 1
                y = 20
                content = component.label
                font_size = 10
            }
        }
    }
}
"#;

    let input = dir.path().join("main.wcl");
    std::fs::write(&input, site).expect("write site file");
    std::fs::write(pages_dir.join("page.wcl"), page).expect("write page file");
    std::fs::write(controls_dir.join("button.wcl"), control).expect("write control file");
    let output = dir.path().join("out");

    Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "wdoc",
            "build",
            input.to_str().unwrap(),
            "--output",
            output.to_str().unwrap(),
        ])
        .assert()
        .success();

    let html = std::fs::read_to_string(output.join("p-ds_button.html")).expect("read page");
    assert!(html.contains("fill=\"red\""));
    assert!(html.contains(">Button</text>"));
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
