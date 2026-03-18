use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use wcl_core::ast::*;
use wcl_core::diagnostic::{Diagnostic, DiagnosticBag};
use wcl_core::span::{SourceMap, Span};

/// Trait for file system access (enables testing with in-memory FS).
pub trait FileSystem {
    fn read_file(&self, path: &Path) -> Result<String, String>;
    fn canonicalize(&self, path: &Path) -> Result<PathBuf, String>;
    fn exists(&self, path: &Path) -> bool;
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

/// Resolves `import` directives in WCL documents.
///
/// Handles path resolution, jail checking, import-once semantics, depth limits,
/// and recursive resolution of imports within imported files.
pub struct ImportResolver<'a, FS: FileSystem> {
    fs: &'a FS,
    source_map: &'a mut SourceMap,
    root_dir: PathBuf,
    max_depth: u32,
    allow_imports: bool,
    loaded: HashSet<PathBuf>,
    diagnostics: DiagnosticBag,
}

impl<'a, FS: FileSystem> ImportResolver<'a, FS> {
    pub fn new(
        fs: &'a FS,
        source_map: &'a mut SourceMap,
        root_dir: PathBuf,
        max_depth: u32,
        allow_imports: bool,
    ) -> Self {
        ImportResolver {
            fs,
            source_map,
            root_dir,
            max_depth,
            allow_imports,
            loaded: HashSet::new(),
            diagnostics: DiagnosticBag::new(),
        }
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
                    self.diagnostics.error(
                        "imports are disabled in this context",
                        import.span,
                    );
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

            let import_path_str = string_lit_to_plain(&import.path);
            let span = import.span;

            // Check depth limit
            if depth >= self.max_depth {
                self.diagnostics.error(
                    format!(
                        "import depth limit exceeded (max {})",
                        self.max_depth
                    ),
                    span,
                );
                continue;
            }

            // Resolve the path
            let resolved = match self.resolve_path(&import_path_str, current_file) {
                Ok(p) => p,
                Err(diag) => {
                    self.diagnostics.add(diag);
                    continue;
                }
            };

            // Jail check
            if let Err(diag) = self.check_jail(&resolved, span) {
                self.diagnostics.add(diag);
                continue;
            }

            // Import-once: skip if already loaded
            if self.loaded.contains(&resolved) {
                doc.items.remove(idx);
                continue;
            }
            self.loaded.insert(resolved.clone());

            // Read the file
            let source = match self.fs.read_file(&resolved) {
                Ok(s) => s,
                Err(e) => {
                    self.diagnostics.error(
                        format!("cannot read imported file '{}': {}", resolved.display(), e),
                        span,
                    );
                    continue;
                }
            };

            // Add to source map and parse
            let file_id = self.source_map.add_file(
                resolved.to_string_lossy().into_owned(),
                source.clone(),
            );
            let (mut imported_doc, parse_diags) = wcl_core::parse(&source, file_id);
            self.diagnostics.merge(parse_diags);

            // Recursively resolve imports in the imported document
            let child_diags = self.resolve(&mut imported_doc, &resolved, depth + 1);
            self.diagnostics.merge(child_diags);

            // E035: Check re-exports reference defined names in the imported file
            // Must check against the full imported doc items (before filtering)
            for item in &imported_doc.items {
                if let DocItem::ReExport(re_export) = item {
                    let name_exists = imported_doc.items.iter().any(|mi| match mi {
                        DocItem::ExportLet(el) => el.name.name == re_export.name.name,
                        DocItem::Body(BodyItem::LetBinding(lb)) => {
                            lb.name.name == re_export.name.name
                        }
                        DocItem::Body(BodyItem::Block(b)) => b
                            .inline_id
                            .as_ref()
                            .map(|id| match id {
                                InlineId::Literal(lit) => lit.value == re_export.name.name,
                                _ => false,
                            })
                            .unwrap_or(false),
                        _ => false,
                    });
                    if !name_exists {
                        self.diagnostics.error_with_code(
                            format!(
                                "re-export of undefined name '{}'",
                                re_export.name.name
                            ),
                            re_export.span,
                            "E035",
                        );
                    }
                }
            }

            // Collect mergeable items from the imported document
            let mut merged_items: Vec<DocItem> = Vec::new();
            for item in imported_doc.items {
                match &item {
                    // Private let bindings are file-private, skip them
                    DocItem::Body(BodyItem::LetBinding(_)) => {}
                    // Everything else gets merged
                    DocItem::Import(_) => {
                        // Already resolved recursively, should not appear
                    }
                    DocItem::ExportLet(_)
                    | DocItem::ReExport(_)
                    | DocItem::Body(_) => {
                        merged_items.push(item);
                    }
                }
            }

            // E034: Check for duplicate exported variable names across imports
            for item in &merged_items {
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
            for (offset, item) in merged_items.into_iter().enumerate() {
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
                format!("home-relative import paths are forbidden: '{}'", import_path),
                dummy_span,
            ));
        }

        // Reject remote/scheme paths
        if import_path.contains("://") {
            return Err(Diagnostic::error(
                format!("remote imports are forbidden: '{}'", import_path),
                dummy_span,
            ));
        }

