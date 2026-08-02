//! The typed model: what a wskill folder *is*, as data.
//!
//! Everything here is owned — no borrow of the [`Document`](wcl_lang::Document)
//! it was read from — because the two consumers outlive the read: the editor
//! answers one request from it, and the curator holds two of them (this
//! revision and a baseline) at once.
//!
//! Two properties are deliberate rather than incidental:
//!
//! - **Every node carries its [`Anchor`]** — the declaring file and byte span.
//!   They are free (the load is already walking the AST) and a span-free
//!   "pure semantic graph" would force the editor to re-parse to recover
//!   them. The curator ignores them; ops stay id-addressed.
//! - **`related_editable` is model-side**, not editor-side: a `related` list
//!   written as a computed expression can be *read* but must never be
//!   rewritten, and that is the difference between "I can write here" and
//!   "I must file a comment instead" — which the curator needs as much as
//!   the editing UI does.
//!
//! Layout is not here. A graph view lays the nodes out (`wcl_wdoc::layout_graph`);
//! the model says what the nodes and edges are.

use std::fmt;
use std::path::PathBuf;

use wcl_lang::Span;

/// Where a block is written: the declaring file, relative to the graph's
/// [`root`](Graph::root), and its byte span within that file.
///
/// Relative-to-root (rather than absolute) is what makes two graphs
/// comparable: a graph loaded at a git revision was read from a scratch
/// tree that no longer exists by the time anything looks at it.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Anchor {
    pub file: PathBuf,
    pub span: Span,
}

/// A node's identity in the graph: its kind and its id (`concept:alpha`).
/// Ids are unique per wskill in practice, but the kind is what turns an id
/// into something an op can address without a lookup.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(into = "String")]
pub struct NodeKey {
    pub kind: String,
    pub id: String,
}

impl fmt::Display for NodeKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.kind, self.id)
    }
}

impl From<NodeKey> for String {
    fn from(k: NodeKey) -> String {
        k.to_string()
    }
}

/// Serialize a [`NodeKey`] as `{kind, id}` rather than the `"kind:id"`
/// display form the graph's edges use.
///
/// Where a node is the *subject* of a record — a finding, an audit row —
/// a consumer routing on the kind should not have to split a string, and
/// there is one spelling of that shape because there is one function.
pub(crate) fn node_key_fields<S: serde::Serializer>(
    key: &NodeKey,
    s: S,
) -> Result<S::Ok, S::Error> {
    use serde::ser::SerializeStruct;
    let mut st = s.serialize_struct("NodeKey", 2)?;
    st.serialize_field("kind", &key.kind)?;
    st.serialize_field("id", &key.id)?;
    st.end()
}

/// A block's declared `@except(sites = […])` visibility, as far as it can be
/// read mechanically.
///
/// `custom` means the block carries visibility this reading cannot express
/// (`@only`, a positional `@except`, a non-literal site list) — a writer must
/// leave it alone and send the author to the source.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Visibility {
    pub except_sites: Vec<String>,
    pub custom: bool,
}

impl Visibility {
    /// Whether a block with this visibility renders in the named site.
    /// Custom visibility reports visible: the model declines to guess, and
    /// the `custom` flag is what a caller keys off.
    pub fn shows_in(&self, site: &str) -> bool {
        !self.except_sites.iter().any(|s| s == site)
    }
}

/// One projection of the wskill — an `artifact` block in the registry, paired
/// with the site name its entry document declares.
///
/// `kind` stays a plain string: the artifact-kind vocabulary is topic-owned
/// (`schema/kinds.wcl` is hand-editable), so a wskill may declare kinds this
/// crate has never heard of. The routing rules below name only the two
/// (`book`, `ai_skill`) that reference content is shared between.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct View {
    /// The `artifact` block's id.
    pub id: String,
    /// The artifact's `kind` symbol (`book` / `ai_skill` / `presentation` /
    /// `training` / a topic-declared extension).
    pub kind: String,
    /// The projection entry document, relative to the wskill root.
    pub entry: String,
    /// The `site` name that entry declares — what a `@except(sites = […])`
    /// decorator names. `None` when the entry declares no named site.
    pub site: Option<String>,
}

