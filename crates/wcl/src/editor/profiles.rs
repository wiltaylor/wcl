//! Wskill profile toggles for the editor's Design mode.
//!
//! `POST /api/wskill/profile` enables or disables one projection view of a
//! wskill (its `artifact` registry entry + the `wdoc/<view>/` files):
//!
//! - **disable** removes the matching `artifact` block(s) from the registry
//!   (through the validating [`crate::edit::commit`] pipeline) and deletes
//!   each artifact entry's projection folder — recoverable via git.
//! - **enable** re-evaluates the built-in `wskill` scaffold template with
//!   answers derived from the existing `topic`, writes the view's files
//!   that don't exist yet (merging aggregator imports into existing files
//!   via [`ast_edit::ensure_import`]), and splices the template's own
//!   `artifact` block for the view into the registry — all in one commit.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::Response;

use wcl_lang::ast::{Expr, Item};
use wcl_lang::{edit as ast_edit, format as wcl_format, parse_for_edit};

use super::preview::Sessions;
use super::{EditorState, Workspace, run_blocking};
use crate::serve::{json_error, parse_json_body};

/// The scaffold's directory name for an artifact kind (`wdoc/<dir>/`,
/// `data/<dir>/`, `schema/<dir>.wcl`).
fn view_dir(kind: &str) -> &str {
    match kind {
        "ai_skill" => "skill",
        other => other,
    }
}

/// Body: `{ registry, kind, enable }` — `registry` is the repo-relative
/// wskill registry file (the `wskill.wcl` the sites payload names), `kind`
/// an `ArtifactKind` name.
pub(super) async fn handle_profile(
    State(state): State<Arc<EditorState>>,
    body: String,
) -> Response {
    let v = match parse_json_body(&body) {
        Ok(v) => v,
        Err(e) => return json_error(StatusCode::BAD_REQUEST, &e),
    };
    let state2 = Arc::clone(&state);
    run_blocking(move || profile(&state2.ws, &state2.sessions, &v)).await
}

fn profile(
    ws: &Workspace,
    previews: &Sessions,
    v: &serde_json::Value,
) -> Result<serde_json::Value, String> {
    let registry = crate::edit::str_field(v, "registry")?;
    let registry_abs = ws.abs(registry)?;
    let kind = crate::edit::str_field(v, "kind")?;
    if !super::blocks::is_identifier(kind) {
        return Err(format!("`{kind}` is not an artifact kind"));
    }
    let enable = v
        .get("enable")
        .and_then(serde_json::Value::as_bool)
        .ok_or("missing `enable`")?;
    let result = if enable {
        enable_profile(&registry_abs, kind)
    } else {
        disable_profile(&registry_abs, kind)
    };
    if result.is_ok() {
        // Projection files were written or deleted: every built preview is
        // behind the disk.
        previews.invalidate();
    }
    result
}

/// The registry's `artifact` blocks of one kind: `(span, entry path)`.
fn artifacts_of_kind(src: &wcl_lang::ast::Source, kind: &str) -> Vec<(wcl_lang::Span, String)> {
    src.items
        .iter()
        .filter_map(|it| match it {
            Item::Block(b) if b.kind == "artifact" => {
                let is_kind = b.items.iter().any(|it| {
                    matches!(it, Item::Field(f) if f.name == "kind"
                        && matches!(&f.expr, Expr::Symbol(s) if s == kind))
                });
                let entry = b.items.iter().find_map(|it| match it {
                    Item::Field(f) if f.name == "entry" => match &f.expr {
                        Expr::Utf8(s) | Expr::Ascii(s) => Some(s.clone()),
                        _ => None,
                    },
                    _ => None,
                });
                (is_kind && entry.is_some()).then(|| (b.span, entry.unwrap()))
            }
            _ => None,
        })
        .collect()
}

