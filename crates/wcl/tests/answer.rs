//! Integration tests for `wcl answer` — guided answer mode over
//! `@answerable` question blocks (fixture: `examples/answer/plan.wcl`).

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

/// Copy the plan fixture into a tempdir so write-back tests don't touch the
/// repo copy.
fn plan_copy() -> (TempDir, PathBuf) {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let dest = tmp.path().join("plan.wcl");
    std::fs::copy(examples_dir().join("answer/plan.wcl"), &dest).expect("copy fixture");
    (tmp, dest)
}

#[test]
fn list_reports_pending_questions_as_json() {
    let out = wcl()
        .arg("answer")
        .arg(examples_dir().join("answer/plan.wcl"))
        .arg("--list")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).expect("json output");
    let ids: Vec<&str> = v
        .as_array()
        .expect("array")
        .iter()
        .map(|q| q["id"].as_str().unwrap())
        .collect();
    // q_done is already :answered and must not be listed.
    assert_eq!(ids, ["q_platforms", "q_features", "q_name"]);
    let platforms = &v[0];
    assert_eq!(platforms["kind"], "single_select");
    assert_eq!(platforms["skippable"], true);
    assert_eq!(platforms["options"][0]["id"], "linux");
    assert_eq!(platforms["options"][0]["label"], "Linux x86_64");
    // Option label falls back to the option's id.
    assert_eq!(v[1]["options"][0]["label"], "lsp");
    assert_eq!(v[2]["kind"], "free_text");
}

#[test]
fn answer_by_id_writes_response_and_flips_status() {
    let (_tmp, plan) = plan_copy();
    wcl()
        .arg("answer")
        .arg(&plan)
        .args([
            "--id",
            "q_platforms",
            "--pick",
            "linux",
            "--text",
            "musl builds",
        ])
        .assert()
        .success()
        .stderr(predicate::str::contains("answered `q_platforms`"));
    let src = std::fs::read_to_string(&plan).expect("read back");
    assert!(src.contains("answer = \"Linux x86_64 — musl builds\""));
    // Answered → no longer pending.
    let out = wcl()
        .arg("answer")
        .arg(&plan)
        .arg("--list")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let v: serde_json::Value = serde_json::from_slice(&out).expect("json output");
    assert!(
        v.as_array()
            .unwrap()
            .iter()
            .all(|q| q["id"] != "q_platforms")
    );
    // The edited file still checks clean.
    wcl().arg("check").arg(&plan).assert().success();
}

#[test]
fn multi_select_combines_picks_and_text() {
    let (_tmp, plan) = plan_copy();
    wcl()
        .arg("answer")
        .arg(&plan)
        .args(["--id", "q_features", "--pick", "lsp", "--pick", "fmt"])
        .args(["--text", "debugger later"])
        .assert()
        .success();
    let src = std::fs::read_to_string(&plan).expect("read back");
    assert!(src.contains("answer = \"lsp, Formatter — debugger later\""));
}

#[test]
fn skip_writes_the_declared_skipped_status() {
    let (_tmp, plan) = plan_copy();
    wcl()
        .arg("answer")
        .arg(&plan)
        .args(["--id", "q_name", "--skip"])
        .assert()
        .success()
        .stderr(predicate::str::contains("skipped `q_name`"));
    let src = std::fs::read_to_string(&plan).expect("read back");
    assert!(src.contains("status = :dropped"));
}

#[test]
fn unknown_id_lists_the_pending_ones() {
    wcl()
        .arg("answer")
        .arg(examples_dir().join("answer/plan.wcl"))
        .args(["--id", "nope"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("no pending question `nope`"))
        .stderr(predicate::str::contains("q_platforms, q_features, q_name"));
}

#[test]
fn unknown_pick_lists_the_options() {
    wcl()
        .arg("answer")
        .arg(examples_dir().join("answer/plan.wcl"))
        .args(["--id", "q_platforms", "--pick", "amiga"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("no option `amiga`"))
        .stderr(predicate::str::contains("linux, mac, win"));
}

#[test]
fn single_select_rejects_multiple_picks() {
    wcl()
        .arg("answer")
        .arg(examples_dir().join("answer/plan.wcl"))
        .args(["--id", "q_platforms", "--pick", "linux", "--pick", "mac"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("single selection"));
}

#[test]
fn empty_answer_is_rejected() {
    wcl()
        .arg("answer")
        .arg(examples_dir().join("answer/plan.wcl"))
        .args(["--id", "q_name"])
        .assert()
        .code(3)
        .stderr(predicate::str::contains("empty answer"));
}

#[test]
fn interactive_line_mode_walks_all_questions() {
    let (_tmp, plan) = plan_copy();
    // q_platforms: option 2; no extra. q_features: both options + a note.
    // q_name: free text.
    wcl()
        .arg("answer")
        .arg(&plan)
        .write_stdin("2\n\n1 2\nships together\nCall it wplan\n")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "3 answered, 0 skipped, 0 left pending.",
        ));
    let src = std::fs::read_to_string(&plan).expect("read back");
    assert!(src.contains("answer = \"macOS (Apple Silicon)\""));
    assert!(src.contains("answer = \"lsp, Formatter — ships together\""));
    assert!(src.contains("answer = \"Call it wplan\""));
    wcl().arg("check").arg(&plan).assert().success();
}

#[test]
fn interactive_commands_defer_skip_and_quit() {
    let (_tmp, plan) = plan_copy();
    // :later defers q_platforms, :skip drops q_features, :quit leaves q_name
    // pending — so two questions remain pending afterwards.
    wcl()
        .arg("answer")
        .arg(&plan)
        .write_stdin(":later\n:skip\n:quit\n")
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "0 answered, 1 skipped, 2 left pending.",
        ));
    let src = std::fs::read_to_string(&plan).expect("read back");
    assert!(src.contains("status = :dropped"));
    assert!(src.contains("kind = :single_select")); // q_platforms untouched
}