impl View {
    /// The site name visibility decorators use, falling back to the artifact
    /// id for an entry with no named `site` block.
    pub fn site_name(&self) -> &str {
        self.site.as_deref().unwrap_or(&self.id)
    }
}

/// The wskill's `topic` block — the one-per-folder subject.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Topic {
    pub id: String,
    pub name: String,
    pub summary: Option<String>,
    pub anchor: Anchor,
}

/// One content block inside a unit's body: what the block is, a short
/// preview of it, and where it is written.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ContentBlock {
    pub kind: String,
    /// The block's first label — enough to recognise it in a list, never the
    /// rendered content. Whole: how much of it a list can show is the
    /// reader's business, not the model's.
    pub preview: String,
    pub anchor: Anchor,
    pub visibility: Visibility,
}

/// One `related` link: the id it names, and the author's reason for it.
///
/// The reason is `None` for the bare `related = [other]` form the corpus is
/// written in today, and `Some` for the `{id, why}` record form. Reading
/// both is what lets the reason-shaped screens
/// ([`Rule::DuplicateReason`](crate::lint::Rule::DuplicateReason),
/// [`Rule::MirroredPin`](crate::lint::Rule::MirroredPin)) be one rule rather
/// than two, before and after the format carries reasons.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct Link {
    pub id: String,
    /// Absent rather than null in the serialized form: the corpus is 725
    /// bare edges, and a reader asking "is there a reason?" should not have
    /// to distinguish two spellings of no.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub why: Option<String>,
}

impl Link {
    /// A bare link — the `related = [other]` form.
    pub fn bare(id: impl Into<String>) -> Link {
        Link {
            id: id.into(),
            why: None,
        }
    }
}

/// A reference unit: a `concept` / `fact` / `procedure` / `lesson` / … block.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Unit {
    pub id: String,
    pub kind: String,
    pub title: String,
    /// The unit's one-line description (`summary`), when it declares one.
    pub summary: Option<String>,
    /// Every literal string the block's subtree carries, newline-joined —
    /// the unit's prose as a searchable blob. See [`Index::text`] for why it
    /// is only the content.
    pub text: String,
    /// The `audience` routing symbol (`book` / `ai` / `both`) — the block's
    /// own field, else its schema's declared default.
    pub audience: String,
    pub anchor: Anchor,
    pub visibility: Visibility,
    /// The links this unit declares (its `related` list, in authored order).
    pub related: Vec<Link>,
    /// Whether `related` may be rewritten: false when the field is a
    /// computed expression, which a pin/unpin/reorder write must not clobber.
    pub related_editable: bool,
    /// The unit's body blocks, one level deep.
    pub blocks: Vec<ContentBlock>,
    /// How many words of prose the body carries — every literal string in
    /// the block subtree, whitespace-split. The denominator of the
    /// words-per-link screen, and the only body measurement the model
    /// takes: a curator that wants the prose itself reads the file.
    pub words: usize,
}

impl Unit {
    pub fn key(&self) -> NodeKey {
        NodeKey {
            kind: self.kind.clone(),
            id: self.id.clone(),
        }
    }

    /// The ids this unit links to, in authored order.
    pub fn related_ids(&self) -> impl Iterator<Item = &str> {
        self.related.iter().map(|l| l.id.as_str())
    }

    /// Whether this unit renders in `view`: its declared visibility AND the
    /// wskill audience routing (see [`routes_to`]).
    pub fn shows_in(&self, view: &View) -> bool {
        self.visibility.shows_in(view.site_name())
            && routes_to(&self.kind, &self.audience, &view.kind)
    }
}

