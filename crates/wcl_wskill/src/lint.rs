//! The rule engine: one pass over a [`Graph`], three severities, one finding
//! shape.
//!
//! **Nomination is a severity, not a separate command.** The line between
//! "mechanically certain" and "needs judgement" is a split in what the
//! *consumer* does with a finding, not in how it is computed: CI reads the
//! errors, the curator reads the candidates, and both run the same pass. So
//! there is one [`Rule`] vocabulary and one [`Finding`], and the only thing a
//! caller chooses is which severities it cares about.
//!
//! **The rule engine never writes.** It takes `&Graph` and returns findings.
//!
//! The three tiers are drawn by how certain a rule is, not by how much it
//! matters:
//!
//! - [`Severity::Error`] — mechanically certain and always wrong. A dangling
//!   id renders no bullet and warns nobody; a duplicate id makes every
//!   id-addressed op ambiguous.
//! - [`Severity::Warn`] — real signal with a real exception rate. The
//!   `related` cap is guidance an author may knowingly break, so it never
//!   fails CI.
//! - [`Severity::Candidate`] — a *nomination*: a screen that cannot decide
//!   without reading the prose. Every candidate rule is thresholded, so a
//!   500-unit wskill nominates about as many units as a 65-unit one — which
//!   is what keeps the curator's read bounded. The accepted cost is a false
//!   negative: a defect below threshold is invisible.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use wcl_lang::Span;

use crate::model::{Anchor, Graph, Index, NodeKey, Unit};

/// The `related` cap for a content unit. The guidance is 3–5 links, so 5 is
/// the last legal value; indexes are explicitly exempt (a curated index is a
/// list of pins by definition).
const RELATED_CAP: usize = 5;

/// Fewest links a unit needs before the hub screens look at it at all. Below
/// this every unit is terse, and the screens would nominate the whole corpus.
const SCREEN_MIN_LINKS: usize = 3;

/// Words per link below which a unit is nominated as hub-shaped. The corpus's
/// terse-but-legitimate units measure 30–56 words over 2–4 links (7.5–28
/// words per link), so this catches the bottom of that band deliberately: the
/// screen cannot separate a hub from a genuinely atomic unit without reading
/// the body, which is exactly why it nominates rather than warns. One unit
/// across the four in-repo wskills sits below it.
const WORDS_PER_LINK_FLOOR: usize = 8;

/// How much of a unit's link set must share the signal before the clustering
/// screen fires, as a fraction of its links.
const CLUSTER_SHARE: f64 = 0.75;

/// Fewest characters of shared id prefix that count as "these targets are
/// named after me" — shorter and every `to_`/`is_` id would cluster.
const PREFIX_MIN: usize = 3;

/// Jaccard similarity over reason word sets at or above which two reasons
/// are "the same reason twice".
const REASON_SIMILARITY: f64 = 0.8;

/// How certain a finding is — and therefore who acts on it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Mechanically certain. CI fails.
    Error,
    /// Real signal, real exception rate. Never fails on its own.
    Warn,
    /// A nomination to the curator. Fails nothing.
    Candidate,
}

impl Severity {
    /// Every severity, most certain first — the order findings sort in.
    pub const ALL: [Severity; 3] = [Severity::Error, Severity::Warn, Severity::Candidate];

    pub fn as_str(&self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warn => "warn",
            Severity::Candidate => "candidate",
        }
    }

    /// The severity a caller named, e.g. in `--severity error,warn`.
    pub fn parse(s: &str) -> Option<Severity> {
        Severity::ALL.into_iter().find(|sev| sev.as_str() == s)
    }
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// What was checked. The slug is the stable name an agent and UI use, so it
/// belongs to the rule, not to any one message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, serde::Serialize)]
#[serde(into = "&'static str")]
pub enum Rule {
    /// A `related` id naming no unit and no index.
    DanglingRelated,
    /// Two nodes declaring the same id — every id-addressed op is ambiguous.
    DuplicateId,
    /// A `related` id naming an index with no body: a dangling link wearing
    /// a valid id, because a body-less index has no page.
    RelatedBodylessIndex,
    /// More `related` links on a content unit than the cap allows.
    RelatedOverCap,
    /// A unit no index pins and no projection organizes structurally.
    Unindexed,
    /// A unit that renders in none of the declared projections.
    NoProjection,
    /// Hub screen: many links over little prose.
    LinkDensity,
    /// Hub screen: the targets are named after the source.
    NamePrefixCluster,
    /// Hub screen: one unit's edges give near-identical reasons.
    DuplicateReason,
    /// An index with no body — nothing can link to it.
    BodylessIndex,
}