        // Resolve relative to importing file's directory
        let base_dir = current_file
            .parent()
            .unwrap_or_else(|| Path::new("."));
        let resolved = base_dir.join(import_path);

        // Canonicalize
        self.fs
            .canonicalize(&resolved)
            .map_err(|e| {
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
            ));
        }
        Ok(())
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
        );

        let result = resolver.resolve_path("~/file.wcl", Path::new("/project/main.wcl"));
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .message
            .contains("home-relative"));
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
        );

        let result = resolver.check_jail(
            Path::new("/project/sub/file.wcl"),
            Span::dummy(),
        );
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
        );

        let result = resolver.check_jail(
            Path::new("/other/file.wcl"),
            Span::dummy(),
        );
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .message
            .contains("escapes root directory"));
    }

    #[test]
    fn jail_check_rejects_parent_traversal() {
        let mut fs = InMemoryFs::new();
        fs.add_file(
            PathBuf::from("/project/sub/../../../etc/passwd"),
            "bad",
        );
        let mut sm = make_source_map();
        let resolver = ImportResolver::new(
            &fs,
            &mut sm,
            PathBuf::from("/project"),
            32,
            true,
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
        fs.add_file(
            PathBuf::from("/project/a.wcl"),
            "export let port = 8080",
        );
        fs.add_file(
            PathBuf::from("/project/b.wcl"),
            "export let port = 9090",
        );
        fs.add_file(
            PathBuf::from("/project/main.wcl"),
            "import \"./a.wcl\"\nimport \"./b.wcl\"",
        );

        let mut sm = make_source_map();
        let file_id = sm.add_file("main.wcl".to_string(), "import \"./a.wcl\"\nimport \"./b.wcl\"".to_string());
        let (mut doc, _parse_diags) = wcl_core::parse("import \"./a.wcl\"\nimport \"./b.wcl\"", file_id);

        let mut resolver = ImportResolver::new(
            &fs,
            &mut sm,
            PathBuf::from("/project"),
            32,
            true,
        );
        let diags = resolver.resolve(&mut doc, Path::new("/project/main.wcl"), 0);

        let e034_errors: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E034"))
            .collect();
        assert_eq!(e034_errors.len(), 1, "expected one E034 error, got: {:?}", e034_errors);
        assert!(e034_errors[0].message.contains("duplicate exported variable"));
        assert!(e034_errors[0].message.contains("port"));
    }

    #[test]
    fn e034_no_error_for_different_names() {
        let mut fs = InMemoryFs::new();
        fs.add_file(
            PathBuf::from("/project/a.wcl"),
            "export let port = 8080",
        );
        fs.add_file(
            PathBuf::from("/project/b.wcl"),
            "export let host = \"localhost\"",
        );
        fs.add_file(
            PathBuf::from("/project/main.wcl"),
            "import \"./a.wcl\"\nimport \"./b.wcl\"",
        );

        let mut sm = make_source_map();
        let file_id = sm.add_file("main.wcl".to_string(), "import \"./a.wcl\"\nimport \"./b.wcl\"".to_string());
        let (mut doc, _parse_diags) = wcl_core::parse("import \"./a.wcl\"\nimport \"./b.wcl\"", file_id);

        let mut resolver = ImportResolver::new(
            &fs,
            &mut sm,
            PathBuf::from("/project"),
            32,
            true,
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
        fs.add_file(
            PathBuf::from("/project/lib.wcl"),
            "export nonexistent",
        );
        fs.add_file(
            PathBuf::from("/project/main.wcl"),
            "import \"./lib.wcl\"",
        );

        let mut sm = make_source_map();
        let file_id = sm.add_file("main.wcl".to_string(), "import \"./lib.wcl\"".to_string());
        let (mut doc, _parse_diags) = wcl_core::parse("import \"./lib.wcl\"", file_id);

        let mut resolver = ImportResolver::new(
            &fs,
            &mut sm,
            PathBuf::from("/project"),
            32,
            true,
        );
        let diags = resolver.resolve(&mut doc, Path::new("/project/main.wcl"), 0);

        let e035_errors: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E035"))
            .collect();
        assert_eq!(e035_errors.len(), 1, "expected one E035 error, got: {:?}", e035_errors);
        assert!(e035_errors[0].message.contains("re-export of undefined name"));
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
        fs.add_file(
            PathBuf::from("/project/main.wcl"),
            "import \"./lib.wcl\"",
        );

        let mut sm = make_source_map();
        let file_id = sm.add_file("main.wcl".to_string(), "import \"./lib.wcl\"".to_string());
        let (mut doc, _parse_diags) = wcl_core::parse("import \"./lib.wcl\"", file_id);

        let mut resolver = ImportResolver::new(
            &fs,
            &mut sm,
            PathBuf::from("/project"),
            32,
            true,
        );
        let diags = resolver.resolve(&mut doc, Path::new("/project/main.wcl"), 0);

        let e035_errors: Vec<_> = diags
            .diagnostics()
            .iter()
            .filter(|d| d.code.as_deref() == Some("E035"))
            .collect();
        assert_eq!(e035_errors.len(), 0);
    }
}
