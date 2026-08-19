//! Decorator validation.
//!
//! Checks every decorator a document writes against the `@decorator`
//! declaration that gives it meaning: that the name is declared at all,
//! that `@applies_to` admits the syntactic position it was written in,
//! and that a non-repeatable decorator occurs at most once per node.
//!
//! Argument checking is not here — a decorator's arguments are validated
//! against its schema type by the ordinary schema pass, since a
//! decorator declaration is an ordinary type declaration.

use std::collections::HashSet;
use std::path::PathBuf;

use miette::NamedSource;

use crate::ast::{self, Span};
use crate::error::EvalError;
use crate::value::Value;

use super::cells::{DecoratorCell, ItemCellKind, ItemCells, LoadedImport};
use super::imports::load_import_lazily;
use super::schema_check::{has_annotation_exemption, has_schemaless};
use super::views::{DeclName, Decorator};
use super::{CollectedSchemaErrors, Document, schema_check};

impl Document {
    /// Check every decorator on a list of items against its
    /// `@decorator` declaration — name, applicability and cardinality.
    pub(super) fn validate_decorators_in_items(
        &self,
        items: &[ast::Item],
        cells: &[ItemCells],
        file_ns: &[String],
        seen_imports: &mut HashSet<PathBuf>,
        source: &NamedSource<String>,
        out: &mut CollectedSchemaErrors,
    ) {
        for (item, cell) in items.iter().zip(cells) {
            match item {
                ast::Item::Field(field) => self.collect_decorator_group(
                    &field.decorators,
                    &cell.decorators,
                    "field",
                    None,
                    file_ns,
                    source,
                    out,
                ),
                ast::Item::Let(function) if function.fn_syntax => self.collect_decorator_group(
                    &function.decorators,
                    &cell.decorators,
                    "fn",
                    None,
                    file_ns,
                    source,
                    out,
                ),
                ast::Item::Block(block) => {
                    self.collect_decorator_group(
                        &block.decorators,
                        &cell.decorators,
                        "block",
                        Some(&block.kind),
                        file_ns,
                        source,
                        out,
                    );
                    if has_schemaless(&block.decorators) {
                        continue;
                    }
                    if let ItemCellKind::Block {
                        items: block_cells, ..
                    } = &cell.kind
                    {
                        self.validate_decorators_in_items(
                            &block.items,
                            block_cells,
                            file_ns,
                            seen_imports,
                            source,
                            out,
                        );
                    }
                }
                ast::Item::TypeDecl(declaration) => {
                    self.collect_decorator_group(
                        &declaration.decorators,
                        &cell.decorators,
                        "type",
                        None,
                        file_ns,
                        source,
                        out,
                    );
                    let ItemCellKind::TypeDecl { field_decorators } = &cell.kind else {
                        unreachable!("type declaration has type cells")
                    };
                    for (field, decorator_cells) in declaration.fields.iter().zip(field_decorators)
                    {
                        self.collect_decorator_group(
                            &field.decorators,
                            decorator_cells,
                            "type_field",
                            None,
                            file_ns,
                            source,
                            out,
                        );
                    }
                }
                ast::Item::InterfaceDecl(declaration) => {
                    self.collect_decorator_group(
                        &declaration.decorators,
                        &cell.decorators,
                        "interface",
                        None,
                        file_ns,
                        source,
                        out,
                    );
                    let ItemCellKind::InterfaceDecl { field_decorators } = &cell.kind else {
                        unreachable!("interface declaration has interface cells")
                    };
                    for (field, decorator_cells) in declaration.fields.iter().zip(field_decorators)
                    {
                        self.collect_decorator_group(
                            &field.decorators,
                            decorator_cells,
                            "type_field",
                            None,
                            file_ns,
                            source,
                            out,
                        );
                    }
                }
                ast::Item::UnionDecl(declaration) => {
                    self.collect_decorator_group(
                        &declaration.decorators,
                        &cell.decorators,
                        "union",
                        None,
                        file_ns,
                        source,
                        out,
                    );
                    let ItemCellKind::UnionDecl {
                        variant_decorators,
                        variant_field_decorators,
                    } = &cell.kind
                    else {
                        unreachable!("union declaration has union cells")
                    };
                    for (variant_index, variant) in declaration.variants.iter().enumerate() {
                        self.collect_decorator_group(
                            &variant.decorators,
                            &variant_decorators[variant_index],
                            "variant",
                            None,
                            file_ns,
                            source,
                            out,
                        );
                        if let ast::VariantBody::Record { fields, .. } = &variant.body {
                            for (field, decorator_cells) in
                                fields.iter().zip(&variant_field_decorators[variant_index])
                            {
                                self.collect_decorator_group(
                                    &field.decorators,
                                    decorator_cells,
                                    "type_field",
                                    None,
                                    file_ns,
                                    source,
                                    out,
                                );
                            }
                        }
                    }
                }
                ast::Item::SymbolSetDecl(declaration) => {
                    self.collect_decorator_group(
                        &declaration.decorators,
                        &cell.decorators,
                        "symbol_set",
                        None,
                        file_ns,
                        source,
                        out,
                    );
                    let ItemCellKind::SymbolSetDecl { symbol_decorators } = &cell.kind else {
                        unreachable!("symbol-set declaration has symbol-set cells")
                    };
                    for (symbol, decorator_cells) in
                        declaration.symbols.iter().zip(symbol_decorators)
                    {
                        self.collect_decorator_group(
                            &symbol.decorators,
                            decorator_cells,
                            "symbol",
                            None,
                            file_ns,
                            source,
                            out,
                        );
                    }
                }
                ast::Item::ConnectionDecl(declaration) => self.collect_decorator_group(
                    &declaration.decorators,
                    &cell.decorators,
                    "connection",
                    None,
                    file_ns,
                    source,
                    out,
                ),
                ast::Item::Import(_)
                    if let ItemCellKind::Import {
                        path,
                        system,
                        base_dir,
                        path_span,
                        loaded,
                    } = &cell.kind =>
                {
                    let loaded = loaded.get_or_init(|| {
                        load_import_lazily(
                            path,
                            base_dir.as_deref(),
                            *system,
                            *path_span,
                            self.loader(),
                        )
                    });
                    if let Ok(loaded) = loaded
                        && seen_imports.insert(loaded.path.clone())
                    {
                        self.validate_decorators_in_loaded_import(loaded, seen_imports, out);
                    }
                }
                _ => {}
            }
        }
    }

