//! The editor's **kind model**: everything the editor knows about a data
//! object kind, derived once from the open [`Document`] and read by every
//! surface that describes one.
//!
//! The generic half — which kinds a document gathers, which field gathers
//! them, whether a kind declares a body, what child families it declares —
//! is the language library's ([`Document::gathered_kinds`],
//! [`wcl_lang::TypeDecl::child_families`]). What lives here is the half
//! that is editor *convention*, true of how this editor reads a schema
//! rather than of WCL:
//!
//! - a **parent link** is a scalar `identifier` field whose NAME is another
//!   gathered kind's name (`component.container`, `system.boundary`), plus
//!   `parent` for self-nesting (`infra_node.parent`). Inline id slots name
//!   the block itself and never count.
//! - a **reference** is any other `identifier` / `list<identifier>` field
//!   (`repo`, `built_by`, `supersedes`) — wiring the client may draw, not
//!   containment.
//! - an **edge kind** carries both a `source` and a `destination`
//!   identifier field; its endpoints are neither parents nor references.
//! - **suggestions** are free-text values that REPEAT across a kind's
//!   instances — a taxonomy in practice even where the schema didn't spell
//!   it as a `symbol_set`.
//! - wdoc's own infrastructure gathers (pages, sites, components) and the
//!   wskill plumbing kinds are not data objects a user adds.
//!
//! Every one of those conventions turns on what a field's type IS, and
//! asks [`wcl_lang::TypeField::shape`] for it rather than rendering the
//! type and matching the text. The two differ where it costs most:
//! through an alias (`type NodeId = identifier`) a parent link still
//! reads as one, and a declaration merely NAMED `fnord` is not a
//! function. A printed-string comparison gets both wrong in silence — the
//! field just stops being a parent link, with no error anywhere.
//!
//! Every endpoint that describes a kind serves [`Kind::json`], so the
//! client has one reader. The only member derived from instance data is
//! `suggestions` — it walks every block of the kind and forces field
//! evaluation, so it is opt-in
//! ([`KindModel::json_with_suggestions`]): the Systems view asks, the
//! palette does not.
//!
//! The model is the interface: nothing here hands out the [`Document`],
//! and a nested kind is reached by naming its family
//! ([`KindModel::describe_family`]) rather than by passing a
//! [`TypeDecl`] back in — so a surface cannot quietly grow a twelfth walk
//! over the type declarations beside the one everything else reads. A
//! consumer that needs a new schema fact adds it to [`Kind`].

use wcl_lang::{BuiltinType, ChildFamily, DeclName, Document, FieldShape, TypeDecl, TypeField};

use super::util::{dec_first_string, field_string, first_label, value_string};

/// The namespace wdoc's own document gathers (pages, sites, components, …)
/// are declared in. They're infrastructure, not data objects a user
/// models — but a user kind that happens to share one of their NAMES is,
/// which is why the exclusion is by declaring namespace.
const WDOC_NS: &str = "wdoc";

/// Wskill plumbing kinds that never belong in the add-a-unit palette.
const UNIT_KIND_DENYLIST: &[&str] = &[
    "topic",
    "skill",
    "artifact",
    "source",
    "question",
    "wskill_ref",
];

/// How many distinct values a free-text field may have and still read as a
/// vocabulary worth offering as a list.
const MAX_SUGGESTIONS: usize = 40;

/// A field that nests its instance inside another object: the field
/// written, and the kind it names.
#[derive(Clone)]
pub(super) struct ParentLink {
    pub(super) field: String,
    pub(super) kind: String,
}

/// A field that names other objects without containing them.
#[derive(Clone)]
pub(super) struct RefField {
    pub(super) field: String,
    pub(super) list: bool,
}

/// The pair of identifier fields that make a kind wiring rather than a
/// node.
#[derive(Clone)]
pub(super) struct EdgeFields {
    pub(super) source: String,
    pub(super) destination: String,
}

/// One kind's derived structure — its schema, how instances of it nest,
/// what they reference, and whether the kind is an edge rather than a node.
#[derive(Clone)]
pub(super) struct Kind<'a> {
    kind: String,
    schema: TypeDecl<'a>,
    /// In declaration order — an instance nests under the first it sets.
    parents: Vec<ParentLink>,
    /// Cross-references, not containment.
    refs: Vec<RefField>,
    /// Set when the kind is an edge kind.
    edge: Option<EdgeFields>,
}

