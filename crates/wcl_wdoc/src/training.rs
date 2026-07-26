//! Course answers stored in a `training.wcl` sidecar.
//!
//! A built training site works with no server at all — progress and multiple
//! choice results live in the reader's `localStorage`. When one IS running
//! (`wcl wdoc serve`), the page also POSTs each answer here, which gives two
//! things localStorage can't: answers that survive a browser change, and a
//! place for an agent to grade the free-text (`:text`) checks.
//!
//! Like [`crate::comments`], the sidecar is schemaless and rewritten whole on
//! every change (see [`crate::sidecar`]), and watchers ignore it — recording
//! an answer or a verdict never rebuilds the document.
//!
//! ```wcl
//! answer {
//!   id = "a12ab3"
//!   course = "/"                        // the built course's URL prefix
//!   lesson = "install_and_first_file"   // lesson id (from the page name)
//!   check = "why_schema"                // the check's id
//!   response = "…the learner's answer…"
//!   status = "pending"                  // pending | graded | correct | wrong
//!   verdict = "…the grader's feedback…" // written by `wcl wdoc training grade`
//!   score = "1"
//! }
//! ```
//!
//! Multiple-choice results arrive already graded (`correct` / `wrong`, with
//! the picked choice ids as `response`); only `pending` records need an agent.

use std::path::{Path, PathBuf};

use miette::Report;
use wcl_lang::parse_for_edit;

use crate::build::BuildError;
use crate::sidecar::{
    atomic_write, gen_id, owner_dir, read_blocks, scan_for, sidecar_path, wcl_string,
};

/// The sidecar file name course answers live in.
const SIDECAR: &str = "training.wcl";

/// One `answer { … }` record.
#[derive(Debug, Clone)]
pub struct AnswerRecord {
    pub id: String,
    /// The `training.wcl` that holds this record (where `grade` edits).
    pub file: PathBuf,
    /// The built course's URL prefix — distinguishes two courses served from
    /// one root.
    pub course: String,
    pub lesson: String,
    pub check: String,
    pub response: String,
    /// `pending` (awaiting an agent), `graded`, or a self-graded multiple
    /// choice result (`correct` / `wrong`).
    pub status: String,
    pub verdict: Option<String>,
    pub score: Option<String>,
}

impl AnswerRecord {
    /// Whether this answer is waiting on a grader.
    pub fn is_pending(&self) -> bool {
        self.status == "pending"
    }
}

/// The `training.wcl` owning answers for a page defined in `page_file`.
pub fn training_path(page_file: &Path, root: &Path) -> PathBuf {
    sidecar_path(page_file, root, SIDECAR)
}

/// The `training.wcl` for a course served (or listed) from `dir`.
///
/// A training site is usually entered below its wskill root — `wcl wdoc serve
/// wdoc/training/main.wcl` watches `wdoc/training/` — so this walks *up* to
/// the owning `wskill.wcl` and puts the sidecar beside it, keeping one answer
/// file per wskill however the course was launched. Outside a wskill the
/// sidecar simply sits in `dir`.
pub fn sidecar_for(dir: &Path) -> PathBuf {
    owner_dir(dir)
        .unwrap_or_else(|| dir.to_path_buf())
        .join(SIDECAR)
}

/// Every answer in one specific sidecar file.
pub fn read_sidecar(file: &Path) -> Vec<AnswerRecord> {
    let mut recs = read_file(file);
    recs.sort_by_key(|r| (!r.is_pending(), r.lesson.clone(), r.check.clone()));
    recs
}

/// The current record for one check within a single sidecar — what the served
/// page's long-poll reads.
pub fn find_in(file: &Path, course: &str, check: &str) -> Option<AnswerRecord> {
    read_file(file)
        .into_iter()
        .find(|r| r.check == check && (course.is_empty() || r.course == course))
}

/// Every answer stored in any `training.wcl` under `root`, pending first so a
/// grader sees its work at the top.
pub fn list(root: &Path) -> Result<Vec<AnswerRecord>, BuildError> {
    let mut files = Vec::new();
    scan_for(root, SIDECAR, &mut files);
    let mut out: Vec<AnswerRecord> = files.iter().flat_map(|p| read_file(p)).collect();
    out.sort_by_key(|r| (!r.is_pending(), r.lesson.clone(), r.check.clone()));
    Ok(out)
}