fn disable_profile(registry_abs: &Path, kind: &str) -> Result<serde_json::Value, String> {
    let dir = registry_abs.parent().ok_or("registry has no parent")?;
    let text = crate::edit::read(registry_abs)?;
    let mut src =
        parse_for_edit(&text, registry_abs.display().to_string()).map_err(super::err_str)?;
    let victims = artifacts_of_kind(&src, kind);
    if victims.is_empty() {
        return Err(format!("no `{kind}` artifact in the registry"));
    }
    for (span, _) in &victims {
        ast_edit::remove_block_by_span(&mut src.items, *span);
    }
    // Projection folders of the artifacts that REMAIN must survive even if
    // two views share a folder (computed after the removal).
    let keep: Vec<PathBuf> = src
        .items
        .iter()
        .filter_map(|it| match it {
            Item::Block(b) if b.kind == "artifact" => b.items.iter().find_map(|it| match it {
                Item::Field(f) if f.name == "entry" => match &f.expr {
                    Expr::Utf8(s) | Expr::Ascii(s) => dir.join(s).parent().map(Path::to_path_buf),
                    _ => None,
                },
                _ => None,
            }),
            _ => None,
        })
        .collect();
    crate::edit::commit(
        registry_abs,
        vec![(registry_abs.to_path_buf(), wcl_format::to_source(&src))],
    )?;
    // Delete the removed views' projection folders — but never the wskill
    // root itself, anything outside it, or a folder another view still uses.
    let victim_count = victims.len();
    let mut removed_dirs: Vec<String> = Vec::new();
    for (_, entry) in victims {
        let Some(view_dir) = dir.join(&entry).parent().map(Path::to_path_buf) else {
            continue;
        };
        let canon = std::fs::canonicalize(&view_dir).unwrap_or(view_dir.clone());
        let root = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        let still_used = keep
            .iter()
            .any(|k| std::fs::canonicalize(k).unwrap_or_else(|_| k.clone()) == canon);
        if canon != root && canon.starts_with(&root) && !still_used {
            let _ = std::fs::remove_dir_all(&canon);
            removed_dirs.push(view_dir.display().to_string());
        }
    }
    Ok(serde_json::json!({
        "ok": true,
        "removed_artifacts": victim_count,
        "removed_dirs": removed_dirs,
    }))
}

