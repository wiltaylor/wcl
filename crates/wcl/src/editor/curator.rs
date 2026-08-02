//! Triggering the headless wskill curator from the editor.
//!
//! `POST /api/curator` starts the installed `wskill-curator` agent in
//! Claude Code's non-interactive mode. The agent still does all reads and
//! writes through the headless `wcl wskill` CLI; this endpoint only names
//! the scope and brackets the run with git revisions. If the agent creates
//! a commit, the response carries the exact range the audit view must open.
//! Nothing streams back while it works: this is a post-hoc review flow, not
//! a supervision surface.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;

use super::{EditorState, Workspace, run_blocking};
use crate::serve::{json_error, parse_json_body};

static CURATOR_RUNNING: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AgentStatus {
    Committed,
    NoChanges,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AgentReport {
    status: AgentStatus,
    message: String,
}

trait Runner: Send + Sync {
    fn run(&self, cwd: &Path, prompt: &str) -> Result<AgentReport, String>;
}

struct ClaudeRunner;

/// The schema makes the agent distinguish an honest no-op from a failed
/// gate. Git, not the agent, remains authoritative about whether a commit
/// actually appeared.
const REPORT_SCHEMA: &str = r#"{
  "type":"object",
  "properties":{
    "status":{"type":"string","enum":["committed","no_changes","failed"]},
    "message":{"type":"string"}
  },
  "required":["status","message"],
  "additionalProperties":false
}"#;

impl Runner for ClaudeRunner {
    fn run(&self, cwd: &Path, prompt: &str) -> Result<AgentReport, String> {
        let mut command = Command::new("claude");
        command.current_dir(cwd).args([
            "--print",
            "--agent",
            "wskill-curator",
            "--dangerously-skip-permissions",
            "--no-session-persistence",
            "--output-format",
            "json",
            "--json-schema",
            REPORT_SCHEMA,
            prompt,
        ]);
        // Make the same `wcl` executable that launched the editor visible
        // to the agent even when the editor itself came from `cargo run`.
        if let Ok(exe) = std::env::current_exe()
            && let Some(bin_dir) = exe.parent()
        {
            let inherited_path = std::env::var_os("PATH");
            let paths = std::iter::once(bin_dir.to_path_buf()).chain(
                inherited_path
                    .as_deref()
                    .map(std::env::split_paths)
                    .into_iter()
                    .flatten(),
            );
            if let Ok(path) = std::env::join_paths(paths) {
                command.env("PATH", path);
            }
        }
        let output = command
            .output()
            .map_err(|e| format!("could not start the headless curator (`claude`): {e}"))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stdout = String::from_utf8_lossy(&output.stdout);
            let detail = if stderr.trim().is_empty() {
                stdout.trim()
            } else {
                stderr.trim()
            };
            let suffix = if detail.is_empty() {
                String::new()
            } else {
                format!(": {detail}")
            };
            return Err(format!("the headless curator process failed{suffix}"));
        }
        parse_report(&output.stdout)
    }
}

pub(super) async fn handle_curator(
    State(state): State<Arc<EditorState>>,
    body: String,
) -> Response {
    let value = match parse_json_body(&body) {
        Ok(value) => value,
        Err(error) => return json_error(StatusCode::BAD_REQUEST, &error),
    };
    if CURATOR_RUNNING
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return json_error(StatusCode::CONFLICT, "a curator pass is already running");
    }
    let state2 = Arc::clone(&state);
    run_blocking(move || {
        let _running = RunningGuard;
        curate_with(&state2.ws, &value, &ClaudeRunner)
    })
    .await
}

struct RunningGuard;

impl Drop for RunningGuard {
    fn drop(&mut self) {
        CURATOR_RUNNING.store(false, Ordering::Release);
    }
}

