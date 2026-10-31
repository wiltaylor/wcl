//! Forced evaluation: every evaluation failure in the document, in one
//! pass.
//!
//! Fields evaluate lazily, so a field nobody reads never reports that it
//! fails. [`Document::eval_diagnostics`] reads everything a host could
//! read — every field, label, decorator argument and table row, in
//! every block, imports included — and reports each failure once.
//! `wcl check --eval` is the caller.
//!
//! Template bodies are skipped, by the rule
//! [`Block::is_template_child`] states: they only evaluate once a host
//! binds their names. Failures [`Document::schema_diagnostics`] already
//! reports are left out, so a host that asks for both reports each
//! problem once.

use std::collections::HashSet;
use std::sync::Arc;

use miette::{Diagnostic, NamedSource};

use crate::ast;
use crate::diagnostics::EvalError;

use super::lookup::{iter_blocks, iter_fields};
use super::scope::Scope;
use super::strict::sourced;
use super::views::{Block, Decorator, Field};
use super::{CollectedSchemaErrors, Document};

/// What makes two diagnostics the same failure: code, message, the file
/// rendered against, and the primary label's offset and length.
type FailureKey = (Option<String>, String, Option<String>, usize, usize);

impl Document {
    /// Force every value in the document and return each evaluation
    /// failure — [`eval_diagnostics`](Self::eval_diagnostics) without the
    /// sources.
    pub fn eval_errors(&self) -> Vec<EvalError> {
        self.eval_diagnostics()
            .into_iter()
            .map(|(error, _)| error)
            .collect()
    }

    /// Force every value in the document and return each evaluation
    /// failure, paired with the source its span indexes into — the root
    /// document or the imported file the failing text lives in. `None`
    /// only when the library cannot place the error in a file; a host
    /// renders that one without a snippet.
    ///
    /// Forces, in every source: each top-level field, and in each block,
    /// recursively, its decorator arguments, labels, fields and table
    /// rows. Decorator arguments on `type`, `union` and `symbol_set`
    /// declarations are forced too. The children of a `@contextual`
    /// block and the body of a `@declares_kind` declarer are skipped:
    /// they are templates, evaluated only once a host binds their names
    /// (see [`Block::is_template_child`]).
    ///
    /// Each failure is reported once, in source order within each file:
    /// a failure read through several fields (`b = a` where `a` fails)
    /// counts at the text that raised it. A failure that
    /// [`schema_diagnostics`](Self::schema_diagnostics) also reports — a
    /// lazy read runs the schema check first, so a field that violates
    /// its schema fails with that violation — is left out, so a host that
    /// reports both lists each problem once. Two diagnostics are the same
    /// failure when their code, message, file and primary span agree.
    ///
    /// Some schema rules are checked only when a value is forced — a
    /// record that no variant of its union matches is one. Those arrive
    /// here as [`EvalError::SchemaViolation`], since the schema pass never
    /// saw them; a host that keeps the two families apart (as `wcl check
    /// --eval` does, exiting 2 for either kind of violation) files them
    /// by variant.
    pub fn eval_diagnostics(&self) -> Vec<(EvalError, Option<NamedSource<Arc<str>>>)> {
        let mut walk = Forced::default();
        let sources = self.all_sources();
        for src in &sources {
            let source = self.named_source_for_view(*src);
            for f in iter_fields(src.items, src.cells, self, src.file_ns, Scope::root()) {
                walk.field(&f, Some(&source));
            }
            for b in iter_blocks(src.items, src.cells, self, src.file_ns, Scope::root()) {
                walk.block(&b);
            }
        }
        for t in self.type_decls() {
            let source = self.named_source_for_type(t.ast);
            walk.decorators(t.decorators(), Some(&source));
            for f in t.fields() {
                walk.decorators(f.decorators(), Some(&source));
            }
        }
        for u in self.union_decls() {
            let source = self.named_source_for_union(u.ast);
            walk.decorators(u.decorators(), Some(&source));
            for v in u.variants() {
                walk.decorators(v.decorators(), Some(&source));
                for f in v.fields() {
                    walk.decorators(f.decorators(), Some(&source));
                }
            }
        }
        for s in self.symbol_sets() {
            let source = self.named_source_for_symbol_set(s.ast);
            walk.decorators(s.decorators(), source.as_ref());
            for entry in s.symbols() {
                walk.decorators(entry.decorators(), source.as_ref());
            }
        }

        let schema: HashSet<FailureKey> = self
            .schema_diagnostics()
            .iter()
            .map(|(error, source)| failure_key(error, source.as_ref()))
            .collect();
        let mut seen = HashSet::new();
        let mut out: CollectedSchemaErrors = walk
            .out
            .into_iter()
            .filter(|(error, source)| {
                let key = failure_key(error, source.as_ref());
                !schema.contains(&key) && seen.insert(key)
            })
            .collect();

        // Source order: the files in the order the document loads them
        // (root first), then by position within each. A file reached only
        // through a lazy import sorts after them; an unplaced error last.
        let order: Vec<String> = sources
            .iter()
            .map(|src| self.named_source_for_view(*src).name().to_string())
            .collect();
        out.sort_by_key(|(error, source)| {
            let file = match source {
                Some(source) => order
                    .iter()
                    .position(|name| name == source.name())
                    .unwrap_or(order.len()),
                None => order.len() + 1,
            };
            (file, primary_span(error).0)
        });
        out
    }

