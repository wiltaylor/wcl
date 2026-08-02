//! `wcl wskill` — the wskill model from the command line.
//!
//! The model itself lives in [`wcl_wskill`], and so do the lint rule engine,
//! the range audit and the op vocabulary. This CLI face also validates and
//! installs model-declared projections, so an agent (or a script, or a human)
//! can read, check, install and structurally edit a wskill without a browser
//! editor running.
//!
//! `op` is the one write half, and it writes the way the editor does: the
//! library turns an op into file changes, and [`crate::edit::commit`] — the
//! editor's own write / validate / roll-back pipeline — puts them on disk. So
//! a curated op and a browser drag are one operation validated one way.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

use wcl_wskill::audit::{Audit, Change, Metric, NodeDelta, Range};
use wcl_wskill::ops::{self, Op};
use wcl_wskill::{Finding, Rule, Severity};

use crate::{EXIT_EVAL, EXIT_IO, EXIT_OK, EXIT_PARSE};

/// The exit codes of the `wskill` subcommands are their own: **0** clean,
/// **1** findings at or above the denied severity, **2** the model could not
/// be read.
///
/// They deliberately differ from the CLI-wide codes (where 1 is a parse error
/// and 2 a schema violation), because a linter's caller asks one question —
/// did it pass? — and the answer must not depend on *how* a failing wskill
/// failed. A tool failure is 2 whether the cause was a parse error or a
/// missing folder.
///
/// `audit` never returns 1: it is a review, not a gate. Deciding a range made
/// things worse is a judgement over what it reports, and a command that
/// exited non-zero for it would be a health gate wearing a review's clothes.
///
/// `op` shares the shape for the same reason it is shared at all: its caller
/// asks one question — did the gated run land? A refused op or run gate is 1;
/// unreadable input/model or a git failure is 2.
const WSKILL_OK: u8 = 0;
const WSKILL_FINDINGS: u8 = 1;
const WSKILL_TOOL_FAILURE: u8 = 2;

mod check;
mod install;
mod support;

pub(crate) use check::run as run_check;
pub(crate) use install::run as run_install;

/// Which lint autofixes the caller requested. Bare `--fix` selects every
/// rule that declares one; `--fix=<rule>` narrows the same pass to one rule.
#[derive(Debug, Clone, Copy)]
pub(crate) enum FixSelection {
    All,
    Rule(Rule),
}

impl FixSelection {
    fn includes(self, rule: Rule) -> bool {
        match self {
            FixSelection::All => rule.has_autofix(),
            FixSelection::Rule(selected) => rule == selected,
        }
    }
}

/// Parse the optional value of `--fix` in the library's rule vocabulary.
pub(crate) fn parse_fix(s: &str) -> Result<FixSelection, String> {
    if s == "all" {
        return Ok(FixSelection::All);
    }
    let rule = Rule::parse(s).ok_or_else(|| {
        format!(
            "expected `all` or one of {}",
            Rule::ALL
                .iter()
                .filter(|rule| rule.has_autofix())
                .map(Rule::slug)
                .collect::<Vec<_>>()
                .join(", ")
        )
    })?;
    if !rule.has_autofix() {
        return Err(format!("rule `{rule}` does not declare an autofix"));
    }
    Ok(FixSelection::Rule(rule))
}

/// How much of a commit sha an audit header shows. Long enough to paste back
/// into `git`, short enough that the two ends of a range fit on the line
/// with the path.
const SHORT_SHA: usize = 8;

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

