//! Integration tests for `wcl wskill` — the model on stdout (`graph`), its
//! findings (`lint`) and the structural writes (`op`), with no editor and no
//! build. Each test scaffolds a real wskill with `wcl init wskill`, so the
//! assertions run against the shipped base schema rather than a hand-written
//! miniature.

use std::path::{Path, PathBuf};

use assert_cmd::Command;
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

/// Scaffold a wskill into `<tmp>/topic` and author two concepts into its
/// reference file, one of them pinned into an index.
fn scaffolded_wskill(tmp: &TempDir) -> PathBuf {
    let dest = tmp.path().join("topic");
    wcl()
        .args(["init", "wskill"])
        .arg(&dest)
        .args(["-D", "topic_id=demo", "-D", "topic_name=Demo", "--defaults"])
        .assert()
        .success();
    write_reference(&dest, "[alpha]", "Beta");
    dest
}

fn scaffold_named_wskill(parent: &Path, id: &str) -> PathBuf {
    let dest = parent.join(id);
    wcl()
        .args(["init", "wskill"])
        .arg(&dest)
        .args([
            "-D",
            &format!("topic_id={id}"),
            "-D",
            &format!("topic_name={id}"),
            "--defaults",
        ])
        .assert()
        .success();
    dest
}

fn append_agent(dest: &Path, name: &str) {
    let path = dest.join("wskill.wcl");
    let mut source = std::fs::read_to_string(&path).unwrap();
    source.push_str(&format!(
        "\nagent \"{name}\" {{\n  description = \"A generated test agent.\"\n  body {{ p \"Do the test task.\" }}\n}}\n"
    ));
    std::fs::write(path, source).unwrap();
}

/// Rewrite the scaffold's reference file with the two concepts and the index,
/// parameterised by what the index pins and what beta is called — the two
/// things the revision test changes between commits.
fn write_reference(dest: &Path, pinned: &str, beta_name: &str) {
    write_units(
        dest,
        &format!(
            "concept beta {{\n  name    = \"{beta_name}\"\n  summary = \"The second idea.\"\n}}\n\n\
             index reference {{\n  name    = \"Reference\"\n  summary = \"Everything, pinned.\"\n  \
             related = {pinned}\n}}\n"
        ),
    );
}

/// Rewrite the reference file as `alpha` plus whatever the test authors,
/// keeping the scaffold's authoring guide above it. `alpha` is always there
/// so the guide stays findable however often the file is rewritten.
fn write_units(dest: &Path, units: &str) {
    let path = dest.join("data/reference/reference.wcl");
    let template = std::fs::read_to_string(&path).unwrap();
    let guide = template
        .split("\nconcept alpha {")
        .next()
        .unwrap()
        .to_string();
    std::fs::write(
        &path,
        format!(
            "{guide}\nconcept alpha {{\n  name    = \"Alpha\"\n  summary = \"The first idea.\"\n\n  \
             body {{\n    p \"Alpha explained.\"\n  }}\n}}\n\n{units}"
        ),
    )
    .unwrap();
}

/// Turn `dir` into a git repo with everything in it committed.
fn git_init(dir: &Path) {
    git(dir, &["init", "-q"]);
    commit(dir, "baseline");
}

fn commit(dir: &Path, message: &str) {
    git(dir, &["add", "-A"]);
    git(
        dir,
        &[
            "-c",
            "user.email=t@example.com",
            "-c",
            "user.name=t",
            "commit",
            "-qm",
            message,
        ],
    );
}