impl<'a> Kind<'a> {
    pub(super) fn kind(&self) -> &str {
        &self.kind
    }

    /// The namespace the schema is declared in — what makes two kinds
    /// sharing a name neighbours or strangers.
    pub(super) fn namespace(&self) -> Vec<String> {
        self.schema.namespace()
    }

    /// The `@child` / `@children` families the schema declares, each
    /// resolved through the declaring field's own type.
    pub(super) fn child_families(&self) -> Vec<ChildFamily<'a>> {
        self.schema.child_families()
    }

    /// The fully-qualified schema name — the value every endpoint echoes as
    /// `type_name`, and the create path matches on. A bare kind name is
    /// ambiguous across namespaces (a WAD `container` vs wdoc's
    /// diagram-grouping shape); this is not.
    pub(super) fn type_name(&self) -> String {
        self.schema.full_name()
    }

    /// The parent links in declaration order — an instance nests under the
    /// first of these it actually sets.
    pub(super) fn parents(&self) -> &[ParentLink] {
        &self.parents
    }

    /// The endpoint fields when the kind wires two objects together rather
    /// than being one.
    pub(super) fn edge(&self) -> Option<&EdgeFields> {
        self.edge.as_ref()
    }

    /// Whether instances carry a prose `body` (either child form).
    pub(super) fn has_body(&self) -> bool {
        self.schema.declares_body()
    }

    /// Whether the schema's `@inline(0)` slot is identifier-typed — the
    /// wskill/WAD unit convention (`concept alpha` rather than
    /// `concept "alpha"`).
    pub(super) fn id_is_identifier(&self) -> bool {
        self.id_field_decl()
            .map(|f| is_identifier(&f))
            .unwrap_or(false)
    }

    /// A field's declared `@default`, rendered as a string.
    pub(super) fn field_default(&self, name: &str) -> Option<String> {
        self.schema
            .effective_fields()
            .into_iter()
            .find(|f| f.name() == name)
            .and_then(|f| f.default_value().as_ref().map(value_string))
    }

    fn id_field_decl(&self) -> Option<TypeField<'a>> {
        self.schema
            .effective_fields()
            .into_iter()
            .find(|f| f.inline_slot() == Some(0))
    }

    /// The one wire shape every endpoint serves. `suggestions` is null —
    /// nothing here touches instance data, so this costs a schema walk.
    pub(super) fn json(&self) -> serde_json::Value {
        let mut fields: Vec<serde_json::Value> = Vec::new();
        // Declares a `@children(...)` family — an `insert_child` may nest
        // blocks inside instances of this kind (a wireframe container
        // widget, a diagram grouping). The widget palette keys
        // append-inside vs insert-after off it.
        let mut accepts_children = false;
        for f in self.schema.effective_fields() {
            if f.children_kind_or_union().is_some() {
                accepts_children = true;
            }
            // Child blocks / connections aren't form fields.
            if !is_scalar(&f) {
                continue;
            }
            // The shape is resolved in the field's OWN namespace and sees
            // through type aliases — a `wcl.wad` field typed
            // `ContainerKind` must not pick up a same-named set
            // elsewhere, and `type NodeId = identifier` is an identifier.
            let shape = f.shape();
            // Function-valued fields (an SvgBlock's `lower`, computed
            // hooks) aren't form-editable properties. A field typed by a
            // declaration merely NAMED like the keyword is one.
            if shape.is_function() {
                continue;
            }
            let symbols: Option<Vec<String>> = match &shape {
                FieldShape::Symbols(ss) => {
                    Some(ss.symbols().map(|s| s.name().to_string()).collect())
                }
                _ => None,
            };
            fields.push(serde_json::json!({
                "name": f.name(),
                "type": f.type_ref().to_string(),
                "optional": f.optional(),
                "inline_slot": f.inline_slot(),
                "symbols": symbols,
                "default": f.default_value().as_ref().map(value_string),
                "doc": f.doc_comment(),
            }));
        }
        let child_families: Vec<serde_json::Value> = self
            .schema
            .child_families()
            .iter()
            .map(|f| {
                serde_json::json!({
                    "field": f.field(),
                    "kind": f.kind(),
                    "many": f.many(),
                    "doc": f.doc_comment(),
                })
            })
            .collect();
        serde_json::json!({
            "kind": self.kind,
            "type_name": self.type_name(),
            "doc": self.schema.doc_comment(),
            "fields": fields,
            "has_body": self.has_body(),
            "accepts_children": accepts_children,
            "child_families": child_families,
            "parents": self
                .parents
                .iter()
                .map(|p| serde_json::json!({ "field": p.field, "kind": p.kind }))
                .collect::<Vec<_>>(),
            "refs": self
                .refs
                .iter()
                .map(|r| serde_json::json!({ "field": r.field, "list": r.list }))
                .collect::<Vec<_>>(),
            "edge": match &self.edge {
                Some(e) => serde_json::json!({
                    "source": e.source,
                    "destination": e.destination,
                }),
                None => serde_json::Value::Null,
            },
            "id_field": self.id_field_decl().map(|f| f.name().to_string()),
            "suggestions": serde_json::Value::Null,
        })
    }

    /// The values already in use for each free-text field of the kind.
    ///
    /// A `utf8` field whose values REPEAT across instances is a taxonomy in
    /// practice, even where the schema didn't spell it as a `symbol_set` —
    /// a `component`'s `kind` ("module" / "handler" / "store") is the WAD
    /// base schema's own example. Offering those values back stops the
    /// editor inventing a new category out of a typo; a field whose values
    /// are all distinct (every `name`, every `summary`) suggests nothing.
    fn suggestions(&self, doc: &Document) -> serde_json::Value {
        let blocks: Vec<wcl_lang::Block<'_>> =
            doc.blocks().filter(|b| b.kind() == self.kind).collect();
        let mut out = serde_json::Map::new();
        for f in self.schema.effective_fields() {
            // The inline id names the instance; it is never a shared
            // vocabulary.
            if f.inline_slot().is_some() || !is_scalar(&f) || !is_free_text(&f) {
                continue;
            }
            let values: Vec<String> = blocks
                .iter()
                .filter_map(|b| field_string(b, f.name()))
                .collect();
            let mut distinct: Vec<String> = values.clone();
            distinct.sort();
            distinct.dedup();
            if distinct.is_empty()
                || distinct.len() >= values.len()
                || distinct.len() > MAX_SUGGESTIONS
            {
                continue;
            }
            out.insert(f.name().to_string(), serde_json::json!(distinct));
        }
        serde_json::Value::Object(out)
    }
}