fn curate_with(
    ws: &Workspace,
    body: &serde_json::Value,
    runner: &dyn Runner,
) -> Result<serde_json::Value, String> {
    let entry = crate::edit::str_field(body, "entry")?;
    let entry_abs = ws.abs(entry)?;
    let model = wcl_wskill::Graph::open(&entry_abs).map_err(|e| e.to_string())?;
    let root = std::fs::canonicalize(&model.root).unwrap_or_else(|_| model.root.clone());
    let scope = crate::edit::str_field(body, "scope")?;
    let scope_prompt = match scope {
        "whole_graph" => "the whole graph".to_string(),
        "index" => {
            let id = crate::edit::str_field(body, "index")?;
            if model.index(id).is_none() {
                return Err(format!("no index `{id}` in {}", root.display()));
            }
            format!("index `{id}` (that index and its complete subtree)")
        }
        other => {
            return Err(format!(
                "unknown curator scope `{other}` (expected `index` or `whole_graph`)"
            ));
        }
    };

    let repo = repo_root(&root)?;
    let before = git_stdout(&repo, &["rev-parse", "HEAD"])?;
    let prompt = format!(
        "Run one complete curator pass for the wskill at `{}`. Scope the pass to {}. \
         Work headlessly through the wskill CLI exactly as your contract requires; do not ask for \
         approval and do not edit prose. Return `committed` only after the gated op run creates its \
         commit, `no_changes` only when the scoped pass honestly needs no commit, and `failed` with \
         the gate or tool failure in `message`.",
        root.display(),
        scope_prompt
    );

    let run = runner.run(&repo, &prompt);
    let after = git_stdout(&repo, &["rev-parse", "HEAD"])?;

    // The commit is the event the UI cares about. Even if the agent's final
    // response is lost after committing, the repository gives us a stable,
    // reproducible range to audit.
    if after != before {
        let message = run
            .as_ref()
            .map(|r| r.message.clone())
            .unwrap_or_else(|e| format!("The curator committed before reporting: {e}"));
        return Ok(serde_json::json!({
            "ok": true,
            "status": "committed",
            "commit": after,
            "range": format!("{before}..{after}"),
            "message": message,
        }));
    }

    match run? {
        AgentReport {
            status: AgentStatus::NoChanges,
            message,
        } => Ok(serde_json::json!({
            "ok": true,
            "status": "no_changes",
            "message": message,
        })),
        AgentReport {
            status: AgentStatus::Failed,
            message,
        } => Err(format!("curator pass failed: {message}")),
        AgentReport {
            status: AgentStatus::Committed,
            message,
        } => Err(format!(
            "curator reported a commit but HEAD did not change: {message}"
        )),
    }
}

fn parse_report(stdout: &[u8]) -> Result<AgentReport, String> {
    let text = std::str::from_utf8(stdout)
        .map_err(|e| format!("the curator response was not UTF-8: {e}"))?;
    let value: serde_json::Value = serde_json::from_str(text.trim())
        .map_err(|e| format!("the curator returned invalid JSON: {e}"))?;
    let report = value.get("structured_output").unwrap_or(&value);
    let status = match report.get("status").and_then(serde_json::Value::as_str) {
        Some("committed") => AgentStatus::Committed,
        Some("no_changes") => AgentStatus::NoChanges,
        Some("failed") => AgentStatus::Failed,
        Some(other) => return Err(format!("the curator returned unknown status `{other}`")),
        None => return Err("the curator response has no structured status".to_string()),
    };
    let message = report
        .get("message")
        .and_then(serde_json::Value::as_str)
        .ok_or("the curator response has no message")?
        .to_string();
    Ok(AgentReport { status, message })
}

fn repo_root(path: &Path) -> Result<PathBuf, String> {
    git_stdout(path, &["rev-parse", "--show-toplevel"]).map(PathBuf::from)
}