fn enable_profile(registry_abs: &Path, kind: &str) -> Result<serde_json::Value, String> {
    let dir = registry_abs.parent().ok_or("registry has no parent")?;
    let text = crate::edit::read(registry_abs)?;
    let mut src =
        parse_for_edit(&text, registry_abs.display().to_string()).map_err(super::err_str)?;
    if !artifacts_of_kind(&src, kind).is_empty() {
        return Err(format!("a `{kind}` artifact already exists"));
    }

    // Answers from the existing wskill's topic block.
    let topic = src.items.iter().find_map(|it| match it {
        Item::Block(b) if b.kind == "topic" => Some(b),
        _ => None,
    });
    let field = |b: &wcl_lang::ast::Block, name: &str| {
        b.items.iter().find_map(|it| match it {
            Item::Field(f) if f.name == name => match &f.expr {
                Expr::Utf8(s) | Expr::Ascii(s) => Some(s.clone()),
                _ => None,
            },
            _ => None,
        })
    };
    let mut answers = BTreeMap::new();
    if let Some(t) = topic {
        if let Some(id) = super::util::ast_label(t) {
            answers.insert("topic_id".to_string(), id);
        }
        if let Some(n) = field(t, "name") {
            answers.insert("topic_name".to_string(), n);
        }
        if let Some(s) = field(t, "summary") {
            answers.insert("topic_summary".to_string(), s);
        }
        if let Some(d) = field(t, "created") {
            answers.insert("date".to_string(), d);
        }
    }
    answers.insert("include_presentation".to_string(), "yes".to_string());
    answers.insert("include_training".to_string(), "yes".to_string());
    let (files, _) = crate::scaffold::evaluate_template_tree("wskill", answers)?;

    // The view's files: everything under its wdoc dir (a re-add always
    // restores the projection), plus its data/schema starters only when
    // absent (they may hold user content or already exist).
    let vd = view_dir(kind);
    let wdoc_prefix = format!("wdoc/{vd}/");
    let data_prefix = format!("data/{vd}/");
    let schema_file = format!("schema/{vd}.wcl");
    let mut changes: Vec<(PathBuf, String)> = Vec::new();
    for (rel, content) in &files {
        let abs = dir.join(rel);
        let wanted = rel.starts_with(&wdoc_prefix)
            || ((rel.starts_with(&data_prefix) || *rel == schema_file) && !abs.exists());
        if wanted && !abs.exists() {
            changes.push((abs, content.clone()));
        }
    }
    // Merge aggregator imports: any generated file that exists on disk and
    // imports something we're creating gets the missing imports spliced in
    // (e.g. `data/main.wcl` gaining `import "presentation/main.wcl"`).
    let created: Vec<String> = changes
        .iter()
        .filter_map(|(p, _)| {
            p.strip_prefix(dir)
                .ok()
                .map(|r| r.to_string_lossy().replace('\\', "/"))
        })
        .collect();
    for (rel, content) in &files {
        let abs = dir.join(rel);
        if changes.iter().any(|(p, _)| *p == abs) || !abs.exists() {
            continue;
        }
        let Ok(generated) = parse_for_edit(content, rel.clone()) else {
            continue;
        };
        let gen_imports: Vec<String> = generated
            .items
            .iter()
            .filter_map(|it| match it {
                Item::Import(imp) if !imp.system => Some(imp.path.clone()),
                _ => None,
            })
            .collect();
        let base = std::path::Path::new(rel).parent().unwrap_or(Path::new(""));
        let needed: Vec<String> = gen_imports
            .into_iter()
            .filter(|imp| {
                let target = base
                    .join(imp.strip_prefix("./").unwrap_or(imp))
                    .to_string_lossy()
                    .replace('\\', "/");
                created.contains(&target)
            })
            .collect();
        if needed.is_empty() {
            continue;
        }
        let disk = crate::edit::read(&abs)?;
        let mut ast = parse_for_edit(&disk, abs.display().to_string()).map_err(super::err_str)?;
        let mut touched = false;
        for imp in needed {
            touched |= ast_edit::ensure_import(&mut ast, &imp);
        }
        if touched {
            changes.push((abs, wcl_format::to_source(&ast)));
        }
    }

    // The template's own artifact block for this kind, spliced verbatim.
    let gen_registry = files
        .iter()
        .find(|(rel, _)| rel == "wskill.wcl")
        .map(|(_, c)| c.as_str())
        .ok_or("the wskill template generated no wskill.wcl")?;
    let gen_src = parse_for_edit(gen_registry, "<generated wskill.wcl>").map_err(super::err_str)?;
    let artifact = gen_src
        .items
        .iter()
        .find_map(|it| match it {
            Item::Block(b) if b.kind == "artifact" => {
                let is_kind = b.items.iter().any(|it| {
                    matches!(it, Item::Field(f) if f.name == "kind"
                        && matches!(&f.expr, Expr::Symbol(s) if s == kind))
                });
                is_kind.then(|| b.clone())
            }
            _ => None,
        })
        .ok_or_else(|| format!("the wskill template has no `{kind}` artifact"))?;
    ast_edit::append_top_level_block(&mut src, artifact);
    changes.push((registry_abs.to_path_buf(), wcl_format::to_source(&src)));

    let written: Vec<String> = changes
        .iter()
        .map(|(p, _)| p.display().to_string())
        .collect();
    crate::edit::commit(registry_abs, changes)?;
    Ok(serde_json::json!({ "ok": true, "written": written }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::testsupport::workspace_built_by;

    /// Profile toggles on a real template-scaffolded wskill: disable
    /// removes the artifact + projection folder; enable scaffolds them
    /// back (files, aggregator imports, artifact block) and the document
    /// still validates.
    #[test]
    fn disable_then_enable() {
        // Scaffold a full wskill (with the presentation view) from the
        // built-in template.
        let (_td, ws) = workspace_built_by(|root| {
            let answers = std::collections::BTreeMap::from([
                ("topic_id".to_string(), "demo".to_string()),
                ("topic_name".to_string(), "Demo".to_string()),
                ("include_presentation".to_string(), "yes".to_string()),
                ("include_training".to_string(), "no".to_string()),
            ]);
            let (files, folders) =
                crate::scaffold::evaluate_template_tree("wskill", answers).unwrap();
            for dir in &folders {
                std::fs::create_dir_all(root.join(dir)).unwrap();
            }
            for (rel, content) in &files {
                // The scaffold with training off still emits only wanted files.
                let p = root.join(rel);
                std::fs::create_dir_all(p.parent().unwrap()).unwrap();
                std::fs::write(p, content).unwrap();
            }
            assert!(root.join("wdoc/presentation/main.wcl").is_file());
        });
        let root = ws.root_dir().to_path_buf();
        let previews = Sessions::default();
        let toggle = |kind: &str, enable: bool| {
            profile(
                &ws,
                &previews,
                &serde_json::json!({
                    "registry": "wskill.wcl", "kind": kind, "enable": enable,
                }),
            )
        };

        // Disable the presentation profile.
        toggle("presentation", false).expect("disable");
        let reg = std::fs::read_to_string(root.join("wskill.wcl")).unwrap();
        assert!(!reg.contains(":presentation"), "{reg}");
        assert!(!root.join("wdoc/presentation").exists());
        // The book view is untouched.
        assert!(root.join("wdoc/book/main.wcl").is_file());
        assert!(reg.contains(":book"));

        // Re-enable it: files + artifact come back, doc still validates.
        toggle("presentation", true).expect("enable");
        let reg = std::fs::read_to_string(root.join("wskill.wcl")).unwrap();
        assert!(reg.contains(":presentation"), "{reg}");
        assert!(root.join("wdoc/presentation/main.wcl").is_file());
        // Validating open of the registry succeeds (commit already gated
        // this; double-check directly).
        let doc = wcl_wdoc::open_doc_for_edit(&root.join("wskill.wcl")).unwrap();
        assert!(doc.schema_errors().is_empty());
        // Idempotence guards.
        let e = toggle("presentation", true).unwrap_err();
        assert!(e.contains("already exists"), "{e}");
        assert!(toggle("training", false).is_err());
    }
}
