//! What a document says about a *kind*: which kinds it gathers, the field
//! that gathers each one, and the child families and body slot a kind's
//! schema declares.
//!
//! These are facts about a WCL document regardless of who is looking, so
//! they live here rather than in any one consumer — `wcl wdoc`, the
//! language server and the editor all ask the same question and must get
//! the same answer. The *interpretations* built on top (that a scalar
//! `identifier` field named after another kind means containment, that a
//! kind carrying a source and a destination is an edge) are conventions of
//! the tool doing the looking, and stay there.

use super::views::BuiltinDecorator;
use super::{Document, ResolvedType, TypeDecl, TypeField};

/// A block kind gathered by the document's merged `@document` schemas,
/// with the gather field that collects it and the type that schemas it.
pub struct GatheredKind<'a> {
    kind: String,
    field: String,
    schema: TypeDecl<'a>,
}

impl<'a> GatheredKind<'a> {
    /// The block kind, as written in `@children("kind")`.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// The `@document` field gathering instances of the kind.
    pub fn gather_field(&self) -> &str {
        &self.field
    }

    /// The type declaration schemaing an instance.
    pub fn schema(&self) -> &TypeDecl<'a> {
        &self.schema
    }

    /// The schema by value, for callers keeping it past the gather list.
    pub fn into_schema(self) -> TypeDecl<'a> {
        self.schema
    }
}

/// One `@child` / `@children` family a type declares: the nested block kind
/// a field binds, and the type schemaing those nested blocks.
pub struct ChildFamily<'a> {
    field: String,
    kind: String,
    many: bool,
    schema: Option<TypeDecl<'a>>,
    doc: Option<String>,
}

impl<'a> ChildFamily<'a> {
    /// The declaring field's name.
    pub fn field(&self) -> &str {
        &self.field
    }

    /// The nested block kind.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// `true` for `@children` (a list), `false` for a single `@child`.
    pub fn many(&self) -> bool {
        self.many
    }

    /// The type schemaing an instance, resolved through the FIELD's own
    /// declared type — namespace-correct where a bare kind-name lookup is
    /// not.
    pub fn schema(&self) -> Option<&TypeDecl<'a>> {
        self.schema.as_ref()
    }

    /// The declaring field's doc comment.
    pub fn doc_comment(&self) -> Option<&str> {
        self.doc.as_deref()
    }
}

impl Document {
    /// Every block kind the merged `@document` schemas gather, in
    /// declaration order, first declaration winning on a repeated kind.
    ///
    /// `@document` schemas compose per namespace (see
    /// `Document::doc_schemas_for_ns`), so this walks *every*
    /// `@document`-decorated type — a user schema adding top-level gathers
    /// alongside an imported library's contributes its kinds here too. A
    /// gather whose element type resolves to nothing (and whose kind names
    /// no `@block` type either) is skipped: there is no schema to describe.
    pub fn gathered_kinds(&self) -> Vec<GatheredKind<'_>> {
        let mut seen: Vec<String> = Vec::new();
        let mut out: Vec<GatheredKind<'_>> = Vec::new();
        for decl in self.type_decls() {
            if !decl.decorators().any(|d| d.is(BuiltinDecorator::Document)) {
                continue;
            }
            for field in decl.effective_fields() {
                let Some(kind) = field.children_block_kind() else {
                    continue;
                };
                if seen.contains(&kind) {
                    continue;
                }
                seen.push(kind.clone());
                let Some(schema) = field.element_decl().or_else(|| self.block_schema(&kind)) else {
                    continue;
                };
                out.push(GatheredKind {
                    kind,
                    field: field.name().to_string(),
                    schema,
                });
            }
        }
        out
    }

    /// The `@document` field gathering instances of `kind`, if any.
    ///
    /// Deliberately independent of whether the kind's schema resolves: a
    /// caller building a `<gather>.<id>` data path needs the field name,
    /// not the element type.
    pub fn gather_field(&self, kind: &str) -> Option<String> {
        self.type_decls()
            .filter(|d| d.decorators().any(|dec| dec.is(BuiltinDecorator::Document)))
            .flat_map(|d| d.effective_fields())
            .find(|f| f.children_block_kind().as_deref() == Some(kind))
            .map(|f| f.name().to_string())
    }
}