fn graph_of(args: &[&str]) -> serde_json::Value {
    let out = wcl().args(["wskill", "graph"]).args(args).output().unwrap();
    assert!(
        out.status.success(),
        "wcl wskill graph {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("stdout is JSON")
}

#[test]
fn graph_emits_the_model_as_json() {
    let tmp = TempDir::new().unwrap();
    let dest = scaffolded_wskill(&tmp);
    let g = graph_of(&[dest.to_str().unwrap()]);

    assert_eq!(g["topic"]["id"], "demo");
    assert_eq!(g["entry"], "wskill.wcl");
    assert_eq!(g["rev"], serde_json::Value::Null);

    // The registry's projections, each with the site name its entry declares.
    let views: Vec<(&str, &str)> = g["views"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| (v["kind"].as_str().unwrap(), v["site"].as_str().unwrap()))
        .collect();
    assert_eq!(views, [("book", "book"), ("ai_skill", "skill")]);

    // Units carry their kind, title, audience and the file + span they're
    // written at.
    let unit = |id: &str| {
        g["units"]
            .as_array()
            .unwrap()
            .iter()
            .find(|u| u["id"] == id)
            .unwrap_or_else(|| panic!("no unit `{id}` in {g:#}"))
            .clone()
    };
    let alpha = unit("alpha");
    assert_eq!(alpha["kind"], "concept");
    assert_eq!(alpha["title"], "Alpha");
    assert_eq!(alpha["audience"], "book");
    assert_eq!(alpha["anchor"]["file"], "data/reference/reference.wcl");
    assert!(alpha["anchor"]["span"]["end"].as_u64().unwrap() > 0);
    assert_eq!(alpha["related_editable"], true);
    assert_eq!(alpha["blocks"][0]["kind"], "p");
    assert_eq!(alpha["blocks"][0]["preview"], "Alpha explained.");

    // The index pins one of them — and the other is reported unindexed.
    let index = &g["indexes"][0];
    assert_eq!(index["id"], "reference");
    assert_eq!(index["pinned"], serde_json::json!(["alpha"]));
    assert!(
        g["edges"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e["kind"] == "pin"
                && e["from"] == "index:reference"
                && e["to"] == "concept:alpha"),
        "{g:#}"
    );
    assert_eq!(g["unindexed"], serde_json::json!(["beta"]));
}

/// The point of loading at a revision: a baseline to diff the working tree
/// against, read with no checkout of its own.
#[test]
fn graph_reads_the_model_at_a_git_revision() {
    let tmp = TempDir::new().unwrap();
    let dest = scaffolded_wskill(&tmp);
    git_init(&dest);
    // Diverge the working tree: pin beta too, and rename it.
    write_reference(&dest, "[alpha, beta]", "Beta, renamed");

    let dest_str = dest.to_str().unwrap();
    let before = graph_of(&[dest_str, "--rev", "HEAD"]);
    let after = graph_of(&[dest_str]);

    assert_eq!(before["rev"].as_str().map(str::len), Some(40));
    assert_eq!(before["indexes"][0]["pinned"], serde_json::json!(["alpha"]));
    assert_eq!(before["unindexed"], serde_json::json!(["beta"]));
    assert_eq!(
        after["indexes"][0]["pinned"],
        serde_json::json!(["alpha", "beta"])
    );
    assert_eq!(after["unindexed"], serde_json::json!([]));
    // Anchors are wskill-relative on both sides, so they compare directly.
    assert_eq!(before["root"], after["root"]);

    // A revision that doesn't exist is an error, not an empty model.
    wcl()
        .args(["wskill", "graph", dest_str, "--rev", "no-such-rev"])
        .assert()
        .failure();
}

/// `lint` over a scaffolded wskill: the shipped base schema, one authored
/// error, and the exit codes a CI job and the curator both key off.
#[test]
fn lint_reports_findings_as_text_and_json() {
    let tmp = TempDir::new().unwrap();
    let dest = scaffolded_wskill(&tmp);
    let dest_str = dest.to_str().unwrap();

    // As scaffolded and authored: `beta` is pinned by no index (a warning)
    // and the index carries no body (a candidate). Warnings never fail.
    let out = wcl().args(["wskill", "lint", dest_str]).output().unwrap();
    assert_eq!(out.status.code(), Some(0), "warnings must not fail");
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains("warn [unindexed] concept:beta"),
        "expected the unpinned unit: {text}"
    );
    assert!(
        text.contains("data/reference/reference.wcl:"),
        "a finding names the file and line it is written at: {text}"
    );
    // The summary goes to stderr, so it survives a pipe into grep.
    let summary = String::from_utf8(out.stderr).unwrap();
    assert!(summary.contains("0 errors, 1 warning,"), "{summary}");

    // `--deny warn` escalates exactly those warnings.
    wcl()
        .args(["wskill", "lint", dest_str, "--deny", "warn"])
        .assert()
        .failure()
        .code(1);

    // Now author a `related` id naming nothing: an error, so exit 1.
    write_reference(&dest, "[alpha]", "Beta");
    let path = dest.join("data/reference/reference.wcl");
    let src = std::fs::read_to_string(&path).unwrap();
    std::fs::write(
        &path,
        src.replace(
            "concept beta {\n  name    = \"Beta\"",
            "concept beta {\n  related = [nobody]\n  name    = \"Beta\"",
        ),
    )
    .unwrap();

    let out = wcl()
        .args(["wskill", "lint", dest_str, "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "an error fails the run");
    let findings: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap();
    let dangling = findings
        .iter()
        .find(|f| f["rule"] == "dangling-related")
        .unwrap_or_else(|| panic!("no dangling finding in {findings:#?}"));
    assert_eq!(dangling["severity"], "error");
    assert_eq!(
        dangling["unit"],
        serde_json::json!({"kind": "concept", "id": "beta"})
    );
    assert_eq!(dangling["file"], "data/reference/reference.wcl");
    assert!(dangling["span"]["start"].is_number());
    assert!(dangling["message"].as_str().unwrap().contains("nobody"));

    // The curator's phase 1 asks for candidates only — and gets exit 0,
    // because a severity it filtered out cannot fail its run.
    let out = wcl()
        .args([
            "wskill",
            "lint",
            dest_str,
            "--format",
            "json",
            "--severity",
            "candidate",
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    let findings: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert!(!findings.is_empty());
    assert!(findings.iter().all(|f| f["severity"] == "candidate"));
}

// ---------------------------------------------------------------------------
// `op` — the id-addressed write vocabulary
// ---------------------------------------------------------------------------

/// The `related` line of the block `header` opens — the index's pin list, or
/// a unit's own links, both of which the ops rewrite in place.
fn related_line(dest: &Path, header: &str) -> String {
    let text = std::fs::read_to_string(dest.join("data/reference/reference.wcl")).unwrap();
    let at = text
        .find(header)
        .unwrap_or_else(|| panic!("no `{header}` in:\n{text}"));
    text[at..]
        .lines()
        .find(|l| l.trim_start().starts_with("related ="))
        .unwrap_or_else(|| panic!("no `related` line under `{header}` in:\n{text}"))
        .trim()
        .to_string()
}

/// The index's pin list — what every pin, unpin and reorder op rewrites.
fn pinned_line(dest: &Path) -> String {
    related_line(dest, "index reference {")
}

/// The vocabulary applies from the command line, in the same JSON the editor
/// sends — a bare id where it is unambiguous, a `kind:id` where the caller
/// wants to be sure — and a batch is one command.
#[test]
fn op_applies_the_vocabulary_and_a_dry_run_writes_nothing() {
    let tmp = TempDir::new().unwrap();
    let dest = scaffolded_wskill(&tmp);
    let dest_str = dest.to_str().unwrap();
    assert_eq!(pinned_line(&dest), "related = [alpha]");

    // A dry run prints the ops it would apply and touches nothing.
    let before = tree_snapshot(&dest);
    let out = wcl()
        .args(["wskill", "op", dest_str, "--dry-run"])
        .args([
            "--op",
            r#"{"op":"pin_unit","index":"reference","unit":"beta"}"#,
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(before, tree_snapshot(&dest), "a dry run writes nothing");
    let printed: Vec<serde_json::Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        printed,
        vec![serde_json::json!({
            "op": "pin_unit", "index": "index:reference", "unit": "beta",
        })],
        "the op list is the vocabulary's own JSON"
    );

    // Applied for real, as a batch piped in — the printed ops decode back.
    let out = wcl()
        .args(["wskill", "op", dest_str])
        .write_stdin(
            r#"[{"op":"pin_unit","index":"reference","unit":"concept:beta"},
                {"op":"reorder_children","index":"reference","order":["beta","alpha"]}]"#,
        )
        .output()
        .unwrap();
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(pinned_line(&dest), "related = [beta, alpha]");
    let applied: Vec<serde_json::Value> = String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).expect("each applied op is JSON"))
        .collect();
    assert_eq!(applied.len(), 2, "one line per applied op");
    assert_eq!(applied[0]["unit"], "concept:beta");

    // And back the other way, through the other half of the vocabulary.
    wcl()
        .args(["wskill", "op", dest_str])
        .args([
            "--op",
            r#"{"op":"unpin_unit","index":"reference","unit":"beta"}"#,
        ])
        .args(["--op", r#"{"op":"related_add","from":"alpha","to":"beta"}"#])
        .assert()
        .success();
    assert_eq!(pinned_line(&dest), "related = [alpha]");
    assert_eq!(
        related_line(&dest, "concept alpha {"),
        "related = [beta]",
        "the link landed on `alpha` itself"
    );
}

/// Ops apply one commit at a time and stop at the first refusal, saying how
/// far they got — a curator resumes rather than re-runs blind.
#[test]
fn op_stops_at_a_refusal_and_reports_how_far_it_got() {
    let tmp = TempDir::new().unwrap();
    let dest = scaffolded_wskill(&tmp);

    let out = wcl()
        .args(["wskill", "op", dest.to_str().unwrap()])
        .write_stdin(
            r#"[{"op":"unpin_unit","index":"reference","unit":"alpha"},
                {"op":"unpin_unit","index":"reference","unit":"nobody"},
                {"op":"pin_unit","index":"reference","unit":"beta"}]"#,
        )
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1), "a refusal fails the run");
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(
        err.contains("op 2:") && err.contains("not pinned here"),
        "{err}"
    );
    assert!(err.contains("applied 1 of 3 ops"), "{err}");
    // The first op stands; the third never ran.
    assert_eq!(pinned_line(&dest), "related = []");

    // A kind the caller names is checked, not echoed back: pinning
    // `research:beta` must not quietly pin the concept of that id.
    let out = wcl()
        .args(["wskill", "op", dest.to_str().unwrap()])
        .args([
            "--op",
            r#"{"op":"pin_unit","index":"reference","unit":"research:beta"}"#,
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("`beta` is a `concept`, not a `research`")
    );
    assert_eq!(pinned_line(&dest), "related = []");

    // A malformed op is a tool failure, and nothing is attempted: the whole
    // batch decodes before the first write.
    let before = tree_snapshot(&dest);
    let out = wcl()
        .args(["wskill", "op", dest.to_str().unwrap()])
        .write_stdin(
            r#"[{"op":"pin_unit","index":"reference","unit":"beta"},
                {"op":"pin_unit","index":"reference"}]"#,
        )
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("op 2: missing `unit`"),
    );
    assert_eq!(before, tree_snapshot(&dest), "nothing was applied");
}