impl Rule {
    /// Every rule, in severity order.
    pub const ALL: [Rule; 10] = [
        Rule::DanglingRelated,
        Rule::DuplicateId,
        Rule::RelatedBodylessIndex,
        Rule::RelatedOverCap,
        Rule::Unindexed,
        Rule::NoProjection,
        Rule::LinkDensity,
        Rule::NamePrefixCluster,
        Rule::DuplicateReason,
        Rule::BodylessIndex,
    ];

    pub fn slug(&self) -> &'static str {
        match self {
            Rule::DanglingRelated => "dangling-related",
            Rule::DuplicateId => "duplicate-id",
            Rule::RelatedBodylessIndex => "related-bodyless-index",
            Rule::RelatedOverCap => "related-over-cap",
            Rule::Unindexed => "unindexed",
            Rule::NoProjection => "no-projection",
            Rule::LinkDensity => "link-density",
            Rule::NamePrefixCluster => "name-prefix-cluster",
            Rule::DuplicateReason => "duplicate-reason",
            Rule::BodylessIndex => "bodyless-index",
        }
    }

    /// The severity every finding of this rule carries. A rule's certainty is
    /// a property of the rule, so nothing can emit the same rule at two
    /// severities and leave a consumer guessing.
    pub fn severity(&self) -> Severity {
        match self {
            Rule::DanglingRelated | Rule::DuplicateId | Rule::RelatedBodylessIndex => {
                Severity::Error
            }
            Rule::RelatedOverCap | Rule::Unindexed | Rule::NoProjection => Severity::Warn,
            Rule::LinkDensity
            | Rule::NamePrefixCluster
            | Rule::DuplicateReason
            | Rule::BodylessIndex => Severity::Candidate,
        }
    }

    /// The rule named by a CLI slug.
    pub fn parse(s: &str) -> Option<Rule> {
        Rule::ALL.into_iter().find(|rule| rule.slug() == s)
    }
}

impl From<Rule> for &'static str {
    fn from(r: Rule) -> &'static str {
        r.slug()
    }
}

impl std::fmt::Display for Rule {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.slug())
    }
}

/// One finding: what was checked, about which node, written where.
///
/// The `span` is what lets the editor pin the finding and an agent jump to
/// it; it is optional because a rule may have a node to name but no single
/// place to point at. It addresses the **declaring block**, not the offending
/// element inside it — the model anchors nodes, and a finding about one
/// `related` entry still opens the unit that wrote it.
#[derive(Debug, Clone, serde::Serialize)]
pub struct Finding {
    pub severity: Severity,
    pub rule: Rule,
    /// The node the finding is about — a unit, or an `index`.
    #[serde(rename = "unit", serialize_with = "crate::model::node_key_fields")]
    pub node: NodeKey,
    /// The declaring file, relative to [`Graph::root`].
    pub file: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    pub message: String,
}

impl Finding {
    fn new(rule: Rule, node: NodeKey, anchor: &Anchor, message: String) -> Finding {
        Finding {
            severity: rule.severity(),
            rule,
            node,
            file: anchor.file.clone(),
            span: Some(anchor.span),
            message,
        }
    }
}

