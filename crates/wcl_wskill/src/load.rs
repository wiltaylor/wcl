//! Reading a [`Graph`] — from the working tree, from an already-open
//! document, or from a git revision.
//!
//! The block-level detail (decorators, body children, whether a `related`
//! list is a literal) comes from a **parse of each declaring file**, keyed by
//! the spans the document view hands out. The evaluated view answers what a
//! field's value is; only the AST answers how it was written, and "how it was
//! written" is what tells a writer whether it may write.

use std::collections::HashMap;
use std::fmt;
use std::path::{Path, PathBuf};

use wcl_lang::ast::{self, Item};
use wcl_lang::{DeclName, Document, Span, Value, parse_for_edit};

use crate::model::{
    Anchor, ContentBlock, Course, CourseModule, Edge, EdgeKind, Graph, Index, NodeKey, Topic, Unit,
    Visibility,
};
use crate::registry::{ROOT_MARKER, Registry};

/// Wskill plumbing kinds: gathered, but not units of content.
const PLUMBING_KINDS: &[&str] = &[
    "topic",
    "skill",
    "artifact",
    "source",
    "question",
    "wskill_ref",
];

/// How much of a block's label a content-block preview keeps.
const PREVIEW_CHARS: usize = 60;

/// The audience a unit kind gets when neither the block nor its schema says
/// (the base schema defaults every kind but `research` to `:book`).
const DEFAULT_AUDIENCE: &str = "book";

/// Why a model could not be read.
#[derive(Debug)]
pub enum Error {
    /// A file could not be read, or the entry does not exist.
    Io(String),
    /// The document (or one of its files) failed to parse.
    Parse(Box<wcl_lang::ParseError>),
    /// A git operation failed — see [`wcl_wdoc::git`].
    Git(String),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io(msg) | Error::Git(msg) => write!(f, "{msg}"),
            Error::Parse(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<wcl_lang::ParseError> for Error {
    fn from(e: wcl_lang::ParseError) -> Self {
        Error::Parse(Box::new(e))
    }
}

impl Graph {
    /// Read the model of the wskill at `entry` — a `.wcl` entry document or
    /// the wskill directory itself (whose [`ROOT_MARKER`] is then the entry).
    pub fn open(entry: &Path) -> Result<Graph, Error> {
        let entry_file = resolve_entry(entry)?;
        let root = Registry::owner_dir(&entry_file).unwrap_or_else(|| {
            entry_file
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| PathBuf::from("."))
        });
        let doc = wcl_wdoc::open_doc_for_edit(&entry_file)?;
        Graph::from_document(&doc, &root, &entry_file)
    }

    /// [`Graph::open`] at a git revision: the whole repo tree at `rev` is
    /// materialised into a scratch dir, the same repo-relative entry is read
    /// from it, and the scratch tree is dropped — the model owns everything
    /// it needs.
    ///
    /// [`Graph::root`] is reported as the working-tree path of the wskill, so
    /// two revisions of one wskill compare directly.
    pub fn open_at_rev(entry: &Path, rev: &str) -> Result<Graph, Error> {
        let entry_file = resolve_entry_lexical(entry);
        let (repo, rel) =
            wcl_wdoc::git::repo_rel(&entry_file.to_string_lossy()).map_err(Error::Git)?;
        let sha = wcl_wdoc::git::resolve_rev(rev, &repo).map_err(Error::Git)?;
        let tree = wcl_wdoc::git::materialize_rev(rev, &repo).map_err(Error::Git)?;
        let at_rev = tree.path().join(&rel);
        if !at_rev.exists() {
            return Err(Error::Io(format!(
                "'{rel}' does not exist in revision '{rev}'"
            )));
        }
        let mut graph = Graph::open(&at_rev)?;
        // Re-root onto the working tree: the scratch tree is about to go.
        let tree_root = canon(tree.path());
        if let Ok(root_rel) = graph.root.strip_prefix(&tree_root) {
            // `join("")` would leave a trailing separator, so a wskill that
            // *is* the repo root would stop comparing equal to its
            // working-tree reading.
            graph.root = if root_rel.as_os_str().is_empty() {
                repo
            } else {
                repo.join(root_rel)
            };
        }
        graph.rev = Some(sha);
        Ok(graph)
    }

    /// Read the model out of an already-open document — the entry point for
    /// a host that opened the document itself (the editor, which serves
    /// several endpoints from one open).
    ///
    /// `root` is the wskill root every [`Anchor`] is relative to; `entry` the
    /// document's own file.
    pub fn from_document(doc: &Document, root: &Path, entry: &Path) -> Result<Graph, Error> {
        Builder::new(doc, root, entry).build()
    }
}

/// The entry file for a path naming either an entry document or a wskill
/// directory.
fn resolve_entry(entry: &Path) -> Result<PathBuf, Error> {
    let file = if entry.is_dir() {
        entry.join(ROOT_MARKER)
    } else {
        entry.to_path_buf()
    };
    if !file.is_file() {
        return Err(Error::Io(format!("no such file: {}", file.display())));
    }
    Ok(file)
}

/// [`resolve_entry`] without touching the filesystem — for a revision load,
/// where the path may not exist in the working tree at all. A path that
/// doesn't end in `.wcl` is taken for a directory.
fn resolve_entry_lexical(entry: &Path) -> PathBuf {
    if entry.extension().and_then(|e| e.to_str()) == Some("wcl") {
        entry.to_path_buf()
    } else {
        entry.join(ROOT_MARKER)
    }
}

fn canon(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// One index pin, as it is collected during the walk: an index level names a
/// unit, and the graph draws the edge from the TOP-LEVEL index (sub-indexes
/// are not nodes) while attributing it to the level whose `related` list
/// holds it — the level a write must target.
struct Pin {
    top_index: String,
    owning_index: String,
    unit: String,
}

/// One model read: the document, the per-file parses it needs, and the
/// nodes accumulated so far.
struct Builder<'a> {
    doc: &'a Document,
    root: PathBuf,
    entry: PathBuf,
    /// Declaring file (canonical) → its parse.
    asts: HashMap<PathBuf, ast::Source>,
    /// Unit kind → the `audience` its schema declares by default.
    audiences: HashMap<String, String>,
    pins: Vec<Pin>,
}

impl<'a> Builder<'a> {
    fn new(doc: &'a Document, root: &Path, entry: &Path) -> Self {
        Self {
            doc,
            root: canon(root),
            entry: canon(entry),
            asts: HashMap::new(),
            audiences: HashMap::new(),
            pins: Vec::new(),
        }
    }

