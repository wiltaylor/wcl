use std::fs;
use std::path::{Path, PathBuf};

use miette::Diagnostic;
use thiserror::Error;

use super::WSKILL_TOOL_FAILURE;

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
}

impl CommandError {
    pub(super) fn io(context: impl Into<String>, source: std::io::Error) -> Self {
        Self::Io {
            context: context.into(),
            source,
        }
    }
}

/// Every `CommandError` is a tool failure — an unreadable input, an
/// unreadable model, or a projection that would not build.
pub(super) fn report(error: CommandError) -> u8 {
    eprintln!("{:?}", miette::Report::new(error));
    WSKILL_TOOL_FAILURE
}

/// Find one wskill or a deterministic collection of them, as their root
/// folders. Discovery is filesystem-shaped rather than tied to
/// `docs/wskills/`.
pub(super) fn discover(entry: &Path) -> Result<Vec<PathBuf>, CommandError> {
    if let Some(root) = direct_wskill_root(entry) {
        return Ok(vec![root]);
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
        Ok(roots)
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

/// Resolve and render one parsed-model view: the entry must exist as a file
/// before the build is attempted, so a bad `artifact` reads as a bad
/// artifact rather than as a build failure.
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
    wcl_wdoc::build(&entry, out, None)
        .map(|_| ())
        .map_err(|error| {
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
