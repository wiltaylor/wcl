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

#[test]
fn get_is_an_alias_of_eval() {
    let eval_out = wcl()
        .arg("eval")
        .arg(examples_dir().join("basic.wcl"))
        .arg("name")
        .assert()
        .success();
    let get_out = wcl()
        .arg("get")
        .arg(examples_dir().join("basic.wcl"))
        .arg("name")
        .assert()
        .success();
    assert_eq!(
        eval_out.get_output().stdout,
        get_out.get_output().stdout,
        "wcl get and wcl eval produce different output"
    );
}

#[test]
fn set_updates_top_level_field_in_named_file() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let file = tmp.path().join("doc.wcl");
    std::fs::write(&file, "@schemaless brand = \"wcl\"\n").expect("write fixture");

    wcl()
        .arg("set")
        .arg(&file)
        .arg("brand")
        .arg("\"renamed\"")
        .assert()
        .success();

    // Read back via `wcl get` — the round-trip should observe the new value.
    wcl()
        .arg("get")
        .arg(&file)
        .arg("brand")
        .assert()
        .success()
        .stdout(predicate::str::contains("renamed"));
}

#[test]
fn set_updates_nested_field_in_block() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let file = tmp.path().join("nested.wcl");
    std::fs::write(
        &file,
        "@schemaless service \"web\" {\n  port = 8080u32\n}\n",
    )
    .expect("write fixture");

    wcl()
        .arg("set")
        .arg(&file)
        .arg("service.port")
        .arg("9090u32")
        .assert()
        .success();

    wcl()
        .arg("get")
        .arg(&file)
        .arg("service.port")
        .assert()
        .success()
        .stdout(predicate::str::contains("9090"));
}

#[test]
fn set_follows_imports_to_the_right_file() {
    // Copy the imports fixture into a tempdir so we can mutate it.
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src_dir = examples_dir().join("imports");
    for entry in std::fs::read_dir(&src_dir).expect("read examples/imports") {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), tmp.path().join(entry.file_name())).expect("copy fixture");
    }
    let main_file = tmp.path().join("main.wcl");
    let shared_file = tmp.path().join("shared.wcl");
    let main_before = std::fs::read_to_string(&main_file).expect("read main");

    // `brand` is declared in shared.wcl, imported by main.wcl. Setting
    // the value via main.wcl must edit shared.wcl, not main.wcl.
    wcl()
        .arg("set")
        .arg(&main_file)
        .arg("shared.brand")
        .arg("\"renamed\"")
        .assert()
        .success();

    let main_after = std::fs::read_to_string(&main_file).expect("read main after");
    assert_eq!(
        main_before, main_after,
        "main.wcl should be unchanged when setting an imported field"
    );
    let shared_after = std::fs::read_to_string(&shared_file).expect("read shared after");
    assert!(
        shared_after.contains("brand = \"renamed\""),
        "shared.wcl should have been mutated; got:\n{shared_after}"
    );
}

#[test]
fn set_errors_on_missing_path() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let file = tmp.path().join("doc.wcl");
    std::fs::write(&file, "@schemaless name = \"alpha\"\n").expect("write fixture");
    wcl()
        .arg("set")
        .arg(&file)
        .arg("nme")
        .arg("\"new\"")
        .assert()
        .code(3)
        .stderr(predicate::str::contains("no such path"))
        .stderr(predicate::str::contains("did you mean"));
}

#[test]
fn set_errors_when_path_resolves_to_a_block() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let file = tmp.path().join("doc.wcl");
    std::fs::write(
        &file,
        "@schemaless service \"web\" {\n  port = 8080u32\n}\n",
    )
    .expect("write fixture");
    wcl()
        .arg("set")
        .arg(&file)
        .arg("service")
        .arg("9090u32")
        .assert()
        .code(3)
        .stderr(predicate::str::contains("only updates leaf field values"));
}

#[test]
fn set_errors_on_invalid_value_expression() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let file = tmp.path().join("doc.wcl");
    std::fs::write(&file, "@schemaless brand = \"wcl\"\n").expect("write fixture");
    wcl()
        .arg("set")
        .arg(&file)
        .arg("brand")
        .arg("@@@")
        .assert()
        .code(1);
}

