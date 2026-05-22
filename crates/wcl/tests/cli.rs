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

#[test]
fn eval_prints_top_level_field() {
    wcl()
        .arg("eval")
        .arg(examples_dir().join("basic.wcl"))
        .arg("name")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"alpha\""));
}

#[test]
fn eval_walks_dotted_path_into_block() {
    wcl()
        .arg("eval")
        .arg(examples_dir().join("basic.wcl"))
        .arg("service.port")
        .assert()
        .success()
        .stdout(predicate::str::contains("8080"));
}

#[test]
fn eval_reports_unknown_path() {
    wcl()
        .arg("eval")
        .arg(examples_dir().join("basic.wcl"))
        .arg("nope")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no such path"));
}

fn tempdir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("wcl-cli-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("mkdir tempdir");
    dir
}

#[test]
fn check_ok_on_connections_example() {
    wcl()
        .arg("check")
        .arg(examples_dir().join("connections.wcl"))
        .assert()
        .success()
        .stdout(predicate::str::contains("OK"));
}

#[test]
fn parse_prints_connection_decl_and_statements() {
    wcl()
        .arg("parse")
        .arg(examples_dir().join("connections.wcl"))
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "connection DependsOn: Service -> Service : EdgeKind",
        ))
        .stdout(predicate::str::contains("web -> db"))
        .stdout(predicate::str::contains("web -> cache :uses"));
}

#[test]
fn eval_walks_into_decomposed_connections_field() {
    wcl()
        .arg("eval")
        .arg(examples_dir().join("connections.wcl"))
        .arg("deps")
        .assert()
        .success()
        .stdout(predicate::str::contains("DependsOn"))
        .stdout(predicate::str::contains("destination: \"db\""))
        .stdout(predicate::str::contains("destination: \"cache\""));
}