/// Every op goes through the editor's validating commit pipeline, so an edit
/// the schema rejects is rolled back rather than left on disk.
#[test]
fn op_rolls_back_an_edit_the_schema_rejects() {
    let tmp = TempDir::new().unwrap();
    let dest = scaffolded_wskill(&tmp);
    // A topic-declared unit kind whose links are constrained, so emptying
    // them is a violation. The base schema ships embedded and is never
    // edited; `schema/extensions.wcl` is where a wskill adds its own.
    let extensions = dest.join("schema/extensions.wcl");
    let src = std::fs::read_to_string(&extensions).unwrap();
    std::fs::write(
        &extensions,
        format!(
            "{src}\n@block(\"gadget\")\ntype Gadget {{\n  @inline(0) id: identifier\n  \
             name: utf8\n  summary: utf8\n  @non_empty related: list<identifier>\n}}\n\n\
             @document\ntype Extensions {{\n  @children(\"gadget\") gadgets: list<Gadget>\n}}\n"
        ),
    )
    .unwrap();
    write_units(
        &dest,
        "gadget widget {\n  name    = \"Widget\"\n  summary = \"A gadget.\"\n  \
         related = [alpha]\n}\n",
    );

    let out = wcl()
        .args(["wskill", "op", dest.to_str().unwrap()])
        .args([
            "--op",
            r#"{"op":"related_remove","from":"widget","to":"alpha"}"#,
        ])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let err = String::from_utf8(out.stderr).unwrap();
    assert!(
        err.contains("non_empty"),
        "the constraint is reported: {err}"
    );
    assert_eq!(
        related_line(&dest, "gadget widget {"),
        "related = [alpha]",
        "the rejected edit was rolled back"
    );
}

