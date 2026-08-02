//! Auditing a range: two [`Graph`]s read at two revisions, diffed into one
//! model.
//!
//! **The audit model is the union graph — before ∪ after, with removals
//! marked.** That is the one thing a live graph structurally cannot be: a
//! live graph draws what exists, and half of an audit is what stopped
//! existing. Measured on a real 30-unit authoring commit, an after-only
//! reading simply omitted the five deleted units and nineteen removed edges.
//!
//! Three consequences shape everything below:
//!
//! - **Every node of either revision is here**, each carrying a [`Change`].
//!   A consumer that wants the after-state filters `change != Removed`; one
//!   drawing the audit ghosts the removals.
//! - **Findings ride the changed nodes and are scoped to the range**
//!   ([`NodeDelta::findings`]): what is *newly* wrong, not everything that is
//!   wrong. A standing lint run answers the second question and this one must
//!   not, or the range's own damage drowns in the corpus's backlog.
//! - **Health is [summary data](Audit::health), not a report.** A health
//!   report names no unit — it is the shape of a gate, not of a review — so
//!   the metrics ride the header beside the counts and the named nodes stay
//!   the surface.
//!
//! Nothing here writes, and nothing here lays out: an audit view lays out
//! the union graph the same way a graph view lays out a live one.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};

use wcl_lang::Span;

use crate::lint::{Finding, Rule, Severity, lint};
use crate::load::Error;
use crate::model::{Anchor, Edge, EdgeKind, Graph, NodeKey, Visibility};

/// The node kind an `index` carries — the one place this module names the
/// wskill vocabulary, to split the per-family counts.
const INDEX_KIND: &str = "index";

/// The range an audit covers when the caller names none: the previous
/// commit against the working tree. An authoring session's output is
/// usually the last commit, and often not yet committed at all.
pub const DEFAULT_RANGE: &str = "HEAD~1";

/// One end of a range, before git has resolved it.
///
/// [`Endpoint::MergeBase`] exists because `a...b` is the shape of *review a
/// branch*, and its baseline is where the branch started — not wherever the
/// other branch has since got to. Resolving it needs the repo, which is why
/// parsing and resolving are two steps.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Endpoint {
    /// A plain revision, resolved by git the usual way.
    Rev(String),
    /// The commit where two revisions diverged (`a...b`).
    MergeBase(String, String),
}

/// A git range: what to compare against what.
///
/// `after: None` is the **working tree**, which is why `HEAD~1..` and a bare
/// `HEAD~1` mean the same thing: an agent's output is reviewed before it is
/// committed as often as after.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Range {
    pub before: Endpoint,
    pub after: Option<String>,
}

impl Range {
    /// Parse a git range. The forms git itself uses, and no others:
    ///
    /// | spec | before | after |
    /// |---|---|---|
    /// | *(empty)* | `HEAD~1` | working tree |
    /// | `a` | `a` | working tree |
    /// | `a..b` | `a` | `b` |
    /// | `a..` | `a` | working tree |
    /// | `..b` | `HEAD` | `b` |
    /// | `a...b` | merge base of `a` and `b` | `b` |
    /// | `a...` | merge base of `a` and `HEAD` | working tree |
    ///
    /// Infallible: every remaining string is a revision, and whether a
    /// revision exists is git's answer to give, with git's message.
    pub fn parse(spec: &str) -> Range {
        let spec = spec.trim();
        let spec = if spec.is_empty() { DEFAULT_RANGE } else { spec };
        // `...` first: it contains `..`, so the two-dot split would tear it
        // into a revision named `.b`.
        if let Some((a, b)) = spec.split_once("...") {
            let a = non_empty(a).unwrap_or("HEAD");
            return Range {
                before: Endpoint::MergeBase(a.to_string(), non_empty(b).unwrap_or("HEAD").into()),
                after: non_empty(b).map(str::to_string),
            };
        }
        if let Some((a, b)) = spec.split_once("..") {
            return Range {
                before: Endpoint::Rev(non_empty(a).unwrap_or("HEAD").to_string()),
                after: non_empty(b).map(str::to_string),
            };
        }
        Range {
            before: Endpoint::Rev(spec.to_string()),
            after: None,
        }
    }

