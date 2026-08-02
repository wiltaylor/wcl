//! Import loading: eager (at document open) and lazy (on first nested access).
//!
//! `expand_top_level_imports` is called from `Document::open_at` to flatten
//! every `import "..."` at file scope plus its transitive top-level imports
//! into a single `Vec<LoadedImport>` stashed on the document. Each module is
//! loaded at most once per document: a repeated or diamond import of an
//! already-loaded path (by canonical key) is a no-op, while a re-entrant
//! import of a path still in the active chain is a cycle error.
//!
//! `load_import_lazily` is called the first time evaluation crosses into a
//! block-scoped `import` cell; it parses, builds cells, and runs the same
//! eager-import expansion on the loaded file.
//!
//! `BlockSlice` plus `push_loaded_imports` / `push_eager_imports` are used by
//! the lookup helpers to splice imported items+cells into the search list
//! without copying them.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::ast::{self, Span};
use crate::error::{EvalError, ParseError};
use crate::parser::Parser;
use crate::symbols::SymbolIndex;

use super::cells::{ItemCellKind, ItemCells, LoadedImport};
use super::loader::FileLoader;
use super::validate::open_error;

pub(super) struct BlockSlice<'a> {
    pub(super) items: &'a [ast::Item],
    pub(super) cells: &'a [ItemCells],
    /// Namespace of the file the slice's items come from, so block
    /// instances spliced in by an import keep resolving bare kinds in
    /// their declaring file's namespace.
    pub(super) file_ns: &'a [String],
}

/// Cross-file import bookkeeping threaded down the eager-expansion
/// recursion. `loading` is the active import chain (re-entering a path
/// still in it is a cycle error); `seen` is every path already loaded
/// anywhere in the document (re-importing one is a no-op, so a repeated
/// `import <wdoc.wcl>` or a diamond loads the module just once).
#[derive(Default)]
pub(super) struct ImportState {
    loading: HashSet<PathBuf>,
    seen: HashSet<PathBuf>,
}

impl ImportState {
    /// Seed the state for a lazily-loaded file whose own `path` is
    /// already being loaded — so a self re-import is caught and it
    /// isn't reloaded by its own transitive imports.
    pub(super) fn for_lazy(path: PathBuf) -> Self {
        let mut s = Self::default();
        s.loading.insert(path.clone());
        s.seen.insert(path);
        s
    }
}

pub(super) fn push_loaded_imports<'a>(cells: &'a [ItemCells], out: &mut Vec<BlockSlice<'a>>) {
    for cell in cells {
        if let ItemCellKind::Import { loaded, .. } = &cell.kind
            && let Some(Ok(li)) = loaded.get()
        {
            out.push(BlockSlice {
                items: &li.items,
                cells: &li.cells,
                file_ns: &li.file_ns,
            });
            push_eager_imports(&li.eager_imports, out);
        }
    }
}

pub(super) fn push_eager_imports<'a>(imps: &'a [LoadedImport], out: &mut Vec<BlockSlice<'a>>) {
    for imp in imps {
        out.push(BlockSlice {
            items: &imp.items,
            cells: &imp.cells,
            file_ns: &imp.file_ns,
        });
        push_eager_imports(&imp.eager_imports, out);
    }
}

/// Collect the symbol index of every eagerly-imported file (recursively)
/// so the importing file's structural validation can resolve type
/// references against declarations that live in imports — e.g. a user
/// `lower` returning `list<Svg>` where the union is defined in
/// an imported schema file.
pub(super) fn collect_import_symbols<'a>(imps: &'a [LoadedImport], out: &mut Vec<&'a SymbolIndex>) {
    for imp in imps {
        out.push(&imp.symbols);
        collect_import_symbols(&imp.eager_imports, out);
    }
}

/// Collect the distinct, non-empty namespaces declared by every
/// eagerly-imported file (transitively). Used as additional resolution
/// search paths so a document's bare references to imported library
/// declarations resolve (importing a namespaced library brings its
/// names into scope).
pub(super) fn collect_import_namespaces(imps: &[LoadedImport], out: &mut Vec<Vec<String>>) {
    for imp in imps {
        if !imp.file_ns.is_empty() && !out.contains(&imp.file_ns) {
            out.push(imp.file_ns.clone());
        }
        collect_import_namespaces(&imp.eager_imports, out);
    }
}