/// Ops target the wskill root even when the caller names a projection entry:
/// the curator edits the format, not one view of it.
#[test]
fn op_targets_the_wskill_root_from_a_projection_entry() {
    let tmp = TempDir::new().unwrap();
    let dest = scaffolded_wskill(&tmp);

    wcl()
        .args([
            "wskill",
            "op",
            dest.join("wdoc/book/main.wcl").to_str().unwrap(),
        ])
        .args([
            "--op",
            r#"{"op":"unpin_unit","index":"reference","unit":"alpha"}"#,
        ])
        .assert()
        .success();
    assert_eq!(pinned_line(&dest), "related = []");

    // A new index lands in the file the caller names, read relative to the
    // wskill root — so an op list means the same thing wherever it is run.
    wcl()
        .args(["wskill", "op", dest.to_str().unwrap()])
        .args([
            "--op",
            r#"{"op":"create_index","id":"howto","name":"How to","file":"data/reference/reference.wcl"}"#,
        ])
        .assert()
        .success();
    assert!(
        std::fs::read_to_string(dest.join("data/reference/reference.wcl"))
            .unwrap()
            .contains("index howto"),
    );

    // Outside a wskill there is nothing to target, and that is a tool
    // failure — a dry run says so too, rather than approving ops against a
    // target that isn't there.
    for args in [vec!["--dry-run"], vec![]] {
        wcl()
            .args(["wskill", "op", tmp.path().to_str().unwrap()])
            .args([
                "--op",
                r#"{"op":"unpin_unit","index":"reference","unit":"alpha"}"#,
            ])
            .args(args)
            .assert()
            .failure()
            .code(2);
    }
}