    /// The baseline revision, with a merge base resolved against the repo
    /// holding `entry`.
    fn baseline(&self, entry: &Path) -> Result<String, Error> {
        match &self.before {
            Endpoint::Rev(r) => Ok(r.clone()),
            Endpoint::MergeBase(a, b) => {
                // Named as a file inside the wskill, because `repo_rel` runs
                // git in an absolute path's PARENT — handed the folder
                // itself, it would look for the repo one level above the
                // wskill and could find a different one.
                let anchor = crate::load::entry_file_of(entry);
                let (repo, _) =
                    wcl_wdoc::git::repo_rel(&anchor.to_string_lossy()).map_err(Error::Git)?;
                wcl_wdoc::git::merge_base(a, b, &repo).map_err(Error::Git)
            }
        }
    }
}

fn non_empty(s: &str) -> Option<&str> {
    (!s.is_empty()).then_some(s)
}

/// What happened to a node or an edge between the two revisions.
///
/// Declared most-newsworthy first, which is the order an audit reads in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Change {
    Added,
    Removed,
    Modified,
    Unchanged,
}

impl Change {
    /// The one-character marker a changelog line carries.
    pub fn marker(&self) -> char {
        match self {
            Change::Added => '+',
            Change::Removed => '-',
            Change::Modified => '~',
            Change::Unchanged => ' ',
        }
    }
}

/// What about a node differs. A vocabulary rather than a free-text
/// description, because the reviewer's first question about a modified unit
/// is *which part* — and "the prose" and "the links" are read by different
/// people for different reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(into = "&'static str")]
pub enum Aspect {
    Title,
    Audience,
    Visibility,
    /// A unit's `related` links, or an index's pins — ids, order or reasons.
    Related,
    /// The body's block list or its word count.
    Body,
    /// An index's sub-index list.
    Children,
    /// The node moved to another file.
    File,
}

impl Aspect {
    pub fn as_str(&self) -> &'static str {
        match self {
            Aspect::Title => "title",
            Aspect::Audience => "audience",
            Aspect::Visibility => "visibility",
            Aspect::Related => "related",
            Aspect::Body => "body",
            Aspect::Children => "children",
            Aspect::File => "file",
        }
    }
}

impl From<Aspect> for &'static str {
    fn from(a: Aspect) -> &'static str {
        a.as_str()
    }
}

impl std::fmt::Display for Aspect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One node of the union graph: a unit or an index, from either revision.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeDelta {
    #[serde(flatten)]
    pub node: NodeKeyFields,
    pub change: Change,
    pub title: String,
    /// Where it is written, relative to [`Graph::root`] — read from the
    /// after revision for anything that survived, and from the before
    /// revision for a removal, which is where it *was* written and the only
    /// answer there is. A removal's `span` therefore addresses a file as it
    /// no longer is, and a caller resolving it must say so.
    pub file: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    /// What differs, for a [`Change::Modified`] node; empty otherwise.
    pub changed: Vec<Aspect>,
    /// The findings **new in this range** — an after-revision finding whose
    /// exact match did not fire before.
    ///
    /// New rather than current, uniformly, because the audit's question is
    /// what this range did. An over-cap unit that was already over cap is
    /// not what the range broke; a unit left dangling by another unit's
    /// deletion is, even though nothing in it was touched.
    pub findings: Vec<Finding>,
}

/// A node's identity, serialized as `{kind, id}` beside its own fields —
/// the shape [`Finding`] already uses, so a consumer joining findings to
/// rows reads one spelling of identity.
#[derive(Debug, Clone, serde::Serialize)]
pub struct NodeKeyFields {
    pub kind: String,
    pub id: String,
}

impl NodeDelta {
    /// Whether this node is part of the changelog: it changed, or the range
    /// broke something in it.
    pub fn is_news(&self) -> bool {
        self.change != Change::Unchanged || !self.findings.is_empty()
    }

    pub fn key(&self) -> NodeKey {
        NodeKey {
            kind: self.node.kind.clone(),
            id: self.node.id.clone(),
        }
    }
}

/// One edge of the union graph. An edge has no content beyond its endpoints,
/// so it is only ever added, removed or untouched — a link whose *reason*
/// changed is a change to the unit that wrote it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct EdgeDelta {
    pub from: NodeKey,
    pub to: NodeKey,
    pub kind: EdgeKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index_id: Option<String>,
    pub change: Change,
}

/// How many of each thing moved. Nodes are counted per family because "+30
/// units" and "+1 index" are different news.
#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct Counts {
    pub added: usize,
    pub removed: usize,
    pub modified: usize,
}

#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct EdgeCounts {
    pub added: usize,
    pub removed: usize,
}

#[derive(Debug, Clone, Copy, Default, serde::Serialize)]
pub struct Summary {
    pub units: Counts,
    pub indexes: Counts,
    pub edges: EdgeCounts,
}