/// Run every rule over `graph`, most certain finding first.
///
/// Ordering is total and deterministic — severity, then file, then position,
/// then rule — so two runs of the same model diff cleanly. That is what makes
/// the curator's "you may not make it worse" gate a comparison rather than a
/// set operation.
pub fn lint(graph: &Graph) -> Vec<Finding> {
    let ctx = Ctx::new(graph);
    let mut out = Vec::new();
    ctx.dangling_and_bodyless_targets(&mut out);
    ctx.duplicate_ids(&mut out);
    ctx.over_cap(&mut out);
    ctx.unindexed(&mut out);
    ctx.no_projection(&mut out);
    ctx.link_density(&mut out);
    ctx.clustering(&mut out);
    ctx.duplicate_reasons(&mut out);
    ctx.bodyless_indexes(&mut out);
    out.sort_by(|a, b| {
        (
            a.severity,
            &a.file,
            a.span.map(|s| s.start),
            a.rule,
            &a.node,
        )
            .cmp(&(
                b.severity,
                &b.file,
                b.span.map(|s| s.start),
                b.rule,
                &b.node,
            ))
    });
    out
}

/// One lint pass: the graph plus the id lookups every rule needs.
struct Ctx<'a> {
    graph: &'a Graph,
    /// Every index level by id.
    indexes: HashMap<&'a str, &'a Index>,
    /// Every unit by id (the first declaration wins — a duplicate is its own
    /// finding).
    units: HashMap<&'a str, &'a Unit>,
}