/// Every data-object kind the document gathers, with the editor's reading
/// of each one. Built from an open [`Document`] and borrowed from it.
pub(super) struct KindModel<'a> {
    doc: &'a Document,
    kinds: Vec<Kind<'a>>,
}

impl<'a> KindModel<'a> {
    pub(super) fn new(doc: &'a Document) -> Self {
        // Excluded by declaring namespace — a namespace SEGMENT, not a
        // prefix of the rendered name, so the filter says what it means
        // and a user kind sharing a name is kept.
        let gathered: Vec<(String, TypeDecl<'a>)> = doc
            .gathered_kinds()
            .into_iter()
            .filter(|g| g.schema().namespace().first().map(String::as_str) != Some(WDOC_NS))
            .map(|g| (g.kind().to_string(), g.into_schema()))
            .collect();
        let names: Vec<String> = gathered.iter().map(|(k, _)| k.clone()).collect();
        let kinds = gathered
            .into_iter()
            .map(|(kind, schema)| derive(kind, schema, &names))
            .collect();
        Self { doc, kinds }
    }

    /// Every gathered data-object kind, in declaration order.
    pub(super) fn kinds(&self) -> &[Kind<'a>] {
        &self.kinds
    }

    pub(super) fn get(&self, kind: &str) -> Option<&Kind<'a>> {
        self.kinds.iter().find(|k| k.kind == kind)
    }

    /// The gathered kind names — what makes a field name mean containment.
    fn kind_names(&self) -> Vec<String> {
        self.kinds.iter().map(|k| k.kind.clone()).collect()
    }

    /// [`Kind::json`] with the values already in use for each of the kind's
    /// free-text fields mined from the document's instances. This forces
    /// evaluation of every block of the kind, so only ask where a form
    /// wants the picker — the model owns the instance walk precisely so
    /// that cost is asked for by name.
    pub(super) fn json_with_suggestions(&self, kind: &Kind<'a>) -> serde_json::Value {
        let mut v = kind.json();
        v["suggestions"] = kind.suggestions(self.doc);
        v
    }