    /// Run decorator validation over an eagerly-loaded import, so a
    /// violation in an imported file is reported against that file.
    fn validate_decorators_in_loaded_import(
        &self,
        loaded: &LoadedImport,
        seen_imports: &mut HashSet<PathBuf>,
        out: &mut CollectedSchemaErrors,
    ) {
        let source = NamedSource::new(loaded.path.display().to_string(), loaded.source.clone());
        self.validate_decorators_in_items(
            &loaded.items,
            &loaded.cells,
            &loaded.file_ns,
            seen_imports,
            &source,
            out,
        );
    }

    #[allow(clippy::too_many_arguments)]
    /// Gather the decorators attached to one syntax node, grouped by
    /// name, so cardinality can be checked per group.
    fn collect_decorator_group(
        &self,
        decorators: &[ast::Decorator],
        decorator_cells: &[DecoratorCell],
        position: &str,
        block_kind: Option<&str>,
        file_ns: &[String],
        source: &NamedSource<String>,
        out: &mut CollectedSchemaErrors,
    ) {
        let mut errors = Vec::new();
        self.validate_decorator_group(
            decorators,
            decorator_cells,
            position,
            block_kind,
            file_ns,
            &mut errors,
        );
        for error in errors {
            let duplicate = out.iter().any(|(existing, existing_source)| {
                existing == &error
                    && existing_source
                        .as_ref()
                        .is_none_or(|existing_source| existing_source.name() == source.name())
            });
            if !duplicate {
                out.push((error, Some(source.clone())));
            }
        }
    }