impl<'a> Ctx<'a> {
    fn new(graph: &'a Graph) -> Ctx<'a> {
        let indexes: HashMap<&str, &Index> =
            graph.index_levels().map(|i| (i.id.as_str(), i)).collect();
        let mut units: HashMap<&str, &Unit> = HashMap::new();
        for u in &graph.units {
            units.entry(u.id.as_str()).or_insert(u);
        }
        Ctx {
            graph,
            indexes,
            units,
        }
    }

    /// Whether `id` names any node at all.
    fn resolves(&self, id: &str) -> bool {
        self.units.contains_key(id) || self.indexes.contains_key(id)
    }

    /// The two id-resolution errors, walked together because they are one
    /// question asked of each link: does this id name something a reader can
    /// actually reach?
    fn dangling_and_bodyless_targets(&self, out: &mut Vec<Finding>) {
        for u in &self.graph.units {
            for id in u.related_ids() {
                if !self.resolves(id) {
                    out.push(Finding::new(
                        Rule::DanglingRelated,
                        u.key(),
                        &u.anchor,
                        format!("`related` id `{id}` names no unit or index"),
                    ));
                } else if self.indexes.get(id).is_some_and(|i| !i.has_body()) {
                    out.push(Finding::new(
                        Rule::RelatedBodylessIndex,
                        u.key(),
                        &u.anchor,
                        format!(
                            "`related` id `{id}` names an index with no body, which has no page \
                             to link to"
                        ),
                    ));
                }
            }
        }
        // An index's `related` list is its pins: same question, same rule.
        for idx in self.graph.index_levels() {
            for id in &idx.pinned {
                if !self.resolves(id) {
                    out.push(Finding::new(
                        Rule::DanglingRelated,
                        idx.key(),
                        &idx.anchor,
                        format!("pinned id `{id}` names no unit or index"),
                    ));
                }
            }
        }
    }

    /// Ids declared more than once, across kinds and across units/indexes
    /// alike: `related` resolves against one flat namespace, so a repeat
    /// makes every link and every id-addressed op ambiguous.
    fn duplicate_ids(&self, out: &mut Vec<Finding>) {
        let mut seen: HashMap<&str, (NodeKey, &Anchor)> = HashMap::new();
        let nodes = self
            .graph
            .units
            .iter()
            .map(|u| (u.id.as_str(), u.key(), &u.anchor))
            .chain(
                self.graph
                    .index_levels()
                    .map(|i| (i.id.as_str(), i.key(), &i.anchor)),
            );
        for (id, key, anchor) in nodes {
            match seen.get(id) {
                None => {
                    seen.insert(id, (key, anchor));
                }
                Some((first, first_anchor)) => out.push(Finding::new(
                    Rule::DuplicateId,
                    key,
                    anchor,
                    format!(
                        "id `{id}` is already declared by `{first}` at {}",
                        first_anchor.file.display()
                    ),
                )),
            }
        }
    }

    fn over_cap(&self, out: &mut Vec<Finding>) {
        for u in &self.graph.units {
            let n = u.related.len();
            if n > RELATED_CAP {
                out.push(Finding::new(
                    Rule::RelatedOverCap,
                    u.key(),
                    &u.anchor,
                    format!("{n} `related` links, over the cap of {RELATED_CAP}"),
                ));
            }
        }
    }

    fn unindexed(&self, out: &mut Vec<Finding>) {
        for u in self.graph.unindexed() {
            out.push(Finding::new(
                Rule::Unindexed,
                u.key(),
                &u.anchor,
                "no index pins this unit — it is reachable only by link".to_string(),
            ));
        }
    }

    /// A unit that renders nowhere. Silent when the folder declares no
    /// projections at all: with nothing to route to, every unit would fire
    /// and the rule would be saying something about the registry, not the
    /// units.
    fn no_projection(&self, out: &mut Vec<Finding>) {
        if self.graph.views.is_empty() {
            return;
        }
        for u in &self.graph.units {
            if !self.graph.views.iter().any(|v| u.shows_in(v)) {
                out.push(Finding::new(
                    Rule::NoProjection,
                    u.key(),
                    &u.anchor,
                    format!(
                        "renders in none of the {} declared projections (audience `{}`{})",
                        self.graph.views.len(),
                        u.audience,
                        if u.visibility.except_sites.is_empty() {
                            String::new()
                        } else {
                            format!(", hidden from {}", u.visibility.except_sites.join(", "))
                        }
                    ),
                ));
            }
        }
    }

    /// Words per link: many links over little prose is the shape of a hub
    /// note. It cannot tell a hub from a genuinely atomic unit, which is why
    /// it nominates.
    fn link_density(&self, out: &mut Vec<Finding>) {
        for u in &self.graph.units {
            let links = u.related.len();
            if links < SCREEN_MIN_LINKS {
                continue;
            }
            let per_link = u.words / links;
            if per_link < WORDS_PER_LINK_FLOOR {
                out.push(Finding::new(
                    Rule::LinkDensity,
                    u.key(),
                    &u.anchor,
                    format!(
                        "{links} links over {} words ({per_link} per link) — hub-shaped; read \
                         the body",
                        u.words
                    ),
                ));
            }
        }
    }

    /// The second hub screen: most of the targets are named after the
    /// source, which is what a family of members hanging off one label looks
    /// like — a table of contents wearing a unit's clothes.
    ///
    /// Co-membership is not evidence by itself: reasons now explain whether a
    /// link adds meaning beyond two units sharing an index.
    fn clustering(&self, out: &mut Vec<Finding>) {
        for u in &self.graph.units {
            let links = u.related.len();
            if links < SCREEN_MIN_LINKS {
                continue;
            }
            let threshold = (links as f64 * CLUSTER_SHARE).ceil() as usize;
            let prefixed = u
                .related_ids()
                .filter(|id| shares_prefix(&u.id, id))
                .count();
            if prefixed >= threshold {
                out.push(Finding::new(
                    Rule::NamePrefixCluster,
                    u.key(),
                    &u.anchor,
                    format!(
                        "{prefixed} of {links} targets are named after it — hub-shaped; read \
                         the body"
                    ),
                ));
            }
        }
    }

    /// A hub's reasons come out near-identical, which is mechanically
    /// detectable in a way hub-ness itself is not.
    fn duplicate_reasons(&self, out: &mut Vec<Finding>) {
        for u in &self.graph.units {
            let reasons: Vec<(&str, HashSet<String>)> = u
                .related
                .iter()
                .filter_map(|l| Some((l.id.as_str(), words_of(l.why.as_deref()?))))
                .filter(|(_, w)| !w.is_empty())
                .collect();
            let mut pairs: Vec<String> = Vec::new();
            for (i, (a_id, a)) in reasons.iter().enumerate() {
                for (b_id, b) in &reasons[i + 1..] {
                    if jaccard(a, b) >= REASON_SIMILARITY {
                        pairs.push(format!("`{a_id}`/`{b_id}`"));
                    }
                }
            }
            if !pairs.is_empty() {
                out.push(Finding::new(
                    Rule::DuplicateReason,
                    u.key(),
                    &u.anchor,
                    format!(
                        "near-identical reasons on {} ({}) — one reason repeated is a hub's \
                         signature",
                        if pairs.len() == 1 { "a pair" } else { "pairs" },
                        pairs.join(", ")
                    ),
                ));
            }
        }
    }

    /// An index with no body is a nav heading: correct by design, but
    /// nothing can link to it. The nomination is "should this area be
    /// linkable?", which only an author can answer.
    fn bodyless_indexes(&self, out: &mut Vec<Finding>) {
        for idx in self.graph.index_levels() {
            if !idx.has_body() {
                out.push(Finding::new(
                    Rule::BodylessIndex,
                    idx.key(),
                    &idx.anchor,
                    "no body, so it has no page and nothing can link to it".to_string(),
                ));
            }
        }
    }
}

/// Whether two ids share a leading name segment: `wdoc_theme` and `wdoc_css`
/// do, `parser` and `parse_tree` do not — a segment is delimited, not a
/// character prefix, or every id starting with the same three letters would
/// cluster.
///
/// One trailing plural is stripped, because naming the family after its
/// members (`themes` over `theme_nord`, `theme_paper`, …) is the hub shape
/// this screen exists to nominate, and a segment comparison would miss
/// exactly that.
fn shares_prefix(a: &str, b: &str) -> bool {
    let seg = |s: &str| {
        let head = s.split(['_', '-']).next().unwrap_or("");
        head.strip_suffix('s').unwrap_or(head).to_string()
    };
    let (a, b) = (seg(a), seg(b));
    a.len() >= PREFIX_MIN && a == b
}

/// A phrase's distinct lowercase words, punctuation stripped.
fn words_of(text: &str) -> HashSet<String> {
    text.split_whitespace()
        .map(|w| {
            w.trim_matches(|c: char| !c.is_alphanumeric())
                .to_lowercase()
        })
        .filter(|w| !w.is_empty())
        .collect()
}

/// Set similarity: shared words over total distinct words.
fn jaccard(a: &HashSet<String>, b: &HashSet<String>) -> f64 {
    let union = a.union(b).count();
    if union == 0 {
        return 0.0;
    }
    a.intersection(b).count() as f64 / union as f64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testsupport::{mini_wskill, with_link_reasons, write};

    /// Every finding as `(rule slug, node id)` — the pair that identifies a
    /// finding without pinning the tests to message wording.
    fn found(td: &tempfile::TempDir) -> Vec<(String, String)> {
        let graph = crate::Graph::open(td.path()).expect("graph");
        lint(&graph)
            .into_iter()
            .map(|f| (f.rule.slug().to_string(), f.node.id))
            .collect()
    }

    fn of_rule(found: &[(String, String)], rule: Rule) -> Vec<&str> {
        found
            .iter()
            .filter(|(r, _)| r == rule.slug())
            .map(|(_, id)| id.as_str())
            .collect()
    }

    /// The fixture as authored: a nav index with no body and one unit no
    /// index pins.
    #[test]
    fn the_fixture_reports_its_standing_findings() {
        let td = mini_wskill();
        assert_eq!(
            found(&td),
            [
                ("unindexed".to_string(), "gamma".to_string()),
                ("bodyless-index".to_string(), "lang".to_string()),
            ]
        );
    }

    /// The three errors: an id naming nothing, an id declared twice, and an
    /// id naming an index a reader cannot reach.
    #[test]
    fn a_related_id_naming_nothing_is_an_error() {
        let td = mini_wskill();
        write(
            td.path(),
            "data/concepts/alpha.wcl",
            "concept alpha {\n  name = \"Alpha\"\n  related = [beta, zeta]\n}\n",
        );
        write(
            td.path(),
            "data/indexes.wcl",
            "index lang {\n  name = \"Language\"\n  related = [alpha, beta, nobody]\n}\n",
        );
        let found = found(&td);
        assert_eq!(of_rule(&found, Rule::DanglingRelated), ["alpha", "lang"]);
        assert_eq!(Rule::DanglingRelated.severity(), Severity::Error);
    }

    #[test]
    fn an_id_declared_twice_is_an_error() {
        let td = mini_wskill();
        // A second `alpha`, under another kind — the case a flat id
        // namespace makes ambiguous and nothing else catches.
        write(
            td.path(),
            "data/concepts/gamma.wcl",
            "research alpha {\n  name = \"Alpha again\"\n}\n",
        );
        let found = found(&td);
        assert_eq!(of_rule(&found, Rule::DuplicateId), ["alpha"]);
    }

    #[test]
    fn a_related_id_naming_a_bodyless_index_is_an_error() {
        let td = mini_wskill();
        write(
            td.path(),
            "data/concepts/beta.wcl",
            "concept beta {\n  name = \"Beta\"\n  related = [lang]\n}\n",
        );
        assert_eq!(
            of_rule(&found(&td), Rule::RelatedBodylessIndex),
            ["beta"],
            "a body-less index has no page to link to"
        );

        // Give the index a body and the same link is fine — the rule is
        // about reachability, not about linking to indexes.
        write(
            td.path(),
            "data/indexes.wcl",
            "index lang {\n  name = \"Language\"\n  related = [alpha, beta]\n\n  \
             body {\n    p \"What this area is.\"\n  }\n}\n",
        );
        let found = found(&td);
        assert!(of_rule(&found, Rule::RelatedBodylessIndex).is_empty());
        assert!(of_rule(&found, Rule::BodylessIndex).is_empty());
    }

    /// The three warnings. Each is real signal with a real exception rate,
    /// so none of them may claim certainty.
    #[test]
    fn over_cap_links_warn_but_indexes_are_exempt() {
        let td = mini_wskill();
        write(
            td.path(),
            "data/concepts/beta.wcl",
            "concept beta {\n  name = \"Beta\"\n  \
             related = [a1, a2, a3, a4, a5, a6]\n}\n\
             concept a1 { name = \"A1\" }\nconcept a2 { name = \"A2\" }\n\
             concept a3 { name = \"A3\" }\nconcept a4 { name = \"A4\" }\n\
             concept a5 { name = \"A5\" }\nconcept a6 { name = \"A6\" }\n",
        );
        // The index pins nine units and is not a finding: a curated index is
        // a list of pins by definition.
        write(
            td.path(),
            "data/indexes.wcl",
            "index lang {\n  name = \"Language\"\n  \
             related = [alpha, beta, a1, a2, a3, a4, a5, a6, gamma]\n}\n",
        );
        let found = found(&td);
        assert_eq!(of_rule(&found, Rule::RelatedOverCap), ["beta"]);
        assert_eq!(Rule::RelatedOverCap.severity(), Severity::Warn);
    }

    #[test]
    fn a_unit_no_index_pins_warns() {
        let td = mini_wskill();
        assert_eq!(of_rule(&found(&td), Rule::Unindexed), ["gamma"]);
    }

    #[test]
    fn a_unit_hidden_from_every_projection_warns() {
        let td = mini_wskill();
        write(
            td.path(),
            "data/concepts/beta.wcl",
            "@except(sites = [:book, :skill])\nconcept beta {\n  name = \"Beta\"\n}\n",
        );
        assert_eq!(of_rule(&found(&td), Rule::NoProjection), ["beta"]);
    }

    /// The five candidate screens. They fail nothing, and each is
    /// thresholded — a bigger wskill must not produce a proportionally
    /// bigger nomination list.
    #[test]
    fn many_links_over_little_prose_is_nominated() {
        let td = mini_wskill();
        write(
            td.path(),
            "data/concepts/beta.wcl",
            "concept beta {\n  name = \"Beta\"\n  related = [alpha, gamma, delta]\n}\n\
             concept delta { name = \"Delta\" }\n",
        );
        let f = found(&td);
        assert_eq!(of_rule(&f, Rule::LinkDensity), ["beta"]);
        assert_eq!(Rule::LinkDensity.severity(), Severity::Candidate);

        // The same links over real prose are not: the screen is a ratio.
        let prose = "word ".repeat(40);
        write(
            td.path(),
            "data/concepts/beta.wcl",
            &format!(
                "concept beta {{\n  name = \"Beta\"\n  related = [alpha, gamma, delta]\n\n  \
                 body {{\n    p \"{prose}\"\n  }}\n}}\n\
                 concept delta {{ name = \"Delta\" }}\n"
            ),
        );
        assert!(of_rule(&found(&td), Rule::LinkDensity).is_empty());

        // A code sample is not explanation: the same 40 words as a listing
        // leave the unit as terse as it was.
        write(
            td.path(),
            "data/concepts/beta.wcl",
            &format!(
                "concept beta {{\n  name = \"Beta\"\n  related = [alpha, gamma, delta]\n\n  \
                 body {{\n    code {{ lang = \"wcl\"  source = \"{prose}\" }}\n  }}\n}}\n\
                 concept delta {{ name = \"Delta\" }}\n"
            ),
        );
        assert_eq!(of_rule(&found(&td), Rule::LinkDensity), ["beta"]);
    }

    #[test]
    fn targets_named_after_their_source_are_nominated() {
        let td = mini_wskill();
        let prose = "word ".repeat(40);
        write(
            td.path(),
            "data/concepts/beta.wcl",
            &format!(
                "concept beta {{\n  name = \"Beta\"\n  \
                 related = [beta_one, beta_two, beta_three]\n\n  \
                 body {{\n    p \"{prose}\"\n  }}\n}}\n\
                 concept beta_one {{ name = \"One\" }}\n\
                 concept beta_two {{ name = \"Two\" }}\n\
                 concept beta_three {{ name = \"Three\" }}\n"
            ),
        );
        assert_eq!(of_rule(&found(&td), Rule::NamePrefixCluster), ["beta"]);

        // The family named after its members — the shape of the one hub the
        // corpus measurement actually found (`themes` over `theme_nord`,
        // `theme_paper`, …). A plain segment comparison would miss it.
        write(
            td.path(),
            "data/concepts/beta.wcl",
            &format!(
                "concept betas {{\n  name = \"Betas\"\n  \
                 related = [beta_one, beta_two, beta_three]\n\n  \
                 body {{\n    p \"{prose}\"\n  }}\n}}\n\
                 concept beta_one {{ name = \"One\" }}\n\
                 concept beta_two {{ name = \"Two\" }}\n\
                 concept beta_three {{ name = \"Three\" }}\n"
            ),
        );
        assert_eq!(of_rule(&found(&td), Rule::NamePrefixCluster), ["betas"]);
    }

    #[test]
    fn one_reason_given_twice_is_nominated() {
        let td = mini_wskill();
        with_link_reasons(td.path());
        write(
            td.path(),
            "data/concepts/alpha.wcl",
            "concept alpha {\n  name = \"Alpha\"\n  related = [\n    \
             {id: \"beta\", why: \"it is part of the language core\"},\n    \
             {id: \"gamma\", why: \"it is part of the language core\"},\n  ]\n}\n",
        );
        let f = found(&td);
        assert_eq!(of_rule(&f, Rule::DuplicateReason), ["alpha"]);

        // Two reasons that actually differ are not a finding — and the
        // annotated form still resolves its ids, so nothing dangles.
        write(
            td.path(),
            "data/concepts/alpha.wcl",
            "concept alpha {\n  name = \"Alpha\"\n  related = [\n    \
             {id: \"beta\", why: \"the parser hands its output to this\"},\n    \
             {id: \"gamma\", why: \"measurements behind the cap\"},\n  ]\n}\n",
        );
        let f = found(&td);
        assert!(of_rule(&f, Rule::DuplicateReason).is_empty());
        assert!(of_rule(&f, Rule::DanglingRelated).is_empty(), "{f:?}");
    }

    #[test]
    fn an_index_with_no_body_is_nominated() {
        let td = mini_wskill();
        assert_eq!(of_rule(&found(&td), Rule::BodylessIndex), ["lang"]);
    }

    /// Two passes over one model must agree down to the message: the
    /// curator's gate is "did this get worse?", which is a diff of two runs.
    /// A message naming an index picked out of a hash map would drift.
    #[test]
    fn two_passes_over_one_model_agree() {
        let td = mini_wskill();
        write(
            td.path(),
            "data/indexes.wcl",
            "index lang {\n  name = \"Language\"\n  related = [alpha, beta, delta]\n}\n\n\
             index tooling {\n  name = \"Tooling\"\n  related = [alpha, beta, delta]\n}\n",
        );
        write(
            td.path(),
            "data/concepts/alpha.wcl",
            "concept alpha {\n  name = \"Alpha\"\n  related = [beta, delta]\n}\n",
        );
        write(
            td.path(),
            "data/concepts/beta.wcl",
            "concept beta {\n  name = \"Beta\"\n}\nconcept delta { name = \"Delta\" }\n",
        );
        let graph = crate::Graph::open(td.path()).expect("graph");
        let render = |fs: Vec<Finding>| {
            fs.iter()
                .map(|f| format!("{} {} {} {}", f.severity, f.rule, f.node, f.message))
                .collect::<Vec<_>>()
        };
        assert_eq!(render(lint(&graph)), render(lint(&graph)));
    }

    /// Findings sort by severity first: the certain ones are what a CI log
    /// and an agent read first, whatever else the pass turned up.
    #[test]
    fn findings_sort_most_certain_first() {
        let td = mini_wskill();
        write(
            td.path(),
            "data/concepts/alpha.wcl",
            "concept alpha {\n  name = \"Alpha\"\n  related = [zeta]\n}\n",
        );
        let graph = crate::Graph::open(td.path()).expect("graph");
        let findings = lint(&graph);
        let severities: Vec<Severity> = findings.iter().map(|f| f.severity).collect();
        assert!(
            severities.windows(2).all(|w| w[0] <= w[1]),
            "{severities:?}"
        );
        assert_eq!(findings[0].rule, Rule::DanglingRelated);
        // Every finding carries the node, the file and a span to jump to.
        let first = &findings[0];
        assert_eq!(first.node.to_string(), "concept:alpha");
        assert_eq!(first.file, std::path::Path::new("data/concepts/alpha.wcl"));
        assert!(first.span.expect("a span").end > 0);
    }

    /// A rule's slug and severity are its identity: every rule has both, and
    /// no two share a slug.
    #[test]
    fn every_rule_has_a_distinct_slug() {
        let slugs: std::collections::HashSet<&str> = Rule::ALL.iter().map(|r| r.slug()).collect();
        assert_eq!(slugs.len(), Rule::ALL.len());
        for sev in Severity::ALL {
            assert_eq!(Severity::parse(sev.as_str()), Some(sev));
            assert!(Rule::ALL.iter().any(|r| r.severity() == sev));
        }
        assert_eq!(Severity::parse("nope"), None);
    }

    /// The finding shape a consumer parses: severity, rule, unit identity,
    /// file, span and message.
    #[test]
    fn a_finding_serializes_with_its_unit_identity() {
        let td = mini_wskill();
        let graph = crate::Graph::open(td.path()).expect("graph");
        let f = lint(&graph).remove(0);
        let json = serde_json::to_value(&f).expect("json");
        assert_eq!(json["severity"], "warn");
        assert_eq!(json["rule"], "unindexed");
        assert_eq!(json["unit"]["kind"], "research");
        assert_eq!(json["unit"]["id"], "gamma");
        assert_eq!(json["file"], "data/concepts/gamma.wcl");
        assert!(json["span"]["start"].is_number());
        assert!(json["message"].as_str().expect("message").len() > 10);
    }
}