/// Run `wcl wskill lint [<entry>] [--format …] [--severity …] [--deny …]
/// [--fix[=<rule>] [--dry-run]]`: report the rule engine's findings, applying
/// any requested autofix ops before the final pass.
///
/// `severity` empty means every severity — a filter nobody set filters
/// nothing. Note that it filters the exit code too: findings the caller asked
/// not to see cannot fail the run, which is what makes
/// `--severity candidate` the curator's phase-1 read rather than a gate.
pub(crate) fn run_lint(
    entry: &Path,
    format: crate::ReportFormat,
    severity: &[Severity],
    deny: Severity,
    fix: Option<FixSelection>,
    dry_run: bool,
) -> u8 {
    let mut graph = match wcl_wskill::Graph::open(entry) {
        Ok(g) => g,
        Err(e) => {
            report(e);
            return WSKILL_TOOL_FAILURE;
        }
    };

    if let Some(selection) = fix {
        let mut seen = HashSet::new();
        let fixes: Vec<Op> = wcl_wskill::lint(&graph)
            .into_iter()
            .filter(|finding| selection.includes(finding.rule))
            .filter_map(|finding| finding.fix)
            .filter(|op| seen.insert(ops::to_json(op).to_string()))
            .collect();

        if dry_run {
            print_lint_dry_run(&fixes);
            return WSKILL_OK;
        }

        if apply_lint_fixes(&graph.root.join(wcl_wskill::ROOT_MARKER), &fixes).is_err() {
            return WSKILL_FINDINGS;
        }
        graph = match wcl_wskill::Graph::open(entry) {
            Ok(g) => g,
            Err(e) => {
                report(e);
                return WSKILL_TOOL_FAILURE;
            }
        };
    }

    let findings: Vec<Finding> = wcl_wskill::lint(&graph)
        .into_iter()
        .filter(|f| severity.is_empty() || severity.contains(&f.severity))
        .collect();

    match format {
        crate::ReportFormat::Json => {
            let json = serde_json::to_string_pretty(&findings)
                .expect("findings serialize (owned strings and numbers)");
            println!("{json}");
        }
        crate::ReportFormat::Text => print_findings(&graph.root, &findings),
    }

    // `<=` is "at least this certain": severities are declared most-certain
    // first, so `--deny warn` covers warnings AND the errors above them.
    if findings.iter().any(|f| f.severity <= deny) {
        WSKILL_FINDINGS
    } else {
        WSKILL_OK
    }
}

// ---------------------------------------------------------------------------
// `op` — apply the id-addressed op vocabulary
// ---------------------------------------------------------------------------

/// Run `wcl wskill op [<entry>] [--op <json>]… [--comment <json>]…`.
///
/// Every op is re-addressed against a freshly loaded model and written through
/// the validating editor pipeline. The run starts from a clean git tree,
/// captures an in-memory lint baseline, applies the whole batch, builds every
/// declared projection, and creates one git commit only if lint did not
/// regress. A refusal or failed run gate restores every touched file.
pub(crate) fn run_op(
    entry: &Path,
    inline: &[String],
    comment_texts: &[String],
    file: Option<&Path>,
    dry_run: bool,
    message: &str,
) -> u8 {
    let mut comments = match decode_comments(comment_texts) {
        Ok(comments) => comments,
        Err(e) => {
            eprintln!("{e}");
            return WSKILL_TOOL_FAILURE;
        }
    };
    let source = match read_ops(inline, file, comments.is_empty()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return WSKILL_TOOL_FAILURE;
        }
    };
    let (ops, piped_comments) = match decode_batch(&source) {
        Ok(batch) => batch,
        Err(e) => {
            eprintln!("{e}");
            return WSKILL_TOOL_FAILURE;
        }
    };
    comments.extend(piped_comments);
    if ops.is_empty() && comments.is_empty() {
        eprintln!(
            "no ops or comments given — pass `--op <json>`, `--comment <json>`, \
             `--file <path>`, or pipe ops in"
        );
        return WSKILL_TOOL_FAILURE;
    }

    // The target is resolved even for a dry run: pointing at something that
    // is not a wskill is a mistake worth hearing about before the ops are
    // approved, and it costs a stat.
    let root = match root_doc(entry) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return WSKILL_TOOL_FAILURE;
        }
    };

    // `--dry-run` prints the ops and touches nothing — deliberately WITHOUT
    // resolving them against the model: each op is addressed against the tree
    // the ops before it left, so anything past the first is unanswerable
    // until they have actually applied. What it does show is the op list as
    // the vocabulary spells it, which is the same JSON the editor sends —
    // and which this command reads back.
    if dry_run {
        let list: Vec<serde_json::Value> = ops.iter().map(ops::to_json).collect();
        if comments.is_empty() {
            println!(
                "{}",
                serde_json::to_string_pretty(&list).expect("ops serialize")
            );
        } else {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "ops": list,
                    "comments": comments.iter().map(PendingComment::to_json).collect::<Vec<_>>(),
                }))
                .expect("ops and comments serialize")
            );
        }
        eprintln!(
            "{}, {} — nothing written",
            plural(ops.len(), "op"),
            plural(comments.len(), "comment")
        );
        return WSKILL_OK;
    }

    let git = match GitRun::begin(&root) {
        Ok(git) => git,
        Err(e) => {
            eprintln!("{e}");
            return WSKILL_TOOL_FAILURE;
        }
    };
    let lint_before = match lint_counts(&root) {
        Ok(counts) => counts,
        Err(e) => {
            eprintln!("capture lint baseline: {e}");
            return WSKILL_TOOL_FAILURE;
        }
    };
    let mut rollback = RunRollback::default();

    for (i, op) in ops.iter().enumerate() {
        if let Err(e) = apply_one(&root, op, &mut rollback) {
            rollback_run(
                &rollback,
                &format!(
                    "op {}: {e}\nrolled back {} of {}",
                    i + 1,
                    i,
                    plural(ops.len(), "op")
                ),
            );
            return WSKILL_FINDINGS;
        }
        println!(
            "{}",
            serde_json::to_string(&ops::to_json(op)).expect("an op serializes")
        );
    }
    let comments_file = root
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join("comments.wcl");
    for (i, comment) in comments.iter().enumerate() {
        if let Err(e) = rollback
            .capture(std::iter::once(comments_file.as_path()))
            .and_then(|()| comment.add_to(&comments_file))
        {
            rollback_run(&rollback, &format!("comment {}: {e}", i + 1));
            return WSKILL_FINDINGS;
        }
    }
    match lint_counts(&root) {
        Ok(after) => {
            let regressions: Vec<String> = after
                .iter()
                .filter_map(|(&(severity, rule), &count)| {
                    let before = lint_before.get(&(severity, rule)).copied().unwrap_or(0);
                    (count > before).then(|| format!("{severity} [{rule}] {before} -> {count}"))
                })
                .collect();
            if !regressions.is_empty() {
                rollback_run(
                    &rollback,
                    &format!("lint findings increased: {}", regressions.join(", ")),
                );
                return WSKILL_FINDINGS;
            }
        }
        Err(e) => {
            rollback_run(&rollback, &format!("run-level lint failed: {e}"));
            return WSKILL_FINDINGS;
        }
    }
    if let Err(e) = build_projections(&root) {
        rollback_run(&rollback, &e);
        return WSKILL_FINDINGS;
    }
    if let Err(e) = git.commit(&root, message) {
        let _ = git.unstage(&root);
        rollback_run(&rollback, &format!("commit failed: {e}"));
        return WSKILL_TOOL_FAILURE;
    }
    eprintln!("applied {}", plural(ops.len(), "op"));
    WSKILL_OK
}

