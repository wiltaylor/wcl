//! `wcl wskill` — the wskill model from the command line.
//!
//! The model itself lives in [`wcl_wskill`], and so do the lint rule engine,
//! the range audit and the op vocabulary; this is the thin CLI face of all
//! four, so an agent (or a script, or a human) can read a wskill's graph, its
//! findings and what a range did to it — and edit its structure — without a
//! browser editor running.
//!
//! `op` is the one write half, and it writes the way the editor does: the
//! library turns an op into file changes, and [`crate::edit::commit`] — the
//! editor's own write / validate / roll-back pipeline — puts them on disk. So
//! a curated op and a browser drag are one operation validated one way.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use wcl_wskill::audit::{Audit, Change, Metric, NodeDelta, Range};
use wcl_wskill::ops::{self, Op};
use wcl_wskill::{Finding, Severity};

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
/// asks one question — did the edit land? — so a refused op is the 1 and an
/// unreadable op list or wskill is the 2.
const WSKILL_OK: u8 = 0;
const WSKILL_FINDINGS: u8 = 1;
const WSKILL_TOOL_FAILURE: u8 = 2;

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

/// Run `wcl wskill lint [<entry>] [--format …] [--severity …] [--deny …]`:
/// report the rule engine's findings.
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
) -> u8 {
    let graph = match wcl_wskill::Graph::open(entry) {
        Ok(g) => g,
        Err(e) => {
            report(e);
            return WSKILL_TOOL_FAILURE;
        }
    };
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

/// Run `wcl wskill op [<entry>] [--op <json>]… [--file <path>] [--dry-run]`.
///
/// Ops apply **in order, one commit each**: every op is re-addressed against
/// a freshly loaded model (the previous one moved the spans it was written
/// at) and committed through the validating pipeline, so a schema violation
/// rolls that op back and stops the run. Earlier ops stay applied — each was
/// valid on its own — and the summary says how far it got, which is what a
/// curator needs to resume rather than to re-run blind.
pub(crate) fn run_op(entry: &Path, inline: &[String], file: Option<&Path>, dry_run: bool) -> u8 {
    let source = match read_ops(inline, file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return WSKILL_TOOL_FAILURE;
        }
    };
    let ops = match decode_ops(&source) {
        Ok(ops) => ops,
        Err(e) => {
            eprintln!("{e}");
            return WSKILL_TOOL_FAILURE;
        }
    };

    // `--dry-run` prints the ops and touches nothing — deliberately WITHOUT
    // resolving them against the model: each op is addressed against the tree
    // the ops before it left, so anything past the first is unanswerable
    // until they have actually applied. What it does show is the op list as
    // the vocabulary spells it, which is the same JSON the editor sends.
    if dry_run {
        let list: Vec<serde_json::Value> = ops.iter().map(ops::to_json).collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&list).expect("ops serialize")
        );
        eprintln!("{} — nothing written", plural(ops.len(), "op"));
        return WSKILL_OK;
    }

    let root = match root_doc(entry) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return WSKILL_TOOL_FAILURE;
        }
    };
    for (i, op) in ops.iter().enumerate() {
        if let Err(e) = apply_one(&root, op) {
            eprintln!("op {}: {e}", i + 1);
            eprintln!("applied {} of {}", i, plural(ops.len(), "op"));
            return WSKILL_FINDINGS;
        }
        println!(
            "{}",
            serde_json::to_string(&ops::to_json(op)).expect("an op serializes")
        );
    }
    eprintln!("applied {}", plural(ops.len(), "op"));
    WSKILL_OK
}

/// Apply one op and commit what it rewrites.
///
/// The model is re-read per op rather than kept: an op rewrites the files the
/// next one is addressed against, and a stale [`Graph`](wcl_wskill::Graph)
/// would point at the file layout of the previous step.
fn apply_one(root: &Path, op: &Op) -> Result<(), String> {
    let graph = wcl_wskill::Graph::open(root).map_err(|e| e.to_string())?;
    let changes = ops::apply(&graph, op).map_err(|e| e.to_string())?;
    crate::edit::commit(
        root,
        changes.into_iter().map(|c| (c.file, c.text)).collect(),
    )?;
    Ok(())
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
fn read_ops(inline: &[String], file: Option<&Path>) -> Result<Vec<String>, String> {
    let mut out: Vec<String> = inline.to_vec();
    match file {
        Some(path) if path == Path::new("-") => out.push(read_stdin()?),
        Some(path) => out.push(
            std::fs::read_to_string(path).map_err(|e| format!("read {}: {e}", path.display()))?,
        ),
        None if out.is_empty() => out.push(read_stdin()?),
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

/// Decode every op, from texts each holding one op object or an array of
/// them. Nothing is applied until they all decode: a typo in the last op of
/// a batch should not leave the first half of it on disk.
fn decode_ops(texts: &[String]) -> Result<Vec<Op>, String> {
    let mut out = Vec::new();
    for text in texts {
        let v: serde_json::Value =
            serde_json::from_str(text.trim()).map_err(|e| format!("the ops are not JSON: {e}"))?;
        let items = match v {
            serde_json::Value::Array(items) => items,
            one => vec![one],
        };
        for item in items {
            out.push(ops::from_json(&item).map_err(|e| format!("op {}: {e}", out.len() + 1))?);
        }
    }
    if out.is_empty() {
        return Err("no ops given — pass `--op <json>`, `--file <path>`, or pipe them in".into());
    }
    Ok(out)
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
