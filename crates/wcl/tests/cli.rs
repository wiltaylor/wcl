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
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("formatted "));

    let after = std::fs::read_to_string(&file).expect("read after fmt");
    assert_eq!(after, "@schemaless x = 1\n");

    // A second run finds nothing to do and says so without rewriting.
    wcl()
        .arg("fmt")
        .arg("--in-place")
        .arg(&file)
        .assert()
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("unchanged"));
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
        .success()
        .stdout(predicate::str::is_empty())
        .stderr(predicate::str::contains("updated brand in"));

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
        .success()
        // The confirmation names the *imported* file actually edited.
        .stderr(predicate::str::contains("shared.wcl"));

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
    // for the second — and, because stdin is piped, exit non-zero so
    // scripts can detect the recovered error.
    wcl()
        .arg("repl")
        .write_stdin("@@@\n1 + 2\n")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("parse error:"))
        .stdout(predicate::str::contains("3"));
}

#[test]
fn repl_piped_eval_error_sets_exit_code() {
    wcl()
        .arg("repl")
        .write_stdin("no_such_name\n")
        .assert()
        .code(3)
        .stderr(predicate::str::contains("eval error:"));
}

#[test]
fn repl_quit_after_error_still_fails_piped_session() {
    wcl()
        .arg("repl")
        .write_stdin("@@@\n:q\n")
        .assert()
        .code(1)
        .stderr(predicate::str::contains("parse error:"));
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

// ── stdin input and `check --json` ────────────────────────────────

#[test]
fn check_reads_stdin_with_dash() {
    wcl()
        .arg("check")
        .arg("-")
        .write_stdin("@schemaless name = \"x\"\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("OK"));
}

#[test]
fn check_json_reports_ok_document() {
    let out = wcl()
        .arg("check")
        .arg("--json")
        .arg("-")
        .write_stdin("@schemaless name = \"x\"\n")
        .assert()
        .success();
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(v["ok"], serde_json::json!(true));
    assert_eq!(v["file"], serde_json::json!("<stdin>"));
    assert_eq!(v["errors"], serde_json::json!([]));
}

#[test]
fn check_json_reports_parse_error_with_span() {
    let out = wcl()
        .arg("check")
        .arg("--json")
        .arg("-")
        .write_stdin("name = \u{1}\n")
        .assert()
        .failure()
        .code(1);
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(v["ok"], serde_json::json!(false));
    let err = &v["errors"][0];
    assert_eq!(err["code"], serde_json::json!("wcl::parse"));
    assert!(err["offset"].is_u64(), "span offset present: {err}");
    assert!(err["length"].is_u64(), "span length present: {err}");
}

#[test]
fn check_json_reports_schema_violations_with_exit_2() {
    let out = wcl()
        .arg("check")
        .arg("--json")
        .arg("-")
        .write_stdin("name = \"x\"\n")
        .assert()
        .failure()
        .code(2);
    let stdout = String::from_utf8_lossy(&out.get_output().stdout).to_string();
    let v: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON");
    assert_eq!(v["ok"], serde_json::json!(false));
    assert_eq!(
        v["errors"][0]["code"],
        serde_json::json!("wcl::eval::schema_violation")
    );
}

#[test]
fn fmt_reads_stdin_and_writes_stdout() {
    wcl()
        .arg("fmt")
        .arg("-")
        .write_stdin("@schemaless x = (1+2)*3\n")
        .assert()
        .success()
        .stdout(predicate::str::contains("x = (1 + 2) * 3"));
}

#[test]
fn fmt_rejects_in_place_with_stdin() {
    wcl()
        .arg("fmt")
        .arg("--in-place")
        .arg("-")
        .write_stdin("x = 1\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("--in-place"));
}

/// Shared schema + helper for the `wcl diff` integration tests.
const DIFF_SCHEMA: &str = "\
@block(\"domain_entity\") type Entity { @inline(0) id: identifier  name: utf8  status: utf8 }
@block(\"spec\") type Spec { @inline(0) id: identifier  title: utf8 }
@document type M {
  @children(\"domain_entity\") entities: list<Entity>
  @children(\"spec\") specs: list<Spec>
}
";

#[test]
fn diff_reports_added_removed_and_modified_entities() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let old = tmp.path().join("old.wcl");
    let new = tmp.path().join("new.wcl");
    std::fs::write(
        &old,
        format!(
            "{DIFF_SCHEMA}\
             domain_entity \"task\" {{ name = \"Task\"  status = \"draft\" }}\n\
             domain_entity \"user\" {{ name = \"User\"  status = \"done\" }}\n\
             spec \"auth\" {{ title = \"Auth\" }}\n"
        ),
    )
    .unwrap();
    std::fs::write(
        &new,
        format!(
            "{DIFF_SCHEMA}\
             domain_entity \"task\" {{ name = \"Task\"  status = \"active\" }}\n\
             spec \"auth\" {{ title = \"Auth\" }}\n\
             spec \"impl_due_dates\" {{ title = \"Due dates\" }}\n"
        ),
    )
    .unwrap();

    let out = wcl().arg("diff").arg(&old).arg(&new).assert().success();
    let stdout = String::from_utf8(out.get_output().stdout.clone()).unwrap();
    let json: serde_json::Value = serde_json::from_str(&stdout).expect("valid JSON array");
    let arr = json.as_array().expect("array");

    // task: status draft -> active (modified); user removed; impl_due_dates added.
    assert!(arr.iter().any(|c| c["op"] == "modified"
        && c["entity"] == "domain_entity:task"
        && c["field"] == "status"
        && c["kind"] == "changed"));
    assert!(
        arr.iter()
            .any(|c| c["op"] == "removed" && c["entity"] == "domain_entity:user")
    );
    assert!(
        arr.iter()
            .any(|c| c["op"] == "added" && c["entity"] == "spec:impl_due_dates")
    );
    // The unchanged spec:auth entity is not reported.
    assert!(!arr.iter().any(|c| c["entity"] == "spec:auth"));
}

#[test]
fn diff_ignores_formatting_only_changes() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let old = tmp.path().join("a.wcl");
    let new = tmp.path().join("b.wcl");
    let body =
        format!("{DIFF_SCHEMA}domain_entity \"task\" {{ name = \"Task\"  status = \"draft\" }}\n");
    std::fs::write(&old, &body).unwrap();
    // Same values, different whitespace / line breaks.
    std::fs::write(
        &new,
        body.replace("{ name", "{\n  name")
            .replace("  status", "\n  status"),
    )
    .unwrap();

    wcl()
        .arg("diff")
        .arg(&old)
        .arg(&new)
        .assert()
        .success()
        .stdout(predicate::str::contains("[]"));
}
