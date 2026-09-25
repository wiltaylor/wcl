//! `wcl diff <old> <new>` — a WCL-aware document diff.
//!
//! The comparison itself is [`wcl_lang::diff`]: it diffs the *evaluated*
//! documents entity by entity and field path by field path, so
//! formatting-only churn produces an empty diff. This module is the
//! command around it — opening each side (a path, or a `<rev>:<path>`
//! git spec), printing the warnings for anything that could not be
//! evaluated, rendering the result, and choosing the exit code.
//!
//! The diff renders one way: a re-parseable **WCL tree** — one `added` /
//! `removed` / `modified` block per entity, carrying the actual old/new
//! values. Because the output is itself a WCL document, a consumer that
//! wants structured data can pipe it back through `wcl parse` rather than
//! needing a second serialization format here.

use wcl_lang::diff::{Change, ChangeOp, diff_documents};
use wcl_lang::{Document, ParseError, Value};

use crate::out::{errln, out};
use crate::{EXIT_DIFFERS, EXIT_IO, EXIT_OK, gitspec, open_document, report_parse_error};

// ---------------------------------------------------------------------------
// WCL rendering (the only output)
// ---------------------------------------------------------------------------

/// Render the changes as a re-parseable WCL document. Entity keys and field
/// paths contain `:` / `.` / `[]`, so they are emitted as quoted string
/// labels; `kind` is a WCL symbol (`:changed`). An empty diff renders as a
/// comment-only document. The output is guaranteed to parse (validated by
/// the round-trip unit test); it is a report, not a faithful reconstruction
/// — see [`value_to_wcl`].
pub(crate) fn render_wcl(changes: &[Change], old_label: &str, new_label: &str) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# wcl diff {old_label} -> {new_label} — generated\n"
    ));
    if changes.is_empty() {
        out.push_str("# no changes\n");
        return out;
    }
    out.push('\n');
    for c in changes {
        match c.op {
            ChangeOp::Modified => {
                out.push_str(&format!("modified {} {{\n", quote_wcl(&c.entity)));
                for f in &c.fields {
                    let path = if f.path.is_empty() {
                        "<value>"
                    } else {
                        &f.path
                    };
                    out.push_str(&format!("  field {} {{\n", quote_wcl(path)));
                    out.push_str(&format!("    kind = :{}\n", f.kind.as_str()));
                    if let Some(o) = &f.old {
                        out.push_str(&format!("    old = {}\n", value_to_wcl(o)));
                    }
                    if let Some(n) = &f.new {
                        out.push_str(&format!("    new = {}\n", value_to_wcl(n)));
                    }
                    out.push_str("  }\n");
                }
                out.push_str("}\n\n");
            }
            // Added, removed, and any whole-entity op a later `wcl_lang`
            // adds: the op's name and the entity's record.
            _ => {
                out.push_str(&format!("{} {} {{\n", c.op.as_str(), quote_wcl(&c.entity)));
                if let Some(v) = &c.entity_value {
                    out.push_str(&format!("  value = {}\n", value_to_wcl(v)));
                }
                out.push_str("}\n\n");
            }
        }
    }
    out
}

/// Render a `Value` as a WCL expression that is guaranteed to parse.
///
/// Lists and records are emitted structurally (records as *bare* record
/// literals — the reified `ty` prefix, e.g. `Entity { … }`, is dropped
/// because `TypeName { … }` is not an expression). Scalars, strings,
/// identifiers, symbols and `none` use their round-trippable `Display`.
/// Forms whose `Display` does not re-parse as an expression — variants,
/// tensors, functions, data-paths, and the empty record (`{}` is a block,
/// not a record) — are quoted as a string: the diff is a report, not a
/// rebuild.
fn value_to_wcl(v: &Value) -> String {
    match v {
        Value::List(items) => {
            let inner: Vec<String> = items.iter().map(value_to_wcl).collect();
            format!("[{}]", inner.join(", "))
        }
        Value::Record { fields, .. } => {
            if fields.is_empty() {
                quote_wcl(&v.to_string())
            } else {
                let inner: Vec<String> = fields
                    .iter()
                    .map(|(k, val)| format!("{k}: {}", value_to_wcl(val)))
                    .collect();
                format!("{{ {} }}", inner.join(", "))
            }
        }
        Value::Bool(_)
        | Value::I8(_)
        | Value::I16(_)
        | Value::I32(_)
        | Value::I64(_)
        | Value::I128(_)
        | Value::Isize(_)
        | Value::U8(_)
        | Value::U16(_)
        | Value::U32(_)
        | Value::U64(_)
        | Value::U128(_)
        | Value::Usize(_)
        | Value::F32(_)
        | Value::F64(_)
        | Value::Utf8(_)
        | Value::Ascii(_)
        | Value::Utf16(_)
        | Value::Utf32(_)
        | Value::Identifier(_)
        | Value::Symbol(_)
        | Value::None => v.to_string(),
        // Variants, tensors, functions and data-paths, plus any value form
        // a later `wcl_lang` adds. A resolved document never carries an
        // unresolved unit literal either; quote it defensively rather than
        // emit a non-re-parseable form.
        _ => quote_wcl(&v.to_string()),
    }
}

/// Quote a string as a WCL inline string literal, escaping the characters
/// the lexer treats specially. Used for entity keys / field paths (which
/// aren't valid identifiers) and as the fallback for non-round-trippable
/// values.
fn quote_wcl(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}