    fn build(mut self) -> Result<Graph, Error> {
        let unit_kinds = self.unit_kinds();

        let mut topic: Option<Topic> = None;
        let mut units: Vec<Unit> = Vec::new();
        let mut indexes: Vec<Index> = Vec::new();

        for (path, b) in self.doc.blocks_with_source() {
            let kind = b.kind().to_string();
            let is_index = kind == "index";
            if !is_index && kind != "topic" && !unit_kinds.contains(&kind) {
                continue;
            }
            let Some(id) = first_label(&b) else { continue };
            let file = path.map(Path::to_path_buf).unwrap_or(self.entry.clone());
            let anchor = self.anchor(&file, b.span());
            self.load_ast(&file)?;
            // How the block was *written* — decorators, and whether its
            // `related` list is a literal — comes from the file's parse.
            let (visibility, related_editable) = {
                let ast_block = self.ast_block_at(&file, b.span());
                (
                    ast_block.map(visibility_of).unwrap_or_default(),
                    related_editable_of(ast_block),
                )
            };

            if kind == "topic" {
                topic.get_or_insert_with(|| Topic {
                    id: id.clone(),
                    name: field_string(&b, "name").unwrap_or_else(|| id.clone()),
                    summary: field_string(&b, "summary"),
                    anchor: anchor.clone(),
                });
                continue;
            }

            let title = ["name", "title", "topic"]
                .iter()
                .find_map(|f| field_string(&b, f))
                .unwrap_or_else(|| id.clone());
            let related = related_ids(&b);

            if is_index {
                let children = self.index_children(&b, &id, &file);
                for rid in &related {
                    self.pins.push(Pin {
                        top_index: id.clone(),
                        owning_index: id.clone(),
                        unit: rid.clone(),
                    });
                }
                indexes.push(Index {
                    audience: self.audience_of(&b, &kind),
                    id,
                    title,
                    anchor,
                    visibility,
                    pinned: related,
                    related_editable,
                    children,
                });
            } else {
                let blocks = self
                    .ast_block_at(&file, b.span())
                    .map(|blk| self.content_blocks(blk, &file))
                    .unwrap_or_default();
                units.push(Unit {
                    audience: self.audience_of(&b, &kind),
                    id,
                    kind,
                    title,
                    anchor,
                    visibility,
                    related,
                    related_editable,
                    blocks,
                });
            }
        }

        let edges = self.edges(&units, &indexes);
        let course = self.course();
        let views = Registry::read(&self.root.join(ROOT_MARKER))
            .map(|r| r.views(&self.root))
            .unwrap_or_default();

        Ok(Graph {
            entry: rel_to(&self.root, &self.entry),
            root: self.root,
            rev: None,
            topic,
            views,
            units,
            indexes,
            edges,
            course,
        })
    }