    /// The source declaring the `symbol_set` `target` — the root document
    /// or an eagerly-loaded import. `None` for a library-synthesised set.
    fn named_source_for_symbol_set(
        &self,
        target: *const ast::SymbolSetDecl,
    ) -> Option<NamedSource<Arc<str>>> {
        self.all_sources().into_iter().find_map(|src| {
            src.items
                .iter()
                .any(|item| matches!(item, ast::Item::SymbolSetDecl(s) if std::ptr::eq(s, target)))
                .then(|| self.named_source_for_view(src))
        })
    }
}

/// The walk's accumulator: every failure met, in walk order, before
/// deduplication.
#[derive(Default)]
struct Forced {
    out: CollectedSchemaErrors,
}

impl Forced {
    /// Record `error`, rendered against the file it names, else
    /// `fallback`: the file holding the text that was being forced.
    fn push(&mut self, error: EvalError, fallback: Option<&NamedSource<Arc<str>>>) {
        match fallback {
            Some(source) => self.out.push(sourced(error, source)),
            None => {
                let source = error.origin();
                self.out.push((error, source));
            }
        }
    }

    fn decorators<'a>(
        &mut self,
        decorators: impl Iterator<Item = Decorator<'a>>,
        source: Option<&NamedSource<Arc<str>>>,
    ) {
        for d in decorators {
            if let Err(e) = d.positional() {
                self.push(e, source);
            }
            for arg in d.named() {
                if let Err(e) = arg.value() {
                    self.push(e, source);
                }
            }
        }
    }

    fn field(&mut self, f: &Field<'_>, source: Option<&NamedSource<Arc<str>>>) {
        self.decorators(f.decorators(), source);
        // A `&T` field reads as the reference it resolves to, as
        // `wcl parse` prints it, rather than through `value`.
        if let Some(reference) = f.reference() {
            if let Err(e) = reference {
                self.push(e, source);
            }
            return;
        }
        if let Err(e) = f.value() {
            self.push(e.clone(), source);
        }
    }

    fn block(&mut self, b: &Block<'_>) {
        let source = b.named_source();
        self.decorators(b.decorators(), Some(&source));
        if let Err(e) = b.labels() {
            self.push(e, Some(&source));
        }
        for f in b.fields() {
            self.field(&f, Some(&source));
        }
        for inner in b.blocks() {
            if !b.is_template_child(&inner) {
                self.block(&inner);
            }
        }
        if !b.is_contextual() {
            for t in b.tables() {
                for r in t.rows() {
                    if let Err(e) = r.values() {
                        self.push(e, Some(&source));
                    }
                }
            }
        }
    }
}

/// The primary label's offset and length, or zeroes for an error with
/// no span.
fn primary_span(error: &EvalError) -> (usize, usize) {
    error
        .labels()
        .and_then(|mut labels| labels.next())
        .map_or((0, 0), |label| (label.offset(), label.len()))
}

fn failure_key(error: &EvalError, source: Option<&NamedSource<Arc<str>>>) -> FailureKey {
    let (offset, len) = primary_span(error);
    (
        error.code().map(|code| code.to_string()),
        error.to_string(),
        source.map(|source| source.name().to_string()),
        offset,
        len,
    )
}