#[test]
fn repl_evaluates_piped_expression() {
    wcl()
        .arg("repl")
        .write_stdin("1 + 2\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("3"));
}

#[test]
fn get_json_outputs_idiomatic_json() {
    // Custom `Value` Serialize impl flattens scalar variants — the
    // string value `"alpha"` should appear bare, not wrapped in
    // `{"Utf8": "alpha"}`.
    wcl()
        .arg("get")
        .arg("--json")
        .arg(examples_dir().join("basic.wcl"))
        .arg("name")
        .assert()
        .success()
        .stdout(predicate::str::contains("\"alpha\""))
        .stdout(predicate::str::contains("Utf8").not());
}

#[test]
fn repl_accepts_multiline_expression() {
    wcl()
        .arg("repl")
        .write_stdin("{\n  let a = 1;\n  let b = 2;\n  a + b\n}\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("3"));
}

#[test]
fn repl_tags_parse_errors_and_keeps_running() {
    // Two lines: a malformed expression then a valid one. The REPL
    // should print a `parse error:` tag for the first and a result
    // for the second, exiting cleanly.
    wcl()
        .arg("repl")
        .write_stdin("@@@\n1 + 2\n")
        .assert()
        .success()
        .stderr(predicate::str::contains("parse error:"))
        .stdout(predicate::str::contains("3"));
}

#[test]
fn repl_quits_on_quit_command() {
    wcl().arg("repl").write_stdin(":q\n").assert().success();
}

#[test]
fn repl_resolves_identifiers_against_open_file() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let file = tmp.path().join("doc.wcl");
    std::fs::write(&file, "@schemaless port = 9090u32\n").expect("write fixture");
    wcl()
        .arg("repl")
        .arg(&file)
        .write_stdin("port\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("9090"));
}

#[test]
fn check_reports_non_utf8_file_as_io_error() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let file = tmp.path().join("bytes.wcl");
    // Bytes that are valid neither UTF-8 nor ASCII: a lone 0xFF.
    std::fs::write(&file, [0xFFu8, 0xFE, 0xFD]).expect("write fixture");
    wcl().arg("check").arg(&file).assert().failure();
}

#[test]
fn fmt_indent_zero_strips_block_indentation() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let file = tmp.path().join("doc.wcl");
    std::fs::write(
        &file,
        "@schemaless\nservice \"web\" {\n  port = 8080u32\n}\n",
    )
    .expect("write fixture");
    let out = wcl()
        .arg("fmt")
        .arg("--indent")
        .arg("0")
        .arg(&file)
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).expect("utf8 stdout");
    // With indent=0 the field inside the block sits at column 0.
    assert!(text.contains("\nport = 8080u32"), "got: {text:?}");
}

#[test]
fn get_json_escapes_special_chars_in_strings() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let file = tmp.path().join("doc.wcl");
    // Value contains a quote, a backslash and a newline — all must be
    // JSON-escaped by the custom Value serializer.
    std::fs::write(&file, "@schemaless\ntricky = \"a\\\"b\\\\c\\nd\"\n").expect("write fixture");
    let out = wcl()
        .arg("get")
        .arg("--json")
        .arg(&file)
        .arg("tricky")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let text = String::from_utf8(out).expect("utf8 stdout");
    // Must be valid JSON that round-trips to the original string.
    let parsed: serde_json::Value = serde_json::from_str(text.trim()).expect("valid JSON");
    let s = parsed.as_str().expect("JSON string");
    assert_eq!(s, "a\"b\\c\nd");
}

#[test]
fn eval_composes_field_from_let_helpers() {
    // `service.port` is `bump(count)` where `bump` is a global `let`
    // function and `count` is a block-scoped `let` — both resolve in
    // the field expression.
    wcl()
        .arg("eval")
        .arg(examples_dir().join("lets.wcl"))
        .arg("service.port")
        .assert()
        .success()
        .stdout(predicate::str::contains("8003"));
}

#[test]
fn eval_does_not_expose_let_helpers_as_data() {
    // A global `let` is usable in expressions but not addressable as
    // document data.
    wcl()
        .arg("eval")
        .arg(examples_dir().join("lets.wcl"))
        .arg("base_port")
        .assert()
        .code(3)
        .stderr(predicate::str::contains("no such path"));
}

#[test]
fn check_ok_on_lets_example() {
    wcl()
        .arg("check")
        .arg(examples_dir().join("lets.wcl"))
        .assert()
        .success()
        .stdout(predicate::str::contains("OK"));
}
