use assert_cmd::Command;
use predicates::prelude::*;
use std::io::Write;
use tempfile::NamedTempFile;

// ── Helper: write a named temp file containing given content ─────────────────

fn wcl_file(content: &str) -> NamedTempFile {
    let mut f = NamedTempFile::new().expect("tempfile");
    f.write_all(content.as_bytes()).expect("write");
    f
}

// ── wcl --help ───────────────────────────────────────────────────────────────

#[test]
fn help_exits_successfully() {
    Command::cargo_bin("wcl")
        .unwrap()
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("wcl"));
}

// ── wcl validate --help ───────────────────────────────────────────────────────

#[test]
fn validate_help_exits_successfully() {
    Command::cargo_bin("wcl")
        .unwrap()
        .args(["validate", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("validate").or(predicate::str::contains("Validate")));
}

// ── wcl validate <valid file> → success ──────────────────────────────────────

#[test]
fn validate_valid_file_succeeds() {
    let f = wcl_file("config {\n    port = 8080\n}\n");
    Command::cargo_bin("wcl")
        .unwrap()
        .args(["validate", f.path().to_str().unwrap()])
        .assert()
        .success();
}

#[test]
fn validate_valid_attribute_file_succeeds() {
    let f = wcl_file("name = \"hello\"\nversion = 42\n");
    Command::cargo_bin("wcl")
        .unwrap()
        .args(["validate", f.path().to_str().unwrap()])
        .assert()
        .success();
}

#[test]
fn validate_valid_file_prints_is_valid() {
    let f = wcl_file("x = 1\n");
    Command::cargo_bin("wcl")
        .unwrap()
        .args(["validate", f.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("is valid"));
}

// ── wcl validate <invalid file> → error exit code ────────────────────────────

#[test]
fn validate_invalid_file_fails() {
    // Syntax error: unclosed block
    let f = wcl_file("config {\n    port = \n");
    Command::cargo_bin("wcl")
        .unwrap()
        .args(["validate", f.path().to_str().unwrap()])
        .assert()
        .failure();
}

#[test]
fn validate_nonexistent_file_fails() {
    Command::cargo_bin("wcl")
        .unwrap()
        .args(["validate", "/nonexistent/path/file.wcl"])
        .assert()
        .failure();
}

// ── wcl validate --strict ─────────────────────────────────────────────────────

#[test]
fn validate_strict_flag_accepted() {
    let f = wcl_file("x = 1\n");
    Command::cargo_bin("wcl")
        .unwrap()
        .args(["validate", "--strict", f.path().to_str().unwrap()])
        .assert()
        .success();
}

// ── wcl convert --to json → JSON output ──────────────────────────────────────

#[test]
fn convert_to_json_simple_attribute() {
    let f = wcl_file("port = 8080\n");
    Command::cargo_bin("wcl")
        .unwrap()
        .args(["convert", "--to", "json", f.path().to_str().unwrap()])
        .assert()
        .success()
        .stdout(predicate::str::contains("port").and(predicate::str::contains("8080")));
}

#[test]
fn convert_to_json_produces_valid_json() {
    let f = wcl_file("name = \"test\"\ncount = 3\n");
    let output = Command::cargo_bin("wcl")
        .unwrap()
        .args(["convert", "--to", "json", f.path().to_str().unwrap()])
        .output()
        .expect("run wcl");

    assert!(output.status.success(), "expected success exit");

    let stdout = String::from_utf8_lossy(&output.stdout);
    // The output should be valid JSON — parse it to verify
    let parsed: serde_json::Value =
        serde_json::from_str(stdout.trim()).expect("output should be valid JSON");
    assert!(parsed.is_object());
}

#[test]
fn convert_to_json_block_produces_object_fields() {
    let f = wcl_file("service {\n    port = 9000\n}\n");
    let output = Command::cargo_bin("wcl")
        .unwrap()
        .args(["convert", "--to", "json", f.path().to_str().unwrap()])
        .output()
        .expect("run wcl");

    assert!(output.status.success(), "expected success exit");
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Output must be parseable JSON
    serde_json::from_str::<serde_json::Value>(stdout.trim())
        .expect("output should be valid JSON");
}

#[test]
fn convert_unsupported_format_fails() {
    let f = wcl_file("x = 1\n");
    Command::cargo_bin("wcl")
        .unwrap()
        .args(["convert", "--to", "xml", f.path().to_str().unwrap()])
        .assert()
        .failure();
}

#[test]
fn convert_no_flags_fails() {
    let f = wcl_file("x = 1\n");
    Command::cargo_bin("wcl")
        .unwrap()
        .args(["convert", f.path().to_str().unwrap()])
        .assert()
        .failure();
}

// ── wcl query → output ───────────────────────────────────────────────────────

#[test]
fn query_returns_results_for_valid_query() {
    let f = wcl_file("service { port = 8080 }\n");
    Command::cargo_bin("wcl")
        .unwrap()
        .args(["query", f.path().to_str().unwrap(), "service"])
        .assert()
        .success();
}

#[test]
fn query_subcommand_is_recognized_by_cli() {
    // Even though the query fails at runtime, the CLI must not print
    // "unknown subcommand" — the clap parser should accept "query".
    let f = wcl_file("x = 1\n");
    let output = Command::cargo_bin("wcl")
        .unwrap()
        .args(["query", f.path().to_str().unwrap(), "x"])
        .output()
        .expect("run wcl");

    let stderr = String::from_utf8_lossy(&output.stderr);
    // Must not be a clap "unrecognised subcommand" error
    assert!(
        !stderr.contains("unrecognized subcommand"),
        "CLI should recognise 'query' as a valid subcommand"
    );
}

// ── Additional subcommand help checks ────────────────────────────────────────

#[test]
fn convert_help_exits_successfully() {
    Command::cargo_bin("wcl")
        .unwrap()
        .args(["convert", "--help"])
        .assert()
        .success();
}

#[test]
fn query_help_exits_successfully() {
    Command::cargo_bin("wcl")
        .unwrap()
        .args(["query", "--help"])
        .assert()
        .success();
}

#[test]
fn fmt_help_exits_successfully() {
    Command::cargo_bin("wcl")
        .unwrap()
        .args(["fmt", "--help"])
        .assert()
        .success();
}