    /// The kinds the add-a-unit palette offers: the gathered kinds minus
    /// wskill plumbing.
    pub(super) fn unit_kinds(&self) -> impl Iterator<Item = &Kind<'a>> {
        self.kinds
            .iter()
            .filter(|k| !UNIT_KIND_DENYLIST.contains(&k.kind.as_str()))
    }

    /// The addable-unit kind names, minus `index` (which the nav model
    /// owns rather than the unit registry).
    pub(super) fn unit_kind_names(&self) -> Vec<String> {
        self.unit_kinds()
            .map(|k| k.kind.clone())
            .filter(|k| k != "index")
            .collect()
    }

    /// Describe any `@block` kind, gathered or not: the gathered entry when
    /// there is one, else a fresh reading of the named schema.
    /// `type_name` (a fully-qualified schema name) disambiguates a kind
    /// name shared across namespaces; a bare name lookup answers whichever
    /// happens to be declared first.
    pub(super) fn describe(&self, kind: &str, type_name: Option<&str>) -> Option<Kind<'a>> {
        if let Some(full) = type_name.filter(|s| !s.is_empty()) {
            if let Some(k) = self.kinds.iter().find(|k| k.type_name() == full) {
                return Some(k.clone());
            }
            if let Some(decl) = self.doc.type_decls().find(|d| d.full_name() == full) {
                return Some(self.describe_decl(kind, decl));
            }
        }
        if let Some(k) = self.get(kind) {
            return Some(k.clone());
        }
        let decl = self.doc.block_schema(kind)?;
        Some(self.describe_decl(kind, decl))
    }

    /// Describe a nested block kind reached through a `@child`/`@children`
    /// family. The schema comes from the declaring field's own type, which
    /// is namespace-correct where a kind-name lookup is not — a WAD
    /// `container` must not be schema'd by wdoc's diagram-grouping shape.
    pub(super) fn describe_family(&self, family: &ChildFamily<'a>) -> Option<Kind<'a>> {
        family
            .schema()
            .map(|d| self.describe_decl(family.kind(), *d))
    }

    fn describe_decl(&self, kind: &str, schema: TypeDecl<'a>) -> Kind<'a> {
        derive(kind.to_string(), schema, &self.kind_names())
    }

    /// The addable diagram shape kinds: every `@block("kind")` type
    /// descending from `wdoc.SvgBlock`. The client curates which surface in
    /// the add-shape palette; the full list is served so any selected shape
    /// — including user-declared ones — gets a schema-driven form.
    pub(super) fn diagram_kinds(&self) -> Vec<serde_json::Value> {
        let names = self.kind_names();
        let mut seen: Vec<String> = Vec::new();
        let mut out: Vec<serde_json::Value> = Vec::new();
        for decl in self.doc.type_decls() {
            let Some(kind) = decl
                .decorators()
                .find(|d| d.name() == "block")
                .and_then(|d| dec_first_string(&d))
            else {
                continue;
            };
            if seen.contains(&kind) || !decl.is_descendant_of("wdoc.SvgBlock") {
                continue;
            }
            seen.push(kind.clone());
            out.push(derive(kind, decl, &names).json());
        }
        out.sort_by(|a, b| a["kind"].as_str().cmp(&b["kind"].as_str()));
        out
    }
}

// ---------------------------------------------------------------------------
// Which data model a document carries
// ---------------------------------------------------------------------------
//
// These read the document's instances, not the kind model, so they are
// free functions over the `Document` rather than methods reaching through
// the model at it. They sit beside the model because they answer the same
// question one step earlier: which vocabulary is this document written in,
// and so which surface should the editor open.

/// Whether the document carries the wskill data model (a gathered `topic`).
pub(super) fn is_wskill(doc: &Document) -> bool {
    doc.blocks().any(|b| b.kind() == "topic")
}

/// Whether the document carries the WAD data model (its one `wad` root
/// metadata block) — the flag that opens the Systems view.
pub(super) fn is_wad(doc: &Document) -> bool {
    doc.blocks().any(|b| b.kind() == "wad")
}

/// `book` / `website` / `presentation`, from the selected `site` block's
/// nav-declaring child (`toc` / `menu` / `deck`).
pub(super) fn site_kind(doc: &Document, site: Option<&str>) -> &'static str {
    let block = doc.blocks().find(|b| {
        b.kind() == "site"
            && match site {
                Some(name) => first_label(b).as_deref() == Some(name),
                None => true,
            }
    });
    let Some(site) = block else { return "book" };
    let child_kinds: Vec<String> = site.blocks().map(|b| b.kind().to_string()).collect();
    if child_kinds.iter().any(|k| k == "deck") {
        "presentation"
    } else if child_kinds.iter().any(|k| k == "menu") {
        "website"
    } else {
        "book"
    }
}