/// A wskill that cannot be read is a tool failure (2), not a finding (1) —
/// the caller's "did it pass?" must not depend on how it failed.
#[test]
fn lint_reports_an_unreadable_wskill_as_a_tool_failure() {
    let tmp = TempDir::new().unwrap();
    wcl()
        .args(["wskill", "lint", tmp.path().join("nope").to_str().unwrap()])
        .assert()
        .failure()
        .code(2);
}

/// Lint never writes: not to the wskill, not to a cache beside it.
#[test]
fn lint_writes_nothing() {
    let tmp = TempDir::new().unwrap();
    let dest = scaffolded_wskill(&tmp);
    let before = tree_snapshot(&dest);
    wcl()
        .args(["wskill", "lint", dest.to_str().unwrap()])
        .assert()
        .success();
    assert_eq!(before, tree_snapshot(&dest));
}

/// `check` is the expensive wskill gate: it reads artifact entries from the
/// parsed registry, builds every declared projection, and reports model
/// coverage without leaving generated output in the wskill.
#[test]
fn check_builds_declared_artifacts_and_reports_coverage() {
    let tmp = TempDir::new().unwrap();
    let dest = scaffolded_wskill(&tmp);
    let before = tree_snapshot(&dest);

    let out = wcl()
        .args(["wskill", "check", dest.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "check failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("checked 2 artifacts"), "{text}");
    assert!(text.contains("coverage book:"), "{text}");
    assert!(text.contains("coverage ai_skill:"), "{text}");
    assert_eq!(before, tree_snapshot(&dest), "check must not write output");
}

/// Artifact resolution comes from the parsed model. Whitespace that defeats
/// the old grep still resolves, while a genuinely missing entry is a tool
/// failure with the artifact identity in the diagnostic.
#[test]
fn check_resolves_artifacts_from_the_parsed_model() {
    let tmp = TempDir::new().unwrap();
    let dest = scaffolded_wskill(&tmp);
    let registry = dest.join("wskill.wcl");
    let source = std::fs::read_to_string(&registry).unwrap();
    std::fs::write(
        &registry,
        source.replace(
            "entry = \"wdoc/book/main.wcl\"",
            "entry\n    =\n    \"wdoc/book/main.wcl\"",
        ),
    )
    .unwrap();
    wcl()
        .args(["wskill", "check", dest.to_str().unwrap()])
        .assert()
        .success();

    let source = std::fs::read_to_string(&registry).unwrap();
    std::fs::write(
        &registry,
        source.replace("wdoc/book/main.wcl", "wdoc/book/missing.wcl"),
    )
    .unwrap();
    let out = wcl()
        .args(["wskill", "check", dest.to_str().unwrap()])
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(2));
    let error = String::from_utf8(out.stderr).unwrap();
    assert!(error.contains("artifact `book`"), "{error}");
    assert!(error.contains("wdoc/book/missing.wcl"), "{error}");
}

#[test]
fn check_accepts_an_entry_file_inside_the_wskill() {
    let tmp = TempDir::new().unwrap();
    let dest = scaffolded_wskill(&tmp);
    wcl()
        .args(["wskill", "check"])
        .arg(dest.join("wdoc/book/main.wcl"))
        .assert()
        .success();
}

/// Install renders from the wskill source straight into the canonical
/// repository locations. Its check mode is read-only and detects both skill
/// and agent drift.
#[test]
fn install_writes_skills_and_agents_and_check_detects_drift() {
    let tmp = TempDir::new().unwrap();
    let collection = tmp.path().join("wskills");
    std::fs::create_dir(&collection).unwrap();
    let dest = scaffold_named_wskill(&collection, "demo");
    append_agent(&dest, "demo-helper");
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();

    wcl()
        .args(["wskill", "install"])
        .arg(&collection)
        .args(["--repo"])
        .arg(&repo)
        .assert()
        .success();
    let skill = repo.join(".claude/skills/demo/SKILL.md");
    let agent = repo.join(".claude/agents/demo-helper.md");
    assert!(skill.is_file(), "skill was not installed");
    assert!(agent.is_file(), "agent was not installed");
    wcl()
        .args(["wskill", "install"])
        .arg(&collection)
        .args(["--repo"])
        .arg(&repo)
        .arg("--check")
        .assert()
        .success();

    std::fs::write(&skill, "drifted\n").unwrap();
    let before = tree_snapshot(&repo);
    let out = wcl()
        .args(["wskill", "install"])
        .arg(&collection)
        .args(["--repo"])
        .arg(&repo)
        .arg("--check")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("skill artifact drift"),
        "check should explain the drift"
    );
    assert_eq!(
        before,
        tree_snapshot(&repo),
        "--check must not repair drift"
    );
}