fn read_file(path: &Path) -> Vec<AnswerRecord> {
    let mut out = Vec::new();
    for mut fields in read_blocks(path, "answer") {
        let Some(id) = fields.remove("id") else {
            continue;
        };
        out.push(AnswerRecord {
            id,
            file: path.to_path_buf(),
            course: fields.remove("course").unwrap_or_default(),
            lesson: fields.remove("lesson").unwrap_or_default(),
            check: fields.remove("check").unwrap_or_default(),
            response: fields.remove("response").unwrap_or_default(),
            status: fields
                .remove("status")
                .unwrap_or_else(|| "pending".to_string()),
            verdict: fields.remove("verdict").filter(|s| !s.is_empty()),
            score: fields.remove("score").filter(|s| !s.is_empty()),
        });
    }
    out
}

/// Record an answer, replacing any earlier one for the same
/// `(course, lesson, check)` — a learner who answers again supersedes their
/// previous attempt rather than stacking up records. Returns the record id.
///
/// Re-answering clears a previous verdict: the grader is judging the text that
/// was submitted, so a new submission is ungraded again.
pub fn record(
    file: &Path,
    course: &str,
    lesson: &str,
    check: &str,
    response: &str,
    status: &str,
) -> Result<String, BuildError> {
    let mut recs = read_file(file);
    let existing = recs
        .iter()
        .position(|r| r.course == course && r.lesson == lesson && r.check == check);
    let id = match existing {
        Some(at) => {
            let id = recs[at].id.clone();
            recs[at].response = response.to_string();
            recs[at].status = status.to_string();
            recs[at].verdict = None;
            recs[at].score = None;
            id
        }
        None => {
            let id = gen_id('a');
            recs.push(AnswerRecord {
                id: id.clone(),
                file: file.to_path_buf(),
                course: course.to_string(),
                lesson: lesson.to_string(),
                check: check.to_string(),
                response: response.to_string(),
                status: status.to_string(),
                verdict: None,
                score: None,
            });
            id
        }
    };
    write_file(file, &recs)?;
    Ok(id)
}

/// The current record for one check, if any — what the page's long-poll reads.
pub fn find(root: &Path, course: &str, check: &str) -> Result<Option<AnswerRecord>, BuildError> {
    Ok(list(root)?
        .into_iter()
        .find(|r| r.check == check && (course.is_empty() || r.course == course)))
}

/// Write a grader's verdict onto the answer with `id`. `pass` sets the
/// resulting status the page shows; `score` is free-form (a mark, a rubric
/// tally). Returns `true` when an answer was found.
pub fn grade(
    root: &Path,
    id: &str,
    verdict: &str,
    pass: bool,
    score: Option<&str>,
) -> Result<bool, BuildError> {
    let Some(rec) = list(root)?.into_iter().find(|r| r.id == id) else {
        return Ok(false);
    };
    let mut recs = read_file(&rec.file);
    let mut found = false;
    for r in &mut recs {
        if r.id == id {
            r.verdict = Some(verdict.to_string());
            r.score = score.map(str::to_string);
            r.status = "graded".to_string();
            r.pass_marker(pass);
            found = true;
        }
    }
    if !found {
        return Ok(false);
    }
    write_file(&rec.file, &recs)?;
    Ok(true)
}

impl AnswerRecord {
    /// Record the pass/fail alongside `graded`, in the `score` field's sibling
    /// slot the client reads (`score` stays free-form).
    fn pass_marker(&mut self, pass: bool) {
        if self.score.is_none() {
            self.score = Some(if pass { "1" } else { "0" }.to_string());
        }
    }

    /// Whether a graded answer passed — the client's `pass` flag. A score that
    /// starts with `0` (or `f`/`n` for "fail"/"no") reads as a fail.
    pub fn passed(&self) -> bool {
        match self.score.as_deref() {
            Some(s) => !s.starts_with(['0', 'f', 'F', 'n', 'N']),
            None => self.status == "correct",
        }
    }
}

