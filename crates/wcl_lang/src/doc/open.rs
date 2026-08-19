//! Opening a document, and the handles a caller gets back.
//!
//! Every constructor funnels into `open_at_with_loader`: parse, resolve
//! the namespace and `use` declarations, expand top-level imports, and
//! build the evaluation cells. The `*_profiled` variants differ only in
//! switching the profiler on before any field is forced.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use miette::NamedSource;

use crate::ast;
use crate::environment::Environment;
use crate::error::ParseError;
use crate::parser::Parser;
use crate::symbols::SymbolIndex;

use super::Document;
use super::cells::{BlockCells, ItemCells, LoadedImport};
use super::imports::{self, expand_top_level_imports};
use super::loader::{self, FileLoader};
use super::validate::validate_document;

impl Document {
    /// Parse `source` into a document, using an empty environment.
    /// `name` is the filename diagnostics report.
    pub fn open(source: &str, name: &str) -> Result<Self, ParseError> {
        Self::open_with(source, name, &Environment::new())
    }

    /// Like [`Document::open`], with a host-supplied environment
    /// providing extra builtins and schemas.
    pub fn open_with(source: &str, name: &str, env: &Environment) -> Result<Self, ParseError> {
        Self::open_at(source, name, None, env)
    }

    /// Variant of [`open_with`] that accepts a base directory for
    /// resolving relative `import` paths. Hosts that synthesise
    /// source in memory (e.g. wdoc prepending a schema) call this
    /// directly so the source's own imports still resolve relative
    /// to its on-disk location.
    pub fn open_at(
        source: &str,
        name: &str,
        base_dir: Option<PathBuf>,
        env: &Environment,
    ) -> Result<Self, ParseError> {
        Self::open_at_with_loader(source, name, base_dir, env, loader::disk_loader())
    }

    /// Like [`open_at`] but uses a caller-supplied [`FileLoader`] for
    /// every imported file. Hosts that maintain in-memory buffers
    /// (e.g. the LSP) pass an [`overlay_loader`] so unsaved edits
    /// participate in import resolution.
    pub fn open_at_with_loader(
        source: &str,
        name: &str,
        base_dir: Option<PathBuf>,
        env: &Environment,
        loader: FileLoader,
    ) -> Result<Self, ParseError> {
        let (ast, symbols) = Parser::new(source, name).parse_source()?;
        let synthetic = env.types().to_vec();
        let synthetic_symbol_sets = env.symbol_sets().to_vec();

        // Resolve top-level imports eagerly. Each LoadedImport carries
        // its own (items, cells, symbols).
        let mut state = imports::ImportState::default();
        let mut eager_imports: Vec<LoadedImport> = Vec::new();
        expand_top_level_imports(
            &ast.items,
            base_dir.as_deref(),
            &mut state,
            &mut eager_imports,
            name,
            source,
            &loader,
        )?;

        let mut import_syms: Vec<&SymbolIndex> = Vec::new();
        imports::collect_import_symbols(&eager_imports, &mut import_syms);
        let mut import_nss: Vec<Vec<String>> = Vec::new();
        imports::collect_import_namespaces(&eager_imports, &mut import_nss);
        let resolved = validate_document(
            &ast,
            &symbols,
            (&synthetic, &synthetic_symbol_sets),
            &import_syms,
            &import_nss,
            source,
            name,
        )?;
        let cells = BlockCells::build(&ast.items, base_dir.as_deref());
        let synthetic_type_cells = synthetic
            .iter()
            .map(|t| ItemCells::build(&ast::Item::TypeDecl(t.clone()), None))
            .collect();
        let synthetic_symbol_set_cells = synthetic_symbol_sets
            .iter()
            .map(|set| ItemCells::build(&ast::Item::SymbolSetDecl(set.clone()), None))
            .collect();
        Ok(Self {
            src: NamedSource::new(name, source.to_string()),
            ast,
            cells,
            file_ns: resolved.file_ns,
            item_aliases: resolved.item_aliases,
            ns_aliases: resolved.ns_aliases,
            wildcards: resolved.wildcards,
            synthetic_types: synthetic,
            synthetic_type_cells,
            synthetic_symbol_sets,
            synthetic_symbol_set_cells,
            symbols,
            env: env.clone(),
            eager_imports,
            loader,
            profile: None,
            ref_registry: std::sync::OnceLock::new(),
            schema_index: std::sync::OnceLock::new(),
            declared_kinds: std::sync::OnceLock::new(),
            deriving: std::sync::Mutex::new(HashSet::new()),
            union_path_memo: std::sync::RwLock::new(HashMap::new()),
            document_schema_locs: std::sync::OnceLock::new(),
            root_let_index: std::sync::OnceLock::new(),
            root_conn_memo: std::sync::RwLock::new(HashMap::new()),
            root_children_memo: std::sync::RwLock::new(HashMap::new()),
            shadow_names: std::sync::OnceLock::new(),
            conn_operand_index: std::sync::OnceLock::new(),
        })
    }

