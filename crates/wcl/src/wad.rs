//! `wcl wad spec` — derive a change-spec skeleton from a WAD diff.
//!
//! The mechanical half of the WAD change workflow: diff the working-tree WAD
//! against a reviewed git revision (`wcl diff`'s evaluated-view semantics,
//! imports resolved on both sides) and write a schema-valid `spec` block
//! into `data/specs/` carrying the exact entity/field change list. The
//! *intent* half — the typed `context` / `instructions` / `acceptance`
//! fields — is left as explicit TODOs for a human or agent to fill; the
//! tool never pretends to know why a change happened.

use std::path::{Path, PathBuf};

use crate::{EXIT_EVAL, EXIT_IO, EXIT_OK, diff, gitspec};

/// Output mode for `wad spec`.
#[derive(Clone, Copy, clap::ValueEnum)]
pub(crate) enum SpecFormat {
    /// Write the spec skeleton into `data/specs/<id>.wcl`.
    Wcl,
    /// Print the filtered change list as JSON to stdout (write nothing).
    Json,
}

/// Run `wcl wad spec --from <rev> [<entry>]`. Returns a CLI exit code.
pub(crate) fn run_spec(
    from: &str,
    entry: &Path,
    id: Option<String>,
    title: Option<String>,
    include_specs: bool,
    format: SpecFormat,
) -> u8 {
    let entry_str = entry.to_string_lossy();

    // Pin the baseline to a full sha before anything else — the spec must
    // record an immutable revision, not a moving branch name.
    let (root, _rel) = match gitspec::repo_rel(&entry_str) {
        Ok(x) => x,
        Err(e) => {
            eprintln!("{e}");
            return EXIT_IO;
        }
    };
    let sha = match gitspec::resolve_rev(from, &root) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("{e}");
            return EXIT_IO;
        }
    };

    // Old side = the WAD at <rev> (whole tree materialised, so imports —
    // including generated data — resolve from that revision); new side =
    // the working tree.
    let (old_doc, _old_tmp) = match crate::open_spec(&format!("{from}:{entry_str}")) {
        Ok(x) => x,
        Err(e) => return e.report(),
    };
    let (new_doc, _new_tmp) = match crate::open_spec(&entry_str) {
        Ok(x) => x,
        Err(e) => return e.report(),
    };

    let mut changes = diff::diff_documents(&old_doc, &new_doc);
    if !include_specs {
        // A diff dominated by previously-written specs is noise: the spec
        // being derived describes the *architecture* delta, not the spec
        // history. `--include-specs` disables the filter.
        changes.retain(|c| c.entity() != "spec" && !c.entity().starts_with("spec:"));
    }

    if changes.is_empty() {
        eprintln!("no changes since {from} ({sha}) — nothing to spec");
        return EXIT_OK;
    }

    if matches!(format, SpecFormat::Json) {
        return match serde_json::to_string_pretty(&diff::changes_to_json(&changes)) {
            Ok(s) => {
                println!("{s}");
                EXIT_OK
            }
            Err(e) => {
                eprintln!("json serialization failed: {e}");
                EXIT_EVAL
            }
        };
    }

    let spec_id = id.unwrap_or_else(|| format!("spec_from_{}", &sha[..8.min(sha.len())]));
    let spec_title = title.unwrap_or_else(|| format!("Changes since {from}"));
    let out_path = spec_path(entry, &spec_id);
    if out_path.exists() {
        eprintln!(
            "refusing to overwrite {} — pass --id to pick another name",
            out_path.display()
        );
        return EXIT_IO;
    }
    if let Some(dir) = out_path.parent()
        && let Err(e) = std::fs::create_dir_all(dir)
    {
        eprintln!("failed to create {}: {e}", dir.display());
        return EXIT_IO;
    }

    let text = render_spec(&spec_id, &spec_title, &sha, &changes);
    if let Err(e) = std::fs::write(&out_path, text) {
        eprintln!("failed to write {}: {e}", out_path.display());
        return EXIT_IO;
    }
    println!("wrote {}", out_path.display());
    println!(
        "next: add `import \"./{spec_id}.wcl\"` to the specs data hub, fill in the TODO context/instructions/acceptance, and keep `wcl check` green."
    );
    EXIT_OK
}

/// Where the skeleton lands: `<entry dir>/data/specs/<id>.wcl`.
fn spec_path(entry: &Path, id: &str) -> PathBuf {
    let dir = entry.parent().unwrap_or(Path::new("."));
    dir.join("data").join("specs").join(format!("{id}.wcl"))
}

/// Days-since-epoch → ISO date, via the standard civil-from-days algorithm
/// (Howard Hinnant). Dependency-free "today", UTC.
fn today_utc() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let z = (secs / 86_400) as i64 + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// The full skeleton file: banner, `spec` block with the mechanical facts
/// (baseline sha, change list) plus TODO intent fields for the author.
fn render_spec(id: &str, title: &str, sha: &str, changes: &[diff::Change]) -> String {
    let mut out = String::new();
    out.push_str(
        "// Change-spec skeleton generated by `wcl wad spec` — the change list is\n\
         // mechanical fact; fill in the summary and the typed context/instructions/\n\
         // acceptance, then move `status` as work proceeds (:planning → :in_progress\n\
         // → :complete).\n\
         namespace wcl.wad\n\n",
    );
    out.push_str(&format!("spec {id} {{\n"));
    out.push_str(&format!("  title   = \"{}\"\n", escape(title)));
    out.push_str("  status  = :planning\n");
    out.push_str(&format!("  created = \"{}\"\n", today_utc()));
    out.push_str("  summary = \"TODO: one line on why this change set exists.\"\n");
    out.push_str(
        "  context = \"TODO(context): the situation this change starts from — why this change set exists.\"\n",
    );
    out.push_str(&format!("  from_rev = \"{sha}\"\n\n"));
    out.push_str(&diff::render_spec_changes(changes));
    // The bar for the filled-in fields: enough detail that an implementing
    // agent needs nothing else — context, ordered instructions naming the
    // files to touch, and checkable acceptance criteria.
    out.push_str(
        "\n  instructions = [\n    \"TODO: ordered implementation steps — name the files/modules to touch.\",\n  ]\n  acceptance = [\n    \"TODO: checkable criteria that prove the work is done (commands, tests, rendered output).\",\n  ]\n}\n",
    );
    out
}

/// Escape a string for a WCL double-quoted literal.
fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}
