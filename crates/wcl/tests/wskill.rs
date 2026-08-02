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
    let path = dest.join("data/reference/reference.wcl");
    let template = std::fs::read_to_string(&path).unwrap();
    // Keep the scaffold's authoring guide above the content.
    let guide = template
        .split("\nconcept alpha {")
        .next()
        .unwrap()
        .to_string();
    std::fs::write(
        &path,
        format!(
            "{guide}\nconcept alpha {{\n  name    = \"Alpha\"\n  summary = \"The first idea.\"\n\n  \
             body {{\n    p \"Alpha explained.\"\n  }}\n}}\n\n\
             concept beta {{\n  name    = \"{beta_name}\"\n  summary = \"The second idea.\"\n}}\n\n\
             index reference {{\n  name    = \"Reference\"\n  summary = \"Everything, pinned.\"\n  \
             related = {pinned}\n}}\n"
        ),
    )
    .unwrap();
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
