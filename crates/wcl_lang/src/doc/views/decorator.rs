//! Views over decorators and their arguments.
//!
//! A decorator's arguments are expressions, so reading one means
//! evaluating it. Both views read through a `DecoratorCell`, which
//! memoises that evaluation — including its failure, so a bad argument
//! reports identically on every read rather than being retried.

use super::*;

#[derive(Debug)]
/// A `@name(args…)` annotation, with its arguments evaluated lazily
/// and cached.
pub struct Decorator<'a> {
    /// The AST node this view borrows.
    ast: &'a ast::Decorator,
    /// Lazily-evaluated cache for this decorator's arguments.
    cell: &'a DecoratorCell,
    /// The document these views read through.
    doc: &'a Document,
    /// Namespace of the source that carries this decorator. Bare names
    /// resolve relative to their use site, not the root document.
    file_ns: &'a [String],
}

/// Pair each decorator AST node with its evaluation cache, yielding a
/// [`Decorator`] view per entry. The two slices are index-aligned by
/// construction.
pub(in crate::doc) fn iter_decorators<'a>(
    ast: &'a [ast::Decorator],
    cells: &'a [DecoratorCell],
    doc: &'a Document,
    file_ns: &'a [String],
) -> impl Iterator<Item = Decorator<'a>> + 'a {
    ast.iter().zip(cells).map(move |(ast, cell)| Decorator {
        ast,
        cell,
        doc,
        file_ns,
    })
}

impl<'a> Decorator<'a> {
    /// Assemble a decorator view from its AST node, evaluation cache and
    /// resolution context.
    pub(in crate::doc) fn from_parts(
        ast: &'a ast::Decorator,
        cell: &'a DecoratorCell,
        doc: &'a Document,
        file_ns: &'a [String],
    ) -> Self {
        Self {
            ast,
            cell,
            doc,
            file_ns,
        }
    }