fn git_stdout(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .output()
        .map_err(|e| format!("run git: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    String::from_utf8(output.stdout)
        .map(|s| s.trim().to_string())
        .map_err(|e| format!("git output is not UTF-8: {e}"))
}

#[cfg(test)]
mod tests {
    use std::process::Command;
    use std::sync::Mutex;

    use super::*;
    use crate::editor::testsupport::{workspace_built_by, write_mini_wskill_nested};

    struct FakeRunner<F>(F);

    impl<F> Runner for FakeRunner<F>
    where
        F: Fn(&Path, &str) -> Result<AgentReport, String> + Send + Sync,
    {
        fn run(&self, cwd: &Path, prompt: &str) -> Result<AgentReport, String> {
            (self.0)(cwd, prompt)
        }
    }

    fn git(dir: &Path, args: &[&str]) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .expect("git");
        assert!(
            output.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        String::from_utf8(output.stdout).unwrap().trim().to_string()
    }

    fn fixture() -> (tempfile::TempDir, Workspace) {
        workspace_built_by(|root| {
            write_mini_wskill_nested(root);
            git(root, &["init", "-q"]);
            git(root, &["config", "user.email", "curator@example.com"]);
            git(root, &["config", "user.name", "Curator Test"]);
            git(root, &["add", "-A"]);
            git(root, &["commit", "-qm", "baseline"]);
        })
    }

    fn report(status: AgentStatus, message: &str) -> Result<AgentReport, String> {
        Ok(AgentReport {
            status,
            message: message.to_string(),
        })
    }

    #[test]
    fn index_pass_runs_headlessly_and_returns_the_exact_commit_range() {
        let (_td, ws) = fixture();
        let seen = Mutex::new(String::new());
        let runner = FakeRunner(|cwd: &Path, prompt: &str| {
            *seen.lock().unwrap() = prompt.to_string();
            std::fs::write(cwd.join("curated.txt"), "done\n").unwrap();
            git(cwd, &["add", "curated.txt"]);
            git(cwd, &["commit", "-qm", "curate language index"]);
            report(AgentStatus::Committed, "Curated language")
        });

        let before = git(ws.root_dir(), &["rev-parse", "HEAD"]);
        let value = curate_with(
            &ws,
            &serde_json::json!({
                "entry": "main.wcl",
                "scope": "index",
                "index": "lang",
            }),
            &runner,
        )
        .expect("curator pass");
        let after = git(ws.root_dir(), &["rev-parse", "HEAD"]);

        assert_ne!(after, before);
        assert_eq!(value["status"], "committed");
        assert_eq!(value["commit"], after);
        assert_eq!(value["range"], format!("{before}..{after}"));
        let prompt = seen.lock().unwrap();
        assert!(prompt.contains("index `lang`"), "{prompt}");
        assert!(
            prompt.contains(&ws.root_dir().display().to_string()),
            "{prompt}"
        );
    }

    #[test]
    fn whole_graph_noop_is_success_without_an_audit_range() {
        let (_td, ws) = fixture();
        let runner = FakeRunner(|_: &Path, prompt: &str| {
            assert!(prompt.contains("whole graph"), "{prompt}");
            report(AgentStatus::NoChanges, "No candidates in scope")
        });

        let value = curate_with(
            &ws,
            &serde_json::json!({ "entry": "main.wcl", "scope": "whole_graph" }),
            &runner,
        )
        .expect("no-op is a completed pass");

        assert_eq!(value["status"], "no_changes");
        assert_eq!(value["message"], "No candidates in scope");
        assert!(value.get("range").is_none());
    }

    #[test]
    fn failed_gate_is_reported_and_does_not_claim_a_commit() {
        let (_td, ws) = fixture();
        let before = git(ws.root_dir(), &["rev-parse", "HEAD"]);
        let runner = FakeRunner(|_: &Path, _: &str| {
            report(AgentStatus::Failed, "projection `book` failed to build")
        });

        let error = curate_with(
            &ws,
            &serde_json::json!({ "entry": "main.wcl", "scope": "whole_graph" }),
            &runner,
        )
        .unwrap_err();

        assert!(
            error.contains("projection `book` failed to build"),
            "{error}"
        );
        assert_eq!(git(ws.root_dir(), &["rev-parse", "HEAD"]), before);
        assert!(git(ws.root_dir(), &["status", "--porcelain"]).is_empty());
    }

    #[test]
    fn unknown_index_is_refused_before_the_agent_runs() {
        let (_td, ws) = fixture();
        let runner = FakeRunner(|_: &Path, _: &str| panic!("runner must not be called"));

        let error = curate_with(
            &ws,
            &serde_json::json!({
                "entry": "main.wcl",
                "scope": "index",
                "index": "missing",
            }),
            &runner,
        )
        .unwrap_err();

        assert!(error.contains("no index `missing`"), "{error}");
    }

    #[test]
    fn parses_claude_structured_output() {
        let parsed = parse_report(
            br#"{"type":"result","structured_output":{"status":"failed","message":"lint gate got worse"}}"#,
        )
        .expect("structured report");
        assert_eq!(parsed.status, AgentStatus::Failed);
        assert_eq!(parsed.message, "lint gate got worse");
    }
}