// ---------------------------------------------------------------------------
// Command driver
// ---------------------------------------------------------------------------

/// Why opening one side of the diff failed. The two arms map to different
/// exit codes, so the failure has to stay distinguishable up to [`run`].
enum OpenErr {
    /// The document did not open: a working-tree file that could not be
    /// read, or any file that did not parse.
    Parse(ParseError),
    /// The document could not be read at all — a missing path, or a git
    /// revision that could not be materialized.
    Io(String),
}

impl OpenErr {
    /// Render the failure to stderr and yield the exit code it maps to.
    fn report(self) -> u8 {
        match self {
            OpenErr::Parse(e) => report_parse_error(e),
            OpenErr::Io(msg) => {
                errln!("{msg}");
                EXIT_IO
            }
        }
    }
}

/// Open one diff side. A plain path opens directly; a `<rev>:<path>` spec is
/// materialized from git into a temp dir first. The returned `TempDir` (if
/// any) must outlive use of the `Document`, so the caller holds it.
fn open_spec(arg: &str) -> Result<(Document, Option<tempfile::TempDir>), OpenErr> {
    match gitspec::parse_spec(arg) {
        gitspec::Spec::Working(path) => {
            let doc = open_document(&path).map_err(OpenErr::Parse)?;
            Ok((doc, None))
        }
        gitspec::Spec::Git { rev, path } => {
            let (root, rel) = gitspec::repo_rel(&path).map_err(OpenErr::Io)?;
            let tmp = gitspec::materialize_rev(&rev, &root).map_err(OpenErr::Io)?;
            let entry = tmp.path().join(&rel);
            if !entry.exists() {
                return Err(OpenErr::Io(format!(
                    "path '{rel}' not found in revision '{rev}'"
                )));
            }
            let doc = open_document(&entry).map_err(OpenErr::Parse)?;
            Ok((doc, Some(tmp)))
        }
    }
}

/// Entry point for the `diff` subcommand. Opens both sides (each a path or a
/// `<rev>:<path>` git spec), computes the WCL-aware entity/field diff, and
/// prints it as a WCL tree. Anything that could not be evaluated is named
/// on stderr as a warning and left out. A parse/git failure on either side
/// renders the diagnostic and yields a non-zero exit code. With
/// `exit_code`, a non-empty diff exits [`EXIT_DIFFERS`] instead of
/// [`EXIT_OK`].
pub(crate) fn run(old: &str, new: &str, exit_code: bool) -> u8 {
    // `_old`/`_new` hold the temp dirs alive until the diff is computed.
    let (old_doc, _old) = match open_spec(old) {
        Ok(x) => x,
        Err(e) => return e.report(),
    };
    let (new_doc, _new) = match open_spec(new) {
        Ok(x) => x,
        Err(e) => return e.report(),
    };
    let diff = diff_documents(&old_doc, &new_doc);
    let rendered = render_wcl(&diff.changes, old, new);
    // Release the git temp dirs before writing: a closed pipe ends the
    // process from inside a write, which would skip their `Drop`.
    drop((old_doc, new_doc, _old, _new));
    for warning in &diff.warnings {
        errln!("warning: {warning}");
    }
    out!("{rendered}");
    if exit_code && !diff.is_empty() {
        EXIT_DIFFERS
    } else {
        EXIT_OK
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn value_to_wcl_strips_record_type_prefix() {
        // A reified block record carries a `ty`; the WCL form must be a
        // bare record literal so it re-parses as an expression.
        let v = Value::Record {
            ty: vec!["Entity".to_string()],
            fields: std::sync::Arc::new(
                [("id".to_string(), Value::Utf8("u".into()))]
                    .into_iter()
                    .collect(),
            ),
        };
        assert_eq!(value_to_wcl(&v), "{ id: \"u\" }");
    }

    #[test]
    fn rendered_wcl_reparses() {
        // Build a representative diff and assert the emitted WCL is
        // well-formed (the "never emit non-parsing WCL" contract). The
        // changes come from real documents: `Change` is non_exhaustive, so
        // outside `wcl_lang` it cannot be built as a literal.
        let old = Document::open("domain_entity task {\n  status = \"draft\"\n}\n", "old.wcl")
            .expect("old side opens");
        let new = Document::open(
            "domain_entity task {\n  status = \"active\"\n}\n\
             spec impl {\n  tags = [\"a\"]\n}\n",
            "new.wcl",
        )
        .expect("new side opens");
        let changes = diff_documents(&old, &new).changes;
        let text = render_wcl(&changes, "old.wcl", "new.wcl");
        assert!(text.contains("modified \"domain_entity:task\""));
        assert!(text.contains("field \"status\""));
        assert!(text.contains("kind = :changed"));
        assert!(text.contains("old = \"draft\""));
        assert!(text.contains("new = \"active\""));
        assert!(text.contains("added \"spec:impl\""));
        // Re-parse to prove well-formedness (syntax only, no schema/eval).
        wcl_lang::parse_for_edit(&text, "<diff>").expect("rendered diff re-parses");
    }

    #[test]
    fn empty_diff_renders_comment_only() {
        let text = render_wcl(&[], "a.wcl", "b.wcl");
        assert!(text.contains("# no changes"));
        wcl_lang::parse_for_edit(&text, "<diff>").expect("empty diff re-parses");
    }
}