/// Collection check mode owns the complete generated set, so it can catch
/// outputs whose producing wskill disappeared without treating hand-authored
/// skills or agents as generated artifacts.
#[test]
fn install_check_detects_stale_generated_output() {
    let tmp = TempDir::new().unwrap();
    let collection = tmp.path().join("wskills");
    std::fs::create_dir(&collection).unwrap();
    scaffold_named_wskill(&collection, "demo");
    let repo = tmp.path().join("repo");
    let stale_skill = repo.join(".claude/skills/stale");
    let agents = repo.join(".claude/agents");
    std::fs::create_dir_all(&stale_skill).unwrap();
    std::fs::create_dir_all(&agents).unwrap();
    std::fs::write(
        stale_skill.join("SKILL.md"),
        "---\nname: stale\nwskill_schema_version: 1.3.0\n---\n",
    )
    .unwrap();
    std::fs::write(
        agents.join("stale-agent.md"),
        format!("{}\nstale\n", wcl_wdoc::GENERATED_AGENT_MARKER),
    )
    .unwrap();
    std::fs::write(agents.join("README.md"), "hand-authored\n").unwrap();
    std::fs::write(agents.join("hand-authored.md"), "hand-authored\n").unwrap();

    let out = wcl()
        .args(["wskill", "install"])
        .arg(&collection)
        .args(["--repo"])
        .arg(&repo)
        .arg("--check")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let error = String::from_utf8(out.stderr).unwrap();
    assert!(error.contains("stale generated skill"), "{error}");
    assert!(error.contains("stale agent"), "{error}");
    assert!(!error.contains("README.md"), "{error}");
    assert!(!error.contains("hand-authored.md"), "{error}");

    wcl()
        .args(["wskill", "install"])
        .arg(&collection)
        .args(["--repo"])
        .arg(&repo)
        .assert()
        .success();
    assert!(!stale_skill.exists(), "stale generated skill is removed");
    assert!(
        !agents.join("stale-agent.md").exists(),
        "stale generated agent is removed"
    );
    assert!(
        agents.join("hand-authored.md").is_file(),
        "hand-authored agent is preserved"
    );
}

/// Agents install into one flat namespace. Detect the collision before any
/// destination is touched, in normal and check modes alike.
#[test]
fn install_refuses_agent_name_collisions() {
    let tmp = TempDir::new().unwrap();
    let collection = tmp.path().join("wskills");
    std::fs::create_dir(&collection).unwrap();
    let alpha = scaffold_named_wskill(&collection, "alpha");
    let beta = scaffold_named_wskill(&collection, "beta");
    append_agent(&alpha, "shared-helper");
    append_agent(&beta, "shared-helper");
    let repo = tmp.path().join("repo");
    std::fs::create_dir(&repo).unwrap();

    let out = wcl()
        .args(["wskill", "install"])
        .arg(&collection)
        .args(["--repo"])
        .arg(&repo)
        .arg("--check")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let error = String::from_utf8(out.stderr).unwrap();
    assert!(
        error.contains("agent name collision: shared-helper"),
        "{error}"
    );
    assert!(
        !repo.join(".claude").exists(),
        "collision must write nothing"
    );
}

/// The reference file of the authoring commit the audit tests review: `beta`
/// deleted, two unpinned concepts written in its place, one of them linking
/// back to `alpha`.
const AUTHORED: &str = "concept gamma {\n  name    = \"Gamma\"\n  \
     summary = \"The third idea.\"\n  related = [alpha]\n}\n\n\
     concept delta {\n  name    = \"Delta\"\n  summary = \"The fourth idea.\"\n}\n\n\
     index reference {\n  name    = \"Reference\"\n  summary = \"Everything, pinned.\"\n  \
     related = [alpha]\n}\n";

/// The same file before it: `beta`, linking to `alpha` and pinned by nothing.
const BASELINE: &str = "concept beta {\n  name    = \"Beta\"\n  \
     summary = \"The second idea.\"\n  related = [alpha]\n}\n\n\
     index reference {\n  name    = \"Reference\"\n  summary = \"Everything, pinned.\"\n  \
     related = [alpha]\n}\n";

