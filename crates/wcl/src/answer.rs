//! Guided answer mode — the core behind `wcl answer`.
//!
//! A document opts a block type into answer mode with the `@answerable`
//! decorator (declared in the `<answer.wcl>` stdlib), which maps the
//! answer-mode *roles* onto the type's own field names:
//!
//! ```wcl
//! @answerable(prompt = "question", response = "answer", status = "status",
//!             pending = :open, resolved = :answered, skipped = :dropped)
//! @block("question")
//! type PlanQuestion { ... }
//! ```
//!
//! [`pending_questions`] scans a [`Document`] for instances of such types
//! whose status field equals the `pending` symbol, reading each one's prompt,
//! selection kind (`:single_select` / `:multi_select` / `:free_text`) and
//! option child blocks. [`compose_response`] renders a selection (picked
//! option labels plus an always-available free-text "other") into the single
//! response field, and [`record_outcome`] writes the answer (or skip) back
//! into the declaring `.wcl` file through the same validate-then-write
//! pipeline the WYSIWYG editor uses — one question at a time, so an
//! interrupted session loses nothing.

use std::path::{Path, PathBuf};

use wcl_lang::{
    Block, DeclName, Decorator, Document, Span, TypeDecl, Value, edit as ast_edit,
    format as wcl_format, parse_expr, parse_for_edit,
};

/// Instance field naming the selection kind when `@answerable` doesn't say.
const DEFAULT_KIND_FIELD: &str = "kind";
/// Child-block kind holding the choices when `@answerable` doesn't say.
const DEFAULT_OPTIONS_KIND: &str = "option";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum SelectKind {
    Single,
    Multi,
    Free,
}

impl SelectKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            SelectKind::Single => "single_select",
            SelectKind::Multi => "multi_select",
            SelectKind::Free => "free_text",
        }
    }
}

/// One declared choice on a question.
pub(crate) struct AnswerOption {
    pub id: String,
    pub label: String,
    pub note: Option<String>,
}

/// A pending (unanswered) question discovered in a document.
pub(crate) struct Question {
    /// The question block's first label (its id), or `""` when unlabelled.
    pub id: String,
    /// The `.wcl` file declaring the block (imports followed).
    pub file: PathBuf,
    pub span: Span,
    pub prompt: String,
    pub kind: SelectKind,
    pub options: Vec<AnswerOption>,
    /// Field names + status symbols resolved from `@answerable`.
    pub response_field: String,
    pub status_field: String,
    pub resolved: String,
    pub skipped: Option<String>,
}

/// What the respondent did with a question.
pub(crate) enum Outcome {
    /// The composed answer text — goes into the response field, status
    /// becomes the `resolved` symbol.
    Answer(String),
    /// Status becomes the `skipped` symbol (an error when none is declared).
    Skip,
}

/// The `@answerable` role mapping read off a block type's decorator.
struct Roles {
    prompt: String,
    response: String,
    status: String,
    pending: String,
    resolved: String,
    skipped: Option<String>,
    kind_field: String,
    options_kind: String,
}