/// One health measurement at both ends of the range.
///
/// **Every metric is oriented so that lower is better**, which is what lets
/// an audit say "worse" without carrying a direction per metric. A quantity
/// that reads better going up is expressed as its complement — reasons on
/// links become *reasonless* links — so the orientation is a property of the
/// vocabulary rather than a flag anything can set wrong.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Metric {
    /// The stable name — what a consumer keys off.
    pub key: &'static str,
    /// How it reads in a header strip.
    pub label: &'static str,
    pub before: f64,
    pub after: f64,
    /// Whether the value is a ratio rather than a count, which is the whole
    /// of the difference between rendering `3.39` and `11`.
    pub ratio: bool,
    /// Whether it moved in the wrong direction.
    pub worse: bool,
}

impl Metric {
    pub fn moved(&self) -> bool {
        self.before != self.after
    }

    /// One end of the metric, formatted the way its own kind reads.
    pub fn format(&self, value: f64) -> String {
        if self.ratio {
            format!("{value:.2}")
        } else {
            format!("{value:.0}")
        }
    }
}

/// The whole audit of one range.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Audit {
    /// The wskill root, as the working tree names it.
    pub root: PathBuf,
    /// The entry document the two models were read from, relative to `root`.
    pub entry: PathBuf,
    /// The baseline commit sha — empty when the baseline was itself read
    /// from a working tree, which only [`Audit::of`] can produce.
    pub before: String,
    /// The compared commit sha, or `None` for the working tree.
    pub after: Option<String>,
    pub summary: Summary,
    pub health: Vec<Metric>,
    /// Every node of either revision, most-newsworthy first.
    pub nodes: Vec<NodeDelta>,
    /// Every edge of either revision.
    pub edges: Vec<EdgeDelta>,
}

impl Audit {
    /// Load both ends of `range` and diff them.
    ///
    /// `entry` is a wskill folder or an entry document inside one, named as
    /// the working tree names it — the same path on both sides, which is
    /// what makes the two readings comparable.
    pub fn across(entry: &Path, range: &Range) -> Result<Audit, Error> {
        let baseline = range.baseline(entry)?;
        let before = Graph::open_at_rev(entry, &baseline)?;
        let after = match &range.after {
            Some(rev) => Graph::open_at_rev(entry, rev)?,
            None => Graph::open(entry)?,
        };
        Ok(Audit::of(&before, &after))
    }

    /// Diff two already-loaded models. The pure half: everything an audit
    /// says is a function of the two graphs.
    pub fn of(before: &Graph, after: &Graph) -> Audit {
        let before_findings = lint(before);
        let after_findings = lint(after);
        let nodes = diff_nodes(before, after, &before_findings, &after_findings);
        let edges = diff_edges(before, after);
        Audit {
            root: after.root.clone(),
            entry: after.entry.clone(),
            before: before.rev.clone().unwrap_or_default(),
            after: after.rev.clone(),
            summary: summarize(&nodes, &edges),
            health: health(
                measure(before, &before_findings),
                measure(after, &after_findings),
            ),
            nodes,
            edges,
        }
    }

    /// The changelog: the nodes this range touched or broke, in reading
    /// order.
    pub fn news(&self) -> impl Iterator<Item = &NodeDelta> {
        self.nodes.iter().filter(|n| n.is_news())
    }

    /// The edges this range added or removed whose source is `node` — the
    /// link churn a changelog row reports under itself.
    pub fn edge_news<'a>(&'a self, node: &'a NodeKey) -> impl Iterator<Item = &'a EdgeDelta> {
        self.edges
            .iter()
            .filter(move |e| e.change != Change::Unchanged && &e.from == node)
    }
}

/// One node of either graph, read uniformly: the diff asks a unit and an
/// index the same questions, and a per-kind comparison would be two places
/// to forget a field.
struct Node<'a> {
    key: NodeKey,
    title: &'a str,
    audience: &'a str,
    anchor: &'a Anchor,
    visibility: &'a Visibility,
    /// `related` links (a unit) or pins (an index), with their reasons.
    related: Vec<(&'a str, Option<&'a str>)>,
    /// Body blocks as (kind, preview, hidden-from).
    blocks: Vec<(&'a str, &'a str, &'a [String])>,
    /// Sub-index ids; always empty for a unit.
    children: Vec<&'a str>,
    /// Body words; always 0 for an index, which the model does not measure.
    words: usize,
}