/// Regenerate `path` from `recs` (deterministic field order), verify it
/// re-parses, then write it atomically.
fn write_file(path: &Path, recs: &[AnswerRecord]) -> Result<(), BuildError> {
    let mut out = String::from(
        "// Course answers for this wskill — written by the training site when it\n\
         // runs under `wcl wdoc serve`, and read back by `wcl wdoc training`.\n\
         // Grade a pending answer with `wcl wdoc training grade <id> …`.\n\
         // Safe to hand-edit or generate.\n\n",
    );
    for r in recs {
        out.push_str("answer {\n");
        out.push_str(&format!("  id = {}\n", wcl_string(&r.id)));
        out.push_str(&format!("  course = {}\n", wcl_string(&r.course)));
        out.push_str(&format!("  lesson = {}\n", wcl_string(&r.lesson)));
        out.push_str(&format!("  check = {}\n", wcl_string(&r.check)));
        out.push_str(&format!("  response = {}\n", wcl_string(&r.response)));
        out.push_str(&format!("  status = {}\n", wcl_string(&r.status)));
        if let Some(v) = &r.verdict {
            out.push_str(&format!("  verdict = {}\n", wcl_string(v)));
        }
        if let Some(s) = &r.score {
            out.push_str(&format!("  score = {}\n", wcl_string(s)));
        }
        out.push_str("}\n\n");
    }
    parse_for_edit(&out, "<training output>").map_err(|e| BuildError::Parse(Report::new(e)))?;
    atomic_write(path, &out).map_err(|e| BuildError::Io(e, format!("write {}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok<T>(r: Result<T, BuildError>) -> T {
        match r {
            Ok(v) => v,
            Err(e) => panic!("{}", e.render_plain()),
        }
    }

    fn tempdir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("wcl-training-{}", gen_id('t')));
        std::fs::create_dir_all(&dir).expect("create tempdir");
        dir
    }

    #[test]
    fn record_then_grade_round_trips() {
        let dir = tempdir();
        let file = dir.join(SIDECAR);
        let id = ok(record(
            &file,
            "/",
            "lesson_one",
            "why",
            "because",
            "pending",
        ));

        let all = ok(list(&dir));
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].response, "because");
        assert!(all[0].is_pending());

        assert!(ok(grade(
            &dir,
            &id,
            "Good — covers the key point.",
            true,
            None
        )));
        let all = ok(list(&dir));
        assert_eq!(all[0].status, "graded");
        assert_eq!(
            all[0].verdict.as_deref(),
            Some("Good — covers the key point.")
        );
        assert!(all[0].passed());
    }

    #[test]
    fn re_answering_replaces_and_clears_the_verdict() {
        let dir = tempdir();
        let file = dir.join(SIDECAR);
        let id = ok(record(&file, "/", "l", "c", "first try", "pending"));
        assert!(ok(grade(&dir, &id, "Not quite.", false, None)));

        let again = ok(record(&file, "/", "l", "c", "second try", "pending"));
        assert_eq!(again, id, "the same check keeps one record");
        let all = ok(list(&dir));
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].response, "second try");
        assert_eq!(all[0].verdict, None, "a new submission is ungraded again");
        assert!(all[0].is_pending());
    }

    #[test]
    fn pending_answers_sort_first() {
        let dir = tempdir();
        let file = dir.join(SIDECAR);
        let done = ok(record(
            &file,
            "/",
            "a_lesson",
            "graded_check",
            "x",
            "pending",
        ));
        ok(record(&file, "/", "z_lesson", "open_check", "y", "pending"));
        assert!(ok(grade(&dir, &done, "ok", true, None)));

        let all = ok(list(&dir));
        assert_eq!(all[0].check, "open_check", "pending first, despite z > a");
        assert_eq!(all[1].check, "graded_check");
    }

    #[test]
    fn multiple_choice_records_arrive_already_graded() {
        let dir = tempdir();
        let file = dir.join(SIDECAR);
        ok(record(&file, "/", "l", "mc", "choice_a", "correct"));
        let all = ok(list(&dir));
        assert!(!all[0].is_pending(), "self-graded answers need no agent");
        assert!(all[0].passed());
    }

    #[test]
    fn find_scopes_to_the_course() {
        let dir = tempdir();
        let file = dir.join(SIDECAR);
        ok(record(&file, "/one/", "l", "shared_id", "first", "pending"));
        ok(record(
            &file,
            "/two/",
            "l",
            "shared_id",
            "second",
            "pending",
        ));
        let hit = ok(find(&dir, "/two/", "shared_id")).expect("found");
        assert_eq!(hit.response, "second");
    }
}