/// Read one kind off its schema, against the gathered kind `names` that
/// make a field name mean containment.
fn derive<'a>(kind: String, schema: TypeDecl<'a>, names: &[String]) -> Kind<'a> {
    let mut parents: Vec<ParentLink> = Vec::new();
    let mut refs: Vec<RefField> = Vec::new();
    let mut source = None;
    let mut destination = None;
    for f in schema.effective_fields() {
        if !is_scalar(&f) {
            continue;
        }
        let name = f.name().to_string();
        if is_identifier(&f) {
            if name == "source" {
                source = Some(name.clone());
            } else if name == "destination" {
                destination = Some(name.clone());
            }
            if f.inline_slot().is_some() {
                continue;
            }
            if name == "parent" {
                parents.push(ParentLink {
                    field: name,
                    kind: kind.clone(),
                });
            } else if names.contains(&name) {
                parents.push(ParentLink {
                    field: name.clone(),
                    kind: name,
                });
            } else {
                refs.push(RefField {
                    field: name,
                    list: false,
                });
            }
        } else if is_identifier_list(&f) {
            refs.push(RefField {
                field: name,
                list: true,
            });
        }
    }
    let edge = match (source, destination) {
        (Some(source), Some(destination)) => Some(EdgeFields {
            source,
            destination,
        }),
        _ => None,
    };
    if let Some(e) = &edge {
        parents.retain(|p| p.field != e.source && p.field != e.destination);
        refs.retain(|r| r.field != e.source && r.field != e.destination);
    }
    Kind {
        kind,
        schema,
        parents,
        refs,
        edge,
    }
}

// The three readings of a field's TYPE the conventions above turn on.
// Each asks [`TypeField::shape`], which resolves in the field's own
// namespace and sees through type aliases — where matching the printed
// type reclassifies the field silently.

/// Whether the field holds exactly one `identifier` — the type that makes
/// a field a link to another object.
fn is_identifier(f: &TypeField<'_>) -> bool {
    f.shape().builtin() == Some(BuiltinType::Identifier)
}

/// Whether the field holds `list<identifier>` — the same link, many times.
fn is_identifier_list(f: &TypeField<'_>) -> bool {
    f.shape().list_element().and_then(FieldShape::builtin) == Some(BuiltinType::Identifier)
}

/// Whether the field holds free text — the only kind whose repeated
/// values read as a vocabulary.
fn is_free_text(f: &TypeField<'_>) -> bool {
    matches!(
        f.shape().builtin(),
        Some(BuiltinType::Utf8 | BuiltinType::Ascii)
    )
}

