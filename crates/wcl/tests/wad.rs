//! Integration tests for `wcl wad spec` — the diff→spec half of the WAD
//! change workflow. Each test scaffolds a fresh WAD with `wcl init wad`,
//! commits it, mutates the data, and derives a spec skeleton.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::Path;
use tempfile::TempDir;

fn wcl() -> Command {
    Command::cargo_bin("wcl").expect("wcl binary built")
}

fn git(dir: &Path, args: &[&str]) {
    let ok = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .expect("run git")
        .status
        .success();
    assert!(ok, "git {args:?} failed in {}", dir.display());
}

/// Scaffold a WAD into `<tmp>/proj`, git-init and commit it.
fn committed_wad(tmp: &TempDir) -> std::path::PathBuf {
    let dest = tmp.path().join("proj");
    wcl()
        .args(["init", "wad"])
        .arg(&dest)
        .args([
            "-D",
            "system_id=demo",
            "-D",
            "system_name=Demo",
            "--defaults",
        ])
        .assert()
        .success();
    git(&dest, &["init", "-q"]);
    git(&dest, &["add", "-A"]);
    git(
        &dest,
        &[
            "-c",
            "user.email=t@example.com",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "baseline",
        ],
    );
    dest
}

/// Add a system + container to the WAD's data, wired through the view hub.
fn add_core_system(dest: &Path) {
    std::fs::write(
        dest.join("data/systems/core.wcl"),
        "namespace wcl.wad\n\
         system core { name = \"Core\"  summary = \"The core system.\" }\n\
         container api { system = core  name = \"API\"  summary = \"The API.\"  kind = :service }\n",
    )
    .expect("write core.wcl");
    let hub = dest.join("data/systems/main.wcl");
    let mut text = std::fs::read_to_string(&hub).expect("read systems hub");
    text.push_str("import \"./core.wcl\"\n");
    std::fs::write(&hub, text).expect("write systems hub");
}

/// The happy path: data added since the baseline yields a schema-valid
/// `:planning` skeleton carrying the change list and the baseline sha, and
/// the WAD still checks (and lists the spec) once the import line is added.
#[test]
fn wad_spec_generates_planning_skeleton() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let dest = committed_wad(&tmp);
    add_core_system(&dest);

    wcl()
        .current_dir(&dest)
        .args(["wad", "spec", "--from", "HEAD", "--id", "t1", "wad.wcl"])
        .assert()
        .success()
        .stdout(predicate::str::contains("data/specs/t1.wcl"));

    let spec = std::fs::read_to_string(dest.join("data/specs/t1.wcl")).expect("spec written");
    assert!(spec.contains("status  = :planning"), "{spec}");
    assert!(spec.contains("from_rev = \""), "{spec}");
    assert!(spec.contains("change \"system:core\""), "{spec}");
    assert!(spec.contains("change \"container:api\""), "{spec}");
    assert!(spec.contains("op = :added"), "{spec}");
    // The intent skeleton is typed fields (schema 0.4.0), not a prose body.
    assert!(spec.contains("context = \"TODO"), "{spec}");
    assert!(spec.contains("instructions = ["), "{spec}");
    assert!(spec.contains("acceptance = ["), "{spec}");
    assert!(!spec.contains("body {"), "{spec}");

    // Wire the import and prove the whole model still validates — this is
    // the contract lock between the tool's output and the WAD schema.
    let hub = dest.join("data/specs/main.wcl");
    let mut text = std::fs::read_to_string(&hub).expect("read specs hub");
    text.push_str("import \"./t1.wcl\"\n");
    std::fs::write(&hub, text).expect("write specs hub");
    wcl()
        .current_dir(&dest)
        .args(["check", "wad.wcl"])
        .assert()
        .success();
}

/// A modified field produces a `field_change` row carrying old/new values.
#[test]
fn wad_spec_reports_field_changes() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let dest = committed_wad(&tmp);
    add_core_system(&dest);
    git(&dest, &["add", "-A"]);
    git(
        &dest,
        &[
            "-c",
            "user.email=t@example.com",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            "core",
        ],
    );
    let core = dest.join("data/systems/core.wcl");
    let text = std::fs::read_to_string(&core)
        .expect("read core.wcl")
        .replace("The API.", "The public API.");
    std::fs::write(&core, text).expect("write core.wcl");

    wcl()
        .current_dir(&dest)
        .args(["wad", "spec", "--from", "HEAD", "--id", "t2", "wad.wcl"])
        .assert()
        .success();
    let spec = std::fs::read_to_string(dest.join("data/specs/t2.wcl")).expect("spec written");
    assert!(spec.contains("op = :modified"), "{spec}");
    assert!(spec.contains("field_change \"summary\""), "{spec}");
    assert!(spec.contains("old = \"\\\"The API.\\\"\""), "{spec}");
    assert!(spec.contains("new = \"\\\"The public API.\\\"\""), "{spec}");
}

/// An existing spec file is never overwritten.
#[test]
fn wad_spec_refuses_overwrite() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let dest = committed_wad(&tmp);
    add_core_system(&dest);
    wcl()
        .current_dir(&dest)
        .args(["wad", "spec", "--from", "HEAD", "--id", "t3", "wad.wcl"])
        .assert()
        .success();
    wcl()
        .current_dir(&dest)
        .args(["wad", "spec", "--from", "HEAD", "--id", "t3", "wad.wcl"])
        .assert()
        .code(4)
        .stderr(predicate::str::contains("refusing to overwrite"));
}

/// No changes since the baseline → no file, friendly message, success.
#[test]
fn wad_spec_empty_diff_writes_nothing() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let dest = committed_wad(&tmp);
    wcl()
        .current_dir(&dest)
        .args(["wad", "spec", "--from", "HEAD", "wad.wcl"])
        .assert()
        .success()
        .stderr(predicate::str::contains("no changes since HEAD"));
    let specs: Vec<_> = std::fs::read_dir(dest.join("data/specs"))
        .expect("read specs dir")
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().starts_with("spec_"))
        .collect();
    assert!(specs.is_empty(), "no skeleton expected: {specs:?}");
}
