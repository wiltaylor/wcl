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
//! Every endpoint that describes a kind serves [`Kind::json`], so the
//! client has one reader. The only member derived from instance data is
//! `suggestions` — it walks every block of the kind and forces field
//! evaluation, so it is opt-in ([`Kind::json_with_suggestions`]): the
//! Systems view asks, the palette does not.
//!
//! `GET /api/palette` lives here too: it is kind introspection with a
//! curated body-block list bolted on.

use std::sync::Arc;

use axum::extract::State;
use axum::http::Uri;
use axum::response::Response;

use wcl_lang::{DeclName, Document, ResolvedType, TypeDecl, TypeField};

use super::blocks::{dec_first_string, field_string, first_label, value_string};
use super::{EditorState, Workspace, run_blocking};
use crate::serve::query_param;

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

/// One kind's derived structure — its schema, how instances of it nest,
/// what they reference, and whether the kind is an edge rather than a node.
pub(super) struct Kind<'a> {
    kind: String,
    schema: TypeDecl<'a>,
    /// `(field name, parent kind)` in declaration order.
    parents: Vec<(String, String)>,
    /// `(field name, is a list)` — cross-references, not containment.
    refs: Vec<(String, bool)>,
    /// `(source field, destination field)` when the kind is an edge kind.
    edge: Option<(String, String)>,
}

impl<'a> Kind<'a> {
    pub(super) fn kind(&self) -> &str {
        &self.kind
    }