fn audit_out(args: &[&str]) -> std::process::Output {
    let out = wcl().args(["wskill", "audit"]).args(args).output().unwrap();
    assert!(
        out.status.success(),
        "wcl wskill audit {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    out
}

/// A real authoring commit — units added *and* deleted — read back as the
/// union graph, which is the whole reason an audit is not the after-state.
#[test]
fn audit_reports_the_union_of_both_revisions() {
    let tmp = TempDir::new().unwrap();
    let dest = scaffolded_wskill(&tmp);
    write_units(&dest, BASELINE);
    git_init(&dest);
    write_units(&dest, AUTHORED);
    commit(&dest, "atomize the reference");

    let out = audit_out(&[dest.to_str().unwrap(), "--range", "HEAD~1..HEAD"]);
    let text = String::from_utf8(out.stdout).unwrap();

    // The header: counts, then the metrics that moved the wrong way.
    assert!(text.contains("units +2 -1"), "{text}");
    assert!(text.contains("edges +1 -1"), "{text}");
    assert!(
        text.contains("worse   units no index pins 1 → 2"),
        "health is header material, not a report: {text}"
    );

    // The deletion — and the edge that died with it. Neither exists in the
    // after-state, so neither is in a live graph.
    assert!(text.contains("- concept:beta \"Beta\""), "{text}");
    assert!(text.contains("- related → concept:alpha"), "{text}");

    // The addition, carrying the finding the range gave it. Every unit of
    // the measured commit landed unpinned and nothing said so at the time;
    // this is the line that says so.
    assert!(text.contains("+ concept:gamma \"Gamma\""), "{text}");
    assert!(text.contains("warn [unindexed]"), "{text}");
    assert!(text.contains("+ related → concept:alpha"), "{text}");

    // `alpha` and the index were not touched and are not news — findings are
    // scoped to the range, not to the corpus.
    assert!(!text.contains("concept:alpha \""), "{text}");
    let summary = String::from_utf8(out.stderr).unwrap();
    assert!(summary.contains("3 nodes changed, 2 findings"), "{summary}");
}

/// The default range is the previous commit against the working tree, so an
/// agent's output can be audited before it is committed.
#[test]
fn audit_defaults_to_the_previous_commit_and_the_working_tree() {
    let tmp = TempDir::new().unwrap();
    let dest = scaffolded_wskill(&tmp);
    write_units(&dest, BASELINE);
    git_init(&dest);
    write_units(&dest, AUTHORED);
    commit(&dest, "atomize the reference");
    // …and one more unit the agent has not committed yet.
    write_units(
        &dest,
        &format!(
            "{AUTHORED}\nconcept epsilon {{\n  name    = \"Epsilon\"\n  \
                  summary = \"The fifth idea.\"\n}}\n"
        ),
    );

    let text = String::from_utf8(audit_out(&[dest.to_str().unwrap()]).stdout).unwrap();
    assert!(text.contains("(working tree)"), "{text}");
    // The baseline is HEAD~1 and the other side is the tree as it stands, so
    // both the committed authoring and the uncommitted unit are in scope.
    assert!(text.contains("units +3 -1"), "{text}");
    assert!(text.contains("- concept:beta \"Beta\""), "{text}");
    assert!(text.contains("+ concept:epsilon \"Epsilon\""), "{text}");
}

/// `a...b` starts where the branches diverged: reviewing a topic branch must
/// not report the trunk's own work as the branch's doing.
#[test]
fn audit_starts_a_three_dot_range_at_the_merge_base() {
    let tmp = TempDir::new().unwrap();
    let dest = scaffolded_wskill(&tmp);
    write_units(&dest, BASELINE);
    git_init(&dest);
    git(&dest, &["checkout", "-b", "base", "-q"]);
    git(&dest, &["checkout", "-b", "topic", "-q"]);
    write_units(&dest, AUTHORED);
    commit(&dest, "atomize the reference");
    // Meanwhile, the trunk renames `beta`.
    git(&dest, &["checkout", "base", "-q"]);
    write_reference(&dest, "[alpha]", "Beta, renamed on the trunk");
    commit(&dest, "meanwhile, on the trunk");
    git(&dest, &["checkout", "topic", "-q"]);

    let dest_str = dest.to_str().unwrap();
    let branch =
        String::from_utf8(audit_out(&[dest_str, "--range", "base...HEAD"]).stdout).unwrap();
    assert!(branch.contains("- concept:beta \"Beta\""), "{branch}");
    assert!(
        !branch.contains("renamed on the trunk"),
        "the trunk's rename is not the branch's doing: {branch}"
    );

    // The two-dot range is the other question — what this branch differs
    // from the trunk's tip by — and answers it differently on purpose.
    let tips = String::from_utf8(audit_out(&[dest_str, "--range", "base..HEAD"]).stdout).unwrap();
    assert!(tips.contains("renamed on the trunk"), "{tips}");
}

/// The wire shape the editor's audit view renders.
#[test]
fn audit_emits_the_union_graph_as_json() {
    let tmp = TempDir::new().unwrap();
    let dest = scaffolded_wskill(&tmp);
    write_units(&dest, BASELINE);
    git_init(&dest);
    write_units(&dest, AUTHORED);
    commit(&dest, "atomize the reference");

    let out = audit_out(&[
        dest.to_str().unwrap(),
        "--range",
        "HEAD~1..HEAD",
        "--format",
        "json",
    ]);
    let a: serde_json::Value = serde_json::from_slice(&out.stdout).expect("stdout is JSON");

    assert_eq!(a["entry"], "wskill.wcl");
    assert_eq!(a["before"].as_str().map(str::len), Some(40));
    assert_eq!(a["after"].as_str().map(str::len), Some(40));
    assert_eq!(
        a["summary"],
        serde_json::json!({
            "units": {"added": 2, "removed": 1, "modified": 0},
            "indexes": {"added": 0, "removed": 0, "modified": 0},
            "edges": {"added": 1, "removed": 1},
        })
    );

    let node = |id: &str| {
        a["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == id)
            .unwrap_or_else(|| panic!("no node `{id}` in {a:#}"))
            .clone()
    };
    // Both revisions' nodes are here, each marked — including the ones the
    // range left alone, because the view draws a graph and not a list.
    assert_eq!(node("beta")["change"], "removed");
    assert_eq!(node("beta")["kind"], "concept");
    assert_eq!(node("gamma")["change"], "added");
    assert_eq!(node("gamma")["findings"][0]["rule"], "unindexed");
    assert_eq!(node("alpha")["change"], "unchanged");
    assert_eq!(node("alpha")["findings"], serde_json::json!([]));
    assert_eq!(node("reference")["kind"], "index");

    let edge = |from: &str, to: &str| {
        a["edges"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["from"] == from && e["to"] == to)
            .unwrap_or_else(|| panic!("no edge {from} → {to} in {a:#}"))
            .clone()
    };
    assert_eq!(edge("concept:beta", "concept:alpha")["change"], "removed");
    assert_eq!(edge("concept:gamma", "concept:alpha")["change"], "added");
    assert_eq!(
        edge("index:reference", "concept:alpha")["change"],
        "unchanged"
    );

    let health = a["health"].as_array().unwrap();
    let unindexed = health
        .iter()
        .find(|m| m["key"] == "unindexed_units")
        .unwrap();
    assert_eq!(unindexed["before"], 1.0);
    assert_eq!(unindexed["after"], 2.0);
    assert_eq!(unindexed["worse"], true);
}

/// Auditing the commit that *created* a wskill is the commonest review of
/// all — the whole folder is the range's output — and its baseline
/// necessarily predates the folder. That reads as everything added, not as
/// a broken invocation.
#[test]
fn audit_reads_a_baseline_before_the_wskill_as_all_added() {
    let tmp = TempDir::new().unwrap();
    // A repo whose first commit has no wskill in it at all.
    git(tmp.path(), &["init", "-q"]);
    std::fs::write(tmp.path().join("README.md"), "before the wskill\n").unwrap();
    commit(tmp.path(), "empty repo");
    let dest = scaffolded_wskill(&tmp);
    write_units(&dest, BASELINE);
    commit(tmp.path(), "create the wskill");

    let out = audit_out(&[dest.to_str().unwrap(), "--range", "HEAD~1..HEAD"]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains("  units +2   indexes +1   edges +2\n"),
        "everything is added and nothing removed: {text}"
    );
    assert!(text.contains("+ concept:alpha \"Alpha\""), "{text}");
    assert!(text.contains("+ index:reference \"Reference\""), "{text}");
    // The baseline is still named by its sha, so the audit is reproducible
    // even though nothing was read from it.
    let sha = text
        .lines()
        .next()
        .expect("a header")
        .split_whitespace()
        .last();
    assert!(
        sha.is_some_and(|r| r.split("..").next().is_some_and(|b| b.len() == 8)),
        "{text}"
    );
}

/// A range git cannot resolve is a tool failure (2), like an unreadable
/// model — the caller's question is the same one.
#[test]
fn audit_reports_an_unresolvable_range_as_a_tool_failure() {
    let tmp = TempDir::new().unwrap();
    let dest = scaffolded_wskill(&tmp);
    git_init(&dest);
    wcl()
        .args([
            "wskill",
            "audit",
            dest.to_str().unwrap(),
            "--range",
            "no-such-rev",
        ])
        .assert()
        .failure()
        .code(2);
}

/// Every file under `dir` with its bytes — enough to catch a write, a
/// deletion or a reformat.
fn tree_snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in std::fs::read_dir(&d).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                stack.push(path);
            } else {
                out.push((path.clone(), std::fs::read(&path).unwrap()));
            }
        }
    }
    out.sort();
    out
}