/// Scan `doc` for pending answerable questions. Returns the questions in
/// document order plus any warnings (mis-declared decorators, unreadable
/// role fields) — the callers surface warnings but keep going, so one typo'd
/// type doesn't hide the rest of the interview.
pub(crate) fn pending_questions(doc: &Document, root_file: &Path) -> (Vec<Question>, Vec<String>) {
    let mut out = Vec::new();
    let mut warnings = Vec::new();
    let mut bad_types: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for (path, block) in doc.blocks_with_source() {
        let Some(decl) = block.schema() else {
            continue;
        };
        let Some(dec) = answerable_decorator(&decl) else {
            continue;
        };
        let roles = match read_roles(&dec) {
            Ok(r) => r,
            Err(e) => {
                if bad_types.insert(decl.full_name()) {
                    warnings.push(format!("type {}: {e}", decl.full_name()));
                }
                continue;
            }
        };
        let id = block
            .labels()
            .ok()
            .and_then(|ls| ls.first().map(value_text))
            .unwrap_or_default();
        let at = |what: &str| {
            if id.is_empty() {
                format!("{} block: {what}", block.kind())
            } else {
                format!("{} {id}: {what}", block.kind())
            }
        };
        let Some(status) = field_symbol(&block, &roles.status) else {
            warnings.push(at(&format!(
                "status field `{}` is missing or not a symbol",
                roles.status
            )));
            continue;
        };
        if status != roles.pending {
            continue;
        }
        let Some(prompt) = field_text(&block, &roles.prompt) else {
            warnings.push(at(&format!(
                "prompt field `{}` is missing or not text",
                roles.prompt
            )));
            continue;
        };
        let options: Vec<AnswerOption> = block
            .blocks()
            .filter(|b| b.kind() == roles.options_kind)
            .map(|b| {
                let oid = b
                    .labels()
                    .ok()
                    .and_then(|ls| ls.first().map(value_text))
                    .unwrap_or_default();
                let label = field_text(&b, "label").filter(|s| !s.is_empty());
                AnswerOption {
                    label: label.unwrap_or_else(|| oid.clone()),
                    note: field_text(&b, "note").filter(|s| !s.is_empty()),
                    id: oid,
                }
            })
            .collect();
        let kind = match field_symbol(&block, &roles.kind_field) {
            Some(s) => match s.as_str() {
                "single_select" => SelectKind::Single,
                "multi_select" => SelectKind::Multi,
                "free_text" => SelectKind::Free,
                other => {
                    warnings.push(at(&format!(
                        "unknown selection kind `:{other}` — treating as free text"
                    )));
                    SelectKind::Free
                }
            },
            // No declared kind: a question carrying options is a
            // single-select, one without is free text.
            None if options.is_empty() => SelectKind::Free,
            None => SelectKind::Single,
        };
        out.push(Question {
            id,
            file: path
                .map(Path::to_path_buf)
                .unwrap_or_else(|| root_file.to_path_buf()),
            span: block.span(),
            prompt,
            kind,
            options,
            response_field: roles.response,
            status_field: roles.status,
            resolved: roles.resolved,
            skipped: roles.skipped,
        });
    }
    (out, warnings)
}

/// The `@answerable` application on a type, if any. Matched by name so it
/// works both qualified (`@answer.answerable`) and bare.
fn answerable_decorator<'a>(decl: &TypeDecl<'a>) -> Option<Decorator<'a>> {
    decl.decorators()
        .find(|d| d.name() == "answerable" || d.full_name() == "answer.answerable")
}

fn read_roles(dec: &Decorator) -> Result<Roles, String> {
    let text = |name: &str| -> Result<String, String> {
        match arg(dec, name) {
            Some(Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s)) => Ok(s),
            Some(_) => Err(format!("@answerable `{name}` must be a field-name string")),
            None => Err(format!(
                "@answerable is missing `{name}` (a field-name string)"
            )),
        }
    };
    let sym = |name: &str| -> Result<String, String> {
        match arg(dec, name) {
            Some(Value::Symbol(s)) => Ok(s),
            Some(_) => Err(format!("@answerable `{name}` must be a symbol")),
            None => Err(format!("@answerable is missing `{name}` (a symbol)")),
        }
    };
    let opt_text = |name: &str, default: &str| -> Result<String, String> {
        match arg(dec, name) {
            Some(Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s)) => Ok(s),
            Some(Value::None) | None => Ok(default.to_string()),
            Some(_) => Err(format!("@answerable `{name}` must be a field-name string")),
        }
    };
    let skipped = match arg(dec, "skipped") {
        Some(Value::Symbol(s)) => Some(s),
        Some(Value::None) | None => None,
        Some(_) => return Err("@answerable `skipped` must be a symbol".to_string()),
    };
    Ok(Roles {
        prompt: text("prompt")?,
        response: text("response")?,
        status: text("status")?,
        pending: sym("pending")?,
        resolved: sym("resolved")?,
        skipped,
        kind_field: opt_text("kind", DEFAULT_KIND_FIELD)?,
        options_kind: opt_text("options", DEFAULT_OPTIONS_KIND)?,
    })
}