    pub(super) fn schema(&self) -> &TypeDecl<'a> {
        &self.schema
    }

    /// The fully-qualified schema name — the value every endpoint echoes as
    /// `type_name`, and the create path matches on. A bare kind name is
    /// ambiguous across namespaces (a WAD `container` vs wdoc's
    /// diagram-grouping shape); this is not.
    pub(super) fn type_name(&self) -> String {
        self.schema.full_name()
    }

    /// `(field name, parent kind)` in declaration order — an instance nests
    /// under the first of these it actually sets.
    pub(super) fn parents(&self) -> &[(String, String)] {
        &self.parents
    }

    /// `(source field, destination field)` when the kind wires two objects
    /// together rather than being one.
    pub(super) fn edge(&self) -> Option<&(String, String)> {
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
            .map(|f| f.type_ref().to_string() == "identifier")
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
            let ty = f.type_ref();
            // Function-valued fields (an SvgBlock's `lower`, computed
            // hooks) aren't form-editable properties.
            if ty.to_string().starts_with("fn") {
                continue;
            }
            // Resolved in the field's OWN namespace — a `wcl.wad` field
            // typed `ContainerKind` must not pick up a same-named set
            // elsewhere.
            let symbols: Option<Vec<String>> = match f.resolved_type() {
                ResolvedType::SymbolSet(ss) => {
                    Some(ss.symbols().map(|s| s.name().to_string()).collect())
                }
                _ => None,
            };
            fields.push(serde_json::json!({
                "name": f.name(),
                "type": ty.to_string(),
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
                .map(|(field, kind)| serde_json::json!({ "field": field, "kind": kind }))
                .collect::<Vec<_>>(),
            "refs": self
                .refs
                .iter()
                .map(|(field, list)| serde_json::json!({ "field": field, "list": list }))
                .collect::<Vec<_>>(),
            "edge": match &self.edge {
                Some((s, d)) => serde_json::json!({ "source": s, "destination": d }),
                None => serde_json::Value::Null,
            },
            "id_field": self.id_field_decl().map(|f| f.name().to_string()),
            "suggestions": serde_json::Value::Null,
        })
    }

    /// [`Kind::json`] with the values already in use for each free-text
    /// field mined from `doc`'s instances. This forces evaluation of every
    /// block of the kind, so only ask where a form wants the picker.
    pub(super) fn json_with_suggestions(&self, doc: &Document) -> serde_json::Value {
        let mut v = self.json();
        v["suggestions"] = self.suggestions(doc);
        v
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
            if f.inline_slot().is_some() || !is_scalar(&f) {
                continue;
            }
            let ty = bare_type(&f);
            if ty != "utf8" && ty != "ascii" {
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
        // wdoc's own document gathers (pages, sites, components, …) are
        // infrastructure, not data objects a user models; they're excluded
        // by declaring namespace so a user kind sharing a name is kept.
        let gathered: Vec<(String, TypeDecl<'a>)> = doc
            .gathered_kinds()
            .into_iter()
            .filter(|g| !g.schema().full_name().starts_with("wdoc."))
            .map(|g| (g.kind().to_string(), g.into_schema()))
            .collect();
        let names: Vec<String> = gathered.iter().map(|(k, _)| k.clone()).collect();
        let kinds = gathered
            .into_iter()
            .map(|(kind, schema)| derive(kind, schema, &names))
            .collect();
        Self { doc, kinds }
    }

    pub(super) fn document(&self) -> &'a Document {
        self.doc
    }

    /// Every gathered data-object kind, in declaration order.
    pub(super) fn kinds(&self) -> &[Kind<'a>] {
        &self.kinds
    }

    pub(super) fn get(&self, kind: &str) -> Option<&Kind<'a>> {
        self.kinds.iter().find(|k| k.kind == kind)
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
        let names: Vec<String> = self.kinds.iter().map(|k| k.kind.clone()).collect();
        if let Some(full) = type_name.filter(|s| !s.is_empty()) {
            if let Some(k) = self.kinds.iter().find(|k| k.type_name() == full) {
                return Some(derive(k.kind.clone(), k.schema, &names));
            }
            if let Some(decl) = self.doc.type_decls().find(|d| d.full_name() == full) {
                return Some(derive(kind.to_string(), decl, &names));
            }
        }
        if let Some(k) = self.get(kind) {
            return Some(derive(k.kind.clone(), k.schema, &names));
        }
        let decl = self.doc.block_schema(kind)?;
        Some(derive(kind.to_string(), decl, &names))
    }

    /// Describe a nested block kind reached through a `@child`/`@children`
    /// family, whose schema resolves through the declaring field's own type
    /// (namespace-correct where a kind-name lookup is not).
    pub(super) fn describe_decl(&self, kind: &str, schema: TypeDecl<'a>) -> Kind<'a> {
        let names: Vec<String> = self.kinds.iter().map(|k| k.kind.clone()).collect();
        derive(kind.to_string(), schema, &names)
    }

    /// The addable diagram shape kinds: every `@block("kind")` type
    /// descending from `wdoc.SvgBlock`. The client curates which surface in
    /// the add-shape palette; the full list is served so any selected shape
    /// — including user-declared ones — gets a schema-driven form.
    pub(super) fn diagram_kinds(&self) -> Vec<serde_json::Value> {
        let names: Vec<String> = self.kinds.iter().map(|k| k.kind.clone()).collect();
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

    /// Whether the document carries the wskill data model (a gathered
    /// `topic`).
    pub(super) fn is_wskill(&self) -> bool {
        self.doc.blocks().any(|b| b.kind() == "topic")
    }

    /// Whether the document carries the WAD data model (its one `wad` root
    /// metadata block) — the flag that opens the Systems view.
    pub(super) fn is_wad(&self) -> bool {
        self.doc.blocks().any(|b| b.kind() == "wad")
    }

    /// `book` / `website` / `presentation`, from the selected `site`
    /// block's nav-declaring child (`toc` / `menu` / `deck`).
    pub(super) fn site_kind(&self, site: Option<&str>) -> &'static str {
        let block = self.doc.blocks().find(|b| {
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
}

/// Read one kind off its schema, against the gathered kind `names` that
/// make a field name mean containment.
fn derive<'a>(kind: String, schema: TypeDecl<'a>, names: &[String]) -> Kind<'a> {
    let mut parents: Vec<(String, String)> = Vec::new();
    let mut refs: Vec<(String, bool)> = Vec::new();
    let mut source = None;
    let mut destination = None;
    for f in schema.effective_fields() {
        if !is_scalar(&f) {
            continue;
        }
        let name = f.name().to_string();
        let ty = bare_type(&f);
        if ty == "identifier" {
            if name == "source" {
                source = Some(name.clone());
            } else if name == "destination" {
                destination = Some(name.clone());
            }
            if f.inline_slot().is_some() {
                continue;
            }
            if name == "parent" {
                parents.push((name, kind.clone()));
            } else if names.contains(&name) {
                parents.push((name.clone(), name));
            } else {
                refs.push((name, false));
            }
        } else if ty == "list<identifier>" {
            refs.push((name, true));
        }
    }
    let edge = match (source, destination) {
        (Some(s), Some(d)) => Some((s, d)),
        _ => None,
    };
    if let Some((s, d)) = &edge {
        parents.retain(|(f, _)| f != s && f != d);
        refs.retain(|(f, _)| f != s && f != d);
    }
    Kind {
        kind,
        schema,
        parents,
        refs,
        edge,
    }
}

/// A field's declared type with a trailing `?` stripped.
fn bare_type(f: &TypeField<'_>) -> String {
    let ty = f.type_ref().to_string();
    ty.strip_suffix('?').unwrap_or(&ty).to_string()
}

/// Scalar fields only: child blocks, child-block lists and connections are
/// structure, not properties.
fn is_scalar(f: &TypeField<'_>) -> bool {
    f.child_kind_or_union().is_none()
        && f.children_kind_or_union().is_none()
        && f.connection_schema().is_none()
}

// ---------------------------------------------------------------------------
// `GET /api/palette` — what the add-block UI can insert here
// ---------------------------------------------------------------------------

/// The curated body-block palette: `(kind, label, canonical snippet)`.
/// Static because most of these render via Rust fundamentals — there is no
/// WCL schema rich enough to introspect an insertion template from.
const BODY_KINDS: &[(&str, &str, &str)] = &[
    ("p", "Paragraph", "p \"New paragraph\""),
    ("h2", "Heading", "h2 \"New heading\""),
    ("h3", "Subheading", "h3 \"New subheading\""),
    (
        "code",
        "Code block",
        "code \"text\" {\n  source = <<'SRC'\n\nSRC\n}",
    ),
    (
        "callout",
        "Callout",
        "callout \"Note\" {\n  body = \"Callout text\"\n}",
    ),
    ("list", "List", "list {\n  li \"First item\"\n}"),
    (
        "table",
        "Table",
        "table {\n  rows:\n    | \"Column\" | \"Column\" |\n    | \"\" | \"\" |\n}",
    ),
    ("image", "Image", "image \"\" {\n  alt = \"\"\n}"),
];

/// Query: `entry`, `site?`, `page_file?` → `{ site_type, wskill, wad,
/// unit_kinds, diagram_kinds, body_kinds, components }`. Unit and diagram
/// kinds come from the kind model (the generated create and property forms
/// are built from their `fields`); body kinds are the curated wdoc content
/// blocks with canonical insertion snippets; components are the
/// `wdoc_component` declarations authored inside the served tree, with
/// their slots.
///
/// Nothing here mines suggestions: the palette must open promptly on a
/// large model, and suggestions are the one member that evaluates every
/// instance.
pub(super) async fn handle_palette(State(state): State<Arc<EditorState>>, uri: Uri) -> Response {
    let entry = query_param(&uri, "entry");
    let site = query_param(&uri, "site");
    let page_file = query_param(&uri, "page_file");
    let state2 = Arc::clone(&state);
    run_blocking(move || {
        let entry = entry.ok_or("missing entry")?;
        palette(&state2.ws, &entry, site.as_deref(), page_file.as_deref())
    })
    .await
}

fn palette(
    ws: &Workspace,
    entry: &str,
    site: Option<&str>,
    page_file: Option<&str>,
) -> Result<serde_json::Value, String> {
    let doc_entry = ws.doc_entry(entry, page_file)?;
    let doc = wcl_wdoc::open_doc_for_edit(&doc_entry).map_err(super::err_str)?;
    let model = KindModel::new(&doc);

    let body_kinds: Vec<serde_json::Value> = BODY_KINDS
        .iter()
        .map(|(kind, label, snippet)| {
            serde_json::json!({ "kind": kind, "label": label, "template_source": snippet })
        })
        .collect();

    Ok(serde_json::json!({
        "ok": true,
        "site_type": model.site_kind(site),
        "wskill": model.is_wskill(),
        "wad": model.is_wad(),
        "unit_kinds": model.unit_kinds().map(Kind::json).collect::<Vec<_>>(),
        "diagram_kinds": model.diagram_kinds(),
        "body_kinds": body_kinds,
        "components": components(ws, &doc),
    }))
}

/// `wdoc_component` declarations authored inside the served tree (stdlib
/// components are excluded — their sources live outside the root), with the
/// slot list that drives the property form.
fn components(ws: &Workspace, doc: &Document) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for (path, block) in doc.blocks_with_source() {
        if block.kind() != "wdoc_component" {
            continue;
        }
        if let Some(p) = path
            && !p.starts_with(ws.root_dir())
        {
            continue;
        }
        let Some(name) = first_label(&block) else {
            continue;
        };
        let slots: Vec<serde_json::Value> = block
            .blocks()
            .filter(|b| b.kind() == "wdoc_slot")
            .map(|slot| {
                let default = slot
                    .field("default")
                    .and_then(|f| f.value().ok().cloned())
                    .as_ref()
                    .map(value_string);
                let required = default.is_none();
                serde_json::json!({
                    "name": first_label(&slot),
                    "default": default,
                    "required": required,
                })
            })
            .collect();
        let file = path.and_then(|p| ws.rel(p).ok());
        out.push(serde_json::json!({ "name": name, "file": file, "slots": slots }));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::testsupport::{OBJECT_DOC, workspace_with};

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
    /// so a family names its own type even when a same-named `@block` kind
    /// is declared elsewhere.
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
        let families = grid.schema().child_families();
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
        let v = model.get("part").expect("part").json_with_suggestions(&doc);
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

    #[test]
    fn suggestions_are_opt_in() {
        let doc = open("part a { name = \"A\"  kind = \"handler\" }\n");
        assert!(json_of(&doc, "part")["suggestions"].is_null());
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

    #[test]
    fn palette_lists_kinds_and_components() {
        let doc = format!(
            "{OBJECT_DOC}\nwdoc_component metric_card {{\n  wdoc_slot label\n  wdoc_slot status {{\n    default = \"ok\"\n  }}\n  wdoc_body {{\n    p $\"${{label}}\"\n  }}\n}}\n"
        );
        let (_td, ws) = workspace_with(&doc);

        let v = palette(&ws, "main.wcl", Some("docs"), None).expect("palette");
        assert_eq!(v["site_type"], "book");
        assert_eq!(v["wskill"], false);
        // The user schema kind, with introspected fields.
        let kinds = v["unit_kinds"].as_array().unwrap();
        let thing = kinds
            .iter()
            .find(|k| k["kind"] == "thing")
            .unwrap_or_else(|| panic!("no thing kind: {v:#}"));
        let fields = thing["fields"].as_array().unwrap();
        let name = fields.iter().find(|f| f["name"] == "name").unwrap();
        assert_eq!(name["inline_slot"], 0);
        assert_eq!(name["optional"], false);
        let note = fields.iter().find(|f| f["name"] == "note").unwrap();
        assert_eq!(note["optional"], true);
        // wdoc's own document gathers (site, page, …) are not offered.
        assert!(
            !kinds
                .iter()
                .any(|k| k["kind"] == "site" || k["kind"] == "page"),
            "{v:#}"
        );
        // Curated body kinds carry insertion snippets.
        let body = v["body_kinds"].as_array().unwrap();
        assert!(body.iter().any(|k| k["kind"] == "p"));
        assert!(
            body.iter()
                .all(|k| k["template_source"].as_str().is_some_and(|s| !s.is_empty()))
        );
        // Diagram shape kinds: SvgBlock descendants with introspected fields.
        let shapes = v["diagram_kinds"].as_array().unwrap();
        let process = shapes
            .iter()
            .find(|k| k["kind"] == "process")
            .unwrap_or_else(|| panic!("no process shape kind: {v:#}"));
        let pf = process["fields"].as_array().unwrap();
        for want in ["x", "y", "width", "height"] {
            assert!(pf.iter().any(|f| f["name"] == want), "{v:#}");
        }
        assert!(shapes.iter().any(|k| k["kind"] == "rect"), "{v:#}");
        // Page-level HTML blocks don't extend SvgBlock.
        assert!(!shapes.iter().any(|k| k["kind"] == "diagram"), "{v:#}");
        // The authored component with its slot contract.
        let comps = v["components"].as_array().unwrap();
        let card = comps.iter().find(|c| c["name"] == "metric_card").unwrap();
        let slots = card["slots"].as_array().unwrap();
        assert_eq!(slots.len(), 2, "{v:#}");
        let label = slots.iter().find(|s| s["name"] == "label").unwrap();
        assert_eq!(label["required"], true);
        let status_slot = slots.iter().find(|s| s["name"] == "status").unwrap();
        assert_eq!(status_slot["required"], false);
        assert_eq!(status_slot["default"], "ok");
    }
}
