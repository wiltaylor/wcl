//! Import loading: eager (at document open) and lazy (on first nested access).
//!
//! `expand_top_level_imports` is called from `Document::open_at` to flatten
//! every `import "..."` at file scope plus its transitive top-level imports
//! into a single `Vec<LoadedImport>` stashed on the document.
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

use super::cells::{ItemCellKind, ItemCells, LoadedImport};
use super::loader::FileLoader;
use super::validate::open_error;

pub(super) struct BlockSlice<'a> {
    pub(super) items: &'a [ast::Item],
    pub(super) cells: &'a [ItemCells],
}

pub(super) fn push_loaded_imports<'a>(cells: &'a [ItemCells], out: &mut Vec<BlockSlice<'a>>) {
    for cell in cells {
        if let ItemCellKind::Import { loaded, .. } = &cell.kind
            && let Some(Ok(li)) = loaded.get()
        {
            out.push(BlockSlice {
                items: &li.items,
                cells: &li.cells,
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
        });
        push_eager_imports(&imp.eager_imports, out);
    }
}

pub(super) fn load_import_lazily(
    path_str: &str,
    base_dir: Option<&Path>,
    path_span: Span,
    loader: &FileLoader,
) -> Result<LoadedImport, EvalError> {
    let path = resolve_import_path(base_dir, path_str)
        .map_err(|e| EvalError::import_failed(path_str, e, path_span))?;
    let src = loader(&path)
        .map_err(|e| EvalError::import_failed(path_str, format!("io: {e}"), path_span))?;
    let display = path.display().to_string();
    let (parsed_ast, parsed_symbols) = Parser::new(&src, &display)
        .parse_source()
        .map_err(|e| EvalError::import_failed(path_str, format!("{e}"), path_span))?;
    let imported_base = path.parent().map(Path::to_path_buf);
    let file_ns = first_namespace(&parsed_ast.items);

    let mut loading: HashSet<PathBuf> = HashSet::new();
    loading.insert(path.clone());
    let mut child_eager: Vec<LoadedImport> = Vec::new();
    expand_top_level_imports(
        &parsed_ast.items,
        imported_base.as_deref(),
        &mut loading,
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
        file_ns,
        items: parsed_ast.items,
        cells,
        symbols: parsed_symbols,
        eager_imports: child_eager,
    })
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
    loading: &mut HashSet<PathBuf>,
    out: &mut Vec<LoadedImport>,
    importer_file: &str,
    importer_source: &str,
    loader: &FileLoader,
) -> Result<(), ParseError> {
    for item in items {
        let ast::Item::Import(imp) = item else {
            continue;
        };
        let path = resolve_import_path(base_dir, &imp.path).map_err(|msg| {
            open_error(
                importer_source,
                importer_file,
                format!("failed to import '{}': {}", imp.path, msg),
                imp.path_span,
                "cannot resolve import",
            )
        })?;
        if !loading.insert(path.clone()) {
            return Err(open_error(
                importer_source,
                importer_file,
                format!("import cycle detected at '{}'", path.display()),
                imp.path_span,
                "cycle",
            ));
        }

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
            loading,
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
            file_ns,
            items: parsed_ast.items,
            cells,
            symbols: parsed_symbols,
            eager_imports: child_eager,
        });

        loading.remove(&path);
    }
    Ok(())
}
