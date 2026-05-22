use std::path::PathBuf;

use assert_cmd::Command;
use predicates::prelude::*;
use tempfile::TempDir;

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
    let tmp = TempDir::new().expect("mkdir tempdir");
    let file = tmp.path().join("bad.wcl");
    std::fs::write(&file, "name =\n").expect("write fixture");
    wcl()
        .arg("check")
        .arg(&file)
        .assert()
        .code(1)
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
        .code(3)
        .stderr(predicate::str::contains("no such path"));
}

#[test]
fn eval_suggests_near_match_for_typo_path() {
    wcl()
        .arg("eval")
        .arg(examples_dir().join("basic.wcl"))
        .arg("nam")
        .assert()
        .code(3)
        .stderr(predicate::str::contains("did you mean: name"));
}

#[test]
fn check_summarises_schema_violations() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let file = tmp.path().join("bad.wcl");
    std::fs::write(&file, "@document\ntype Doc { name: utf8 }\nstray = 1\n")
        .expect("write fixture");
    wcl()
        .arg("check")
        .arg(&file)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("schema violation"));
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
fn eval_renders_interpolated_field() {
    wcl()
        .arg("eval")
        .arg(examples_dir().join("interpolation.wcl"))
        .arg("greeting")
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello, alice!"))
        .stdout(predicate::str::contains("4 item(s)"));
}

#[test]
fn eval_renders_heredoc_field_as_multiline() {
    wcl()
        .arg("eval")
        .arg(examples_dir().join("heredoc.wcl"))
        .arg("message")
        .assert()
        .success()
        .stdout(predicate::str::contains("Hello, world."))
        .stdout(predicate::str::contains("Goodbye, world."));
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

#[test]
fn eval_reflective_decorator_queries() {
    let file = examples_dir().join("reflect_decorators.wcl");
    // `decorator_names` on a type — positional decorator name first.
    wcl()
        .arg("eval")
        .arg(&file)
        .arg("book_decs")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"block\""))
        .stdout(predicate::str::contains("\"schemaless\""));
    // `decorator_arg` reading a positional slot via schema dispatch.
    wcl()
        .arg("eval")
        .arg(&file)
        .arg("book_block_name")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"book\""));
    // Member access walks Type -> TypeField; inline slot resolves.
    wcl()
        .arg("eval")
        .arg(&file)
        .arg("title_inline_idx")
        .assert()
        .success()
        .stdout(predicate::str::contains("0"));
    // Missing decorator -> none.
    wcl()
        .arg("eval")
        .arg(&file)
        .arg("missing_dec")
        .assert()
        .success()
        .stdout(predicate::str::contains("none"));
}

#[test]
fn fmt_to_stdout_is_idempotent_on_basic_example() {
    let first = wcl()
        .arg("fmt")
        .arg(examples_dir().join("basic.wcl"))
        .assert()
        .success();
    let formatted = String::from_utf8(first.get_output().stdout.clone()).unwrap();

    // Write the formatted output to a temp file and reformat it. The
    // second pass must produce byte-identical output (idempotence).
    let tmp = TempDir::new().expect("mkdir tempdir");
    let file = tmp.path().join("once.wcl");
    std::fs::write(&file, &formatted).expect("write fmt output");
    let second = wcl().arg("fmt").arg(&file).assert().success();
    let formatted2 = String::from_utf8(second.get_output().stdout.clone()).unwrap();
    assert_eq!(formatted, formatted2);
}

#[test]
fn fmt_in_place_rewrites_file_atomically() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let file = tmp.path().join("input.wcl");
    // Non-canonical input: extra whitespace + a trailing semicolon-
    // looking thing the printer will normalise.
    std::fs::write(&file, "@schemaless x  =   1\n").expect("write fixture");

    wcl()
        .arg("fmt")
        .arg("--in-place")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::is_empty());

    let after = std::fs::read_to_string(&file).expect("read after fmt");
    assert_eq!(after, "@schemaless x = 1\n");
}

#[test]
fn fmt_reports_parse_error_via_exit_code() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let file = tmp.path().join("broken.wcl");
    std::fs::write(&file, "name =\n").expect("write fixture");
    wcl()
        .arg("fmt")
        .arg(&file)
        .assert()
        .code(1)
        .stderr(predicate::str::contains("expected value"));
}