fn print_lint_dry_run(operations: &[Op]) {
    let list: Vec<serde_json::Value> = operations.iter().map(ops::to_json).collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&list).expect("ops serialize")
    );
    eprintln!("{} — nothing written", plural(operations.len(), "op"));
}

fn apply_lint_fixes(root: &Path, operations: &[Op]) -> Result<(), ()> {
    let mut rollback = RunRollback::default();
    for (i, op) in operations.iter().enumerate() {
        if let Err(e) = apply_one(root, op, &mut rollback) {
            rollback_run(
                &rollback,
                &format!(
                    "op {}: {e}\nrolled back {} of {}",
                    i + 1,
                    i,
                    plural(operations.len(), "op")
                ),
            );
            return Err(());
        }
    }
    eprintln!("applied {}", plural(operations.len(), "op"));
    Ok(())
}

#[derive(Debug)]
struct PendingComment {
    page: Option<String>,
    page_file: Option<String>,
    loc: Option<String>,
    target: Option<String>,
    object_kind: Option<String>,
    object_id: Option<String>,
    body: String,
    author: Option<String>,
    quote: Option<String>,
}

impl PendingComment {
    fn as_new_comment(&self) -> wcl_wdoc::comments::NewComment<'_> {
        wcl_wdoc::comments::NewComment {
            page: self.page.as_deref(),
            page_file: self.page_file.as_deref(),
            loc: self.loc.as_deref(),
            target: self.target.as_deref(),
            object_kind: self.object_kind.as_deref(),
            object_id: self.object_id.as_deref(),
            body: &self.body,
            author: self.author.as_deref(),
            quote: self.quote.as_deref(),
        }
    }

    fn validate(&self) -> Result<(), String> {
        self.as_new_comment()
            .validate()
            .map_err(|e| e.render_plain())
    }

    fn add_to(&self, file: &Path) -> Result<(), String> {
        wcl_wdoc::comments::add_addressed(file, self.as_new_comment())
            .map(|_| ())
            .map_err(|e| e.render_plain())
    }

    fn to_json(&self) -> serde_json::Value {
        serde_json::json!({
            "page": self.page,
            "page_file": self.page_file,
            "loc": self.loc,
            "target": self.target,
            "object_kind": self.object_kind,
            "object_id": self.object_id,
            "body": self.body,
            "author": self.author,
            "quote": self.quote,
        })
    }
}

