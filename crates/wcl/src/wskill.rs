//! `wcl wskill` — the wskill model from the command line.
//!
//! The model itself lives in [`wcl_wskill`], and so does the lint rule
//! engine; this is the thin CLI face of both, so an agent (or a script, or a
//! human) can read a wskill's graph and its findings without a browser editor
//! running.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use wcl_wskill::{Finding, Severity};

use crate::{EXIT_EVAL, EXIT_IO, EXIT_OK, EXIT_PARSE};

/// `lint`'s exit codes are its own: **0** clean, **1** findings at or above
/// the denied severity, **2** the model could not be read.
///
/// They deliberately differ from the CLI-wide codes (where 1 is a parse error
/// and 2 a schema violation), because a linter's caller asks one question —
/// did it pass? — and the answer must not depend on *how* a failing wskill
/// failed. A tool failure is 2 whether the cause was a parse error or a
/// missing folder.
const LINT_OK: u8 = 0;
const LINT_FINDINGS: u8 = 1;
const LINT_TOOL_FAILURE: u8 = 2;

/// Run `wcl wskill graph [<entry>] [--rev <rev>]`: print the model as JSON.
pub(crate) fn run_graph(entry: &Path, rev: Option<&str>) -> u8 {
    let graph = match rev {
        Some(rev) => wcl_wskill::Graph::open_at_rev(entry, rev),
        None => wcl_wskill::Graph::open(entry),
    };
    let graph = match graph {
        Ok(g) => g,
        Err(e) => return report(e),
    };
    match serde_json::to_value(&graph) {
        Ok(mut v) => {
            // The model's own queries, answered once here rather than by
            // every reader walking the edges back.
            if let Some(obj) = v.as_object_mut() {
                let ids: Vec<&str> = graph.unindexed().iter().map(|u| u.id.as_str()).collect();
                obj.insert("unindexed".to_string(), serde_json::json!(ids));
            }
            println!(
                "{}",
                serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string())
            );
            EXIT_OK
        }
        Err(e) => {
            eprintln!("json serialization failed: {e}");
            EXIT_EVAL
        }
    }
}

/// Run `wcl wskill lint [<entry>] [--format …] [--severity …] [--deny …]`:
/// report the rule engine's findings.
///
/// `severity` empty means every severity — a filter nobody set filters
/// nothing. Note that it filters the exit code too: findings the caller asked
/// not to see cannot fail the run, which is what makes
/// `--severity candidate` the curator's phase-1 read rather than a gate.
pub(crate) fn run_lint(
    entry: &Path,
    format: crate::LintFormat,
    severity: &[Severity],
    deny: Severity,
) -> u8 {
    let graph = match wcl_wskill::Graph::open(entry) {
        Ok(g) => g,
        Err(e) => {
            report(e);
            return LINT_TOOL_FAILURE;
        }
    };
    let findings: Vec<Finding> = wcl_wskill::lint(&graph)
        .into_iter()
        .filter(|f| severity.is_empty() || severity.contains(&f.severity))
        .collect();

    match format {
        crate::LintFormat::Json => {
            let json = serde_json::to_string_pretty(&findings)
                .expect("findings serialize (owned strings and numbers)");
            println!("{json}");
        }
        crate::LintFormat::Text => print_findings(&graph.root, &findings),
    }

    // `<=` is "at least this certain": severities are declared most-certain
    // first, so `--deny warn` covers warnings AND the errors above them.
    if findings.iter().any(|f| f.severity <= deny) {
        LINT_FINDINGS
    } else {
        LINT_OK
    }
}

/// One line per finding, plus a count on stderr so the summary survives a
/// pipe into `grep`.
fn print_findings(root: &Path, findings: &[Finding]) {
    let mut lines = Lines::default();
    for f in findings {
        let path = display_path(root, &f.file);
        let at = match f.span.map(|s| lines.line_of(root, &f.file, s.start)) {
            Some(Some(line)) => format!("{}:{line}", path.display()),
            _ => path.display().to_string(),
        };
        println!(
            "{at} {} [{}] {} — {}",
            f.severity, f.rule, f.node, f.message
        );
    }
    let count = |sev: Severity| findings.iter().filter(|f| f.severity == sev).count();
    let plural = |n: usize, word: &str| format!("{n} {word}{}", if n == 1 { "" } else { "s" });
    eprintln!(
        "{}",
        if findings.is_empty() {
            "no findings".to_string()
        } else {
            format!(
                "{}, {}, {}",
                plural(count(Severity::Error), "error"),
                plural(count(Severity::Warn), "warning"),
                plural(count(Severity::Candidate), "candidate")
            )
        }
    );
}

/// A finding's file as the shortest thing that still resolves from the
/// caller's directory: relative to the cwd when it is under it, else
/// absolute. Anchors are wskill-relative in the model, which is what makes
/// two revisions comparable but is not something a terminal can open.
fn display_path(root: &Path, file: &Path) -> PathBuf {
    let full = root.join(file);
    std::env::current_dir()
        .ok()
        .and_then(|cwd| full.strip_prefix(cwd).ok().map(Path::to_path_buf))
        .unwrap_or(full)
}

/// Byte offsets → line numbers, reading each declaring file at most once.
/// The model addresses spans; a person (and every editor's `file:line` jump)
/// wants lines.
#[derive(Default)]
struct Lines {
    files: HashMap<PathBuf, Option<String>>,
}

impl Lines {
    fn line_of(&mut self, root: &Path, file: &Path, offset: usize) -> Option<usize> {
        let text = self
            .files
            .entry(file.to_path_buf())
            .or_insert_with(|| std::fs::read_to_string(root.join(file)).ok())
            .as_deref()?;
        // Counted over bytes, not chars: a span may land mid-character in a
        // file this process never parsed, and slicing there would panic.
        let offset = offset.min(text.len());
        Some(
            text.as_bytes()[..offset]
                .iter()
                .filter(|b| **b == b'\n')
                .count()
                + 1,
        )
    }
}

/// Render a load failure and return its exit code — a parse error gets the
/// usual miette snippet, everything else its message.
fn report(err: wcl_wskill::Error) -> u8 {
    match err {
        wcl_wskill::Error::Parse(e) => {
            eprintln!("{:?}", miette::Report::new(*e));
            EXIT_PARSE
        }
        other => {
            eprintln!("{other}");
            EXIT_IO
        }
    }
}