    /// The content kinds this document gathers: everything but wdoc's own
    /// infrastructure gathers, the wskill plumbing, and `index` (which is a
    /// nav structure, not a unit).
    ///
    /// Every gathered kind's declared `audience` default is recorded on the
    /// way past — `index` included, since an index routes by audience just
    /// like a unit does.
    fn unit_kinds(&mut self) -> Vec<String> {
        let mut out = Vec::new();
        for g in self.doc.gathered_kinds() {
            let kind = g.kind().to_string();
            if g.schema().full_name().starts_with("wdoc.") {
                continue;
            }
            if let Some(default) = g
                .schema()
                .effective_fields()
                .into_iter()
                .find(|f| f.name() == "audience")
                .and_then(|f| f.default_value().as_ref().map(value_string))
            {
                self.audiences.insert(kind.clone(), default);
            }
            if PLUMBING_KINDS.contains(&kind.as_str()) || kind == "index" {
                continue;
            }
            out.push(kind);
        }
        out
    }

    /// A block's audience routing: its own field, else its kind schema's
    /// declared default, else `book`.
    fn audience_of(&self, b: &wcl_lang::Block<'_>, kind: &str) -> String {
        field_string(b, "audience").unwrap_or_else(|| {
            self.audiences
                .get(kind)
                .cloned()
                .unwrap_or_else(|| DEFAULT_AUDIENCE.to_string())
        })
    }

    /// An index's nested sub-index levels, recording their pins against the
    /// top-level index as they go.
    fn index_children(&mut self, b: &wcl_lang::Block<'_>, top_id: &str, file: &Path) -> Vec<Index> {
        let mut out = Vec::new();
        for c in b.blocks().filter(|c| c.kind() == "index") {
            let Some(id) = first_label(&c) else { continue };
            let pinned = related_ids(&c);
            for rid in &pinned {
                self.pins.push(Pin {
                    top_index: top_id.to_string(),
                    owning_index: id.clone(),
                    unit: rid.clone(),
                });
            }
            let children = self.index_children(&c, top_id, file);
            let ast_block = self.ast_block_at(file, c.span());
            out.push(Index {
                title: field_string(&c, "name").unwrap_or_else(|| id.clone()),
                audience: self.audience_of(&c, "index"),
                id,
                anchor: self.anchor(file, c.span()),
                visibility: ast_block.map(visibility_of).unwrap_or_default(),
                pinned,
                related_editable: related_editable_of(ast_block),
                children,
            });
        }
        out
    }

    /// A unit's content blocks, flattened one level: its direct children with
    /// the transparent `body` container spliced, so the model lists the
    /// blocks that actually render.
    fn content_blocks(&self, unit: &ast::Block, file: &Path) -> Vec<ContentBlock> {
        let mut out = Vec::new();
        for item in &unit.items {
            let Item::Block(b) = item else { continue };
            if b.kind == "body" {
                out.extend(b.items.iter().filter_map(|inner| match inner {
                    Item::Block(c) => Some(self.content_block(c, file)),
                    _ => None,
                }));
            } else {
                out.push(self.content_block(b, file));
            }
        }
        out
    }

    fn content_block(&self, b: &ast::Block, file: &Path) -> ContentBlock {
        ContentBlock {
            kind: b.kind.clone(),
            preview: crate::registry::ast_label(b)
                .unwrap_or_default()
                .chars()
                .take(PREVIEW_CHARS)
                .collect(),
            anchor: self.anchor(file, b.span),
            visibility: visibility_of(b),
        }
    }