fn decode_comments(texts: &[String]) -> Result<Vec<PendingComment>, String> {
    let mut out = Vec::new();
    for text in texts {
        let value: serde_json::Value = serde_json::from_str(text.trim())
            .map_err(|e| format!("the comments are not JSON: {e}"))?;
        let items = match value {
            serde_json::Value::Array(items) => items,
            one => vec![one],
        };
        for item in items {
            let string = |key: &str| {
                item.get(key)
                    .and_then(serde_json::Value::as_str)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string)
            };
            let body = string("body")
                .filter(|body| !body.trim().is_empty())
                .ok_or_else(|| format!("comment {}: missing `body`", out.len() + 1))?;
            let comment = PendingComment {
                page: string("page"),
                page_file: string("page_file"),
                loc: string("loc"),
                target: string("target"),
                object_kind: string("object_kind"),
                object_id: string("object_id"),
                body,
                author: match string("author") {
                    Some(author) if author != "curator" => {
                        return Err(format!(
                            "comment {}: curator comments cannot use author `{author}`",
                            out.len() + 1
                        ));
                    }
                    _ => Some("curator".to_string()),
                },
                quote: string("quote"),
            };
            comment
                .validate()
                .map_err(|e| format!("comment {}: {e}", out.len() + 1))?;
            out.push(comment);
        }
    }
    Ok(out)
}

fn lint_counts(root: &Path) -> Result<BTreeMap<(Severity, Rule), usize>, String> {
    let graph = wcl_wskill::Graph::open(root).map_err(|e| e.to_string())?;
    let mut counts = BTreeMap::new();
    for finding in wcl_wskill::lint(&graph) {
        *counts.entry((finding.severity, finding.rule)).or_default() += 1;
    }
    Ok(counts)
}

fn build_projections(root: &Path) -> Result<(), String> {
    let registry = wcl_wskill::Registry::read(root)
        .ok_or_else(|| format!("could not read projection registry from {}", root.display()))?;
    let root_dir = root.parent().unwrap_or_else(|| Path::new("."));
    let output = tempfile::tempdir().map_err(|e| format!("create projection output: {e}"))?;
    for artifact in registry.artifacts {
        let entry = root_dir.join(&artifact.entry);
        let out = output.path().join(&artifact.id);
        let result = if artifact.kind == "ai_skill" {
            wcl_wdoc::skill(&entry, &out, None)
        } else {
            wcl_wdoc::build(&entry, &out, None)
        };
        if let Err(e) = result {
            return Err(format!(
                "projection `{}` failed to build from {}: {}",
                artifact.kind,
                entry.display(),
                e.render_plain()
            ));
        }
    }
    Ok(())
}

/// Apply one op through the editor's validating write pipeline.
///
/// The model is re-read per op rather than kept: an op rewrites the files the
/// next one is addressed against, and a stale [`Graph`](wcl_wskill::Graph)
/// would point at the file layout of the previous step.
fn apply_one(root: &Path, op: &Op, rollback: &mut RunRollback) -> Result<(), String> {
    let graph = wcl_wskill::Graph::open(root).map_err(|e| e.to_string())?;
    let changes = ops::apply(&graph, op).map_err(|e| e.to_string())?;
    rollback.capture(changes.iter().map(|change| change.file.as_path()))?;
    crate::edit::commit(
        root,
        changes.into_iter().map(|c| (c.file, c.text)).collect(),
    )?;
    Ok(())
}

#[derive(Default)]
struct RunRollback {
    originals: BTreeMap<PathBuf, Option<Vec<u8>>>,
}