pub(super) fn load_import_lazily(
    path_str: &str,
    base_dir: Option<&Path>,
    system: bool,
    path_span: Span,
    loader: &FileLoader,
) -> Result<LoadedImport, EvalError> {
    let path = resolve_import_path_kind(base_dir, path_str, system)
        .map_err(|e| EvalError::import_failed(path_str, e, path_span))?;
    let src = loader(&path)
        .map_err(|e| EvalError::import_failed(path_str, format!("io: {e}"), path_span))?;
    let display = path.display().to_string();
    let (parsed_ast, parsed_symbols) = Parser::new(&src, &display)
        .parse_source()
        .map_err(|e| EvalError::import_failed(path_str, format!("{e}"), path_span))?;
    let imported_base = path.parent().map(Path::to_path_buf);
    let file_ns = first_namespace(&parsed_ast.items);

    let mut state = ImportState::for_lazy(path.clone());
    let mut child_eager: Vec<LoadedImport> = Vec::new();
    expand_top_level_imports(
        &parsed_ast.items,
        imported_base.as_deref(),
        &mut state,
        &mut child_eager,
        &display,
        &src,
        loader,
    )
    .map_err(|e| EvalError::import_failed(path_str, format!("{e}"), path_span))?;

    let cells = parsed_ast
        .items
        .iter()
        .map(|i| ItemCells::build(i, imported_base.as_deref()))
        .collect();
    Ok(LoadedImport {
        path,
        source: src,
        file_ns,
        items: parsed_ast.items,
        cells,
        symbols: parsed_symbols,
        eager_imports: child_eager,
    })
}

/// Virtual root under which every angle-bracket `import <...>` (system)
/// import resolves. It is never a real filesystem path, so the
/// cycle-detection keys built from it can't collide with canonicalised
/// disk paths, and a [`FileLoader`] can route system lookups by stripping
/// this prefix (see `Registry::loader`).
pub const SYSTEM_IMPORT_ROOT: &str = "<wcl-system>";

/// Resolve an import path to the key used for loading + cycle detection,
/// branching on whether it's a system (`import <...>`) or disk
/// (`import "..."`) import.
///
/// System imports are resolved *within the registry namespace*: relative
/// to the importing file's registry directory when the importer is itself
/// a system file, otherwise from the registry root. They are never
/// canonicalised against the filesystem.
pub(super) fn resolve_import_path_kind(
    base_dir: Option<&Path>,
    path: &str,
    system: bool,
) -> Result<PathBuf, String> {
    if !system {
        return resolve_import_path(base_dir, path);
    }
    let sys_root = Path::new(SYSTEM_IMPORT_ROOT);
    // The importer's directory inside the registry namespace: empty when
    // the importer is a disk file (or the synthesised root), so its
    // system imports resolve from the registry root.
    let dir_rel = match base_dir {
        Some(d) if d.starts_with(sys_root) => d.strip_prefix(sys_root).unwrap_or(Path::new("")),
        _ => Path::new(""),
    };
    Ok(sys_root.join(lexical_normalize(&dir_rel.join(path))))
}

/// The registry key `import <path>` names when written in the file whose own
/// registry key is `importer` — `None` for a disk file, whose system imports
/// resolve from the registry root.
///
/// The same rule [`resolve_import_path_kind`] applies, in the vocabulary a
/// [`Registry`](super::Registry) speaks: keys, not `<wcl-system>` paths. For a
/// caller that reads a registered file directly rather than through a loader
/// (`wcl_wskill` walks its own embedded library) — so that it cannot disagree
/// with the resolver about what `<../../wdoc.wcl>` means.
pub fn system_import_key(importer: Option<&str>, path: &str) -> String {
    let dir = importer
        .and_then(|k| k.rsplit_once('/'))
        .map_or("", |(dir, _)| dir);
    lexical_normalize(&Path::new(dir).join(path))
        .to_string_lossy()
        .replace('\\', "/")
}

/// Collapse `.`/`..`/empty path components without touching the
/// filesystem. Used to resolve relative system-import paths within the
/// registry namespace (e.g. `../shared/x.wcl`).
fn lexical_normalize(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out: Vec<std::ffi::OsString> = Vec::new();
    for comp in path.components() {
        match comp {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            Component::Normal(s) => out.push(s.to_os_string()),
            // Prefix/RootDir shouldn't appear in registry-relative paths;
            // keep them verbatim if they do.
            other => out.push(other.as_os_str().to_os_string()),
        }
    }
    out.iter().collect()
}