fn nodes_of(graph: &Graph) -> Vec<Node<'_>> {
    let mut out: Vec<Node> = graph
        .units
        .iter()
        .map(|u| Node {
            key: u.key(),
            title: &u.title,
            audience: &u.audience,
            anchor: &u.anchor,
            visibility: &u.visibility,
            related: u
                .related
                .iter()
                .map(|l| (l.id.as_str(), l.why.as_deref()))
                .collect(),
            blocks: blocks_of(&u.blocks),
            children: Vec::new(),
            words: u.words,
        })
        .collect();
    out.extend(graph.index_levels().map(|i| Node {
        key: i.key(),
        title: &i.title,
        audience: &i.audience,
        anchor: &i.anchor,
        visibility: &i.visibility,
        related: i.pinned.iter().map(|p| (p.as_str(), None)).collect(),
        blocks: blocks_of(&i.blocks),
        children: i.children.iter().map(|c| c.id.as_str()).collect(),
        words: 0,
    }));
    out
}

fn blocks_of(blocks: &[crate::model::ContentBlock]) -> Vec<(&str, &str, &[String])> {
    blocks
        .iter()
        .map(|b| {
            (
                b.kind.as_str(),
                b.preview.as_str(),
                b.visibility.except_sites.as_slice(),
            )
        })
        .collect()
}

/// What differs between two readings of one node.
///
/// The body comparison is the model's own reading of a body — its block
/// list and its word count — not the prose. A reword of the same length
/// inside a paragraph therefore does not register: the model carries a
/// 60-character preview per block, not the text. That is the accepted
/// limit of auditing a model rather than a file diff, and the file diff is
/// still there for anyone who wants the words.
fn aspects(before: &Node, after: &Node) -> Vec<Aspect> {
    let mut out = Vec::new();
    if before.title != after.title {
        out.push(Aspect::Title);
    }
    if before.audience != after.audience {
        out.push(Aspect::Audience);
    }
    if before.visibility != after.visibility {
        out.push(Aspect::Visibility);
    }
    if before.related != after.related {
        out.push(Aspect::Related);
    }
    if before.blocks != after.blocks || before.words != after.words {
        out.push(Aspect::Body);
    }
    if before.children != after.children {
        out.push(Aspect::Children);
    }
    if before.anchor.file != after.anchor.file {
        out.push(Aspect::File);
    }
    out
}

/// A finding's identity for the "is this new?" question: the node, the rule
/// and the message. Keyed on the message too, because one unit can dangle
/// two ids under one rule and fixing one of them must leave the other
/// reported — and lint's messages are deterministic by construction.
fn finding_key(f: &Finding) -> (String, &'static str, &str) {
    (f.node.to_string(), f.rule.slug(), f.message.as_str())
}