    /// Every edge whose ends both resolve to a node: unit `related` links,
    /// then index pins (attributed to the level that holds them).
    fn edges(&self, units: &[Unit], indexes: &[Index]) -> Vec<Edge> {
        let keys: HashMap<&str, NodeKey> = units
            .iter()
            .map(|u| (u.id.as_str(), u.key()))
            .chain(indexes.iter().map(|i| (i.id.as_str(), i.key())))
            .collect();
        let mut out: Vec<Edge> = Vec::new();
        for u in units {
            for rid in &u.related {
                if let Some(to) = keys.get(rid.as_str()) {
                    out.push(Edge {
                        from: u.key(),
                        to: to.clone(),
                        kind: EdgeKind::Related,
                        index_id: None,
                    });
                }
            }
        }
        for pin in &self.pins {
            if let (Some(from), Some(to)) = (
                keys.get(pin.top_index.as_str()),
                keys.get(pin.unit.as_str()),
            ) {
                out.push(Edge {
                    from: from.clone(),
                    to: to.clone(),
                    kind: EdgeKind::Pin,
                    index_id: Some(pin.owning_index.clone()),
                });
            }
        }
        out
    }

    /// The course structure, when the document carries one: ungrouped
    /// lessons in `n` order, then each module with its own lessons.
    fn course(&self) -> Option<Course> {
        let ordered = |blocks: Vec<wcl_lang::Block<'_>>| -> Vec<String> {
            let mut v: Vec<(u64, String)> = blocks
                .iter()
                .filter_map(|b| Some((order_of(b), first_label(b)?)))
                .collect();
            v.sort_by_key(|(n, _)| *n);
            v.into_iter().map(|(_, id)| id).collect()
        };
        let lessons = ordered(self.doc.blocks().filter(|b| b.kind() == "lesson").collect());
        let mut modules: Vec<(u64, CourseModule)> = self
            .doc
            .blocks()
            .filter(|b| b.kind() == "module")
            .filter_map(|m| {
                let id = first_label(&m)?;
                Some((
                    order_of(&m),
                    CourseModule {
                        title: field_string(&m, "title").unwrap_or_else(|| id.clone()),
                        id,
                        lessons: ordered(m.blocks().filter(|b| b.kind() == "lesson").collect()),
                    },
                ))
            })
            .collect();
        modules.sort_by_key(|(n, _)| *n);
        if lessons.is_empty() && modules.is_empty() {
            return None;
        }
        Some(Course {
            lessons,
            modules: modules.into_iter().map(|(_, m)| m).collect(),
        })
    }

    fn anchor(&self, file: &Path, span: Span) -> Anchor {
        Anchor {
            file: rel_to(&self.root, file),
            span,
        }
    }

    /// The parsed block at `span` in `file` — `None` when the file wasn't
    /// loaded, or the span names nothing (a synthesised block).
    fn ast_block_at(&self, file: &Path, span: Span) -> Option<&ast::Block> {
        self.asts
            .get(&canon(file))
            .and_then(|src| wcl_lang::edit::block_at_span(&src.items, span))
    }

    /// Parse `file` into the per-read cache, if it isn't there already.
    fn load_ast(&mut self, file: &Path) -> Result<(), Error> {
        let key = canon(file);
        if self.asts.contains_key(&key) {
            return Ok(());
        }
        let text = std::fs::read_to_string(&key)
            .map_err(|e| Error::Io(format!("failed to read {}: {e}", key.display())))?;
        let src = parse_for_edit(&text, key.display().to_string())?;
        self.asts.insert(key, src);
        Ok(())
    }
}

/// `file` relative to `root` when it is inside it, else the path as given.
fn rel_to(root: &Path, file: &Path) -> PathBuf {
    let canon_file = canon(file);
    canon_file
        .strip_prefix(root)
        .map(Path::to_path_buf)
        .unwrap_or(canon_file)
}

/// A block's declared site visibility. The classification is
/// [`wcl_wdoc::declared_visibility`]'s — `@only` / `@except` is wdoc's
/// vocabulary, and the editor's block endpoints read the same one, so the
/// model and a rendered visibility stamp can't disagree about what is
/// `custom`.
fn visibility_of(block: &ast::Block) -> Visibility {
    let v = wcl_wdoc::declared_visibility(block);
    Visibility {
        except_sites: v.except_sites,
        custom: v.custom,
    }
}

/// A `related` list may be rewritten only when it is absent or a literal
/// list — a computed expression must not be clobbered by a pin/unpin write.
fn related_editable_of(block: Option<&ast::Block>) -> bool {
    block.is_some_and(|b| {
        !b.items.iter().any(|it| {
            matches!(it, Item::Field(f)
                if f.name == "related" && !matches!(f.expr, ast::Expr::ListLit { .. }))
        })
    })
}

