use crate::lang::ast::*;
use crate::lang::diagnostic::{Diagnostic, DiagnosticBag};
use crate::lang::span::{SourceMap, Span};
use crate::lang::trivia::Trivia;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Trait for file system access (enables testing with in-memory FS).
pub trait FileSystem: Send + Sync {
    fn read_file(&self, path: &Path) -> Result<String, String>;
    fn canonicalize(&self, path: &Path) -> Result<PathBuf, String>;
    fn exists(&self, path: &Path) -> bool;
    /// Return all paths matching a glob pattern.
    fn glob(&self, pattern: &Path) -> Result<Vec<PathBuf>, String> {
        let _ = pattern;
        Err("glob not supported in this filesystem".to_string())
    }
}

/// Real file system implementation.
pub struct RealFileSystem;

impl FileSystem for RealFileSystem {
    fn read_file(&self, path: &Path) -> Result<String, String> {
        std::fs::read_to_string(path).map_err(|e| e.to_string())
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, String> {
        std::fs::canonicalize(path).map_err(|e| e.to_string())
    }

    fn exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn glob(&self, pattern: &Path) -> Result<Vec<PathBuf>, String> {
        let pattern_str = pattern
            .to_str()
            .ok_or_else(|| "invalid glob pattern".to_string())?;
        let mut paths: Vec<PathBuf> = glob::glob(pattern_str)
            .map_err(|e| format!("invalid glob pattern: {}", e))?
            .filter_map(|entry| entry.ok())
            .collect();
        paths.sort();
        Ok(paths)
    }
}

/// In-memory file system for testing.
pub struct InMemoryFs {
    pub files: HashMap<PathBuf, String>,
}

impl InMemoryFs {
    pub fn new() -> Self {
        InMemoryFs {
            files: HashMap::new(),
        }
    }

    pub fn add_file(&mut self, path: impl Into<PathBuf>, content: impl Into<String>) {
        self.files.insert(path.into(), content.into());
    }
}

impl Default for InMemoryFs {
    fn default() -> Self {
        Self::new()
    }
}

impl FileSystem for InMemoryFs {
    fn read_file(&self, path: &Path) -> Result<String, String> {
        self.files
            .get(path)
            .cloned()
            .ok_or_else(|| format!("file not found: {}", path.display()))
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, String> {
        // In-memory FS: just normalize the path components
        Ok(normalize_path(path))
    }

    fn exists(&self, path: &Path) -> bool {
        let normalized = normalize_path(path);
        self.files.contains_key(&normalized)
    }

    fn glob(&self, pattern: &Path) -> Result<Vec<PathBuf>, String> {
        let pattern_str = pattern
            .to_str()
            .ok_or_else(|| "invalid glob pattern".to_string())?;
        let pat =
            glob::Pattern::new(pattern_str).map_err(|e| format!("invalid glob pattern: {}", e))?;
        let mut paths: Vec<PathBuf> = self
            .files
            .keys()
            .filter(|p| pat.matches_path(p))
            .cloned()
            .collect();
        paths.sort();
        Ok(paths)
    }
}

/// Normalize a path by resolving `.` and `..` components without touching the filesystem.
pub fn normalize_path(path: &Path) -> PathBuf {
    let mut components = Vec::new();
    for component in path.components() {
        match component {
            std::path::Component::ParentDir => {
                components.pop();
            }
            std::path::Component::CurDir => {}
            other => {
                components.push(other);
            }
        }
    }
    components.iter().collect()
}

/// Configuration for library import search paths.
#[derive(Debug, Clone, Default)]
pub struct LibraryConfig {
    /// Extra library search paths (searched before defaults).
    pub extra_paths: Vec<PathBuf>,
    /// If true, skip the default XDG/system search paths entirely.
    pub no_default_paths: bool,
}

/// Return the search paths for well-known WCL library files.
///
/// When `config` is provided, extra paths are prepended and default paths
/// can be disabled with `no_default_paths`.
///
/// Default paths:
///
/// User library (searched first):
///   Linux/macOS: `$XDG_DATA_HOME/wcl/lib/` (default: `~/.local/share/wcl/lib/`)
///   Windows:     `%APPDATA%\wcl\lib\`
///
/// System library (searched second):
///   Linux:   each dir in `$XDG_DATA_DIRS` + `/wcl/lib/` (default: `/usr/local/share/wcl/lib/`, `/usr/share/wcl/lib/`)
///   macOS:   `/usr/local/share/wcl/lib/`
///   Windows: `%PROGRAMDATA%\wcl\lib\`
pub fn library_search_paths(config: &LibraryConfig) -> Vec<PathBuf> {
    let mut paths = Vec::new();

    // Extra paths first (highest priority)
    paths.extend(config.extra_paths.iter().cloned());

    if !config.no_default_paths {
        // User library dir
        if let Some(data_home) = std::env::var_os("XDG_DATA_HOME") {
            paths.push(PathBuf::from(data_home).join("wcl/lib"));
        } else if let Some(home) = std::env::var_os("HOME") {
            paths.push(PathBuf::from(home).join(".local/share/wcl/lib"));
        }
        #[cfg(windows)]
        if let Some(appdata) = std::env::var_os("APPDATA") {
            paths.push(PathBuf::from(appdata).join("wcl\\lib"));
        }

        // System library dirs
        if let Ok(data_dirs) = std::env::var("XDG_DATA_DIRS") {
            for dir in data_dirs.split(':') {
                if !dir.is_empty() {
                    paths.push(PathBuf::from(dir).join("wcl/lib"));
                }
            }
        } else {
            paths.push(PathBuf::from("/usr/local/share/wcl/lib"));
            paths.push(PathBuf::from("/usr/share/wcl/lib"));
        }
        #[cfg(windows)]
        if let Some(pd) = std::env::var_os("PROGRAMDATA") {
            paths.push(PathBuf::from(pd).join("wcl\\lib"));
        }
    }

    paths
}

/// Resolve a library import name to a file path by searching `library_search_paths()`.
pub fn resolve_library_import(
    name: &str,
    fs: &(impl FileSystem + ?Sized),
    config: &LibraryConfig,
) -> Option<PathBuf> {
    for dir in library_search_paths(config) {
        let candidate = dir.join(name);
        if fs.exists(&candidate) {
            return Some(candidate);
        }
    }
    None
}

/// Resolve `import_table(...)` expressions into inline tables (Phase 3a).
///
/// Walks the document, finds tables with `import_expr`, reads the CSV/TSV file,
/// and rewrites them to inline tables with columns and rows. This allows the
/// Phase 5 pre-evaluator to handle all tables uniformly for for-loop iteration.
pub fn resolve_import_tables<FS: FileSystem + ?Sized>(
    doc: &mut Document,
    fs: &FS,
    base_dir: &Path,
    diagnostics: &mut DiagnosticBag,
) {
    resolve_import_tables_in_items(&mut doc.items, fs, base_dir, diagnostics);
}

fn resolve_import_tables_in_items<FS: FileSystem + ?Sized>(
    items: &mut [DocItem],
    fs: &FS,
    base_dir: &Path,
    diagnostics: &mut DiagnosticBag,
) {
    for item in items.iter_mut() {
        match item {
            DocItem::Body(BodyItem::Table(table)) => {
                resolve_single_import_table(table, fs, base_dir, diagnostics);
            }
            DocItem::Body(BodyItem::Block(block)) => {
                resolve_import_tables_in_body(&mut block.body, fs, base_dir, diagnostics);
            }
            DocItem::Body(BodyItem::LetBinding(lb)) => {
                resolve_let_import_table(&mut lb.value, fs, base_dir, diagnostics);
            }
            DocItem::ExportLet(el) => {
                resolve_let_import_table(&mut el.value, fs, base_dir, diagnostics);
            }
            _ => {}
        }
    }
}

fn resolve_import_tables_in_body<FS: FileSystem + ?Sized>(
    body: &mut [BodyItem],
    fs: &FS,
    base_dir: &Path,
    diagnostics: &mut DiagnosticBag,
) {
    for item in body.iter_mut() {
        match item {
            BodyItem::Table(table) => {
                resolve_single_import_table(table, fs, base_dir, diagnostics);
            }
            BodyItem::Block(block) => {
                resolve_import_tables_in_body(&mut block.body, fs, base_dir, diagnostics);
            }
            BodyItem::LetBinding(lb) => {
                resolve_let_import_table(&mut lb.value, fs, base_dir, diagnostics);
            }
            _ => {}
        }
    }
}

fn resolve_let_import_table<FS: FileSystem + ?Sized>(
    expr: &mut Expr,
    fs: &FS,
    base_dir: &Path,
    diagnostics: &mut DiagnosticBag,
) {
    use crate::eval::control_flow::value_to_expr;
    use crate::eval::evaluator::parse_table;

    // Only handle import_table expressions
    let (args, span) = match expr {
        Expr::ImportTable(args, span) => (args, *span),
        _ => return,
    };

    // Extract plain string path (no interpolations at this phase)
    let path_str = string_lit_to_plain(&args.path);
    if path_str.contains("<interpolation>") {
        return;
    }

    let resolved = base_dir.join(&path_str);
    let content = match fs.read_file(&resolved) {
        Ok(c) => c,
        Err(e) => {
            diagnostics.error(
                format!("cannot read import_table file '{}': {}", path_str, e),
                span,
            );
            return;
        }
    };

    let separator = args
        .separator
        .as_ref()
        .and_then(|s| {
            let plain = string_lit_to_plain(s);
            plain.chars().next()
        })
        .unwrap_or(',');

    let has_headers = args.headers.unwrap_or(true);
    let explicit_columns: Option<Vec<String>> = args
        .columns
        .as_ref()
        .map(|cols| cols.iter().map(string_lit_to_plain).collect());

    let parsed = parse_table(
        &content,
        separator,
        has_headers,
        explicit_columns.as_deref(),
    );

    if let Some(list_expr) = value_to_expr(&parsed, span) {
        *expr = list_expr;
    }
}

fn resolve_single_import_table<FS: FileSystem + ?Sized>(
    table: &mut Table,
    fs: &FS,
    base_dir: &Path,
    diagnostics: &mut DiagnosticBag,
) {
    use crate::eval::evaluator::parse_table;
    use crate::eval::value::Value;
    use crate::lang::trivia::Trivia;

    let import_expr = match table.import_expr.take() {
        Some(expr) => expr,
        None => return,
    };

    let (args, span) = match *import_expr {
        Expr::ImportTable(args, span) => (args, span),
        other => {
            // Not an import_table expression — put it back
            table.import_expr = Some(Box::new(other));
            return;
        }
    };

    // Extract plain string path (no interpolations supported at this phase)
    let path_str = string_lit_to_plain(&args.path);
    if path_str.contains("<interpolation>") {
        // Path contains interpolations — can't resolve at this phase, put it back
        table.import_expr = Some(Box::new(Expr::ImportTable(args, span)));
        return;
    }

    // Resolve relative to base_dir
    let resolved = base_dir.join(&path_str);
    let content = match fs.read_file(&resolved) {
        Ok(c) => c,
        Err(e) => {
            diagnostics.error(
                format!("cannot read import_table file '{}': {}", path_str, e),
                span,
            );
            // Put import_expr back so it can fail at Phase 7 as before
            table.import_expr = Some(Box::new(Expr::ImportTable(args, span)));
            return;
        }
    };

    // Determine separator
    let separator = args
        .separator
        .as_ref()
        .and_then(|s| {
            let plain = string_lit_to_plain(s);
            plain.chars().next()
        })
        .unwrap_or(',');

    let has_headers = args.headers.unwrap_or(true);
    let explicit_columns: Option<Vec<String>> = args
        .columns
        .as_ref()
        .map(|cols| cols.iter().map(string_lit_to_plain).collect());

    // Parse the CSV content
    let parsed = parse_table(
        &content,
        separator,
        has_headers,
        explicit_columns.as_deref(),
    );

    // Convert Value back to AST columns + rows
    if let Value::List(rows) = parsed {
        // Determine column names from the first row (all rows have the same keys)
        let col_names: Vec<String> = if let Some(Value::Map(first)) = rows.first() {
            first.keys().cloned().collect()
        } else {
            vec![]
        };

        // Build ColumnDecl entries
        table.columns = col_names
            .iter()
            .map(|name| ColumnDecl {
                decorators: vec![],
                name: Ident {
                    name: name.clone(),
                    span,
                },
                type_expr: TypeExpr::String(span),
                trivia: Trivia::empty(),
                span,
            })
            .collect();

        // Build TableRow entries
        table.rows = rows
            .iter()
            .map(|row| {
                let cells = if let Value::Map(map) = row {
                    col_names
                        .iter()
                        .map(|col| {
                            let val = map.get(col).cloned().unwrap_or(Value::Null);
                            match val {
                                Value::String(s) => Expr::StringLit(StringLit {
                                    parts: vec![StringPart::Literal(s)],
                                    heredoc: None,
                                    span,
                                }),
                                _ => Expr::NullLit(span),
                            }
                        })
                        .collect()
                } else {
                    vec![]
                };
                TableRow { cells, span }
            })
            .collect();

        // import_expr is already None (we took it above)
    }
}

/// Resolves `import` directives in WCL documents.
///
/// Handles path resolution, jail checking, import-once semantics, depth limits,
/// and recursive resolution of imports within imported files.
/// A lazy import that was deferred during resolution.
/// It will only be loaded if its namespace is referenced in the document.
#[derive(Debug, Clone)]
pub struct LazyImport {
    pub import: Import,
    pub current_file: PathBuf,
    pub depth: u32,
}

pub struct ImportResolver<'a, FS: FileSystem + ?Sized> {
    fs: &'a FS,
    source_map: &'a mut SourceMap,
    root_dir: PathBuf,
    max_depth: u32,
    allow_imports: bool,
    loaded: HashSet<PathBuf>,
    diagnostics: DiagnosticBag,
    /// Library search paths computed once from `LibraryConfig` at construction.
    library_paths: Vec<PathBuf>,
    /// Directories containing resolved library files (used to relax jail checks).
    library_roots: HashSet<PathBuf>,
    /// Lazy imports collected during resolution, to be loaded on demand.
    lazy_imports: Vec<LazyImport>,
}

impl<'a, FS: FileSystem + ?Sized> ImportResolver<'a, FS> {
    pub fn new(
        fs: &'a FS,
        source_map: &'a mut SourceMap,
        root_dir: PathBuf,
        max_depth: u32,
        allow_imports: bool,
        library_config: LibraryConfig,
    ) -> Self {
        let library_paths = library_search_paths(&library_config);
        ImportResolver {
            fs,
            source_map,
            root_dir,
            max_depth,
            allow_imports,
            loaded: HashSet::new(),
            diagnostics: DiagnosticBag::new(),
            library_paths,
            library_roots: HashSet::new(),
            lazy_imports: Vec::new(),
        }
    }