/// One decorator argument, schema-resolved when the `<answer.wcl>` import is
/// present, falling back to the raw named argument when it isn't.
fn arg(dec: &Decorator, name: &str) -> Option<Value> {
    match dec.resolved_arg_value(name).or_else(|| dec.named_arg(name)) {
        Some(Ok(v)) => Some(v),
        _ => None,
    }
}

/// A block field's value through the schema-aware path (so `@default`s
/// apply), as a symbol name.
fn field_symbol(block: &Block, name: &str) -> Option<String> {
    match block.typed_field(name)?.value() {
        Ok(Value::Symbol(s)) => Some(s),
        _ => None,
    }
}

/// A block field's value through the schema-aware path, as text.
fn field_text(block: &Block, name: &str) -> Option<String> {
    match block.typed_field(name)?.value() {
        Ok(Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s)) => Some(s),
        _ => None,
    }
}

fn value_text(v: &Value) -> String {
    match v {
        Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) | Value::Symbol(s) => s.clone(),
        other => format!("{other}"),
    }
}

/// Render a selection into the single response field: picked option labels
/// joined with `", "`, any free "other" text appended after `" — "`. Options
/// never constrain the answer — free text alone is always accepted.
pub(crate) fn compose_response(
    kind: SelectKind,
    picked: &[&AnswerOption],
    other: Option<&str>,
) -> Result<String, String> {
    let other = other.map(str::trim).filter(|s| !s.is_empty());
    if kind == SelectKind::Single && picked.len() > 1 {
        return Err("this question takes a single selection".to_string());
    }
    let labels = picked
        .iter()
        .map(|o| o.label.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    match (labels.is_empty(), other) {
        (true, None) => Err("empty answer — pick an option or type a response".to_string()),
        (true, Some(text)) => Ok(text.to_string()),
        (false, None) => Ok(labels),
        (false, Some(text)) => Ok(format!("{labels} — {text}")),
    }
}

/// Write an outcome back into the question's declaring file: set the
/// response + status fields (answer) or just the status (skip), then commit
/// through the validating pipeline — the edit rolls back if it introduces a
/// schema error.
pub(crate) fn record_outcome(
    root_file: &Path,
    q: &Question,
    outcome: &Outcome,
) -> Result<(), String> {
    let src = std::fs::read_to_string(&q.file)
        .map_err(|e| format!("failed to read {}: {e}", q.file.display()))?;
    let mut ast = parse_for_edit(&src, q.file.display().to_string())
        .map_err(|e| format!("{:?}", miette::Report::new(e)))?;
    let block = ast_edit::find_block_by_span(&mut ast.items, q.span).ok_or_else(|| {
        format!(
            "could not relocate the question block in {} — was the file edited meanwhile?",
            q.file.display()
        )
    })?;
    let status_sym = match outcome {
        Outcome::Answer(text) => {
            ast_edit::set_or_insert_field(
                block,
                &q.response_field,
                ast_edit::string_literal_expr(text),
            );
            &q.resolved
        }
        Outcome::Skip => q
            .skipped
            .as_ref()
            .ok_or("this question declares no skipped status")?,
    };
    let sym_expr = parse_expr(&format!(":{status_sym}"), "<answer status>")
        .map_err(|e| format!("bad status symbol `:{status_sym}`: {e}"))?;
    ast_edit::set_or_insert_field(block, &q.status_field, sym_expr);
    crate::edit::commit(
        root_file,
        vec![(q.file.clone(), wcl_format::to_source(&ast))],
    )?;
    Ok(())
}

// ---------------------------------------------------------------------------
// `wcl answer` CLI driver
// ---------------------------------------------------------------------------

/// `wcl answer <file>` — list pending questions (`--list`), answer/skip one
/// non-interactively (`--id`), or walk them all interactively. Every answer
/// is committed immediately, and the document is re-opened between questions
/// because each write reformats its file and invalidates the other pending
/// questions' byte spans.
pub(crate) fn run_answer(
    file: &Path,
    list: bool,
    id: Option<&str>,
    text: Option<&str>,
    picks: &[String],
    skip: bool,
) -> u8 {
    let (questions, warnings) = match discover(file) {
        Ok(x) => x,
        Err(code) => return code,
    };
    for w in &warnings {
        eprintln!("warning: {w}");
    }
    if list {
        let arr: Vec<_> = questions.iter().map(question_json).collect();
        match serde_json::to_string_pretty(&serde_json::Value::Array(arr)) {
            Ok(s) => println!("{s}"),
            Err(e) => {
                eprintln!("json serialization failed: {e}");
                return crate::EXIT_EVAL;
            }
        }
        return crate::EXIT_OK;
    }
    if let Some(id) = id {
        return answer_one(file, questions, id, text, picks, skip);
    }
    run_interactive(file, warnings.len())
}

/// Open the document and discover its pending questions; parse failures
/// render the diagnostic and map to the exit code.
fn discover(file: &Path) -> Result<(Vec<Question>, Vec<String>), u8> {
    let doc = wcl_wdoc::open_doc_for_edit(file).map_err(|e| {
        eprintln!("{:?}", miette::Report::new(e));
        crate::EXIT_PARSE
    })?;
    Ok(pending_questions(&doc, file))
}

/// The `--id` path: resolve the question and record one outcome.
fn answer_one(
    file: &Path,
    questions: Vec<Question>,
    id: &str,
    text: Option<&str>,
    picks: &[String],
    skip: bool,
) -> u8 {
    let Some(q) = questions.iter().find(|q| q.id == id) else {
        eprintln!("no pending question `{id}`");
        let ids: Vec<&str> = questions
            .iter()
            .map(|q| q.id.as_str())
            .filter(|s| !s.is_empty())
            .collect();
        if !ids.is_empty() {
            eprintln!("pending: {}", ids.join(", "));
        }
        return crate::EXIT_EVAL;
    };
    let outcome = if skip {
        Outcome::Skip
    } else {
        let mut chosen: Vec<&AnswerOption> = Vec::new();
        for p in picks {
            match q.options.iter().find(|o| &o.id == p) {
                Some(o) => chosen.push(o),
                None => {
                    eprintln!("question `{id}` has no option `{p}`");
                    let ids: Vec<&str> = q.options.iter().map(|o| o.id.as_str()).collect();
                    if !ids.is_empty() {
                        eprintln!("options: {}", ids.join(", "));
                    }
                    return crate::EXIT_EVAL;
                }
            }
        }
        match compose_response(q.kind, &chosen, text) {
            Ok(t) => Outcome::Answer(t),
            Err(e) => {
                eprintln!("{e}");
                return crate::EXIT_EVAL;
            }
        }
    };
    match record_outcome(file, q, &outcome) {
        Ok(()) => {
            let verb = if skip { "skipped" } else { "answered" };
            eprintln!("{verb} `{id}` in {}", q.file.display());
            crate::EXIT_OK
        }
        Err(e) => {
            eprintln!("{e}");
            crate::EXIT_SCHEMA
        }
    }
}

/// The interactive walk-through. Questions deferred with "later" are keyed by
/// declaring file + prompt (spans shift with every write, prompts don't).
fn run_interactive(file: &Path, mut warned: usize) -> u8 {
    let mut deferred: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut answered = 0usize;
    let mut skipped = 0usize;
    // Still-pending count from the latest discovery — what "left pending"
    // reports whether the loop drains naturally or the respondent quits.
    let mut left;
    loop {
        let (questions, warnings) = match discover(file) {
            Ok(x) => x,
            Err(code) => return code,
        };
        // A write can surface new warnings; show only ones not yet printed.
        for w in warnings.iter().skip(warned) {
            eprintln!("warning: {w}");
        }
        warned = warned.max(warnings.len());
        left = questions.len();
        let Some(q) = questions
            .into_iter()
            .find(|q| !deferred.contains(&defer_key(q)))
        else {
            break;
        };
        println!("\x1b[2m{left} pending\x1b[0m");
        let items: Vec<crate::answer_tui::MenuItem> = q
            .options
            .iter()
            .map(|o| crate::answer_tui::MenuItem {
                label: o.label.clone(),
                note: o.note.clone(),
            })
            .collect();
        let choice = match crate::answer_tui::ask(
            &q.prompt,
            &items,
            q.kind == SelectKind::Multi,
            q.skipped.is_some(),
        ) {
            Ok(c) => c,
            Err(e) => {
                eprintln!("input error: {e}");
                return crate::EXIT_IO;
            }
        };
        let outcome = match choice {
            crate::answer_tui::Choice::Selection(picks, other) => {
                let chosen: Vec<&AnswerOption> = picks.iter().map(|&i| &q.options[i]).collect();
                match compose_response(q.kind, &chosen, other.as_deref()) {
                    Ok(t) => Outcome::Answer(t),
                    Err(e) => {
                        eprintln!("{e}");
                        continue;
                    }
                }
            }
            crate::answer_tui::Choice::Skip => Outcome::Skip,
            crate::answer_tui::Choice::Later => {
                deferred.insert(defer_key(&q));
                continue;
            }
            crate::answer_tui::Choice::Quit => break,
        };
        match record_outcome(file, &q, &outcome) {
            Ok(()) => match outcome {
                Outcome::Answer(_) => answered += 1,
                Outcome::Skip => skipped += 1,
            },
            Err(e) => {
                eprintln!("failed to record: {e}");
                return crate::EXIT_SCHEMA;
            }
        }
    }
    println!("{answered} answered, {skipped} skipped, {left} left pending.");
    crate::EXIT_OK
}

fn defer_key(q: &Question) -> String {
    format!("{}\u{0}{}", q.file.display(), q.prompt)
}

/// The JSON shape `wcl answer --list` emits.
pub(crate) fn question_json(q: &Question) -> serde_json::Value {
    serde_json::json!({
        "id": q.id,
        "prompt": q.prompt,
        "kind": q.kind.as_str(),
        "options": q.options.iter().map(|o| serde_json::json!({
            "id": o.id,
            "label": o.label,
            "note": o.note,
        })).collect::<Vec<_>>(),
        "skippable": q.skipped.is_some(),
        "file": q.file.display().to_string(),
        "span": format!("{}:{}", q.span.start, q.span.end),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn opt(label: &str) -> AnswerOption {
        AnswerOption {
            id: label.to_lowercase(),
            label: label.to_string(),
            note: None,
        }
    }

    #[test]
    fn compose_free_text() {
        assert_eq!(
            compose_response(SelectKind::Free, &[], Some("  hello ")).unwrap(),
            "hello"
        );
        assert!(compose_response(SelectKind::Free, &[], Some("  ")).is_err());
        assert!(compose_response(SelectKind::Free, &[], None).is_err());
    }

    #[test]
    fn compose_single_pick_and_optional_text() {
        let a = opt("Linux");
        assert_eq!(
            compose_response(SelectKind::Single, &[&a], None).unwrap(),
            "Linux"
        );
        assert_eq!(
            compose_response(SelectKind::Single, &[&a], Some("musl only")).unwrap(),
            "Linux — musl only"
        );
        let b = opt("Mac");
        assert!(compose_response(SelectKind::Single, &[&a, &b], None).is_err());
        // Options never constrain: free text alone answers a select question.
        assert_eq!(
            compose_response(SelectKind::Single, &[], Some("neither")).unwrap(),
            "neither"
        );
    }

    #[test]
    fn compose_multi_joins_labels() {
        let a = opt("LSP");
        let b = opt("Formatter");
        assert_eq!(
            compose_response(SelectKind::Multi, &[&a, &b], None).unwrap(),
            "LSP, Formatter"
        );
        assert_eq!(
            compose_response(SelectKind::Multi, &[&a, &b], Some("later: debugger")).unwrap(),
            "LSP, Formatter — later: debugger"
        );
    }
}
