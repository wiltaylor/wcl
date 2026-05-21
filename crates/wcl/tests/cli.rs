use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("examples")
}

fn wcl() -> Command {
    Command::cargo_bin("wcl").expect("wcl binary built")
}

#[test]
fn check_ok_on_basic_example() {
    wcl()
        .arg("check")
        .arg(examples_dir().join("basic.wcl"))
        .assert()
        .success()
        .stdout(predicate::str::contains("OK"));
}

#[test]
fn parse_prints_document_tree() {
    wcl()
        .arg("parse")
        .arg(examples_dir().join("basic.wcl"))
        .assert()
        .success()
        .stdout(predicate::str::contains("service \"web\" {"))
        .stdout(predicate::str::contains("name = \"alpha\""))
        .stdout(predicate::str::contains("port = 8080"));
}

#[test]
fn check_reports_syntax_error_and_exits_nonzero() {
    let tmp = tempdir();
    let file = tmp.join("bad.wcl");
    std::fs::write(&file, "name =\n").expect("write fixture");
    wcl()
        .arg("check")
        .arg(&file)
        .assert()
        .failure()
        .stderr(predicate::str::contains("expected value"));
}

#[test]
fn check_reports_missing_file() {
    wcl()
        .arg("check")
        .arg("does-not-exist.wcl")
        .assert()
        .failure();
}

fn tempdir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("wcl-cli-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir tempdir");
    dir
}