impl RunRollback {
    fn capture<'a>(&mut self, files: impl Iterator<Item = &'a Path>) -> Result<(), String> {
        for file in files {
            if self.originals.contains_key(file) {
                continue;
            }
            let original = match std::fs::read(file) {
                Ok(bytes) => Some(bytes),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => return Err(format!("read {} for rollback: {e}", file.display())),
            };
            self.originals.insert(file.to_path_buf(), original);
        }
        Ok(())
    }

    fn restore(&self) -> Result<(), String> {
        for (file, original) in &self.originals {
            match original {
                Some(bytes) => std::fs::write(file, bytes)
                    .map_err(|e| format!("restore {}: {e}", file.display()))?,
                None => match std::fs::remove_file(file) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => return Err(format!("remove {}: {e}", file.display())),
                },
            }
        }
        Ok(())
    }
}

fn rollback_run(rollback: &RunRollback, failure: &str) {
    eprintln!("{failure}");
    if let Err(e) = rollback.restore() {
        eprintln!("rollback failed: {e}");
    }
}

struct GitRun {
    repo: PathBuf,
}

impl GitRun {
    fn begin(root: &Path) -> Result<GitRun, String> {
        let wskill_dir = root.parent().unwrap_or_else(|| Path::new("."));
        let repo = git_stdout(wskill_dir, &["rev-parse", "--show-toplevel"])?;
        let repo = PathBuf::from(repo.trim());
        let status = git_stdout(
            &repo,
            &["status", "--porcelain=v1", "--untracked-files=all"],
        )?;
        if !status.trim().is_empty() {
            return Err(format!(
                "working tree must be clean before a wskill op run:\n{}",
                status.trim_end()
            ));
        }
        Ok(GitRun { repo })
    }

    fn commit(&self, root: &Path, message: &str) -> Result<(), String> {
        let wskill_dir = root.parent().unwrap_or_else(|| Path::new("."));
        git_success(
            &self.repo,
            &["add", "--", &wskill_dir.display().to_string()],
        )?;
        git_success(&self.repo, &["commit", "-m", message])
    }

    fn unstage(&self, root: &Path) -> Result<(), String> {
        let wskill_dir = root.parent().unwrap_or_else(|| Path::new("."));
        git_success(
            &self.repo,
            &[
                "reset",
                "--quiet",
                "HEAD",
                "--",
                &wskill_dir.display().to_string(),
            ],
        )
    }
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
    String::from_utf8(output.stdout).map_err(|e| format!("git output is not UTF-8: {e}"))
}

fn git_success(dir: &Path, args: &[&str]) -> Result<(), String> {
    git_stdout(dir, args).map(|_| ())
}

/// The wskill root document an op targets — `wskill.wcl`, found from
/// whatever the caller named (the folder, the root itself, or a projection
/// entry inside it).
///
/// A projection entry is deliberately *not* what gets opened: a book's
/// `main.wcl` sees the units through one view's template set, and the
/// curator operates on the format rather than on one view of it. It is also
/// the document the commit validates against, so an op that would break the
/// model is caught against the whole model.
fn root_doc(entry: &Path) -> Result<PathBuf, String> {
    let named = if entry.is_dir() {
        entry.join(wcl_wskill::ROOT_MARKER)
    } else {
        entry.to_path_buf()
    };
    if !named.exists() {
        return Err(format!("no such file: {}", named.display()));
    }
    let root = wcl_wskill::root_for(&named).join(wcl_wskill::ROOT_MARKER);
    if !root.is_file() {
        return Err(format!(
            "{} is not inside a wskill: no `{}` here or above it",
            entry.display(),
            wcl_wskill::ROOT_MARKER
        ));
    }
    Ok(root)
}

/// The op JSON, from `--op` values and/or `--file` — stdin when the caller
/// named neither, so a curator can pipe the list it just built.
fn read_ops(
    inline: &[String],
    file: Option<&Path>,
    stdin_when_empty: bool,
) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = inline.to_vec();
    match file {
        Some(path) if path == Path::new("-") => out.push(read_stdin()?),
        Some(path) => out.push(
            std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?,
        ),
        None if out.is_empty() && stdin_when_empty => out.push(read_stdin()?),
        None => {}
    }
    Ok(out)
}

fn read_stdin() -> Result<String, String> {
    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .map_err(|e| format!("read stdin: {e}"))?;
    Ok(buf)
}