/// The ordered `related` ids of a block (empty when absent or not
/// list-valued).
fn related_ids(b: &wcl_lang::Block<'_>) -> Vec<String> {
    match b.field("related").and_then(|f| f.value().ok().cloned()) {
        Some(Value::List(items)) => items.iter().map(value_string).collect(),
        _ => Vec::new(),
    }
}

/// A course block's `n` (its position); missing / non-numeric sorts last.
fn order_of(b: &wcl_lang::Block<'_>) -> u64 {
    match b.field("n").and_then(|f| f.value().ok().cloned()) {
        Some(Value::U32(n)) => n as u64,
        Some(Value::U64(n)) => n,
        Some(Value::I64(n)) if n >= 0 => n as u64,
        _ => u64::MAX,
    }
}

fn first_label(b: &wcl_lang::Block<'_>) -> Option<String> {
    b.labels()
        .ok()
        .and_then(|ls| ls.first().map(value_string))
        .filter(|s| !s.is_empty())
}

/// A block field's value as a plain string, when it evaluates to a scalar.
fn field_string(b: &wcl_lang::Block<'_>, name: &str) -> Option<String> {
    b.field(name)
        .and_then(|f| f.value().ok().cloned())
        .as_ref()
        .map(value_string)
        .filter(|s| !s.is_empty())
}