    /// [`open`](Self::open) with profiling enabled. The resulting
    /// document records timings into a tree visible via
    /// [`profile`](Self::profile).
    pub fn open_profiled(source: &str, name: &str) -> Result<Self, ParseError> {
        Self::open_with_profiled(source, name, &Environment::new())
    }

    /// [`open_with`](Self::open_with) with profiling enabled.
    pub fn open_with_profiled(
        source: &str,
        name: &str,
        env: &Environment,
    ) -> Result<Self, ParseError> {
        let mut doc = Self::open_at(source, name, None, env)?;
        doc.profile = Some(crate::diagnostics::ProfileState::new_root());
        Ok(doc)
    }

    /// Switch on profiling for an already-opened document; subsequent
    /// evaluation records timings visible via [`profile`](Self::profile).
    /// For hosts (e.g. `wcl wdoc build` under `WCL_PROFILE`) whose constructor has
    /// no `*_profiled` twin.
    pub fn enable_profiling(&mut self) {
        self.profile = Some(crate::diagnostics::ProfileState::new_root());
    }

    /// [`from_file`](Self::from_file) with profiling enabled.
    pub fn from_file_profiled(path: &Path) -> Result<Self, ParseError> {
        let mut doc = Self::from_file(path)?;
        doc.profile = Some(crate::diagnostics::ProfileState::new_root());
        Ok(doc)
    }

    /// The file loader this document was opened with. Lazy in-block
    /// imports go through it so the same overlay (if any) applies.
    pub(crate) fn loader(&self) -> &FileLoader {
        &self.loader
    }

    /// Snapshot of the profile tree, when profiling is enabled.
    /// Returns `None` if the document was not opened through one of
    /// the `*_profiled` constructors.
    pub fn profile(&self) -> Option<crate::diagnostics::Profile> {
        self.profile
            .as_ref()
            .map(|m| m.lock().unwrap_or_else(|p| p.into_inner()).snapshot())
    }

    /// Internal helper used by hook sites. Returns a no-op guard when
    /// profiling is disabled. Wrap the work to be measured by binding
    /// the return value to `let _guard = …;`.
    pub(crate) fn profile_enter(
        &self,
        key: crate::diagnostics::ProfileKey,
    ) -> crate::diagnostics::ProfileGuard<'_> {
        crate::diagnostics::ProfileGuard::enter(self.profile.as_ref(), key)
    }

    /// The identifier index built incrementally during parsing.
    /// See [`SymbolIndex`] for what it covers and what is excluded.
    pub fn symbols(&self) -> &SymbolIndex {
        &self.symbols
    }

    /// Read and parse the file at `path`. Relative imports inside it
    /// resolve against its own directory.
    pub fn from_file(path: &Path) -> Result<Self, ParseError> {
        Self::from_file_with_loader(path, &Environment::new(), loader::disk_loader())
    }

    /// Like [`from_file`] but also accepts a custom `Environment`. Use
    /// this when the host registers built-ins or schema types.
    pub fn from_file_with(path: &Path, env: &Environment) -> Result<Self, ParseError> {
        Self::from_file_with_loader(path, env, loader::disk_loader())
    }

    /// [`from_file_with`] plus a caller-supplied [`FileLoader`]. The
    /// loader is consulted for the root file *and* every transitive
    /// import (eager + lazy in-block). Use this with
    /// [`overlay_loader`] to make a long-running host's open buffers
    /// shadow disk contents.
    pub fn from_file_with_loader(
        path: &Path,
        env: &Environment,
        loader: FileLoader,
    ) -> Result<Self, ParseError> {
        let source = loader(path)?;
        let base_dir = path.parent().map(Path::to_path_buf);
        Self::open_at_with_loader(&source, &path.display().to_string(), base_dir, env, loader)
    }

    /// The root source text and name, as diagnostics render it.
    pub fn source(&self) -> &NamedSource<String> {
        &self.src
    }

    /// The host environment (synthetic types + builtins) that this
    /// document was opened with. Exposed so tooling (e.g. the LSP)
    /// can enumerate registered builtins for completion.
    pub fn environment(&self) -> &Environment {
        &self.env
    }
}