    /// The declared name.
    pub fn name(&self) -> &'a str {
        self.ast
            .name
            .last()
            .expect("decorator name has at least one segment")
    }

    /// The declared name, split on `.`.
    pub fn name_segments(&self) -> &'a [String] {
        &self.ast.name
    }

    /// The dotted name as a single string.
    pub fn full_name(&self) -> String {
        self.ast.name.join(".")
    }

    /// `true` if this decorator's single-segment name matches the
    /// canonical name of `dec`. Cheap (no allocation, unlike
    /// `full_name()`), so prefer this for filtering against builtin
    /// decorator names.
    pub(crate) fn is(&self, dec: BuiltinDecorator) -> bool {
        self.ast.name.len() == 1 && self.ast.name[0] == dec.as_str()
    }

    /// Source span of this node in the file that declares it.
    pub fn span(&self) -> Span {
        self.ast.span
    }

    /// Span of the dotted decorator name, excluding `@` and arguments.
    pub fn name_span(&self) -> Span {
        self.ast.name_span
    }

    /// Source spans index-aligned with this decorator's positional arguments.
    pub fn positional_spans(&self) -> &'a [Span] {
        &self.ast.positional_spans
    }

    /// The decorator schema selected by this decorator's authored name.
    /// A dotted prefix is a namespace qualifier; a bare name prefers this
    /// decorator's use-site namespace before the root and first match.
    pub fn schema(&self) -> Option<TypeDecl<'a>> {
        let (name, qualifier) = self.ast.name.split_last()?;
        self.doc.decorator_schema_in(qualifier, name, self.file_ns)
    }

    /// Evaluate every positional argument. The result is cached so
    /// repeated calls return the same eval outcome without re-running.
    pub fn positional(&self) -> Result<Vec<Value>, EvalError> {
        let result = self.cell.positional.get_or_init(|| {
            self.ast
                .positional
                .iter()
                .map(|e| self.doc.eval(e))
                .collect()
        });
        match result {
            Ok(v) => Ok(v.clone()),
            Err(e) => Err(e.clone()),
        }
    }

    /// The decorator's named `key = value` arguments, in source order.
    pub fn named(&self) -> impl Iterator<Item = NamedArg<'a>> + 'a {
        let parent_ast = self.ast;
        let cell = self.cell;
        let doc = self.doc;
        self.ast.named.iter().map(move |n| NamedArg {
            ast: n,
            parent_ast,
            parent: cell,
            doc,
        })
    }

    /// Evaluate the named argument `name`, or `None` when the decorator
    /// does not carry one. The result is cached across calls, so a
    /// failing argument reports the same error every time rather than
    /// being re-evaluated.
    pub fn named_arg(&self, name: &str) -> Option<Result<Value, EvalError>> {
        let map = self.cell.named.get_or_init(|| {
            self.ast
                .named
                .iter()
                .map(|n| (n.name.clone(), self.doc.eval(&n.value)))
                .collect()
        });
        map.get(name).map(|r| match r {
            Ok(v) => Ok(v.clone()),
            Err(e) => Err(e.clone()),
        })
    }

    /// Resolve the value of one declared slot on this decorator's
    /// schema. If the slot's declared type is a union, the
    /// decorator's positional + named args are dispatched into a
    /// `Value::Variant` by structural shape. Otherwise, the named
    /// arg is consulted first, then the positional arg at the slot's
    /// `@inline(N)` index — so `@block("books")` resolves the `name`
    /// slot from positional[0] when no `name = ...` was written. A slot
    /// without `@inline` is named-only.
    ///
    /// If neither a named nor positional argument fills the slot, its
    /// declared default is returned. Returns `None` when the decorator has
    /// no registered schema, the schema doesn't declare the named slot, or
    /// the slot is absent and has no default.
    pub fn resolved_arg_value(&self, slot_name: &str) -> Option<Result<Value, EvalError>> {
        let schema = self.schema()?;
        let slot = schema.field(slot_name)?;
        if let Some(v) = self.named_arg(slot_name) {
            return Some(v);
        }
        if let Some(slot_idx) = slot.inline_slot() {
            let positional = match self.positional() {
                Ok(v) => v,
                Err(e) => return Some(Err(e)),
            };
            if let Some(value) = positional.into_iter().nth(slot_idx as usize) {
                return Some(Ok(value));
            }
        }
        // Resolve a union slot from the decorator schema's namespace, not
        // the decorator use site's. Two libraries may both call the union
        // `Choice`, just as they may both declare this decorator name.
        // Arguments claimed by the schema's other slots are removed before
        // structural dispatch, so `@select("label", message = "chosen")`
        // can carry both ordinary metadata and a union payload.
        if let ResolvedType::Union(union) = slot.resolved_type() {
            let claimed_positions: HashSet<usize> = schema
                .fields()
                .filter(|field| field.name() != slot_name)
                .filter_map(|field| field.inline_slot().map(|index| index as usize))
                .collect();
            let (positional, positional_spans): (Vec<Value>, Vec<Span>) = match self.positional() {
                Ok(values) => values
                    .into_iter()
                    .zip(self.positional_spans().iter().copied())
                    .enumerate()
                    .filter_map(|(index, value_and_span)| {
                        (!claimed_positions.contains(&index)).then_some(value_and_span)
                    })
                    .unzip(),
                Err(error) => return Some(Err(error)),
            };
            let mut named = std::collections::BTreeMap::new();
            let mut named_spans = std::collections::BTreeMap::new();
            for argument in self.named() {
                if schema.field(argument.name()).is_some() {
                    continue;
                }
                let value = match argument.value() {
                    Ok(value) => value,
                    Err(error) => return Some(Err(error)),
                };
                named_spans.insert(argument.name().to_string(), argument.span());
                named.insert(argument.name().to_string(), value);
            }
            if positional.is_empty()
                && named.is_empty()
                && let Some(default) = slot.default_value()
            {
                return Some(Ok(default));
            }
            return Some(variant_dispatch::decorator_to_variant(
                self.doc,
                &positional,
                &positional_spans,
                &named,
                &named_spans,
                union,
                self.ast.span,
            ));
        }
        slot.default_value().map(Ok)
    }

    /// Dispatch the decorator's positional + named args into a
    /// `Value::Variant` for the given union, by structural shape.
    /// Returns `VariantNoMatch` if the args don't fit any variant,
    /// `VariantAmbiguous` defensively if multiple variants match.
    pub fn dispatch_into_union(&self, union: UnionDecl<'a>) -> Result<Value, EvalError> {
        let positional = self.positional()?;
        let positional_spans = self.positional_spans();
        let mut named_map: std::collections::BTreeMap<String, Value> =
            std::collections::BTreeMap::new();
        let mut named_spans: std::collections::BTreeMap<String, Span> =
            std::collections::BTreeMap::new();
        for n in self.named() {
            let v = n.value()?;
            named_spans.insert(n.name().to_string(), n.span());
            named_map.insert(n.name().to_string(), v);
        }
        variant_dispatch::decorator_to_variant(
            self.doc,
            &positional,
            positional_spans,
            &named_map,
            &named_spans,
            union,
            self.ast.span,
        )
    }
}

/// One `key = value` argument of a [`Decorator`].
pub struct NamedArg<'a> {
    /// The AST node this view borrows.
    ast: &'a ast::NamedArg,
    /// The parent decorator's full AST, used to seed the shared named-arg
    /// cache on first access from any sibling.
    parent_ast: &'a ast::Decorator,
    /// Cache of the decorator this argument belongs to.
    parent: &'a DecoratorCell,
    /// The document these views read through.
    doc: &'a Document,
}

impl<'a> NamedArg<'a> {
    /// The declared name.
    pub fn name(&self) -> &'a str {
        &self.ast.name
    }

    /// Cached via the parent [`DecoratorCell`]'s named-arg map.
    pub fn value(&self) -> Result<Value, EvalError> {
        let map = self.parent.named.get_or_init(|| {
            self.parent_ast
                .named
                .iter()
                .map(|n| (n.name.clone(), self.doc.eval(&n.value)))
                .collect()
        });
        match map.get(&self.ast.name) {
            Some(Ok(v)) => Ok(v.clone()),
            Some(Err(e)) => Err(e.clone()),
            None => self.doc.eval(&self.ast.value),
        }
    }

    /// Source span of this node in the file that declares it.
    pub fn span(&self) -> Span {
        self.ast.span
    }
}