    /// Resolve a library name using this resolver's cached search paths.
    fn resolve_library(&self, name: &str) -> Option<PathBuf> {
        for dir in &self.library_paths {
            let candidate = dir.join(name);
            if self.fs.exists(&candidate) {
                return Some(candidate);
            }
        }
        None
    }

    /// Check if a path is within any known library root directory.
    fn is_within_library_root(&self, path: &Path) -> bool {
        self.library_roots.iter().any(|root| path.starts_with(root))
    }

    /// Resolve all imports in a document, returning accumulated diagnostics.
    ///
    /// For each `DocItem::Import` in `doc.items`:
    /// 1. Resolve the path relative to `current_file`'s directory
    /// 2. Check jail (must be within `root_dir`)
    /// 3. Check depth limit
    /// 4. Check import-once cache
    /// 5. Read, lex, parse the imported file
    /// 6. Recursively resolve imports in the imported file
    /// 7. Merge the imported file's exportable items into the current document
    /// 8. Replace the `Import` item with the merged items
    pub fn resolve(
        &mut self,
        doc: &mut Document,
        current_file: &Path,
        depth: u32,
    ) -> DiagnosticBag {
        if !self.allow_imports {
            // If imports are disabled, report errors for any import directives
            for item in &doc.items {
                if let DocItem::Import(import) = item {
                    self.diagnostics
                        .error("imports are disabled in this context", import.span);
                }
            }
            return std::mem::take(&mut self.diagnostics);
        }

        // Track exported names across imports for E034 duplicate detection
        let mut exported_names: HashSet<String> = HashSet::new();

        // Collect indices of Import items (in reverse order for safe replacement)
        let import_indices: Vec<usize> = doc
            .items
            .iter()
            .enumerate()
            .filter_map(|(i, item)| {
                if matches!(item, DocItem::Import(_)) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();

        // Process imports in reverse order so indices remain valid during replacement
        for idx in import_indices.into_iter().rev() {
            let import = match &doc.items[idx] {
                DocItem::Import(imp) => imp.clone(),
                _ => unreachable!(),
            };

            // Lazy imports: collect for deferred resolution, remove from doc
            if import.lazy_namespace.is_some() {
                doc.items.remove(idx);
                self.lazy_imports.push(LazyImport {
                    import,
                    current_file: current_file.to_path_buf(),
                    depth,
                });
                continue;
            }

            let import_path_str = string_lit_to_plain(&import.path);
            let span = import.span;

            // Check depth limit
            if depth >= self.max_depth {
                self.diagnostics.error_with_code(
                    format!("import depth limit exceeded (max {})", self.max_depth),
                    span,
                    "E014",
                );
                continue;
            }

            // Determine which files to import (glob expansion or single file)
            let resolved_files: Vec<PathBuf> = if import.kind == ImportKind::Library {
                // Library imports: no glob support
                match self.resolve_library(&import_path_str) {
                    Some(p) => vec![p],
                    None => {
                        if !import.optional {
                            self.diagnostics.error_with_code(
                                format!("library '{}' not found in search paths", import_path_str),
                                span,
                                "E015",
                            );
                        } else {
                            doc.items.remove(idx);
                        }
                        continue;
                    }
                }
            } else if import_path_str.contains('*') {
                // Glob import: expand pattern
                let base_dir = current_file.parent().unwrap_or_else(|| Path::new("."));
                let pattern = normalize_path(&base_dir.join(&import_path_str));
                match self.fs.glob(&pattern) {
                    Ok(mut paths) => {
                        // Filter to .wcl files only
                        paths.retain(|p| p.extension().map(|e| e == "wcl").unwrap_or(false));
                        if paths.is_empty() && !import.optional {
                            self.diagnostics.error_with_code(
                                format!("glob pattern '{}' matched no .wcl files", import_path_str),
                                span,
                                "E016",
                            );
                        }
                        if paths.is_empty() {
                            continue;
                        }
                        paths
                    }
                    Err(e) => {
                        self.diagnostics.error_with_code(
                            format!("glob error for '{}': {}", import_path_str, e),
                            span,
                            "E016",
                        );
                        continue;
                    }
                }
            } else {
                // Single file import
                match self.resolve_path(&import_path_str, current_file) {
                    Ok(p) => vec![p],
                    Err(diag) => {
                        if !import.optional {
                            self.diagnostics.add(diag);
                        } else {
                            doc.items.remove(idx);
                        }
                        continue;
                    }
                }
            };

            // Check if the importing file is within a library root
            let current_in_library = self.is_within_library_root(current_file);

            // Process each resolved file
            let mut all_merged_items: Vec<DocItem> = Vec::new();
            for resolved in resolved_files {
                // Track library roots for resolved library files
                if import.kind == ImportKind::Library {
                    if let Some(parent) = resolved.parent() {
                        self.library_roots.insert(parent.to_path_buf());
                    }
                }

                // Jail check: skip for library imports, files within library roots,
                // and imports from files that are themselves within a library root
                if import.kind != ImportKind::Library
                    && !current_in_library
                    && !self.is_within_library_root(&resolved)
                {
                    if let Err(diag) = self.check_jail(&resolved, span) {
                        self.diagnostics.add(diag);
                        continue;
                    }
                }

                // Import-once: skip if already loaded
                if self.loaded.contains(&resolved) {
                    continue;
                }
                self.loaded.insert(resolved.clone());

                // Read the file
                let source = match self.fs.read_file(&resolved) {
                    Ok(s) => s,
                    Err(e) => {
                        if !import.optional {
                            self.diagnostics.error_with_code(
                                format!(
                                    "cannot read imported file '{}': {}",
                                    resolved.display(),
                                    e
                                ),
                                span,
                                "E010",
                            );
                        }
                        continue;
                    }
                };

                // Add to source map and parse
                let file_id = self
                    .source_map
                    .add_file(resolved.to_string_lossy().into_owned(), source.clone());
                let (mut imported_doc, parse_diags) = crate::lang::parse(&source, file_id);
                self.diagnostics.merge(parse_diags);

                // Recursively resolve imports in the imported document
                let child_diags = self.resolve(&mut imported_doc, &resolved, depth + 1);
                self.diagnostics.merge(child_diags);

                // E035: Check re-exports reference defined names in the imported file
                let defined_names: HashSet<&str> = imported_doc
                    .items
                    .iter()
                    .filter_map(|mi| match mi {
                        DocItem::ExportLet(el) => Some(el.name.name.as_str()),
                        DocItem::Body(BodyItem::LetBinding(lb)) => Some(lb.name.name.as_str()),
                        DocItem::Body(BodyItem::Block(b)) => {
                            b.inline_id.as_ref().and_then(|id| match id {
                                InlineId::Literal(lit) => Some(lit.value.as_str()),
                                _ => None,
                            })
                        }
                        _ => None,
                    })
                    .collect();
                for item in &imported_doc.items {
                    if let DocItem::ReExport(re_export) = item {
                        if !defined_names.contains(re_export.name.name.as_str()) {
                            self.diagnostics.error_with_code(
                                format!("re-export of undefined name '{}'", re_export.name.name),
                                re_export.span,
                                "E035",
                            );
                        }
                    }
                }

                // Collect mergeable items from the imported document
                for item in imported_doc.items {
                    match &item {
                        DocItem::Body(BodyItem::LetBinding(_)) => {}
                        DocItem::Import(_) => {}
                        DocItem::ExportLet(_)
                        | DocItem::ReExport(_)
                        | DocItem::Body(_)
                        | DocItem::FunctionDecl(_)
                        | DocItem::Namespace(_)
                        | DocItem::Use(_) => {
                            all_merged_items.push(item);
                        }
                    }
                }
            }

            // E034: Check for duplicate exported variable names across imports
            for item in &all_merged_items {
                if let DocItem::ExportLet(export) = item {
                    if !exported_names.insert(export.name.name.clone()) {
                        self.diagnostics.error_with_code(
                            format!(
                                "duplicate exported variable '{}' across imports",
                                export.name.name
                            ),
                            export.span,
                            "E034",
                        );
                    }
                }
            }

            // Replace the import directive with the merged items
            doc.items.remove(idx);
            for (offset, item) in all_merged_items.into_iter().enumerate() {
                doc.items.insert(idx + offset, item);
            }
        }

        std::mem::take(&mut self.diagnostics)
    }

    /// Resolve an import path string relative to the directory containing `current_file`.
    pub fn resolve_path(
        &self,
        import_path: &str,
        current_file: &Path,
    ) -> Result<PathBuf, Diagnostic> {
        let dummy_span = Span::dummy();

        // Reject absolute paths
        if import_path.starts_with('/') {
            return Err(Diagnostic::error(
                format!("absolute import paths are forbidden: '{}'", import_path),
                dummy_span,
            ));
        }

        // Reject home-relative paths
        if import_path.starts_with('~') {
            return Err(Diagnostic::error(
                format!(
                    "home-relative import paths are forbidden: '{}'",
                    import_path
                ),
                dummy_span,
            ));
        }

        // Reject remote/scheme paths
        if import_path.contains("://") {
            return Err(Diagnostic::error(
                format!("remote imports are forbidden: '{}'", import_path),
                dummy_span,
            )
            .with_code("E013"));
        }

        // Resolve relative to importing file's directory
        let base_dir = current_file.parent().unwrap_or_else(|| Path::new("."));
        let resolved = base_dir.join(import_path);

        // Canonicalize
        self.fs.canonicalize(&resolved).map_err(|e| {
            Diagnostic::error(
                format!("cannot resolve import path '{}': {}", import_path, e),
                dummy_span,
            )
        })
    }

    /// Check that a resolved path is within the root directory (jail check).
    pub fn check_jail(&self, resolved: &Path, span: Span) -> Result<(), Diagnostic> {
        let canonical_root = self
            .fs
            .canonicalize(&self.root_dir)
            .unwrap_or_else(|_| self.root_dir.clone());

        if !resolved.starts_with(&canonical_root) {
            return Err(Diagnostic::error(
                format!(
                    "import path '{}' escapes root directory '{}'",
                    resolved.display(),
                    canonical_root.display()
                ),
                span,
            )
            .with_code("E011"));
        }
        Ok(())
    }

    /// Return the set of files that were actually loaded during resolution.
    pub fn loaded_paths(&self) -> &HashSet<PathBuf> {
        &self.loaded
    }

    /// Return collected lazy imports.
    pub fn lazy_imports(&self) -> &[LazyImport] {
        &self.lazy_imports
    }

    /// Take the collected lazy imports out of the resolver.
    pub fn take_lazy_imports(&mut self) -> Vec<LazyImport> {
        std::mem::take(&mut self.lazy_imports)
    }

    /// Resolve lazy imports whose namespaces are referenced in the document.
    ///
    /// For each lazy import whose namespace (or a nested namespace) is referenced,
    /// load the file, wrap its exported items in a `NamespaceDecl`, and insert
    /// into `doc.items`.
    pub fn resolve_lazy(
        &mut self,
        doc: &mut Document,
        lazy_imports: &[LazyImport],
        referenced: &HashSet<String>,
    ) -> DiagnosticBag {
        for lazy in lazy_imports {
            let ns_path = lazy
                .import
                .lazy_namespace
                .as_ref()
                .expect("lazy import must have namespace");
            let ns_str = join_path(ns_path);

            // Skip if this namespace is not referenced
            if !referenced.contains(&ns_str) {
                continue;
            }

            let import = &lazy.import;
            let import_path_str = string_lit_to_plain(&import.path);
            let span = import.span;
            let depth = lazy.depth;

            // Check depth limit
            if depth >= self.max_depth {
                self.diagnostics.error_with_code(
                    format!("import depth limit exceeded (max {})", self.max_depth),
                    span,
                    "E014",
                );
                continue;
            }

            // Resolve file path(s) — same logic as regular imports
            let resolved_files: Vec<PathBuf> = if import.kind == ImportKind::Library {
                match self.resolve_library(&import_path_str) {
                    Some(p) => vec![p],
                    None => {
                        if !import.optional {
                            self.diagnostics.error_with_code(
                                format!("library '{}' not found in search paths", import_path_str),
                                span,
                                "E015",
                            );
                        }
                        continue;
                    }
                }
            } else if import_path_str.contains('*') {
                let base_dir = lazy.current_file.parent().unwrap_or_else(|| Path::new("."));
                let pattern = normalize_path(&base_dir.join(&import_path_str));
                match self.fs.glob(&pattern) {
                    Ok(mut paths) => {
                        paths.retain(|p| p.extension().map(|e| e == "wcl").unwrap_or(false));
                        if paths.is_empty() && !import.optional {
                            self.diagnostics.error_with_code(
                                format!("glob pattern '{}' matched no .wcl files", import_path_str),
                                span,
                                "E016",
                            );
                        }
                        if paths.is_empty() {
                            continue;
                        }
                        paths
                    }
                    Err(e) => {
                        self.diagnostics.error_with_code(
                            format!("glob error for '{}': {}", import_path_str, e),
                            span,
                            "E016",
                        );
                        continue;
                    }
                }
            } else {
                match self.resolve_path(&import_path_str, &lazy.current_file) {
                    Ok(p) => vec![p],
                    Err(diag) => {
                        if !import.optional {
                            self.diagnostics.add(diag);
                        }
                        continue;
                    }
                }
            };

            let current_in_library = self.is_within_library_root(&lazy.current_file);

            let mut ns_items: Vec<DocItem> = Vec::new();
            for resolved in resolved_files {
                if import.kind == ImportKind::Library {
                    if let Some(parent) = resolved.parent() {
                        self.library_roots.insert(parent.to_path_buf());
                    }
                }

                if import.kind != ImportKind::Library
                    && !current_in_library
                    && !self.is_within_library_root(&resolved)
                {
                    if let Err(diag) = self.check_jail(&resolved, span) {
                        self.diagnostics.add(diag);
                        continue;
                    }
                }

                if self.loaded.contains(&resolved) {
                    continue;
                }
                self.loaded.insert(resolved.clone());

                let source = match self.fs.read_file(&resolved) {
                    Ok(s) => s,
                    Err(e) => {
                        if !import.optional {
                            self.diagnostics.error_with_code(
                                format!(
                                    "cannot read imported file '{}': {}",
                                    resolved.display(),
                                    e
                                ),
                                span,
                                "E010",
                            );
                        }
                        continue;
                    }
                };

                let file_id = self
                    .source_map
                    .add_file(resolved.to_string_lossy().into_owned(), source.clone());
                let (mut imported_doc, parse_diags) = crate::lang::parse(&source, file_id);
                self.diagnostics.merge(parse_diags);

                let child_diags = self.resolve(&mut imported_doc, &resolved, depth + 1);
                self.diagnostics.merge(child_diags);

                // Collect mergeable items (same filter as regular imports)
                for item in imported_doc.items {
                    match &item {
                        DocItem::Body(BodyItem::LetBinding(_)) => {}
                        DocItem::Import(_) => {}
                        DocItem::ExportLet(_)
                        | DocItem::ReExport(_)
                        | DocItem::Body(_)
                        | DocItem::FunctionDecl(_)
                        | DocItem::Namespace(_)
                        | DocItem::Use(_) => {
                            ns_items.push(item);
                        }
                    }
                }
            }

            if !ns_items.is_empty() {
                // Wrap all imported items in the lazy namespace
                let ns_decl = NamespaceDecl {
                    path: ns_path.clone(),
                    items: ns_items,
                    file_level: false,
                    trivia: Trivia::default(),
                    span,
                };
                doc.items.push(DocItem::Namespace(ns_decl));
            }
        }

        std::mem::take(&mut self.diagnostics)
    }

    /// Consume the resolver and return accumulated diagnostics.
    pub fn into_diagnostics(self) -> DiagnosticBag {
        self.diagnostics
    }
}

/// Extract a plain string from a `StringLit` (ignoring interpolations for path resolution).
fn string_lit_to_plain(lit: &StringLit) -> String {
    let mut result = String::new();
    for part in &lit.parts {
        match part {
            StringPart::Literal(s) => result.push_str(s),
            StringPart::Interpolation(_) => {
                // Interpolated import paths are not supported; ignore for now
                result.push_str("<interpolation>");
            }
        }
    }
    result
}

/// Scan a document's AST for references to any of the given lazy namespace prefixes.
///
/// Returns the set of lazy namespace strings that are referenced anywhere in the
/// document — in `use` declarations, block kinds, schema names, let bindings,
/// decorator arguments, expressions, etc.
pub fn find_lazy_namespace_references(
    doc: &Document,
    lazy_namespaces: &[String],
) -> HashSet<String> {
    let mut referenced = HashSet::new();
    if lazy_namespaces.is_empty() {
        return referenced;
    }

    for item in &doc.items {
        scan_doc_item(item, lazy_namespaces, &mut referenced);
    }
    referenced
}

fn starts_with_lazy_ns<'a>(name: &str, lazy_namespaces: &'a [String]) -> Option<&'a str> {
    for ns in lazy_namespaces {
        if name == ns.as_str() || name.starts_with(&format!("{}::", ns)) {
            return Some(ns.as_str());
        }
    }
    None
}

fn check_name(name: &str, lazy_namespaces: &[String], referenced: &mut HashSet<String>) {
    if let Some(ns) = starts_with_lazy_ns(name, lazy_namespaces) {
        referenced.insert(ns.to_string());
    }
}

fn check_string_lit(lit: &StringLit, lazy_namespaces: &[String], referenced: &mut HashSet<String>) {
    let mut text = String::new();
    for part in &lit.parts {
        if let StringPart::Literal(s) = part {
            text.push_str(s);
        }
    }
    check_name(&text, lazy_namespaces, referenced);
}

fn scan_doc_item(item: &DocItem, lazy_namespaces: &[String], referenced: &mut HashSet<String>) {
    match item {
        DocItem::Use(use_decl) => {
            // Check if the use path starts with a lazy namespace
            if let Some(first) = use_decl.namespace_path.first() {
                let full_path = join_path(&use_decl.namespace_path);
                check_name(&full_path, lazy_namespaces, referenced);
                check_name(&first.name, lazy_namespaces, referenced);
            }
        }
        DocItem::Namespace(ns) => {
            let ns_path = join_path(&ns.path);
            check_name(&ns_path, lazy_namespaces, referenced);
            for inner in &ns.items {
                scan_doc_item(inner, lazy_namespaces, referenced);
            }
        }
        DocItem::Body(body) => {
            scan_body_item(body, lazy_namespaces, referenced);
        }
        DocItem::ExportLet(el) => {
            check_name(&el.name.name, lazy_namespaces, referenced);
            scan_expr(&el.value, lazy_namespaces, referenced);
        }
        DocItem::FunctionDecl(fd) => {
            check_name(&fd.name.name, lazy_namespaces, referenced);
        }
        DocItem::ReExport(re) => {
            check_name(&re.name.name, lazy_namespaces, referenced);
        }
        DocItem::Import(_) => {}
    }
}

fn scan_body_item(item: &BodyItem, lazy_namespaces: &[String], referenced: &mut HashSet<String>) {
    match item {
        BodyItem::Block(b) => {
            check_name(&b.kind.name, lazy_namespaces, referenced);
            for dec in &b.decorators {
                scan_decorator(dec, lazy_namespaces, referenced);
            }
            for child in &b.body {
                scan_body_item(child, lazy_namespaces, referenced);
            }
        }
        BodyItem::Schema(s) => {
            check_string_lit(&s.name, lazy_namespaces, referenced);
            for dec in &s.decorators {
                scan_decorator(dec, lazy_namespaces, referenced);
            }
            for field in &s.fields {
                for dec in &field.decorators_before {
                    scan_decorator(dec, lazy_namespaces, referenced);
                }
                for dec in &field.decorators_after {
                    scan_decorator(dec, lazy_namespaces, referenced);
                }
            }
        }
        BodyItem::DecoratorSchema(ds) => {
            check_string_lit(&ds.name, lazy_namespaces, referenced);
        }
        BodyItem::LetBinding(lb) => {
            check_name(&lb.name.name, lazy_namespaces, referenced);
            scan_expr(&lb.value, lazy_namespaces, referenced);
        }
        BodyItem::Attribute(attr) => {
            scan_expr(&attr.value, lazy_namespaces, referenced);
            for dec in &attr.decorators {
                scan_decorator(dec, lazy_namespaces, referenced);
            }
        }
        BodyItem::MacroDef(m) => {
            check_name(&m.name.name, lazy_namespaces, referenced);
        }
        BodyItem::MacroCall(mc) => {
            check_name(&mc.name.name, lazy_namespaces, referenced);
        }
        BodyItem::Table(t) => {
            for col in &t.columns {
                check_name(&col.name.name, lazy_namespaces, referenced);
            }
        }
        BodyItem::ForLoop(fl) => {
            scan_expr(&fl.iterable, lazy_namespaces, referenced);
            for child in &fl.body {
                scan_body_item(child, lazy_namespaces, referenced);
            }
        }
        BodyItem::Conditional(cond) => {
            scan_conditional(cond, lazy_namespaces, referenced);
        }
        BodyItem::Validation(v) => {
            for lb in &v.lets {
                scan_expr(&lb.value, lazy_namespaces, referenced);
            }
            scan_expr(&v.check, lazy_namespaces, referenced);
            scan_expr(&v.message, lazy_namespaces, referenced);
        }
        BodyItem::StructDef(s) => {
            check_string_lit(&s.name, lazy_namespaces, referenced);
        }
        BodyItem::SymbolSetDecl(ss) => {
            check_name(&ss.name.name, lazy_namespaces, referenced);
        }
    }
}

fn scan_decorator(dec: &Decorator, lazy_namespaces: &[String], referenced: &mut HashSet<String>) {
    check_name(&dec.name.name, lazy_namespaces, referenced);
    for arg in &dec.args {
        match arg {
            DecoratorArg::Positional(e) => scan_expr(e, lazy_namespaces, referenced),
            DecoratorArg::Named(name, e) => {
                check_name(&name.name, lazy_namespaces, referenced);
                scan_expr(e, lazy_namespaces, referenced);
            }
        }
    }
}

fn scan_conditional(
    cond: &Conditional,
    lazy_namespaces: &[String],
    referenced: &mut HashSet<String>,
) {
    scan_expr(&cond.condition, lazy_namespaces, referenced);
    for child in &cond.then_body {
        scan_body_item(child, lazy_namespaces, referenced);
    }
    if let Some(ref else_branch) = cond.else_branch {
        match else_branch {
            ElseBranch::ElseIf(nested) => {
                scan_conditional(nested, lazy_namespaces, referenced);
            }
            ElseBranch::Else(body, _, _) => {
                for child in body {
                    scan_body_item(child, lazy_namespaces, referenced);
                }
            }
        }
    }
}

fn scan_expr(expr: &Expr, lazy_namespaces: &[String], referenced: &mut HashSet<String>) {
    match expr {
        Expr::Ident(ident) => {
            check_name(&ident.name, lazy_namespaces, referenced);
        }
        Expr::IdentifierLit(lit) => {
            check_name(&lit.value, lazy_namespaces, referenced);
        }
        Expr::StringLit(lit) => {
            check_string_lit(lit, lazy_namespaces, referenced);
        }
        Expr::BinaryOp(lhs, _, rhs, _) => {
            scan_expr(lhs, lazy_namespaces, referenced);
            scan_expr(rhs, lazy_namespaces, referenced);
        }
        Expr::UnaryOp(_, inner, _) => {
            scan_expr(inner, lazy_namespaces, referenced);
        }
        Expr::FnCall(callee, args, _) => {
            scan_expr(callee, lazy_namespaces, referenced);
            for arg in args {
                match arg {
                    CallArg::Positional(e) => scan_expr(e, lazy_namespaces, referenced),
                    CallArg::Named(name, e) => {
                        check_name(&name.name, lazy_namespaces, referenced);
                        scan_expr(e, lazy_namespaces, referenced);
                    }
                }
            }
        }
        Expr::List(items, _) => {
            for item in items {
                scan_expr(item, lazy_namespaces, referenced);
            }
        }
        Expr::Map(entries, _) => {
            for (key, val) in entries {
                if let MapKey::Ident(ident) = key {
                    check_name(&ident.name, lazy_namespaces, referenced);
                }
                scan_expr(val, lazy_namespaces, referenced);
            }
        }
        Expr::MemberAccess(base, _, _) => {
            scan_expr(base, lazy_namespaces, referenced);
        }
        Expr::IndexAccess(base, idx, _) => {
            scan_expr(base, lazy_namespaces, referenced);
            scan_expr(idx, lazy_namespaces, referenced);
        }
        Expr::Ternary(cond, then_expr, else_expr, _) => {
            scan_expr(cond, lazy_namespaces, referenced);
            scan_expr(then_expr, lazy_namespaces, referenced);
            scan_expr(else_expr, lazy_namespaces, referenced);
        }
        Expr::Lambda(_, body, _) => {
            scan_expr(body, lazy_namespaces, referenced);
        }
        Expr::BlockExpr(lets, body, _) => {
            for lb in lets {
                scan_expr(&lb.value, lazy_namespaces, referenced);
            }
            scan_expr(body, lazy_namespaces, referenced);
        }
        Expr::Paren(inner, _) => {
            scan_expr(inner, lazy_namespaces, referenced);
        }
        // Ref, Query, literals, ImportRaw, ImportTable — no namespace references
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_source_map() -> SourceMap {
        SourceMap::new()
    }

    #[test]
    fn resolve_path_relative_to_current_file() {
        let fs = InMemoryFs::new();
        let mut sm = make_source_map();
        let resolver = ImportResolver::new(
            &fs,
            &mut sm,
            PathBuf::from("/project"),
            32,
            true,
            LibraryConfig::default(),
        );

        let result = resolver
            .resolve_path("./schemas.wcl", Path::new("/project/main.wcl"))
            .unwrap();
        assert_eq!(result, PathBuf::from("/project/schemas.wcl"));
    }

    #[test]
    fn resolve_path_nested_relative() {
        let fs = InMemoryFs::new();
        let mut sm = make_source_map();
        let resolver = ImportResolver::new(
            &fs,
            &mut sm,
            PathBuf::from("/project"),
            32,
            true,
            LibraryConfig::default(),
        );

        let result = resolver
            .resolve_path("./sub/file.wcl", Path::new("/project/dir/main.wcl"))
            .unwrap();
        assert_eq!(result, PathBuf::from("/project/dir/sub/file.wcl"));
    }

    #[test]
    fn resolve_path_rejects_absolute() {
        let fs = InMemoryFs::new();
        let mut sm = make_source_map();
        let resolver = ImportResolver::new(
            &fs,
            &mut sm,
            PathBuf::from("/project"),
            32,
            true,
            LibraryConfig::default(),
        );

        let result = resolver.resolve_path("/etc/passwd", Path::new("/project/main.wcl"));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .message
            .contains("absolute import paths are forbidden"));
    }

    #[test]
    fn resolve_path_rejects_home_relative() {
        let fs = InMemoryFs::new();
        let mut sm = make_source_map();
        let resolver = ImportResolver::new(
            &fs,
            &mut sm,
            PathBuf::from("/project"),
            32,
            true,
            LibraryConfig::default(),
        );

        let result = resolver.resolve_path("~/file.wcl", Path::new("/project/main.wcl"));
        assert!(result.is_err());
        assert!(result.unwrap_err().message.contains("home-relative"));
    }

    #[test]
    fn resolve_path_rejects_remote() {
        let fs = InMemoryFs::new();
        let mut sm = make_source_map();
        let resolver = ImportResolver::new(
            &fs,
            &mut sm,
            PathBuf::from("/project"),
            32,
            true,
            LibraryConfig::default(),
        );

        let result = resolver.resolve_path(
            "https://example.com/file.wcl",
            Path::new("/project/main.wcl"),
        );
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .message
            .contains("remote imports are forbidden"));
    }