impl<'a> TypeDecl<'a> {
    /// The `@child` / `@children` families this type declares, in field
    /// order. Each family's schema resolves through the declaring field's
    /// own type, so a nested kind whose name is shared across namespaces
    /// (an `acme` `container` vs wdoc's diagram-grouping shape) resolves to
    /// the one the field actually names.
    pub fn child_families(&self) -> Vec<ChildFamily<'a>> {
        let mut out = Vec::new();
        for f in self.effective_fields() {
            let (kind, many) = match (f.child_block_kind(), f.children_block_kind()) {
                (Some(k), _) => (k, false),
                (_, Some(k)) => (k, true),
                _ => continue,
            };
            let schema = f.element_decl().or_else(|| self.doc.block_schema(&kind));
            out.push(ChildFamily {
                field: f.name().to_string(),
                kind,
                many,
                schema,
                doc: f.doc_comment(),
            });
        }
        out
    }

    /// Whether this type declares a `body` child slot — the prose a
    /// projection renders, in either the single (`@child("body")`) or the
    /// addressable-list (`@children("body")`) form.
    pub fn declares_body(&self) -> bool {
        self.effective_fields().into_iter().any(|f| {
            f.child_block_kind().as_deref() == Some("body")
                || f.children_block_kind().as_deref() == Some("body")
        })
    }
}

impl<'a> TypeField<'a> {
    /// The type declaration this field's declared type names, looking
    /// through `list<…>` and `&…` wrappers. Resolved in the field's OWN
    /// namespace (see [`TypeField::resolved_type`]), which is what makes it
    /// namespace-correct where a kind-name lookup is not.
    pub fn element_decl(&self) -> Option<TypeDecl<'a>> {
        fn named(t: ResolvedType<'_>) -> Option<TypeDecl<'_>> {
            match t {
                ResolvedType::Named(d) => Some(d),
                ResolvedType::List(inner) | ResolvedType::Reference(inner) => named(*inner),
                _ => None,
            }
        }
        named(self.resolved_type())
    }
}

#[cfg(test)]
mod tests {
    use crate::{DeclName, Document};

    const SRC: &str = r#"
@block("part")
type Part { @inline(0) id: identifier  name: utf8 }

@block("note")
type Note { @inline(0) id: identifier  text: utf8 }

@block("holder")
type Holder {
  @inline(0) id: identifier
  // Attached prose.
  @child("body") body: Note?
  @children("part") parts: list<Part>
}

@document
type D {
  @children("holder") holders: list<Holder>
  @children("part")   parts:   list<Part>
}
"#;

    fn doc() -> Document {
        Document::open(SRC, "test.wcl").expect("parse")
    }

    #[test]
    fn gathered_kinds_carry_their_gather_field_and_schema() {
        let doc = doc();
        let kinds = doc.gathered_kinds();
        let names: Vec<&str> = kinds.iter().map(|g| g.kind()).collect();
        assert_eq!(names, ["holder", "part"]);
        assert_eq!(kinds[0].gather_field(), "holders");
        assert_eq!(kinds[1].schema().name(), "Part");
        assert_eq!(doc.gather_field("part").as_deref(), Some("parts"));
        assert_eq!(doc.gather_field("note"), None);
    }

    #[test]
    fn child_families_resolve_through_the_field_type() {
        let doc = doc();
        let holder = doc.block_schema("holder").expect("holder schema");
        let families = holder.child_families();
        let kinds: Vec<&str> = families.iter().map(|f| f.kind()).collect();
        assert_eq!(kinds, ["body", "part"]);
        assert!(!families[0].many());
        assert_eq!(families[0].doc_comment(), Some("Attached prose."));
        // The `body` slot is typed `Note`, so that — not a kind named
        // "body" — is what schemas it.
        assert_eq!(families[0].schema().map(|d| d.name()), Some("Note"));
        assert!(families[1].many());
        assert_eq!(families[1].schema().map(|d| d.name()), Some("Part"));
    }

    #[test]
    fn declares_body_covers_both_child_forms() {
        let doc = doc();
        assert!(doc.block_schema("holder").expect("holder").declares_body());
        assert!(!doc.block_schema("part").expect("part").declares_body());

        let listed = Document::open(
            "@block(\"b\")\ntype B { text: utf8 }\n\
             @block(\"h\")\ntype H { @children(\"body\") bodies: list<B> }\n",
            "t.wcl",
        )
        .expect("parse");
        assert!(listed.block_schema("h").expect("h").declares_body());
    }
}