fn diff_nodes(
    before: &Graph,
    after: &Graph,
    before_findings: &[Finding],
    after_findings: &[Finding],
) -> Vec<NodeDelta> {
    let old: BTreeMap<NodeKey, Node> = nodes_of(before)
        .into_iter()
        .map(|n| (n.key.clone(), n))
        .collect();
    let new: BTreeMap<NodeKey, Node> = nodes_of(after)
        .into_iter()
        .map(|n| (n.key.clone(), n))
        .collect();

    let seen: HashSet<(String, &'static str, &str)> =
        before_findings.iter().map(finding_key).collect();
    let mut fresh: HashMap<NodeKey, Vec<Finding>> = HashMap::new();
    for f in after_findings {
        if !seen.contains(&finding_key(f)) {
            fresh.entry(f.node.clone()).or_default().push(f.clone());
        }
    }

    let mut out: Vec<NodeDelta> = Vec::new();
    for (key, node) in new.iter() {
        let (change, changed) = match old.get(key) {
            None => (Change::Added, Vec::new()),
            Some(was) => {
                let changed = aspects(was, node);
                if changed.is_empty() {
                    (Change::Unchanged, changed)
                } else {
                    (Change::Modified, changed)
                }
            }
        };
        out.push(delta(node, change, changed, fresh.remove(key)));
    }
    // The removals — the half a live graph cannot show. Their findings are
    // not carried: a node that no longer exists cannot be newly wrong, and
    // what its deletion broke is reported on whatever still points at it.
    for (key, node) in old.iter() {
        if !new.contains_key(key) {
            out.push(delta(node, Change::Removed, Vec::new(), None));
        }
    }

    out.sort_by(|a, b| {
        (a.change, &a.file, a.span.map(|s| s.start), a.key()).cmp(&(
            b.change,
            &b.file,
            b.span.map(|s| s.start),
            b.key(),
        ))
    });
    out
}

fn delta(
    node: &Node,
    change: Change,
    changed: Vec<Aspect>,
    findings: Option<Vec<Finding>>,
) -> NodeDelta {
    NodeDelta {
        node: NodeKeyFields {
            kind: node.key.kind.clone(),
            id: node.key.id.clone(),
        },
        change,
        title: node.title.to_string(),
        file: node.anchor.file.clone(),
        span: Some(node.anchor.span),
        changed,
        findings: findings.unwrap_or_default(),
    }
}

/// An edge's identity: its endpoints, why it exists, and — for a pin — the
/// index level that holds it, since re-pinning a unit from a parent index to
/// a sub-index is a real move and not a no-op.
type EdgeId = (String, String, &'static str, Option<String>);

fn edge_id(e: &Edge) -> EdgeId {
    (
        e.from.to_string(),
        e.to.to_string(),
        e.kind.as_str(),
        e.index_id.clone(),
    )
}

fn diff_edges(before: &Graph, after: &Graph) -> Vec<EdgeDelta> {
    let old: BTreeSet<EdgeId> = before.edges.iter().map(edge_id).collect();
    let new: BTreeSet<EdgeId> = after.edges.iter().map(edge_id).collect();
    let mut out: Vec<EdgeDelta> = after
        .edges
        .iter()
        .map(|e| {
            edge_delta(
                e,
                if old.contains(&edge_id(e)) {
                    Change::Unchanged
                } else {
                    Change::Added
                },
            )
        })
        .collect();
    out.extend(
        before
            .edges
            .iter()
            .filter(|e| !new.contains(&edge_id(e)))
            .map(|e| edge_delta(e, Change::Removed)),
    );
    out.sort_by(|a, b| {
        (a.change, &a.from, &a.to, a.kind.as_str(), &a.index_id).cmp(&(
            b.change,
            &b.from,
            &b.to,
            b.kind.as_str(),
            &b.index_id,
        ))
    });
    out
}

fn edge_delta(e: &Edge, change: Change) -> EdgeDelta {
    EdgeDelta {
        from: e.from.clone(),
        to: e.to.clone(),
        kind: e.kind,
        index_id: e.index_id.clone(),
        change,
    }
}

fn summarize(nodes: &[NodeDelta], edges: &[EdgeDelta]) -> Summary {
    let mut s = Summary::default();
    for n in nodes {
        // `index` is the kind an index node carries in the model, so the
        // split is read off the identity every node already has rather than
        // a second flag two constructions could disagree about.
        let counts = if n.node.kind == INDEX_KIND {
            &mut s.indexes
        } else {
            &mut s.units
        };
        match n.change {
            Change::Added => counts.added += 1,
            Change::Removed => counts.removed += 1,
            Change::Modified => counts.modified += 1,
            Change::Unchanged => {}
        }
    }
    for e in edges {
        match e.change {
            Change::Added => s.edges.added += 1,
            Change::Removed => s.edges.removed += 1,
            _ => {}
        }
    }
    s
}

/// One side's health reading.
struct Measure {
    key: &'static str,
    label: &'static str,
    value: f64,
    ratio: bool,
}

/// The eight measurements an audit header carries, in reading order.
///
/// They are deliberately a small, fixed set: a header strip that grows with
/// the rule table stops being a header. Six come straight off the lint pass
/// (so a rule and its metric cannot disagree about what they count) and two
/// are structural facts the rules do not screen for.
fn measure(graph: &Graph, findings: &[Finding]) -> Vec<Measure> {
    let of_rule = |rule: Rule| findings.iter().filter(|f| f.rule == rule).count() as f64;
    let hubs: HashSet<&NodeKey> = findings
        .iter()
        .filter(|f| matches!(f.rule, Rule::LinkDensity | Rule::NamePrefixCluster))
        .map(|f| &f.node)
        .collect();
    let units = graph.units.len();
    let related_edges = graph
        .edges
        .iter()
        .filter(|e| e.kind == EdgeKind::Related)
        .count();
    let reasonless = graph
        .units
        .iter()
        .flat_map(|u| u.related.iter())
        .filter(|l| l.why.is_none())
        .count();
    vec![
        Measure {
            key: "errors",
            label: "errors",
            value: findings
                .iter()
                .filter(|f| f.severity == Severity::Error)
                .count() as f64,
            ratio: false,
        },
        Measure {
            key: "unindexed_units",
            label: "units no index pins",
            value: of_rule(Rule::Unindexed),
            ratio: false,
        },
        Measure {
            key: "unprojected_units",
            label: "units no projection renders",
            value: of_rule(Rule::NoProjection),
            ratio: false,
        },
        Measure {
            key: "over_cap_units",
            label: "units over the link cap",
            value: of_rule(Rule::RelatedOverCap),
            ratio: false,
        },
        Measure {
            key: "hub_units",
            label: "hub-shaped units",
            value: hubs.len() as f64,
            ratio: false,
        },
        Measure {
            key: "bodyless_indexes",
            label: "indexes with no body",
            value: of_rule(Rule::BodylessIndex),
            ratio: false,
        },
        Measure {
            key: "reasonless_edges",
            label: "links with no reason",
            value: reasonless as f64,
            ratio: false,
        },
        Measure {
            key: "edges_per_unit",
            label: "links per unit",
            value: if units == 0 {
                0.0
            } else {
                related_edges as f64 / units as f64
            },
            ratio: true,
        },
    ]
}

/// Pair the two sides' measurements. They are produced by one function, so
/// the two lists are the same metrics in the same order by construction.
fn health(before: Vec<Measure>, after: Vec<Measure>) -> Vec<Metric> {
    before
        .into_iter()
        .zip(after)
        .map(|(b, a)| Metric {
            key: a.key,
            label: a.label,
            before: b.value,
            after: a.value,
            ratio: a.ratio,
            worse: a.value > b.value,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testsupport::{mini_wskill, write};

    /// Read the fixture as it stands, then apply `edit` and read it again —
    /// the two graphs an audit compares, without a git repo in the way.
    fn audit_after(td: &tempfile::TempDir, edit: impl FnOnce(&Path)) -> Audit {
        let before = Graph::open(td.path()).expect("before");
        edit(td.path());
        let after = Graph::open(td.path()).expect("after");
        Audit::of(&before, &after)
    }

    fn node<'a>(audit: &'a Audit, id: &str) -> &'a NodeDelta {
        audit
            .nodes
            .iter()
            .find(|n| n.node.id == id)
            .unwrap_or_else(|| panic!("no node `{id}` in {:#?}", audit.nodes))
    }

    fn metric<'a>(audit: &'a Audit, key: &str) -> &'a Metric {
        audit.health.iter().find(|m| m.key == key).expect("metric")
    }

    #[test]
    fn the_range_defaults_to_the_previous_commit() {
        assert_eq!(
            Range::parse(""),
            Range {
                before: Endpoint::Rev("HEAD~1".into()),
                after: None
            }
        );
        // A bare revision and an open-ended range mean the same thing: the
        // working tree is the other side.
        assert_eq!(Range::parse("HEAD~3"), Range::parse("HEAD~3.."));
        assert_eq!(
            Range::parse("v1.0..v2.0"),
            Range {
                before: Endpoint::Rev("v1.0".into()),
                after: Some("v2.0".into())
            }
        );
        assert_eq!(
            Range::parse("..topic"),
            Range {
                before: Endpoint::Rev("HEAD".into()),
                after: Some("topic".into())
            }
        );
    }

    /// `a...b` is the branch-review range, and its baseline is the merge
    /// base — not `a`, which has moved on since the branch left it.
    #[test]
    fn a_three_dot_range_is_a_merge_base() {
        assert_eq!(
            Range::parse("main...topic"),
            Range {
                before: Endpoint::MergeBase("main".into(), "topic".into()),
                after: Some("topic".into())
            }
        );
        // Open-ended: the merge base with HEAD, compared against the
        // uncommitted working tree.
        assert_eq!(
            Range::parse("main..."),
            Range {
                before: Endpoint::MergeBase("main".into(), "HEAD".into()),
                after: None
            }
        );
    }

    /// The union graph: a removed unit is still a node, marked removed —
    /// the whole reason the audit is not the after-graph.
    #[test]
    fn a_removed_unit_and_its_edges_survive_as_removals() {
        let td = mini_wskill();
        let audit = audit_after(&td, |root| {
            write(root, "data/concepts/beta.wcl", "");
            // `alpha` linked to `beta`; drop the link so the removal is the
            // only news.
            write(
                root,
                "data/concepts/alpha.wcl",
                "concept alpha {\n  name = \"Alpha\"\n}\n",
            );
            write(
                root,
                "data/indexes.wcl",
                "index lang {\n  name = \"Language\"\n  related = [alpha]\n}\n",
            );
        });

        let beta = node(&audit, "beta");
        assert_eq!(beta.change, Change::Removed);
        assert_eq!(beta.title, "Beta");
        assert_eq!(beta.file, Path::new("data/concepts/beta.wcl"));
        assert_eq!(audit.summary.units.removed, 1);

        // Both edges into it are removals, and both are still in the model.
        let removed: Vec<String> = audit
            .edges
            .iter()
            .filter(|e| e.change == Change::Removed)
            .map(|e| format!("{} {} {}", e.from, e.kind, e.to))
            .collect();
        assert_eq!(
            removed,
            [
                "concept:alpha related concept:beta",
                "index:lang pin concept:beta"
            ]
        );
        assert_eq!(audit.summary.edges.removed, 2);
    }

    /// The measured motivation: units land unpinned and nothing says so at
    /// the time. An added unit carries its own findings.
    #[test]
    fn an_added_unit_carries_its_new_findings() {
        let td = mini_wskill();
        let audit = audit_after(&td, |root| {
            write(
                root,
                "data/concepts/main.wcl",
                "import \"./alpha.wcl\"\nimport \"./beta.wcl\"\nimport \"./gamma.wcl\"\n\
                 import \"./delta.wcl\"\n",
            );
            write(
                root,
                "data/concepts/delta.wcl",
                "concept delta {\n  name = \"Delta\"\n}\n",
            );
        });

        let delta = node(&audit, "delta");
        assert_eq!(delta.change, Change::Added);
        assert_eq!(
            delta.findings.iter().map(|f| f.rule).collect::<Vec<_>>(),
            [Rule::Unindexed]
        );
        assert_eq!(audit.summary.units.added, 1);
        assert_eq!(metric(&audit, "unindexed_units").before, 1.0);
        assert_eq!(metric(&audit, "unindexed_units").after, 2.0);
        assert!(metric(&audit, "unindexed_units").worse);
    }

    /// Findings are scoped to the range, not to the corpus: a unit that was
    /// already unpinned and still is says nothing, because nothing about it
    /// is news.
    #[test]
    fn a_standing_finding_is_not_the_ranges_news() {
        let td = mini_wskill();
        let audit = audit_after(&td, |root| {
            write(
                root,
                "data/concepts/alpha.wcl",
                "concept alpha {\n  name = \"Alpha renamed\"\n  related = [beta]\n}\n",
            );
        });
        // `gamma` is unpinned in both revisions — a standing warning, and
        // not this range's doing.
        let gamma = node(&audit, "gamma");
        assert_eq!(gamma.change, Change::Unchanged);
        assert!(gamma.findings.is_empty());
        assert!(!gamma.is_news());
        assert!(!audit.news().any(|n| n.node.id == "gamma"));
    }

    /// The other half of scoping: an untouched unit the range *broke* is
    /// news, even though its own source did not change.
    #[test]
    fn a_unit_broken_by_someone_elses_deletion_is_news() {
        let td = mini_wskill();
        let audit = audit_after(&td, |root| {
            // Delete `beta` and leave `alpha`'s link to it dangling.
            write(root, "data/concepts/beta.wcl", "");
            write(
                root,
                "data/indexes.wcl",
                "index lang {\n  name = \"Language\"\n  related = [alpha]\n}\n",
            );
        });
        let alpha = node(&audit, "alpha");
        assert_eq!(alpha.change, Change::Unchanged, "alpha's source is intact");
        assert_eq!(
            alpha.findings.iter().map(|f| f.rule).collect::<Vec<_>>(),
            [Rule::DanglingRelated]
        );
        assert!(alpha.is_news());
        assert_eq!(metric(&audit, "errors").after, 1.0);
    }

    /// A modified node names which part of it moved — the reviewer's first
    /// question about a `~` row.
    #[test]
    fn a_modified_node_names_what_changed() {
        let td = mini_wskill();
        let audit = audit_after(&td, |root| {
            write(
                root,
                "data/concepts/alpha.wcl",
                "concept alpha {\n  name = \"Alpha, renamed\"\n  audience = :ai\n  \
                 related = [beta, gamma]\n\n  body {\n    p \"Rewritten entirely.\"\n  }\n}\n",
            );
        });
        let alpha = node(&audit, "alpha");
        assert_eq!(alpha.change, Change::Modified);
        assert_eq!(
            alpha.changed,
            [
                Aspect::Title,
                Aspect::Audience,
                Aspect::Related,
                Aspect::Body
            ]
        );
        assert_eq!(alpha.title, "Alpha, renamed");
        // The link it grew is reported under it.
        let grew: Vec<String> = audit
            .edge_news(&alpha.key())
            .map(|e| format!("{} {}", e.change.marker(), e.to))
            .collect();
        assert_eq!(grew, ["+ research:gamma"]);
    }

    /// A file the formatter touched but the author did not is not a change:
    /// the audit compares the model, so a span shift alone says nothing.
    #[test]
    fn a_reformat_alone_is_not_a_change() {
        let td = mini_wskill();
        let audit = audit_after(&td, |root| {
            write(
                root,
                "data/concepts/beta.wcl",
                "\n\nconcept beta {\n\n  name    = \"Beta\"\n\n}\n",
            );
        });
        assert_eq!(node(&audit, "beta").change, Change::Unchanged);
        assert!(
            audit.news().count() == 0,
            "{:#?}",
            audit.news().collect::<Vec<_>>()
        );
    }

    /// Moving a unit between files is a change worth naming — nothing else
    /// about it differs, and a reviewer reading a file diff sees a deletion
    /// and an unrelated addition.
    #[test]
    fn a_unit_that_moved_file_is_modified_not_replaced() {
        let td = mini_wskill();
        let audit = audit_after(&td, |root| {
            write(root, "data/concepts/beta.wcl", "");
            write(
                root,
                "data/concepts/gamma.wcl",
                "research gamma {\n  name = \"Gamma\"\n}\n\nconcept beta {\n  name = \"Beta\"\n}\n",
            );
        });
        let beta = node(&audit, "beta");
        assert_eq!(beta.change, Change::Modified);
        assert_eq!(beta.changed, [Aspect::File]);
        assert_eq!(beta.file, Path::new("data/concepts/gamma.wcl"));
    }

    /// Health is a fixed strip of comparable numbers, each oriented so that
    /// lower is better.
    #[test]
    fn health_pairs_both_revisions_and_marks_the_worse_end() {
        let td = mini_wskill();
        let audit = audit_after(&td, |root| {
            write(
                root,
                "data/concepts/alpha.wcl",
                "concept alpha {\n  name = \"Alpha\"\n  related = [beta, gamma]\n}\n",
            );
        });
        let links = metric(&audit, "edges_per_unit");
        assert!(links.ratio);
        assert_eq!(links.format(links.before), "0.33");
        assert_eq!(links.format(links.after), "0.67");
        assert!(links.worse && links.moved());
        // Every metric is present on both sides, in one fixed order.
        let keys: Vec<&str> = audit.health.iter().map(|m| m.key).collect();
        assert_eq!(keys.len(), 8);
        assert!(keys.contains(&"errors") && keys.contains(&"reasonless_edges"));
    }

    /// The union graph is the whole of both revisions — an unchanged node is
    /// still a node, because the audit view draws a graph and not a list.
    #[test]
    fn every_node_of_either_revision_is_in_the_model() {
        let td = mini_wskill();
        let audit = audit_after(&td, |root| {
            write(root, "data/concepts/beta.wcl", "");
            write(
                root,
                "data/concepts/main.wcl",
                "import \"./alpha.wcl\"\nimport \"./beta.wcl\"\nimport \"./gamma.wcl\"\n\
                 import \"./delta.wcl\"\n",
            );
            write(
                root,
                "data/concepts/delta.wcl",
                "concept delta {\n  name = \"Delta\"\n}\n",
            );
        });
        let ids: BTreeSet<&str> = audit.nodes.iter().map(|n| n.node.id.as_str()).collect();
        assert_eq!(
            ids,
            BTreeSet::from(["alpha", "beta", "gamma", "delta", "lang"])
        );
        // Added first, then removals, then modifications — reading order.
        let changes: Vec<Change> = audit.news().map(|n| n.change).collect();
        assert!(changes.windows(2).all(|w| w[0] <= w[1]), "{changes:?}");
    }

    /// The wire shape a consumer parses.
    #[test]
    fn an_audit_serializes_with_its_range_and_its_rows() {
        let td = mini_wskill();
        let audit = audit_after(&td, |root| {
            write(root, "data/concepts/beta.wcl", "");
        });
        let json = serde_json::to_value(&audit).expect("json");
        assert_eq!(json["entry"], "wskill.wcl");
        assert!(json["after"].is_null(), "the working tree has no sha");
        assert_eq!(json["summary"]["units"]["removed"], 1);
        let beta = json["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .find(|n| n["id"] == "beta")
            .expect("beta");
        assert_eq!(beta["kind"], "concept");
        assert_eq!(beta["change"], "removed");
        assert_eq!(beta["file"], "data/concepts/beta.wcl");
        let edge = json["edges"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["to"] == "concept:beta" && e["kind"] == "pin")
            .expect("the pin edge");
        assert_eq!(edge["change"], "removed");
        assert_eq!(json["health"][0]["key"], "errors");
        assert!(json["health"][0]["worse"].is_boolean());
    }
}