/// Scalar fields only: child blocks, child-block lists and connections are
/// structure, not properties.
fn is_scalar(f: &TypeField<'_>) -> bool {
    f.child_kind_or_union().is_none()
        && f.children_kind_or_union().is_none()
        && f.connection_schema().is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A miniature schema exercising every derivation rule: a three-level
    /// containment chain, a two-candidate parent, a self-parent, plain
    /// references, an edge kind, a nested child family and a body slot.
    const SCHEMA: &str = r#"
@block("zone")
type Zone { @inline(0) id: identifier  name: utf8 }

@block("system")
type System { @inline(0) id: identifier  name: utf8  zone: identifier?  repo: identifier? }

@block("wparam")
type WParam { @inline(0) id: identifier  name: utf8  @default(false) required: bool }

@block("wendpoint")
type WEndpoint {
  @inline(0) id: identifier
  path: utf8
  @children("wparam") params: list<WParam>
}

@block("prose")
type Prose { text: utf8? }

@block("part")
type Part {
  @inline(0) id: identifier
  name: utf8
  kind: utf8?
  system: identifier?
  zone: identifier?
  tags: list<identifier>
  @children("wendpoint") endpoints: list<WEndpoint>
  @child("body") body: Prose?
}

@block("host")
type Host { @inline(0) id: identifier  name: utf8  parent: identifier? }

@block("link")
type Link { @inline(0) id: identifier  source: identifier  destination: identifier  kind: utf8? }

@block("topic")
type Topic { @inline(0) id: identifier  name: utf8 }

@document
type D {
  @children("zone")   zones:   list<Zone>
  @children("system") systems: list<System>
  @children("part")   parts:   list<Part>
  @children("host")   hosts:   list<Host>
  @children("link")   links:   list<Link>
  @children("topic")  topics:  list<Topic>
}
"#;

    fn open(data: &str) -> Document {
        Document::open(&format!("{SCHEMA}\n{data}"), "test.wcl").expect("parse")
    }

    fn json_of(doc: &Document, kind: &str) -> serde_json::Value {
        KindModel::new(doc)
            .get(kind)
            .unwrap_or_else(|| panic!("no kind {kind}"))
            .json()
    }

    #[test]
    fn a_field_named_after_a_gathered_kind_is_a_parent_link() {
        let doc = open("");
        let system = json_of(&doc, "system");
        assert_eq!(
            system["parents"],
            serde_json::json!([{ "field": "zone", "kind": "zone" }])
        );
        // `system.repo` names nothing gathered → a plain reference.
        assert_eq!(
            system["refs"],
            serde_json::json!([{ "field": "repo", "list": false }])
        );
    }

    #[test]
    fn parent_candidates_keep_declaration_order() {
        let doc = open("");
        assert_eq!(
            json_of(&doc, "part")["parents"],
            serde_json::json!([
                { "field": "system", "kind": "system" },
                { "field": "zone", "kind": "zone" },
            ])
        );
    }

    #[test]
    fn a_parent_field_self_nests() {
        let doc = open("");
        assert_eq!(
            json_of(&doc, "host")["parents"],
            serde_json::json!([{ "field": "parent", "kind": "host" }])
        );
    }

    #[test]
    fn the_inline_id_is_never_a_parent_or_a_reference() {
        let doc = open("");
        let zone = json_of(&doc, "zone");
        assert_eq!(zone["parents"], serde_json::json!([]));
        assert_eq!(zone["refs"], serde_json::json!([]));
        assert_eq!(zone["id_field"], "id");
    }

    #[test]
    fn list_identifier_fields_are_references() {
        let doc = open("");
        assert_eq!(
            json_of(&doc, "part")["refs"],
            serde_json::json!([{ "field": "tags", "list": true }])
        );
    }

    #[test]
    fn a_source_and_destination_kind_is_an_edge() {
        let doc = open("");
        let link = json_of(&doc, "link");
        assert_eq!(
            link["edge"],
            serde_json::json!({ "source": "source", "destination": "destination" })
        );
        // An edge's endpoints are wiring, not containment or references.
        assert_eq!(link["parents"], serde_json::json!([]));
        assert_eq!(link["refs"], serde_json::json!([]));
        assert!(json_of(&doc, "part")["edge"].is_null());
    }

    #[test]
    fn body_and_child_families_come_off_the_schema() {
        let doc = open("");
        let part = json_of(&doc, "part");
        assert_eq!(part["has_body"], true);
        assert_eq!(part["accepts_children"], true);
        assert_eq!(
            part["child_families"],
            serde_json::json!([
                { "field": "endpoints", "kind": "wendpoint", "many": true, "doc": null },
                { "field": "body", "kind": "body", "many": false, "doc": null },
            ])
        );
        assert_eq!(json_of(&doc, "zone")["has_body"], false);
        assert_eq!(json_of(&doc, "zone")["accepts_children"], false);
    }

    /// A nested kind's schema resolves through the declaring FIELD's type,
    /// not by looking the kind name up. That is what keeps a kind name
    /// shared across schemas (a WAD `container` vs wdoc's diagram-grouping
    /// shape) resolving to the type the field actually names — a name
    /// lookup answers whichever happens to be declared first.
    #[test]
    fn a_child_family_schema_resolves_through_the_field_not_the_name() {
        let doc = Document::open(
            "@block(\"cell\")\n\
             type OtherCell { @inline(0) id: identifier  wrong: utf8? }\n\
             @block(\"cell\")\n\
             type OwnCell { @inline(0) id: identifier  right: utf8? }\n\
             @block(\"grid\")\n\
             type Grid { @inline(0) id: identifier  @children(\"cell\") cells: list<OwnCell> }\n\
             @document\n\
             type D { @children(\"grid\") grids: list<Grid> }\n",
            "test.wcl",
        )
        .expect("parse");
        let model = KindModel::new(&doc);
        let grid = model.get("grid").expect("grid");
        let families = grid.child_families();
        let cells = families
            .iter()
            .find(|f| f.kind() == "cell")
            .expect("cell family");
        assert_eq!(cells.schema().map(DeclName::name), Some("OwnCell"));
        // The name lookup is the ambiguous one this exists to avoid.
        assert_eq!(
            doc.block_schema("cell").map(|d| d.name()),
            Some("OtherCell")
        );
    }

    /// `type_name` is the FULLY-QUALIFIED schema name everywhere — the
    /// value the create path matches on, and the one a bare kind name
    /// cannot stand in for across namespaces.
    #[test]
    fn every_entry_carries_the_qualified_schema_name() {
        let doc = open("");
        let model = KindModel::new(&doc);
        let part = model.get("part").expect("part");
        assert_eq!(part.type_name(), "Part");
        assert_eq!(part.json()["type_name"], "Part");
    }

    #[test]
    fn the_palette_denylist_hides_wskill_plumbing() {
        let doc = open("");
        let model = KindModel::new(&doc);
        assert!(model.get("topic").is_some(), "still a gathered kind");
        assert!(
            !model.unit_kind_names().contains(&"topic".to_string()),
            "but not an addable unit"
        );
    }

    #[test]
    fn a_repeated_free_text_value_becomes_a_suggestion() {
        let doc = open(
            "part a { name = \"A\"  kind = \"handler\" }\n\
             part b { name = \"B\"  kind = \"handler\" }\n\
             part c { name = \"C\"  kind = \"store\" }\n",
        );
        let model = KindModel::new(&doc);
        let v = model.json_with_suggestions(model.get("part").expect("part"));
        assert_eq!(
            v["suggestions"]["kind"],
            serde_json::json!(["handler", "store"])
        );
        // Every `name` is distinct — a vocabulary it is not.
        assert!(v["suggestions"].get("name").is_none(), "{v:#}");
        // Identifier-typed and inline fields are never suggested.
        assert!(v["suggestions"].get("id").is_none());
        assert!(v["suggestions"].get("system").is_none());
    }

    /// Repetition alone isn't a vocabulary: past [`MAX_SUGGESTIONS`]
    /// distinct values the field is free text with a long tail, and a
    /// picker of that many options is worse than typing.
    #[test]
    fn too_many_distinct_values_suppress_the_suggestion() {
        // Every value used twice, so only the distinct count decides.
        let instances = |distinct: usize| {
            (0..distinct)
                .map(|i| {
                    format!(
                        "part a{i} {{ name = \"A{i}\"  kind = \"k{i}\" }}\n\
                         part b{i} {{ name = \"B{i}\"  kind = \"k{i}\" }}\n"
                    )
                })
                .collect::<String>()
        };
        let suggestions_for = |data: &str| {
            let doc = open(data);
            let model = KindModel::new(&doc);
            model.json_with_suggestions(model.get("part").expect("part"))
        };

        let at_the_limit = suggestions_for(&instances(MAX_SUGGESTIONS));
        assert_eq!(
            at_the_limit["suggestions"]["kind"].as_array().map(Vec::len),
            Some(MAX_SUGGESTIONS),
            "{at_the_limit:#}"
        );

        let past_it = suggestions_for(&instances(MAX_SUGGESTIONS + 1));
        assert!(past_it["suggestions"].get("kind").is_none(), "{past_it:#}");
    }

    #[test]
    fn suggestions_are_opt_in() {
        let doc = open("part a { name = \"A\"  kind = \"handler\" }\n");
        assert!(json_of(&doc, "part")["suggestions"].is_null());
    }

    /// What a field IS decides how it reads, not how it prints. Declared
    /// through an alias (`type NodeId = identifier`), a parent link is
    /// still a parent link, a list of them still a reference, and the
    /// inline slot still the unit-id convention — every one of which a
    /// comparison against the printed type drops without a word.
    #[test]
    fn an_aliased_identifier_reads_as_an_identifier() {
        let doc = Document::open(
            "type NodeId = identifier\n\
             @block(\"zone\")\n\
             type Zone { @inline(0) id: NodeId  name: utf8 }\n\
             @block(\"system\")\n\
             type System { @inline(0) id: NodeId  zone: NodeId?  tags: list<NodeId> }\n\
             @document\n\
             type D { @children(\"zone\") zones: list<Zone>  @children(\"system\") systems: list<System> }\n",
            "test.wcl",
        )
        .expect("parse");
        let model = KindModel::new(&doc);
        let system = model.get("system").expect("system");
        assert!(system.id_is_identifier(), "the inline slot is an id slot");
        let json = system.json();
        assert_eq!(
            json["parents"],
            serde_json::json!([{ "field": "zone", "kind": "zone" }])
        );
        assert_eq!(
            json["refs"],
            serde_json::json!([{ "field": "tags", "list": true }])
        );
    }

    /// Only a function-VALUED field is dropped from the form fields. A
    /// field typed by a declaration whose name merely begins with `fn` is
    /// an ordinary property.
    #[test]
    fn a_type_named_like_the_fn_keyword_is_not_a_function() {
        let doc = Document::open(
            "type fnord { label: utf8 }\n\
             @block(\"widget\")\n\
             type Widget { @inline(0) id: identifier  boxed: fnord?  lower: fn(utf8) -> utf8 }\n\
             @document\n\
             type D { @children(\"widget\") widgets: list<Widget> }\n",
            "test.wcl",
        )
        .expect("parse");
        let names: Vec<String> = json_of(&doc, "widget")["fields"]
            .as_array()
            .expect("fields")
            .iter()
            .map(|f| f["name"].as_str().unwrap_or_default().to_string())
            .collect();
        assert!(names.contains(&"boxed".to_string()), "{names:?}");
        assert!(!names.contains(&"lower".to_string()), "{names:?}");
    }

    /// Reclassifying a field is a change to the model, and the model is
    /// what every surface serves — so the change SHOWS. Retyped from
    /// `identifier` to free text, `system.zone` stops being containment
    /// and becomes an ordinary property with a suggestion picker.
    #[test]
    fn reclassifying_a_field_moves_it_in_the_model() {
        let schema = |ty: &str| {
            format!(
                "@block(\"zone\")\n\
                 type Zone {{ @inline(0) id: identifier  name: utf8 }}\n\
                 @block(\"system\")\n\
                 type System {{ @inline(0) id: identifier  zone: {ty} }}\n\
                 @document\n\
                 type D {{ @children(\"zone\") zones: list<Zone>  @children(\"system\") systems: list<System> }}\n\
                 system a {{ zone = {value} }}\n\
                 system b {{ zone = {value} }}\n",
                value = if ty.starts_with("identifier") {
                    "core"
                } else {
                    "\"core\""
                },
            )
        };
        let linked = Document::open(&schema("identifier?"), "test.wcl").expect("parse");
        let model = KindModel::new(&linked);
        let system = model.get("system").expect("system");
        assert_eq!(
            system.json()["parents"],
            serde_json::json!([{ "field": "zone", "kind": "zone" }])
        );
        assert!(model.json_with_suggestions(system)["suggestions"]["zone"].is_null());

        let text = Document::open(&schema("utf8?"), "test.wcl").expect("parse");
        let model = KindModel::new(&text);
        let system = model.get("system").expect("system");
        assert_eq!(system.json()["parents"], serde_json::json!([]));
        assert_eq!(
            model.json_with_suggestions(system)["suggestions"]["zone"],
            serde_json::json!(["core"])
        );
    }

    #[test]
    fn describe_prefers_the_qualified_name_over_the_kind_name() {
        let doc = Document::open(
            "namespace app\n@block(\"widget\")\ntype Widget { @inline(0) id: identifier  size: utf8? }\n\
             @document\ntype D { @children(\"widget\") widgets: list<Widget> }\n",
            "test.wcl",
        )
        .expect("parse");
        let model = KindModel::new(&doc);
        let k = model
            .describe("widget", Some("app.Widget"))
            .expect("widget");
        assert_eq!(k.type_name(), "app.Widget");
        // An unknown qualified name falls back to the kind lookup.
        let k = model
            .describe("widget", Some("nope.Widget"))
            .expect("widget");
        assert_eq!(k.type_name(), "app.Widget");
    }
}
