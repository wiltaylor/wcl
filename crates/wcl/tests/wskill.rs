//! Integration tests for `wcl wskill graph` — the model on stdout, with no
//! editor and no build. Each test scaffolds a real wskill with
//! `wcl init wskill`, so the assertions run against the shipped base schema
//! rather than a hand-written miniature.

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
    assert!(
        summary.contains("3 nodes changed, 2 new findings"),
        "{summary}"
    );
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