/// Resolve an `import "path"` literal against an optional base
/// directory. Returns the canonicalised path on success. Returns
/// `Err(_)` when there's no base directory and the path is relative,
/// or when canonicalisation fails (file not found).
pub(super) fn resolve_import_path(base_dir: Option<&Path>, path: &str) -> Result<PathBuf, String> {
    let p = Path::new(path);
    let joined = if p.is_absolute() {
        p.to_path_buf()
    } else {
        match base_dir {
            Some(dir) => dir.join(p),
            None => {
                return Err(format!(
                    "no base directory to resolve relative import '{path}'; \
                     use Document::from_file or supply a base directory"
                ));
            }
        }
    };
    std::fs::canonicalize(&joined)
        .map_err(|e| format!("failed to resolve '{}': {e}", joined.display()))
}

/// Extract the file-namespace declared by the first `NamespaceDecl`
/// (if any) in an items list.
pub(super) fn first_namespace(items: &[ast::Item]) -> Vec<String> {
    items
        .iter()
        .find_map(|i| match i {
            ast::Item::NamespaceDecl(n) => Some(n.path.clone()),
            _ => None,
        })
        .unwrap_or_default()
}

/// Eagerly walk `items`, follow each top-level `Item::Import`, parse
/// the imported file, and append the resulting `LoadedImport` records
/// to `out`. Each `LoadedImport` carries its own symbol index whose
/// paths point into that loaded file's `items`/`cells` — lookups
/// across the document tree check the importer's index and then each
/// import's index in source order.
pub(super) fn expand_top_level_imports(
    items: &[ast::Item],
    base_dir: Option<&Path>,
    state: &mut ImportState,
    out: &mut Vec<LoadedImport>,
    importer_file: &str,
    importer_source: &str,
    loader: &FileLoader,
) -> Result<(), ParseError> {
    for item in items {
        let ast::Item::Import(imp) = item else {
            continue;
        };
        let path = resolve_import_path_kind(base_dir, &imp.path, imp.system).map_err(|msg| {
            open_error(
                importer_source,
                importer_file,
                format!("failed to import '{}': {}", imp.path, msg),
                imp.path_span,
                "cannot resolve import",
            )
        })?;
        // A path still in `loading` is an ancestor of the current import
        // chain — a real cycle, and an error.
        if state.loading.contains(&path) {
            return Err(open_error(
                importer_source,
                importer_file,
                format!("import cycle detected at '{}'", path.display()),
                imp.path_span,
                "cycle",
            ));
        }
        // A path already loaded elsewhere in the document — a repeated
        // import (e.g. `import <wdoc.wcl>` in several files) or a diamond
        // — is a no-op: the module's declarations are already spliced
        // into the tree, so importing it again would only duplicate them.
        if !state.seen.insert(path.clone()) {
            continue;
        }
        state.loading.insert(path.clone());

        let src = loader(&path).map_err(|e| {
            open_error(
                importer_source,
                importer_file,
                format!("failed to read '{}': {e}", path.display()),
                imp.path_span,
                "io error",
            )
        })?;
        let display = path.display().to_string();
        let (parsed_ast, parsed_symbols) = Parser::new(&src, &display).parse_source()?;
        let imported_base = path.parent().map(Path::to_path_buf);
        let file_ns = first_namespace(&parsed_ast.items);

        // Recursively process the imported file's own top-level imports.
        let mut child_eager: Vec<LoadedImport> = Vec::new();
        expand_top_level_imports(
            &parsed_ast.items,
            imported_base.as_deref(),
            state,
            &mut child_eager,
            &display,
            &src,
            loader,
        )?;

        // Build cells for the imported file with its own base_dir.
        let cells = parsed_ast
            .items
            .iter()
            .map(|i| ItemCells::build(i, imported_base.as_deref()))
            .collect();

        out.push(LoadedImport {
            path: path.clone(),
            source: src,
            file_ns,
            items: parsed_ast.items,
            cells,
            symbols: parsed_symbols,
            eager_imports: child_eager,
        });

        state.loading.remove(&path);
    }
    Ok(())
}