    #[test]
    fn jail_check_allows_within_root() {
        let fs = InMemoryFs::new();
        let mut sm = make_source_map();
        let resolver = ImportResolver::new(
            &fs,
            &mut sm,
            PathBuf::from("/project"),
            32,
            true,
            LibraryConfig::default(),
        );

        let result = resolver.check_jail(Path::new("/project/sub/file.wcl"), Span::dummy());
        assert!(result.is_ok());
    }

    #[test]
    fn jail_check_rejects_outside_root() {
        let fs = InMemoryFs::new();
        let mut sm = make_source_map();
        let resolver = ImportResolver::new(
            &fs,
            &mut sm,
            PathBuf::from("/project"),
            32,
            true,
            LibraryConfig::default(),
        );

        let result = resolver.check_jail(Path::new("/other/file.wcl"), Span::dummy());
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .message
            .contains("escapes root directory"));
    }

    #[test]
    fn jail_check_rejects_parent_traversal() {
        let mut fs = InMemoryFs::new();
        fs.add_file(PathBuf::from("/project/sub/../../../etc/passwd"), "bad");
        let mut sm = make_source_map();
        let resolver = ImportResolver::new(
            &fs,
            &mut sm,
            PathBuf::from("/project"),
            32,
            true,
            LibraryConfig::default(),
        );

        // After normalization, /project/sub/../../../etc/passwd becomes /etc/passwd
        let normalized = normalize_path(Path::new("/project/sub/../../../etc/passwd"));
        let result = resolver.check_jail(&normalized, Span::dummy());
        assert!(result.is_err());
    }

    #[test]
    fn normalize_path_resolves_dot_and_dotdot() {
        assert_eq!(
            normalize_path(Path::new("/a/b/../c/./d")),
            PathBuf::from("/a/c/d")
        );
        assert_eq!(
            normalize_path(Path::new("/a/b/c/../../d")),
            PathBuf::from("/a/d")
        );
    }

    #[test]
    fn in_memory_fs_basic_operations() {
        let mut fs = InMemoryFs::new();
        fs.add_file(PathBuf::from("/project/main.wcl"), "content");

        assert!(fs.exists(Path::new("/project/main.wcl")));
        assert!(!fs.exists(Path::new("/project/other.wcl")));
        assert_eq!(
            fs.read_file(Path::new("/project/main.wcl")).unwrap(),
            "content"
        );
        assert!(fs.read_file(Path::new("/missing")).is_err());
    }

    #[test]
    fn e034_duplicate_exported_variable_across_imports() {
        let mut fs = InMemoryFs::new();
        // Two imported files export the same variable name
        fs.add_file(PathBuf::from("/project/a.wcl"), "export let port = 8080");
        fs.add_file(PathBuf::from("/project/b.wcl"), "export let port = 9090");
        fs.add_file(
            PathBuf::from("/project/main.wcl"),
            "import \"./a.wcl\"\nimport \"./b.wcl\"",
        );

        let mut sm = make_source_map();
        let file_id = sm.add_file(
            "main.wcl".to_string(),
            "import \"./a.wcl\"\nimport \"./b.wcl\"".to_string(),
        );
        let (mut doc, _parse_diags) =
            crate::lang::parse("import \"./a.wcl\"\nimport \"./b.wcl\"", file_id);

        let mut resolver = ImportResolver::new(
            &fs,
            &mut sm,
            PathBuf::from("/project"),
            32,
            true,
            LibraryConfig::default(),
        );
        let diags = resolver.resolve(&mut doc, Path::new("/project/main.wcl"), 0);

        let e034_errors: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E034"))
            .collect();
        assert_eq!(
            e034_errors.len(),
            1,
            "expected one E034 error, got: {:?}",
            e034_errors
        );
        assert!(e034_errors[0]
            .message
            .contains("duplicate exported variable"));
        assert!(e034_errors[0].message.contains("port"));
    }

    #[test]
    fn e034_no_error_for_different_names() {
        let mut fs = InMemoryFs::new();
        fs.add_file(PathBuf::from("/project/a.wcl"), "export let port = 8080");
        fs.add_file(
            PathBuf::from("/project/b.wcl"),
            "export let host = \"localhost\"",
        );
        fs.add_file(
            PathBuf::from("/project/main.wcl"),
            "import \"./a.wcl\"\nimport \"./b.wcl\"",
        );

        let mut sm = make_source_map();
        let file_id = sm.add_file(
            "main.wcl".to_string(),
            "import \"./a.wcl\"\nimport \"./b.wcl\"".to_string(),
        );
        let (mut doc, _parse_diags) =
            crate::lang::parse("import \"./a.wcl\"\nimport \"./b.wcl\"", file_id);

        let mut resolver = ImportResolver::new(
            &fs,
            &mut sm,
            PathBuf::from("/project"),
            32,
            true,
            LibraryConfig::default(),
        );
        let diags = resolver.resolve(&mut doc, Path::new("/project/main.wcl"), 0);

        let e034_errors: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E034"))
            .collect();
        assert_eq!(e034_errors.len(), 0);
    }

    #[test]
    fn e035_re_export_of_undefined_name() {
        let mut fs = InMemoryFs::new();
        // The imported file re-exports a name that doesn't exist
        fs.add_file(PathBuf::from("/project/lib.wcl"), "export nonexistent");
        fs.add_file(PathBuf::from("/project/main.wcl"), "import \"./lib.wcl\"");

        let mut sm = make_source_map();
        let file_id = sm.add_file("main.wcl".to_string(), "import \"./lib.wcl\"".to_string());
        let (mut doc, _parse_diags) = crate::lang::parse("import \"./lib.wcl\"", file_id);

        let mut resolver = ImportResolver::new(
            &fs,
            &mut sm,
            PathBuf::from("/project"),
            32,
            true,
            LibraryConfig::default(),
        );
        let diags = resolver.resolve(&mut doc, Path::new("/project/main.wcl"), 0);

        let e035_errors: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E035"))
            .collect();
        assert_eq!(
            e035_errors.len(),
            1,
            "expected one E035 error, got: {:?}",
            e035_errors
        );
        assert!(e035_errors[0]
            .message
            .contains("re-export of undefined name"));
        assert!(e035_errors[0].message.contains("nonexistent"));
    }

    #[test]
    fn e035_no_error_when_name_is_defined() {
        let mut fs = InMemoryFs::new();
        // The imported file defines a let binding and re-exports it
        fs.add_file(
            PathBuf::from("/project/lib.wcl"),
            "let port = 8080\nexport port",
        );
        fs.add_file(PathBuf::from("/project/main.wcl"), "import \"./lib.wcl\"");

        let mut sm = make_source_map();
        let file_id = sm.add_file("main.wcl".to_string(), "import \"./lib.wcl\"".to_string());
        let (mut doc, _parse_diags) = crate::lang::parse("import \"./lib.wcl\"", file_id);

        let mut resolver = ImportResolver::new(
            &fs,
            &mut sm,
            PathBuf::from("/project"),
            32,
            true,
            LibraryConfig::default(),
        );
        let diags = resolver.resolve(&mut doc, Path::new("/project/main.wcl"), 0);

        let e035_errors: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E035"))
            .collect();
        assert_eq!(e035_errors.len(), 0);
    }

    #[test]
    fn glob_with_in_memory_fs() {
        let mut fs = InMemoryFs::new();
        fs.add_file(PathBuf::from("/project/schemas/a.wcl"), "schema \"a\" {}");
        fs.add_file(PathBuf::from("/project/schemas/b.wcl"), "schema \"b\" {}");
        fs.add_file(PathBuf::from("/project/schemas/c.txt"), "not wcl");

        let matches = fs.glob(Path::new("/project/schemas/*.wcl")).unwrap();
        assert_eq!(matches.len(), 2);
        assert!(matches.contains(&PathBuf::from("/project/schemas/a.wcl")));
        assert!(matches.contains(&PathBuf::from("/project/schemas/b.wcl")));
    }

    #[test]
    fn glob_no_matches_returns_empty() {
        let fs = InMemoryFs::new();
        let matches = fs.glob(Path::new("/project/schemas/*.wcl")).unwrap();
        assert!(matches.is_empty());
    }

    #[test]
    fn optional_import_missing_file_no_error() {
        let fs = InMemoryFs::new();
        let mut sm = make_source_map();
        let file_id = sm.add_file(
            "main.wcl".to_string(),
            "import? \"./missing.wcl\"".to_string(),
        );
        let (mut doc, _) = crate::lang::parse("import? \"./missing.wcl\"", file_id);

        let mut resolver = ImportResolver::new(
            &fs,
            &mut sm,
            PathBuf::from("/project"),
            32,
            true,
            LibraryConfig::default(),
        );
        let diags = resolver.resolve(&mut doc, Path::new("/project/main.wcl"), 0);

        assert!(
            !diags.has_errors(),
            "optional import should not produce errors: {:?}",
            diags.diagnostics()
        );
    }

    #[test]
    fn optional_glob_no_matches_no_error() {
        let fs = InMemoryFs::new();
        let mut sm = make_source_map();
        let file_id = sm.add_file(
            "main.wcl".to_string(),
            "import? \"./schemas/*.wcl\"".to_string(),
        );
        let (mut doc, _) = crate::lang::parse("import? \"./schemas/*.wcl\"", file_id);

        let mut resolver = ImportResolver::new(
            &fs,
            &mut sm,
            PathBuf::from("/project"),
            32,
            true,
            LibraryConfig::default(),
        );
        let diags = resolver.resolve(&mut doc, Path::new("/project/main.wcl"), 0);

        assert!(
            !diags.has_errors(),
            "optional glob should not produce errors: {:?}",
            diags.diagnostics()
        );
    }

    #[test]
    fn extra_paths_searched_before_defaults() {
        let mut fs = InMemoryFs::new();
        fs.add_file(PathBuf::from("/custom/lib/mylib.wcl"), "x = 1");

        let config = LibraryConfig {
            extra_paths: vec![PathBuf::from("/custom/lib")],
            no_default_paths: false,
        };
        let result = resolve_library_import("mylib.wcl", &fs, &config);
        assert_eq!(result, Some(PathBuf::from("/custom/lib/mylib.wcl")));
    }

    #[test]
    fn no_default_paths_disables_defaults() {
        let config = LibraryConfig {
            extra_paths: vec![],
            no_default_paths: true,
        };
        let paths = library_search_paths(&config);
        assert!(paths.is_empty(), "expected no paths, got: {:?}", paths);
    }

    #[test]
    fn multiple_extra_paths_searched_in_order() {
        let mut fs = InMemoryFs::new();
        fs.add_file(PathBuf::from("/first/mylib.wcl"), "x = 1");
        fs.add_file(PathBuf::from("/second/mylib.wcl"), "x = 2");

        let config = LibraryConfig {
            extra_paths: vec![PathBuf::from("/first"), PathBuf::from("/second")],
            no_default_paths: true,
        };
        let result = resolve_library_import("mylib.wcl", &fs, &config);
        assert_eq!(result, Some(PathBuf::from("/first/mylib.wcl")));
    }

    #[test]
    fn default_library_config_behaves_like_before() {
        let config = LibraryConfig::default();
        let paths = library_search_paths(&config);
        // Should include at least one default path
        assert!(!paths.is_empty());
    }

    #[test]
    fn library_file_relative_import_no_jail_error() {
        // Library file at /usr/lib/wcl/mylib.wcl imports ./helper.wcl
        // The project root is /project — helper.wcl is outside project root
        // but should NOT trigger a jail error because mylib.wcl is a library file.
        let mut fs = InMemoryFs::new();
        fs.add_file(
            PathBuf::from("/libdir/mylib.wcl"),
            "import \"./helper.wcl\"",
        );
        fs.add_file(PathBuf::from("/libdir/helper.wcl"), "x = 42");

        let mut sm = make_source_map();
        let source = "import <mylib.wcl>";
        let file_id = sm.add_file("main.wcl".to_string(), source.to_string());
        let (mut doc, _) = crate::lang::parse(source, file_id);

        let config = LibraryConfig {
            extra_paths: vec![PathBuf::from("/libdir")],
            no_default_paths: true,
        };
        let mut resolver =
            ImportResolver::new(&fs, &mut sm, PathBuf::from("/project"), 32, true, config);
        let diags = resolver.resolve(&mut doc, Path::new("/project/main.wcl"), 0);

        let jail_errors: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E011"))
            .collect();
        assert!(
            jail_errors.is_empty(),
            "expected no jail errors for library-nested imports, got: {:?}",
            jail_errors
        );
    }

    #[test]
    fn library_importing_another_library_works() {
        let mut fs = InMemoryFs::new();
        fs.add_file(PathBuf::from("/libdir/outer.wcl"), "import <inner.wcl>");
        fs.add_file(PathBuf::from("/libdir/inner.wcl"), "y = 99");

        let mut sm = make_source_map();
        let source = "import <outer.wcl>";
        let file_id = sm.add_file("main.wcl".to_string(), source.to_string());
        let (mut doc, _) = crate::lang::parse(source, file_id);

        let config = LibraryConfig {
            extra_paths: vec![PathBuf::from("/libdir")],
            no_default_paths: true,
        };
        let mut resolver =
            ImportResolver::new(&fs, &mut sm, PathBuf::from("/project"), 32, true, config);
        let diags = resolver.resolve(&mut doc, Path::new("/project/main.wcl"), 0);

        assert!(
            !diags.has_errors(),
            "expected no errors, got: {:?}",
            diags.diagnostics()
        );
    }

    // ── resolve_let_import_table tests ──────────────────────────────

    fn make_let_import_table_doc(path: &str) -> Document {
        use crate::lang::span::{FileId, Span};
        use crate::lang::trivia::Trivia;

        let span = Span::new(FileId(0), 0, 0);
        let lb = LetBinding {
            decorators: vec![],
            partial: false,
            name: Ident {
                name: "data".to_string(),
                span,
            },
            value: Expr::ImportTable(
                ImportTableArgs {
                    path: StringLit {
                        parts: vec![StringPart::Literal(path.to_string())],
                        heredoc: None,
                        span,
                    },
                    separator: None,
                    headers: None,
                    columns: None,
                },
                span,
            ),
            trivia: Trivia::empty(),
            span,
        };

        Document {
            items: vec![DocItem::Body(BodyItem::LetBinding(lb))],
            trivia: Trivia::empty(),
            span,
        }
    }

    #[test]
    fn resolve_import_tables_converts_let_binding() {
        let mut fs = InMemoryFs::new();
        fs.add_file(PathBuf::from("/project/data.csv"), "name,value\nalice,42");

        let mut doc = make_let_import_table_doc("data.csv");
        let mut diags = DiagnosticBag::new();
        resolve_import_tables(&mut doc, &fs, Path::new("/project"), &mut diags);

        assert!(
            !diags.has_errors(),
            "unexpected errors: {:?}",
            diags.diagnostics()
        );

        if let DocItem::Body(BodyItem::LetBinding(lb)) = &doc.items[0] {
            assert!(
                matches!(lb.value, Expr::List(..)),
                "expected let value to be a list, got: {:?}",
                lb.value
            );
        } else {
            panic!("expected let binding");
        }
    }

    #[test]
    fn resolve_import_tables_let_missing_file() {
        let fs = InMemoryFs::new();
        let mut doc = make_let_import_table_doc("missing.csv");
        let mut diags = DiagnosticBag::new();
        resolve_import_tables(&mut doc, &fs, Path::new("/project"), &mut diags);

        assert!(diags.has_errors());
        // value should still be import_table (unchanged)
        if let DocItem::Body(BodyItem::LetBinding(lb)) = &doc.items[0] {
            assert!(
                matches!(lb.value, Expr::ImportTable(..)),
                "expected import_table to be preserved"
            );
        }
    }

    #[test]
    fn resolve_import_tables_let_with_interpolation() {
        use crate::lang::span::{FileId, Span};
        use crate::lang::trivia::Trivia;

        let span = Span::new(FileId(0), 0, 0);
        let lb = LetBinding {
            decorators: vec![],
            partial: false,
            name: Ident {
                name: "data".to_string(),
                span,
            },
            value: Expr::ImportTable(
                ImportTableArgs {
                    path: StringLit {
                        parts: vec![
                            StringPart::Literal("data_".to_string()),
                            StringPart::Interpolation(Box::new(Expr::MemberAccess(
                                Box::new(Expr::Ident(Ident {
                                    name: "env".to_string(),
                                    span,
                                })),
                                Ident {
                                    name: "name".to_string(),
                                    span,
                                },
                                span,
                            ))),
                            StringPart::Literal(".csv".to_string()),
                        ],
                        heredoc: None,
                        span,
                    },
                    separator: None,
                    headers: None,
                    columns: None,
                },
                span,
            ),
            trivia: Trivia::empty(),
            span,
        };

        let mut doc = Document {
            items: vec![DocItem::Body(BodyItem::LetBinding(lb))],
            trivia: Trivia::empty(),
            span,
        };

        let fs = InMemoryFs::new();
        let mut diags = DiagnosticBag::new();
        resolve_import_tables(&mut doc, &fs, Path::new("/project"), &mut diags);

        // Should be left unchanged (no error, no transformation)
        assert!(!diags.has_errors());
        if let DocItem::Body(BodyItem::LetBinding(lb)) = &doc.items[0] {
            assert!(
                matches!(lb.value, Expr::ImportTable(..)),
                "interpolated path should be left unchanged"
            );
        }
    }

    #[test]
    fn resolve_import_tables_export_let() {
        use crate::lang::span::{FileId, Span};
        use crate::lang::trivia::Trivia;

        let mut fs = InMemoryFs::new();
        fs.add_file(PathBuf::from("/project/data.csv"), "name,role\nalice,admin");

        let span = Span::new(FileId(0), 0, 0);
        let el = ExportLet {
            decorators: vec![],
            name: Ident {
                name: "data".to_string(),
                span,
            },
            value: Expr::ImportTable(
                ImportTableArgs {
                    path: StringLit {
                        parts: vec![StringPart::Literal("data.csv".to_string())],
                        heredoc: None,
                        span,
                    },
                    separator: None,
                    headers: None,
                    columns: None,
                },
                span,
            ),
            trivia: Trivia::empty(),
            span,
        };

        let mut doc = Document {
            items: vec![DocItem::ExportLet(el)],
            trivia: Trivia::empty(),
            span,
        };

        let mut diags = DiagnosticBag::new();
        resolve_import_tables(&mut doc, &fs, Path::new("/project"), &mut diags);

        assert!(!diags.has_errors());
        if let DocItem::ExportLet(el) = &doc.items[0] {
            assert!(
                matches!(el.value, Expr::List(..)),
                "expected export let value to be a list"
            );
        } else {
            panic!("expected export let");
        }
    }

    // ── resolve_import_tables tests ─────────────────────────────────

    fn make_import_table_doc(path: &str) -> Document {
        use crate::lang::span::{FileId, Span};
        use crate::lang::trivia::Trivia;

        let span = Span::new(FileId(0), 0, 0);
        let table = Table {
            decorators: vec![],
            partial: false,
            inline_id: Some(InlineId::Literal(IdentifierLit {
                value: "my_table".to_string(),
                span,
            })),
            schema_ref: None,
            columns: vec![],
            rows: vec![],
            import_expr: Some(Box::new(Expr::ImportTable(
                ImportTableArgs {
                    path: StringLit {
                        parts: vec![StringPart::Literal(path.to_string())],
                        heredoc: None,
                        span,
                    },
                    separator: None,
                    headers: None,
                    columns: None,
                },
                span,
            ))),
            trivia: Trivia::empty(),
            span,
        };

        Document {
            items: vec![DocItem::Body(BodyItem::Table(table))],
            trivia: Trivia::empty(),
            span,
        }
    }

    #[test]
    fn resolve_import_tables_converts_csv_to_inline() {
        let mut fs = InMemoryFs::new();
        fs.add_file(
            PathBuf::from("/project/data.csv"),
            "name,value\nalice,42\nbob,99",
        );

        let mut doc = make_import_table_doc("data.csv");
        let mut diags = DiagnosticBag::new();
        resolve_import_tables(&mut doc, &fs, Path::new("/project"), &mut diags);

        assert!(
            !diags.has_errors(),
            "unexpected errors: {:?}",
            diags.diagnostics()
        );

        // Table should now have columns and rows, no import_expr
        if let DocItem::Body(BodyItem::Table(table)) = &doc.items[0] {
            assert!(table.import_expr.is_none(), "import_expr should be cleared");
            assert_eq!(table.columns.len(), 2);
            assert_eq!(table.columns[0].name.name, "name");
            assert_eq!(table.columns[1].name.name, "value");
            assert_eq!(table.rows.len(), 2);
        } else {
            panic!("expected table");
        }
    }

    #[test]
    fn resolve_import_tables_missing_file_emits_diagnostic() {
        let fs = InMemoryFs::new();
        let mut doc = make_import_table_doc("missing.csv");
        let mut diags = DiagnosticBag::new();
        resolve_import_tables(&mut doc, &fs, Path::new("/project"), &mut diags);

        assert!(diags.has_errors());
        // import_expr should be preserved
        if let DocItem::Body(BodyItem::Table(table)) = &doc.items[0] {
            assert!(
                table.import_expr.is_some(),
                "import_expr should be preserved on error"
            );
        }
    }

    #[test]
    fn resolve_import_tables_skips_non_import_tables() {
        use crate::lang::span::{FileId, Span};
        use crate::lang::trivia::Trivia;

        let span = Span::new(FileId(0), 0, 0);
        let table = Table {
            decorators: vec![],
            partial: false,
            inline_id: Some(InlineId::Literal(IdentifierLit {
                value: "inline_table".to_string(),
                span,
            })),
            schema_ref: None,
            columns: vec![ColumnDecl {
                decorators: vec![],
                name: Ident {
                    name: "col".to_string(),
                    span,
                },
                type_expr: TypeExpr::String(span),
                trivia: Trivia::empty(),
                span,
            }],
            rows: vec![TableRow {
                cells: vec![Expr::StringLit(StringLit {
                    parts: vec![StringPart::Literal("val".to_string())],
                    heredoc: None,
                    span,
                })],
                span,
            }],
            import_expr: None,
            trivia: Trivia::empty(),
            span,
        };

        let mut doc = Document {
            items: vec![DocItem::Body(BodyItem::Table(table))],
            trivia: Trivia::empty(),
            span,
        };

        let fs = InMemoryFs::new();
        let mut diags = DiagnosticBag::new();
        resolve_import_tables(&mut doc, &fs, Path::new("/project"), &mut diags);

        assert!(!diags.has_errors());
        if let DocItem::Body(BodyItem::Table(table)) = &doc.items[0] {
            assert_eq!(table.columns.len(), 1);
            assert_eq!(table.rows.len(), 1);
        }
    }

    #[test]
    fn resolve_import_tables_custom_separator() {
        use crate::lang::span::{FileId, Span};
        use crate::lang::trivia::Trivia;

        let mut fs = InMemoryFs::new();
        fs.add_file(
            PathBuf::from("/project/data.tsv"),
            "name\trole\nalice\tadmin",
        );

        let span = Span::new(FileId(0), 0, 0);
        let table = Table {
            decorators: vec![],
            partial: false,
            inline_id: Some(InlineId::Literal(IdentifierLit {
                value: "my_table".to_string(),
                span,
            })),
            schema_ref: None,
            columns: vec![],
            rows: vec![],
            import_expr: Some(Box::new(Expr::ImportTable(
                ImportTableArgs {
                    path: StringLit {
                        parts: vec![StringPart::Literal("data.tsv".to_string())],
                        heredoc: None,
                        span,
                    },
                    separator: Some(StringLit {
                        parts: vec![StringPart::Literal("\t".to_string())],
                        heredoc: None,
                        span,
                    }),
                    headers: None,
                    columns: None,
                },
                span,
            ))),
            trivia: Trivia::empty(),
            span,
        };

        let mut doc = Document {
            items: vec![DocItem::Body(BodyItem::Table(table))],
            trivia: Trivia::empty(),
            span,
        };

        let mut diags = DiagnosticBag::new();
        resolve_import_tables(&mut doc, &fs, Path::new("/project"), &mut diags);

        assert!(!diags.has_errors());
        if let DocItem::Body(BodyItem::Table(table)) = &doc.items[0] {
            assert_eq!(table.columns.len(), 2);
            assert_eq!(table.columns[0].name.name, "name");
            assert_eq!(table.columns[1].name.name, "role");
            assert_eq!(table.rows.len(), 1);
        }
    }

    #[test]
    fn find_lazy_refs_detects_use_decl() {
        use crate::lang::span::{FileId, Span};
        use crate::lang::trivia::Trivia;

        let dummy = Span::new(FileId(0), 0, 0);
        let doc = Document {
            items: vec![DocItem::Use(UseDecl {
                namespace_path: vec![Ident {
                    name: "net".to_string(),
                    span: dummy,
                }],
                targets: vec![UseTarget {
                    name: Ident {
                        name: "service".to_string(),
                        span: dummy,
                    },
                    alias: None,
                    span: dummy,
                }],
                trivia: Trivia::default(),
                span: dummy,
            })],
            trivia: Trivia::default(),
            span: dummy,
        };

        let lazy_ns = vec!["net".to_string(), "other".to_string()];
        let refs = find_lazy_namespace_references(&doc, &lazy_ns);
        assert!(refs.contains("net"));
        assert!(!refs.contains("other"));
    }

    #[test]
    fn find_lazy_refs_detects_block_kind() {
        use crate::lang::span::{FileId, Span};
        use crate::lang::trivia::Trivia;

        let dummy = Span::new(FileId(0), 0, 0);
        let doc = Document {
            items: vec![DocItem::Body(BodyItem::Block(Block {
                decorators: vec![],
                partial: false,
                kind: Ident {
                    name: "net::service".to_string(),
                    span: dummy,
                },
                inline_id: None,
                arrow_target: None,
                inline_args: vec![],
                body: vec![],
                text_content: None,
                trivia: Trivia::default(),
                span: dummy,
            }))],
            trivia: Trivia::default(),
            span: dummy,
        };

        let lazy_ns = vec!["net".to_string()];
        let refs = find_lazy_namespace_references(&doc, &lazy_ns);
        assert!(refs.contains("net"));
    }

    #[test]
    fn find_lazy_refs_empty_when_no_references() {
        use crate::lang::span::{FileId, Span};
        use crate::lang::trivia::Trivia;

        let dummy = Span::new(FileId(0), 0, 0);
        let doc = Document {
            items: vec![DocItem::Body(BodyItem::Block(Block {
                decorators: vec![],
                partial: false,
                kind: Ident {
                    name: "config".to_string(),
                    span: dummy,
                },
                inline_id: None,
                arrow_target: None,
                inline_args: vec![],
                body: vec![],
                text_content: None,
                trivia: Trivia::default(),
                span: dummy,
            }))],
            trivia: Trivia::default(),
            span: dummy,
        };

        let lazy_ns = vec!["net".to_string()];
        let refs = find_lazy_namespace_references(&doc, &lazy_ns);
        assert!(refs.is_empty());
    }

    #[test]
    fn lazy_imports_collected_during_resolve() {
        use crate::lang::span::{FileId, Span};
        use crate::lang::trivia::Trivia;

        let dummy = Span::new(FileId(0), 0, 0);
        let fs = InMemoryFs::new();
        let mut sm = make_source_map();
        let mut resolver = ImportResolver::new(
            &fs,
            &mut sm,
            PathBuf::from("/project"),
            32,
            true,
            LibraryConfig::default(),
        );

        let mut doc = Document {
            items: vec![DocItem::Import(Import {
                path: StringLit {
                    parts: vec![StringPart::Literal("./lib.wcl".to_string())],
                    heredoc: None,
                    span: dummy,
                },
                kind: ImportKind::Relative,
                optional: false,
                lazy_namespace: Some(vec![Ident {
                    name: "net".to_string(),
                    span: dummy,
                }]),
                trivia: Trivia::default(),
                span: dummy,
            })],
            trivia: Trivia::default(),
            span: dummy,
        };

        let _diags = resolver.resolve(&mut doc, Path::new("/project/main.wcl"), 0);
        // Lazy import should have been collected, not loaded
        let lazy = resolver.take_lazy_imports();
        assert_eq!(lazy.len(), 1);
        assert!(lazy[0].import.lazy_namespace.is_some());
        // Doc should have no items left (lazy import removed)
        assert!(doc.items.is_empty());
    }
}