/// A curated index: an ordered list of pinned unit ids, plus nested
/// sub-indexes. Sub-indexes are part of their top-level index's tree, not
/// nodes of their own — their pins ride the top-level index's edges with an
/// `index_id` attribution.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Index {
    pub id: String,
    pub title: String,
    /// The index's one-line description (`summary`), when it declares one.
    pub summary: Option<String>,
    /// Every literal string the block's subtree carries, newline-joined: the
    /// index's own prose, as a blob a search can match against.
    ///
    /// Deliberately only the *content* — field names, block kinds,
    /// identifiers and symbols stay out, or searching "related" would hit
    /// every block that declares the field rather than the one whose prose
    /// says the word. Strings a *computed* field would produce are likewise
    /// absent: this reads the source, not an evaluation.
    pub text: String,
    pub audience: String,
    pub anchor: Anchor,
    pub visibility: Visibility,
    /// The pinned unit ids, in authored order.
    pub pinned: Vec<String>,
    /// Whether the pin list may be rewritten (see [`Unit::related_editable`]).
    pub related_editable: bool,
    /// The index's own body blocks — its sub-indexes excluded, since those
    /// are structure, not content.
    pub blocks: Vec<ContentBlock>,
    pub children: Vec<Index>,
}

impl Index {
    pub fn key(&self) -> NodeKey {
        NodeKey {
            kind: "index".to_string(),
            id: self.id.clone(),
        }
    }

    /// Whether this index carries a body — which is what makes it a
    /// linkable node rather than a pure nav heading: a body-less index has
    /// no page, so a `related` id naming one resolves to nothing.
    pub fn has_body(&self) -> bool {
        !self.blocks.is_empty()
    }

    /// This index and every sub-index below it, outermost first.
    pub fn levels(&self) -> Vec<&Index> {
        let mut out = vec![self];
        for c in &self.children {
            out.extend(c.levels());
        }
        out
    }

    pub fn shows_in(&self, view: &View) -> bool {
        self.visibility.shows_in(view.site_name()) && routes_to("index", &self.audience, &view.kind)
    }
}

/// Why two nodes are joined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EdgeKind {
    /// A unit's own `related` link.
    Related,
    /// An index pinning a unit.
    Pin,
}

impl EdgeKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            EdgeKind::Related => "related",
            EdgeKind::Pin => "pin",
        }
    }
}

impl fmt::Display for EdgeKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct Edge {
    pub from: NodeKey,
    pub to: NodeKey,
    pub kind: EdgeKind,
    /// For a pin: the index level whose `related` list holds it — the
    /// sub-index itself for a nested pin, which is what a write must target.
    pub index_id: Option<String>,
}

/// One part of a training course: its id, title, and lesson ids in `n` order.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CourseModule {
    pub id: String,
    pub title: String,
    pub lessons: Vec<String>,
}

/// A training course's structure. A course has no `index` blocks — its
/// structure IS the data, `module`s and `lesson`s ordered by `n` — so it is
/// modelled apart from the curated indexes rather than synthesised into one.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Course {
    /// Lessons that sit outside any module, in `n` order.
    pub lessons: Vec<String>,
    pub modules: Vec<CourseModule>,
}

/// The whole model of one wskill folder.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Graph {
    /// The wskill root directory — the folder holding `wskill.wcl`. Every
    /// [`Anchor`] is relative to it. For a graph loaded at a revision this
    /// is the *working-tree* path of that folder: the scratch tree the read
    /// actually happened in is gone by then, and a caller comparing two
    /// revisions wants the stable name.
    pub root: PathBuf,
    /// The entry document the model was read from, relative to `root`.
    pub entry: PathBuf,
    /// The revision the model was read at, when it wasn't the working tree.
    pub rev: Option<String>,
    pub topic: Option<Topic>,
    /// The projections declared by the registry, in registry order. Empty
    /// when the folder has no `wskill.wcl` (the model still loads — a
    /// document carrying the vocabulary is enough).
    pub views: Vec<View>,
    pub units: Vec<Unit>,
    /// The top-level indexes; each may nest sub-indexes.
    pub indexes: Vec<Index>,
    pub edges: Vec<Edge>,
    pub course: Option<Course>,
}

impl Graph {
    pub fn unit(&self, id: &str) -> Option<&Unit> {
        self.units.iter().find(|u| u.id == id)
    }

    /// Every index level in the graph, top-level and nested alike, in
    /// declaration order — the walk anything asking "which indexes?" wants,
    /// since a sub-index pins as truly as its parent.
    pub fn index_levels(&self) -> impl Iterator<Item = &Index> {
        self.indexes.iter().flat_map(Index::levels)
    }

    /// An index by id, at any nesting level.
    pub fn index(&self, id: &str) -> Option<&Index> {
        self.index_levels().find(|i| i.id == id)
    }

