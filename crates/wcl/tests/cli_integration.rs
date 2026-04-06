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

// ── wcl validate --schema ────────────────────────────────────────────────────

#[test]
fn validate_with_external_schema_valid_config() {
    let schema = wcl_file(
        r#"
schema "server" {
    port: i64
    host: string
}
"#,
    );
    let config = wcl_file(
        r#"
server {
    port = 8080
    host = "localhost"
}
"#,
    );
    Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "validate",
            "--schema",
            schema.path().to_str().unwrap(),
            config.path().to_str().unwrap(),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("is valid"));
}

#[test]
fn validate_with_external_schema_missing_required_field() {
    let schema = wcl_file(
        r#"
schema "server" {
    port: i64
    host: string
}
"#,
    );
    let config = wcl_file(
        r#"
server {
    port = 8080
}
"#,
    );
    Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "validate",
            "--schema",
            schema.path().to_str().unwrap(),
            config.path().to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

#[test]
fn validate_with_external_schema_type_mismatch() {
    let schema = wcl_file(
        r#"
schema "server" {
    port: i64
    host: string
}
"#,
    );
    let config = wcl_file(
        r#"
server {
    port = "not_a_number"
    host = "localhost"
}
"#,
    );
    Command::cargo_bin("wcl")
        .unwrap()
        .args([
            "validate",
            "--schema",
            schema.path().to_str().unwrap(),
            config.path().to_str().unwrap(),
        ])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error"));
}

// ── Additional subcommand help checks ────────────────────────────────────────

#[test]
fn fmt_help_exits_successfully() {
    Command::cargo_bin("wcl")
        .unwrap()
        .args(["fmt", "--help"])
        .assert()
        .success();
}