fn value_string(v: &Value) -> String {
    match v {
        Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) | Value::Symbol(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::I64(n) => n.to_string(),
        Value::U64(n) => n.to_string(),
        Value::F64(n) => n.to_string(),
        other => format!("{other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testsupport::{mini_wskill, write};

    #[test]
    fn reads_units_indexes_and_edges_with_anchors() {
        let td = mini_wskill();
        let g = Graph::open(td.path()).expect("graph");

        assert_eq!(g.entry, PathBuf::from(ROOT_MARKER));
        assert_eq!(g.rev, None);
        let topic = g.topic.as_ref().expect("topic");
        assert_eq!(topic.id, "mini");
        assert_eq!(topic.name, "Mini");

        // Units, with the file + span they're declared at.
        let alpha = g.unit("alpha").expect("alpha");
        assert_eq!(alpha.kind, "concept");
        assert_eq!(alpha.title, "Alpha");
        assert_eq!(alpha.audience, "book");
        assert_eq!(alpha.anchor.file, PathBuf::from("data/concepts/alpha.wcl"));
        let text = std::fs::read_to_string(td.path().join("data/concepts/alpha.wcl")).unwrap();
        assert!(
            text[alpha.anchor.span.start..alpha.anchor.span.end].starts_with("concept alpha"),
            "the span must address the block: {:?}",
            alpha.anchor
        );
        // The schema's `@default(:ai)` is the audience when the block is
        // silent — for an index's kind as much as a unit's.
        assert_eq!(g.unit("gamma").expect("gamma").audience, "ai");
        assert_eq!(g.index("lang").expect("index").audience, "both");

        // The index is not a unit; it pins two of them, in authored order.
        assert!(g.unit("lang").is_none());
        let lang = g.index("lang").expect("index");
        assert_eq!(lang.pinned, ["alpha", "beta"]);
        assert!(lang.related_editable);

        // Edges: one `related` link, two pins.
        let pins: Vec<String> = g
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Pin)
            .map(|e| e.to.to_string())
            .collect();
        assert_eq!(pins, ["concept:alpha", "concept:beta"]);
        let related: Vec<(String, String)> = g
            .edges
            .iter()
            .filter(|e| e.kind == EdgeKind::Related)
            .map(|e| (e.from.to_string(), e.to.to_string()))
            .collect();
        assert_eq!(
            related,
            [("concept:alpha".to_string(), "concept:beta".to_string())]
        );
    }

    /// The body blocks a unit ships, with the per-block visibility a view
    /// toggle writes.
    #[test]
    fn lists_body_blocks_and_their_visibility() {
        let td = mini_wskill();
        let alpha = Graph::open(td.path()).expect("graph");
        let alpha = alpha.unit("alpha").expect("alpha");
        let kinds: Vec<&str> = alpha.blocks.iter().map(|b| b.kind.as_str()).collect();
        assert_eq!(kinds, ["p", "p"]);
        assert_eq!(alpha.blocks[0].preview, "Everywhere");
        assert!(alpha.blocks[0].visibility.except_sites.is_empty());
        assert_eq!(alpha.blocks[1].visibility.except_sites, ["skill"]);
        assert!(!alpha.blocks[1].visibility.custom);
        assert!(!alpha.blocks[1].visibility.shows_in("skill"));
    }

    /// The computed-field flag: a `related` list written as an expression is
    /// read like any other, but must never be rewritten — a pin/unpin write
    /// would clobber the expression that produced it.
    #[test]
    fn flags_a_computed_related_list_as_unwritable() {
        let td = mini_wskill();
        write(
            td.path(),
            "data/concepts/alpha.wcl",
            "concept alpha {\n  name = \"Alpha\"\n  related = flatten([])\n}\n",
        );
        let g = Graph::open(td.path()).expect("graph");
        let alpha = g.unit("alpha").expect("alpha");
        assert!(!alpha.related_editable);
        // The neighbouring literal lists stay writable.
        assert!(g.unit("beta").expect("beta").related_editable);
        assert!(g.index("lang").expect("index").related_editable);
    }

    /// Custom visibility (`@only`, a computed site list) is reported as such
    /// rather than guessed at.
    #[test]
    fn reports_unreadable_visibility_as_custom() {
        let td = mini_wskill();
        write(
            td.path(),
            "data/concepts/beta.wcl",
            "@only(sites = [:book])\nconcept beta {\n  name = \"Beta\"\n}\n",
        );
        let g = Graph::open(td.path()).expect("graph");
        let beta = g.unit("beta").expect("beta");
        assert!(beta.visibility.custom);
        assert!(beta.visibility.except_sites.is_empty());
    }

    #[test]
    fn nests_sub_indexes_and_attributes_their_pins() {
        let td = mini_wskill();
        write(
            td.path(),
            "data/indexes.wcl",
            "index lang {\n  name = \"Language\"\n  related = [alpha]\n\n  \
             index lang_sub {\n    name = \"Sub\"\n    related = [beta]\n  }\n}\n",
        );
        let g = Graph::open(td.path()).expect("graph");
        let lang = g.index("lang").expect("index");
        assert_eq!(lang.pinned, ["alpha"]);
        assert_eq!(lang.children.len(), 1);
        assert_eq!(lang.children[0].title, "Sub");
        assert_eq!(lang.children[0].pinned, ["beta"]);
        // A sub-index is reachable by id but is not a node of its own.
        assert_eq!(g.index("lang_sub").expect("sub").id, "lang_sub");
        assert_eq!(g.indexes.len(), 1);
        // Its pin rides the top-level index's edge, attributed to the level
        // that holds it — which is what a write must target.
        let pin = g
            .edges
            .iter()
            .find(|e| e.kind == EdgeKind::Pin && e.to.id == "beta")
            .expect("pin");
        assert_eq!(pin.from.to_string(), "index:lang");
        assert_eq!(pin.index_id.as_deref(), Some("lang_sub"));
        assert_eq!(
            g.indexes_pinning("beta")
                .iter()
                .map(|i| i.id.as_str())
                .collect::<Vec<_>>(),
            ["lang_sub"]
        );
    }

    /// A unit no index pins is what a curator is looking for.
    #[test]
    fn finds_unindexed_units() {
        let td = mini_wskill();
        let g = Graph::open(td.path()).expect("graph");
        assert_eq!(
            g.unindexed()
                .iter()
                .map(|u| u.id.as_str())
                .collect::<Vec<_>>(),
            ["gamma"]
        );
    }

    /// The registry's projections, paired with the site name each entry
    /// declares — the name a visibility decorator uses.
    #[test]
    fn reads_the_registry_projections() {
        let td = mini_wskill();
        let g = Graph::open(td.path()).expect("graph");
        let views: Vec<(&str, &str, &str)> = g
            .views
            .iter()
            .map(|v| (v.id.as_str(), v.kind.as_str(), v.site_name()))
            .collect();
        assert_eq!(
            views,
            [("book", "book", "book"), ("ai_skill", "ai_skill", "skill")]
        );

        // Routing: alpha (the schema default `:book`) is book content, and
        // gamma (`:ai`) is skill content.
        let book = &g.views[0];
        let skill = &g.views[1];
        assert!(g.unit("alpha").unwrap().shows_in(book));
        assert!(!g.unit("alpha").unwrap().shows_in(skill));
        assert!(!g.unit("gamma").unwrap().shows_in(book));
        assert!(g.unit("gamma").unwrap().shows_in(skill));
        assert!(g.index("lang").unwrap().shows_in(book));
    }

    /// A course is structure, not pins: lessons and modules ordered by `n`.
    #[test]
    fn reads_a_course_in_n_order() {
        let td = mini_wskill();
        write(
            td.path(),
            "data/lessons.wcl",
            "lesson second { title = \"Second\"  n = 2u32 }\n\n\
             lesson first { title = \"First\"  n = 1u32 }\n\n\
             module basics {\n  title = \"Basics\"\n  n = 1u32\n\n  \
             lesson nested { title = \"Nested\"  n = 1u32 }\n}\n",
        );
        let g = Graph::open(td.path()).expect("graph");
        let course = g.course.as_ref().expect("course");
        // Ungrouped lessons only — `nested` belongs to its module.
        assert_eq!(course.lessons, ["first", "second"]);
        assert_eq!(course.modules.len(), 1);
        assert_eq!(course.modules[0].title, "Basics");
        assert_eq!(course.modules[0].lessons, ["nested"]);

        // No index pins a lesson, and this wskill declares no training
        // artifact — so nothing renders them and the curator hears about it.
        assert!(g.unindexed().iter().any(|u| u.id == "first"));

        // Declare the training view and they have a structural home: its
        // syllabus is built from the lesson data itself.
        let registry = std::fs::read_to_string(td.path().join(crate::ROOT_MARKER)).unwrap();
        write(
            td.path(),
            crate::ROOT_MARKER,
            &format!(
                "{registry}\nartifact course {{\n  kind = :training\n  \
                 entry = \"wdoc/training/main.wcl\"\n}}\n"
            ),
        );
        let g = Graph::open(td.path()).expect("graph");
        let first = g.unit("first").expect("first");
        assert_eq!(
            g.organizing_views(first)
                .iter()
                .map(|v| v.id.as_str())
                .collect::<Vec<_>>(),
            ["course"]
        );
        assert!(!g.unindexed().iter().any(|u| u.id == "first"));
    }

    /// Run git in `dir`, failing the test with its stderr.
    fn git(dir: &Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .output()
            .expect("git is required for this test");
        assert!(
            out.status.success(),
            "git {args:?}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// Loading at a revision is what makes an audit possible: the baseline
    /// and the working tree are two graphs held at once, comparable because
    /// both are owned and both anchor relative to the wskill root.
    #[test]
    fn loads_the_model_at_a_git_revision() {
        let td = mini_wskill();
        let root = td.path();
        git(root, &["init", "-q"]);
        git(root, &["add", "-A"]);
        git(
            root,
            &[
                "-c",
                "user.email=t@example.com",
                "-c",
                "user.name=Test",
                "commit",
                "-qm",
                "the baseline",
            ],
        );
        // Now diverge the working tree: rename a unit, add an unpinned one.
        write(
            root,
            "data/concepts/beta.wcl",
            "concept beta {\n  name = \"Beta, renamed\"\n}\n",
        );

        let before = Graph::open_at_rev(root, "HEAD").expect("at HEAD");
        let after = Graph::open(root).expect("working tree");

        assert_eq!(before.unit("beta").expect("beta").title, "Beta");
        assert_eq!(after.unit("beta").expect("beta").title, "Beta, renamed");
        assert_eq!(before.rev.as_ref().map(String::len), Some(40));
        assert_eq!(after.rev, None);
        // Anchors and root are wskill-relative / working-tree paths, so the
        // scratch tree the baseline was read from never leaks out.
        assert_eq!(
            before.unit("beta").expect("beta").anchor.file,
            PathBuf::from("data/concepts/beta.wcl")
        );
        assert_eq!(canon(&before.root), canon(&after.root));

        let missing = Graph::open_at_rev(root, "HEAD~1").expect_err("no parent commit");
        assert!(matches!(missing, Error::Git(_)), "{missing}");
    }

    #[test]
    fn an_entry_that_does_not_exist_is_an_error() {
        let td = tempfile::tempdir().unwrap();
        let err = Graph::open(&td.path().join("nope")).expect_err("missing entry");
        assert!(err.to_string().contains("no such file"), "{err}");
    }
}