    /// The index whose DIRECT children include `id` — `None` when `id` is a
    /// top-level index (or no index at all). Nesting is what the structural
    /// ops (promote / demote / create-under) are defined against.
    pub fn parent_index(&self, id: &str) -> Option<&Index> {
        self.indexes
            .iter()
            .flat_map(Index::levels)
            .find(|i| i.children.iter().any(|c| c.id == id))
    }

    /// Every index level that pins `unit_id`, at any nesting depth.
    pub fn indexes_pinning(&self, unit_id: &str) -> Vec<&Index> {
        self.index_levels()
            .filter(|i| i.pinned.iter().any(|p| p == unit_id))
            .collect()
    }

    /// Units no index pins and no *declared* projection organizes
    /// structurally — the ones a reader can only reach by search.
    ///
    /// A lesson in a wskill that ships a training view is not unindexed: that
    /// view's syllabus is built from the lesson data itself
    /// ([`Graph::organizing_views`]). The same lesson in a wskill that
    /// declares no training artifact IS unindexed — nothing renders it and no
    /// index points at it, which is exactly the content a curator is hunting.
    pub fn unindexed(&self) -> Vec<&Unit> {
        self.units
            .iter()
            .filter(|u| {
                self.organizing_views(u).is_empty() && self.indexes_pinning(&u.id).is_empty()
            })
            .collect()
    }

    /// The declared views that organize `unit` structurally rather than by
    /// pinning — the training view for a lesson, the deck for a
    /// presentation. Empty for reference content, and empty for a
    /// view-owned kind whose view the registry doesn't declare.
    pub fn organizing_views(&self, unit: &Unit) -> Vec<&View> {
        let Some(owner) = structural_view_kind(&unit.kind) else {
            return Vec::new();
        };
        self.views.iter().filter(|v| v.kind == owner).collect()
    }
}

/// The artifact kind that OWNS a unit kind — the one projection built from
/// this data, because no other reads it. `None` for reference content
/// (concept / entity / fact / procedure / research / index), which the book
/// and the skill share and route by `audience` instead.
///
/// These kind names are the wskill base-schema vocabulary, so naming them
/// here is naming the format this crate is about.
pub fn structural_view_kind(unit_kind: &str) -> Option<&'static str> {
    match unit_kind {
        "lesson" | "module" => Some("training"),
        "presentation" => Some("presentation"),
        _ => None,
    }
}

/// Whether a unit of `unit_kind` with this `audience` renders in a
/// projection of `view_kind`.
///
/// A view-owned kind appears only in the view built from it (a lesson is not
/// book content). Everything else is reference content shared by the book and
/// the skill and routed by audience: the book renders `audience != :ai`, the
/// skill `!= :book`, and the data-owned views render none of it.
pub fn routes_to(unit_kind: &str, audience: &str, view_kind: &str) -> bool {
    match structural_view_kind(unit_kind) {
        Some(owner) => view_kind == owner,
        None => match view_kind {
            "book" => audience != "ai",
            "ai_skill" => audience != "book",
            _ => false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn audience_routes_reference_content_between_book_and_skill() {
        assert!(routes_to("concept", "book", "book"));
        assert!(!routes_to("concept", "book", "ai_skill"));
        assert!(routes_to("research", "ai", "ai_skill"));
        assert!(!routes_to("research", "ai", "book"));
        assert!(routes_to("concept", "both", "book"));
        assert!(routes_to("concept", "both", "ai_skill"));
        // A deck renders neither — its data is selected by its template.
        assert!(!routes_to("concept", "both", "presentation"));
    }

    #[test]
    fn a_view_owned_kind_appears_only_in_its_own_view() {
        assert!(routes_to("lesson", "book", "training"));
        assert!(!routes_to("lesson", "book", "book"));
        assert!(routes_to("presentation", "book", "presentation"));
        assert!(!routes_to("presentation", "book", "book"));
    }

    #[test]
    fn a_site_named_in_except_hides_the_block() {
        let vis = Visibility {
            except_sites: vec!["deck".to_string()],
            custom: false,
        };
        assert!(!vis.shows_in("deck"));
        assert!(vis.shows_in("book"));
    }
}