    /// Check one name-group of decorators: that the name is declared,
    /// applies in this position, and is repeatable if repeated.
    pub(super) fn validate_decorator_group(
        &self,
        decorators: &[ast::Decorator],
        decorator_cells: &[DecoratorCell],
        position: &str,
        block_kind: Option<&str>,
        file_ns: &[String],
        out: &mut Vec<EvalError>,
    ) {
        use crate::error::SchemaViolationKind as Kind;

        if has_annotation_exemption(decorators) {
            return;
        }

        let mut seen = HashSet::new();
        for (decorator_ast, decorator_cell) in decorators.iter().zip(decorator_cells) {
            let decorator = Decorator::from_parts(decorator_ast, decorator_cell, self, file_ns);
            let Some(schema) = decorator.schema() else {
                let (kind, message) = if decorator.name_segments().len() > 1 {
                    (
                        Kind::UndeclaredDecorator,
                        format!(
                            "decorator '@{}' has no declaration in the qualified namespace",
                            decorator.full_name()
                        ),
                    )
                } else {
                    (
                        Kind::UndeclaredDecorator,
                        format!(
                            "decorator '{}' has no @decorator declaration",
                            decorator.full_name()
                        ),
                    )
                };
                out.push(EvalError::schema_violation_named(
                    kind,
                    message,
                    decorator.full_name(),
                    decorator.name_span(),
                ));
                continue;
            };
            // The grammar-shaped walk is the only path guaranteed to reach
            // every node (union-dispatched and contextual blocks deliberately
            // bypass recursive block validation). Public block/field checks
            // may already have emitted the same argument error, so retain one
            // diagnostic per offending source span.
            for error in schema_check::decorator_argument_errors(self, &decorator) {
                if !out.contains(&error) {
                    out.push(error);
                }
            }
            let canonical_name = schema.full_name();
            let repeatable = schema
                .decorators()
                .find(|candidate| candidate.full_name() == "decorator")
                .and_then(|declaration| declaration.named_arg("repeatable"))
                .and_then(Result::ok)
                .is_some_and(|value| matches!(value, Value::Bool(true)));
            if !repeatable && !seen.insert(canonical_name) {
                EvalError::push_schema_violation(
                    out,
                    Kind::DecoratorCardinality,
                    format!(
                        "decorator '@{}' may appear at most once on one node",
                        decorator.full_name()
                    ),
                    decorator.name_span(),
                );
            }

            let Some(applies_to) = schema
                .decorators()
                .find(|candidate| candidate.full_name() == "applies_to")
            else {
                continue;
            };
            let Some(Ok(Value::List(positions))) = applies_to.named_arg("on") else {
                continue;
            };
            if let Some(position_set) = self.symbol_set("DecoratorPosition")
                && positions.iter().any(|value| match value {
                    Value::Symbol(position) => !position_set.has(position),
                    _ => true,
                })
            {
                continue;
            }
            let allowed = positions
                .iter()
                .any(|value| matches!(value, Value::Symbol(allowed) if allowed == position));
            if !allowed {
                EvalError::push_schema_violation(
                    out,
                    Kind::DecoratorNotApplicable,
                    format!(
                        "decorator '@{}' is not applicable in the '{position}' position",
                        decorator.full_name()
                    ),
                    decorator.name_span(),
                );
                continue;
            }
            if let Some(kind) = block_kind
                && let Some(Ok(Value::List(kinds))) = applies_to.named_arg("kinds")
            {
                let declared_kinds: Vec<&str> = kinds
                    .iter()
                    .filter_map(|value| match value {
                        Value::Utf8(kind) => Some(kind.as_str()),
                        _ => None,
                    })
                    .collect();
                // A misspelled kind is a declaration error. Do not turn it
                // into one additional error at every otherwise-correct use
                // site; the author needs to fix the declaration first.
                if declared_kinds.iter().any(|declared| {
                    self.block_schema_in(&[], declared, schema.file_ns())
                        .is_none()
                }) {
                    continue;
                }
                let allowed_kind = declared_kinds.contains(&kind);
                if !allowed_kind {
                    EvalError::push_schema_violation(
                        out,
                        Kind::DecoratorNotApplicable,
                        format!(
                            "decorator '@{}' is not applicable to block kind '{kind}'",
                            decorator.full_name()
                        ),
                        decorator.name_span(),
                    );
                }
            }
        }
    }
}

/// The identifier portion of a decorator occurrence, excluding the leading
/// `@` and any argument list. Decorator paths use ASCII `.` separators;
/// identifier lengths are byte lengths, matching parser spans.
pub(super) fn decorator_name_span(decorator: &Decorator<'_>) -> Span {
    decorator_name_span_from_parts(decorator.span(), decorator.name_segments())
}

/// Narrow a decorator's full span to just its dotted name, so a
/// name-level diagnostic does not underline the arguments too.
fn decorator_name_span_from_parts(span: Span, name: &[String]) -> Span {
    let start = span.start + 1;
    let len = name.iter().map(String::len).sum::<usize>() + name.len().saturating_sub(1);
    Span::new(start, start + len)
}