/// Decode every op and embedded comment. Besides the original op object/array
/// forms, this accepts the `{ ops, comments }` envelope printed by a dry run,
/// so the preview can be piped back verbatim for the real run.
fn decode_batch(texts: &[String]) -> Result<(Vec<Op>, Vec<PendingComment>), String> {
    let mut ops_out = Vec::new();
    let mut comment_texts = Vec::new();
    for text in texts {
        let v: serde_json::Value =
            serde_json::from_str(text.trim()).map_err(|e| format!("the ops are not JSON: {e}"))?;
        if let serde_json::Value::Object(batch) = &v
            && (batch.contains_key("ops") || batch.contains_key("comments"))
        {
            let batch_ops = batch
                .get("ops")
                .cloned()
                .unwrap_or_else(|| serde_json::Value::Array(Vec::new()));
            decode_op_value(batch_ops, &mut ops_out)?;
            if let Some(comments) = batch.get("comments") {
                comment_texts.push(
                    serde_json::to_string(comments)
                        .expect("a parsed comment batch serializes back to JSON"),
                );
            }
            continue;
        }
        decode_op_value(v, &mut ops_out)?;
    }
    let comments = decode_comments(&comment_texts)?;
    Ok((ops_out, comments))
}

fn decode_op_value(value: serde_json::Value, out: &mut Vec<Op>) -> Result<(), String> {
    let items = match value {
        serde_json::Value::Array(items) => items,
        one => vec![one],
    };
    for item in items {
        out.push(ops::from_json(&item).map_err(|e| format!("op {}: {e}", out.len() + 1))?);
    }
    Ok(())
}

