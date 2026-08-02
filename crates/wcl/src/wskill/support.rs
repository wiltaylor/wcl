use std::fs;
use std::path::{Path, PathBuf};

use miette::Diagnostic;
use thiserror::Error;

use super::{WSKILL_FINDINGS, WSKILL_TOOL_FAILURE};

#[derive(Debug, Error, Diagnostic)]
pub(super) enum CommandError {
    #[error("{context}: {source}")]
    #[diagnostic(code(wskill::io))]
    Io {
        context: String,
        #[source]
        source: std::io::Error,
    },
    #[error("{0}")]
    #[diagnostic(code(wskill::invalid))]
    Invalid(String),
    #[error("{0}")]
    #[diagnostic(code(wskill::build))]
    Build(String),
    #[error("{0}")]
    #[diagnostic(code(wskill::collision))]
    Collision(String),
    #[error("{0}")]
    #[diagnostic(code(wskill::drift))]
    Drift(String),
    #[error("{0}")]
    #[diagnostic(code(wskill::stale))]
    Stale(String),
}

impl CommandError {
    pub(super) fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }

    fn exit_code(&self) -> u8 {
        match self {
            Self::Collision(_) | Self::Drift(_) | Self::Stale(_) => WSKILL_FINDINGS,
            Self::Io { .. } | Self::Invalid(_) | Self::Build(_) => WSKILL_TOOL_FAILURE,
        }
    }
}

pub(super) fn report(error: CommandError) -> u8 {
    let code = error.exit_code();
    eprintln!("{:?}", miette::Report::new(error));
    code
}

pub(super) fn report_all(errors: Vec<CommandError>) -> u8 {
    let code = errors
        .iter()
        .map(CommandError::exit_code)
        .max()
        .unwrap_or(super::WSKILL_OK);
    for error in errors {
        eprintln!("{:?}", miette::Report::new(error));
    }
    code
}

pub(super) struct Collection {
    pub(super) roots: Vec<PathBuf>,
    pub(super) complete_set: bool,
}

/// Find one wskill or a deterministic collection of them. Collection
/// discovery is filesystem-shaped rather than tied to `docs/wskills/`.
pub(super) fn discover(entry: &Path) -> Result<Collection, CommandError> {
    if let Some(root) = direct_wskill_root(entry) {
        return Ok(Collection {
            roots: vec![root],
            complete_set: false,
        });
    }
    if !entry.is_dir() {
        return Err(CommandError::Invalid(format!(
            "{} is not a wskill or directory",
            entry.display()
        )));
    }

    let mut roots = Vec::new();
    let mut pending = vec![entry.to_path_buf()];
    while let Some(dir) = pending.pop() {
        let entries = fs::read_dir(&dir)
            .map_err(|e| CommandError::io(format!("read {}", dir.display()), e))?;
        for child in entries {
            let path = child
                .map_err(|e| CommandError::io(format!("read {}", dir.display()), e))?
                .path();
            if !path.is_dir() || ignored_collection_dir(&path) {
                continue;
            }
            if path.join(wcl_wskill::ROOT_MARKER).is_file() {
                roots.push(path);
            } else {
                pending.push(path);
            }
        }
    }
    roots.sort();
    if roots.is_empty() {
        Err(CommandError::Invalid(format!(
            "no wskills found under {}",
            entry.display()
        )))
    } else {
        Ok(Collection {
            roots,
            complete_set: true,
        })
    }
}

fn direct_wskill_root(entry: &Path) -> Option<PathBuf> {
    if entry.is_file() {
        wcl_wskill::Registry::owner_dir(entry)
    } else if entry.join(wcl_wskill::ROOT_MARKER).is_file() {
        Some(entry.to_path_buf())
    } else {
        None
    }
}

fn ignored_collection_dir(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.starts_with('.') || matches!(name, "target" | "out" | "node_modules")
        })
}

pub(super) fn open_graph(root: &Path) -> Result<wcl_wskill::Graph, CommandError> {
    wcl_wskill::Graph::open(root).map_err(|e| CommandError::Invalid(e.to_string()))
}

/// Resolve and render one parsed-model view. This is the shared artifact
/// seam for `check` and `install`, so entry validation cannot drift.
pub(super) fn render_view(
    graph: &wcl_wskill::Graph,
    view: &wcl_wskill::View,
    out: &Path,
) -> Result<(), CommandError> {
    let entry = graph.root.join(&view.entry);
    if !entry.is_file() {
        return Err(CommandError::Invalid(format!(
            "artifact `{}` declares `{}`, but {} is not a file",
            view.id,
            view.entry,
            entry.display()
        )));
    }
    let result = if view.kind == "ai_skill" {
        wcl_wdoc::skill(&entry, out, None)
    } else {
        wcl_wdoc::build(&entry, out, None)
    };
    result.map(|_| ()).map_err(|error| {
        CommandError::Build(format!(
            "artifact `{}` ({}) failed to build:\n{}",
            view.id,
            view.entry,
            error.render_plain()
        ))
    })
}

pub(super) fn scratch(context: &str) -> Result<tempfile::TempDir, CommandError> {
    tempfile::tempdir().map_err(|e| CommandError::io(context, e))
}