/// One line per finding, plus a count on stderr so the summary survives a
/// pipe into `grep`.
fn print_findings(root: &Path, findings: &[Finding]) {
    let mut lines = Some(Lines::default());
    for f in findings {
        println!(
            "{} {} [{}] {} — {}",
            written_at(root, &f.file, f.span, &mut lines),
            f.severity,
            f.rule,
            f.node,
            f.message
        );
    }
    let count = |sev: Severity| findings.iter().filter(|f| f.severity == sev).count();
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

/// `n things`, with the `s` only when it earns one.
fn plural(n: usize, word: &str) -> String {
    format!("{n} {word}{}", if n == 1 { "" } else { "s" })
}

/// Run `wcl wskill audit [<entry>] [--range <range>] [--format …]`: the
/// union graph of two revisions, with its changelog.
///
/// There is no `--deny`, and no exit code between clean and unreadable: an
/// audit reports what a range did, and whether that is acceptable is the
/// reviewer's call. See [`WSKILL_OK`].
pub(crate) fn run_audit(entry: &Path, range: &str, format: crate::ReportFormat) -> u8 {
    let audit = match Audit::across(entry, &Range::parse(range)) {
        Ok(a) => a,
        Err(e) => {
            report(e);
            return WSKILL_TOOL_FAILURE;
        }
    };
    match format {
        crate::ReportFormat::Json => {
            let json = serde_json::to_string_pretty(&audit)
                .expect("the audit serializes (owned strings and numbers)");
            println!("{json}");
        }
        crate::ReportFormat::Text => print_audit(&audit),
    }
    WSKILL_OK
}

/// The header strip, then one row per changed node with its own findings and
/// link churn beneath it — the changelog *is* the surface, and the health
/// metrics are its header rather than a report of their own.
fn print_audit(audit: &Audit) {
    let s = &audit.summary;
    println!(
        "{} {}",
        shorten(audit.root.clone()).display(),
        range_label(audit)
    );
    println!(
        "  units {}   indexes {}   edges {}",
        counts(s.units.added, s.units.removed, s.units.modified),
        counts(s.indexes.added, s.indexes.removed, s.indexes.modified),
        // An edge has no content to modify — it exists or it doesn't.
        counts(s.edges.added, s.edges.removed, 0),
    );
    for (label, moved) in [
        (
            "worse",
            audit.health.iter().filter(|m| m.worse).collect::<Vec<_>>(),
        ),
        (
            "better",
            audit
                .health
                .iter()
                .filter(|m| m.moved() && !m.worse)
                .collect::<Vec<_>>(),
        ),
    ] {
        if !moved.is_empty() {
            let parts: Vec<String> = moved.into_iter().map(metric_label).collect();
            println!("  {label:<6}  {}", parts.join(" · "));
        }
    }

    // Line numbers are the working tree's, so they are only offered when the
    // compared side IS the working tree — a line resolved against a file the
    // audit never read would point at the wrong thing with total confidence.
    let mut lines = (audit.after.is_none()).then(Lines::default);
    let mut changed = 0usize;
    let mut findings = 0usize;
    for node in audit.news() {
        changed += 1;
        findings += node.findings.len();
        // A removal's span addresses the file as it *was*, so even against
        // the working tree there is no line to give.
        let mut removed = None;
        let lines = match node.change {
            Change::Removed => &mut removed,
            _ => &mut lines,
        };
        println!(
            "\n{} {}",
            written_at(&audit.root, &node.file, node.span, lines),
            row(node)
        );
        for f in &node.findings {
            println!("    {} [{}] {}", f.severity, f.rule, f.message);
        }
        for e in audit.edge_news(&node.node) {
            let via = match &e.index_id {
                Some(id) if *id != node.node.id => format!(" (via `{id}`)"),
                _ => String::new(),
            };
            println!("    {} {} → {}{via}", e.change.marker(), e.kind, e.to);
        }
    }
    eprintln!(
        "{}",
        if changed == 0 {
            "no changes".to_string()
        } else {
            format!(
                "{} changed, {}",
                plural(changed, "node"),
                plural(findings, "finding")
            )
        }
    );
}

/// `<before>..<after>`, in short shas, naming the working tree for what it
/// is — an audit of uncommitted output must say so, or its reader cannot
/// reproduce it.
fn range_label(audit: &Audit) -> String {
    let short = |sha: &str| sha.chars().take(SHORT_SHA).collect::<String>();
    format!(
        "{}..{}",
        short(&audit.before),
        audit
            .after
            .as_deref()
            .map(short)
            .unwrap_or_else(|| "(working tree)".to_string())
    )
}

/// `+3 -1 ~2`, dropping the zeros. A count that did not move is not news,
/// and a header of zeroes reads as noise.
///
/// The signs come from [`Change::marker`] rather than a second copy here,
/// so the header and the rows below it cannot label the same thing
/// differently.
fn counts(added: usize, removed: usize, modified: usize) -> String {
    let parts: Vec<String> = [
        (Change::Added, added),
        (Change::Removed, removed),
        (Change::Modified, modified),
    ]
    .iter()
    .filter(|(_, n)| *n > 0)
    .map(|(change, n)| format!("{}{n}", change.marker()))
    .collect();
    if parts.is_empty() {
        "—".to_string()
    } else {
        parts.join(" ")
    }
}

fn metric_label(m: &Metric) -> String {
    format!("{} {} → {}", m.label, m.before_text(), m.after_text())
}

/// Where something is written, as a terminal can open it: the shortest path
/// that still resolves, plus a line number when one can honestly be given.
///
/// `lines` is `None` when the caller knows the working-tree file is not the
/// one the span addresses — an audit of two commits, or a node the range
/// deleted. A line resolved against a file this process never read would
/// point at the wrong place with total confidence, so it is left off.
fn written_at(
    root: &Path,
    file: &Path,
    span: Option<wcl_lang::Span>,
    lines: &mut Option<Lines>,
) -> String {
    let path = display_path(root, file).display().to_string();
    match (lines.as_mut(), span) {
        (Some(lines), Some(span)) => match lines.line_of(root, file, span.start) {
            Some(line) => format!("{path}:{line}"),
            None => path,
        },
        _ => path,
    }
}

/// The changelog row itself: what happened, to what, and — for a
/// modification — which part of it.
fn row(node: &NodeDelta) -> String {
    let aspects = if node.changed.is_empty() {
        String::new()
    } else {
        format!(
            " ({})",
            node.changed
                .iter()
                .map(|a| a.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )
    };
    format!(
        "{} {}:{} \"{}\"{aspects}",
        node.change.marker(),
        node.node.kind,
        node.node.id,
        node.title
    )
}

/// A finding's file as the shortest thing that still resolves from the
/// caller's directory: relative to the cwd when it is under it, else
/// absolute. Anchors are wskill-relative in the model, which is what makes
/// two revisions comparable but is not something a terminal can open.
fn display_path(root: &Path, file: &Path) -> PathBuf {
    shorten(root.join(file))
}

fn shorten(full: PathBuf) -> PathBuf {
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
