//! `@auto_id` schema decorator — assigns deterministic inline IDs to blocks
//! whose schema opts in and that the user left anonymous.
//!
//! Without this, two sibling blocks of the same kind without `inline_id`
//! collide in the scope map (see `scope.rs:add_entry`) and only the first
//! survives. `@auto_id` mints `{kind}_{n}` ids so every anonymous sibling
//! ends up with a distinct scope key.

use std::collections::{HashMap, HashSet};

use crate::eval::NamespaceAliases;
use crate::lang::ast::{BodyItem, DocItem, Document, IdentifierLit, InlineId, Schema, StringPart};

/// Walk the document and assign `{kind}_{n}` inline IDs to anonymous blocks
/// whose kind is in the opt-in set. Counters are per `(parent scope, kind)`.
/// Called between Phase 6 (partial merge) and Phase 7 (evaluation).
///
/// `aliases` is the `use`-alias map produced by namespace resolution; we use
/// it to match a block written as `p` against a schema defined as `wdoc::p`.
pub fn assign_auto_ids(doc: &mut Document, aliases: &NamespaceAliases) {
    let schemas = collect_auto_id_schemas(doc);
    if schemas.is_empty() {
        return;
    }
    let mut counters: HashMap<String, u32> = HashMap::new();
    let taken = collect_taken_doc_ids(&doc.items);
    for item in &mut doc.items {
        visit_doc_item(item, &schemas, aliases, &mut counters, &taken);
    }
}

/// Collect top-level inline ids visible from the document root.
fn collect_taken_doc_ids(items: &[DocItem]) -> HashSet<String> {
    let mut set = HashSet::new();
    for item in items {
        if let DocItem::Body(BodyItem::Block(b)) = item {
            if let Some(InlineId::Literal(lit)) = &b.inline_id {
                set.insert(lit.value.clone());
            }
        }
    }
    set
}

/// Resolve a block kind through the alias map. `p` → `wdoc::p` when
/// `use wdoc::{p}` is in scope. Falls back to the original name when no
/// alias applies.
fn resolve_kind(kind: &str, aliases: &NamespaceAliases) -> String {
    aliases
        .aliases
        .get(kind)
        .cloned()
        .unwrap_or_else(|| kind.to_string())
}

fn collect_auto_id_schemas(doc: &Document) -> HashSet<String> {
    let mut set = HashSet::new();
    for item in &doc.items {
        collect_from_doc_item(item, &mut set);
    }
    set
}

fn collect_from_doc_item(item: &DocItem, set: &mut HashSet<String>) {
    match item {
        DocItem::Body(BodyItem::Schema(schema)) => {
            if schema_has_auto_id(schema) {
                if let Some(name) = schema_name(schema) {
                    set.insert(name);
                }
            }
        }
        DocItem::Namespace(ns) => {
            for child in &ns.items {
                collect_from_doc_item(child, set);
            }
        }
        _ => {}
    }
}

fn schema_has_auto_id(schema: &Schema) -> bool {
    schema.decorators.iter().any(|d| d.name.name == "auto_id")
}

fn schema_name(schema: &Schema) -> Option<String> {
    let name: String = schema
        .name
        .parts
        .iter()
        .filter_map(|p| {
            if let StringPart::Literal(s) = p {
                Some(s.as_str())
            } else {
                None
            }
        })
        .collect();
    if name.is_empty() {
        None
    } else {
        Some(name)
    }
}

fn visit_doc_item(
    item: &mut DocItem,
    schemas: &HashSet<String>,
    aliases: &NamespaceAliases,
    counters: &mut HashMap<String, u32>,
    taken: &HashSet<String>,
) {
    match item {
        DocItem::Body(body) => visit_body_item(body, schemas, aliases, counters, taken),
        DocItem::Namespace(ns) => {
            // Namespaces reset the counter namespace — a `p` directly under a
            // namespace shouldn't share numbering with a `p` elsewhere.
            let mut ns_counters: HashMap<String, u32> = HashMap::new();
            let ns_taken = collect_taken_doc_ids(&ns.items);
            for child in &mut ns.items {
                visit_doc_item(child, schemas, aliases, &mut ns_counters, &ns_taken);
            }
        }
        _ => {}
    }
}

fn visit_body_item(
    item: &mut BodyItem,
    schemas: &HashSet<String>,
    aliases: &NamespaceAliases,
    counters: &mut HashMap<String, u32>,
    taken: &HashSet<String>,
) {
    match item {
        BodyItem::Block(block) => {
            let short_kind = block.kind.name.clone();
            let resolved = resolve_kind(&short_kind, aliases);
            if block.inline_id.is_none()
                && (schemas.contains(&resolved) || schemas.contains(&short_kind))
            {
                // Counter is keyed by the resolved kind so different aliases
                // of the same schema share numbering. Skip over numbers that
                // are already in use by an explicit id (including auto-ids
                // minted on a prior pass, which makes retry expansion safe).
                let id = loop {
                    let n = counters.entry(resolved.clone()).or_insert(0);
                    *n += 1;
                    let candidate = format!("{}_{}", short_kind, *n);
                    if !taken.contains(&candidate) {
                        break candidate;
                    }
                };
                block.inline_id = Some(InlineId::Literal(IdentifierLit {
                    value: id,
                    span: block.span,
                }));
            }
            // Recurse into children with a fresh counter scope — nested blocks
            // get their own numbering. Re-scan taken ids for the child body so
            // nested auto-ids can't collide with user-written siblings.
            let mut child_counters: HashMap<String, u32> = HashMap::new();
            let child_taken = collect_taken_ids(&block.body);
            for child in &mut block.body {
                visit_body_item(child, schemas, aliases, &mut child_counters, &child_taken);
            }
        }
        // Skip ForLoop and Conditional bodies entirely. If these are still
        // present after Phase 5, they'll be expanded in Phase 7a; running
        // auto-id over the unexpanded *template* body would stamp the same
        // id onto every clone and trigger E030. Auto-id runs again after
        // the retry expansion when the clones exist as independent blocks.
        BodyItem::ForLoop(_) | BodyItem::Conditional(_) => {}
        _ => {}
    }
}

/// Collect all inline ids present in a body slice. Used to avoid minting
/// a fresh auto-id that collides with a user-written or previously-minted
/// id when auto-id runs a second time (after the for-loop retry pass).
fn collect_taken_ids(body: &[BodyItem]) -> HashSet<String> {
    let mut set = HashSet::new();
    for item in body {
        if let BodyItem::Block(b) = item {
            if let Some(InlineId::Literal(lit)) = &b.inline_id {
                set.insert(lit.value.clone());
            }
        }
    }
    set
}

#[cfg(test)]
mod tests {
    use crate::{parse, ParseOptions};

    fn eval_ok(src: &str) -> indexmap::IndexMap<String, crate::Value> {
        let doc = parse(src, ParseOptions::default());
        let errs: Vec<_> = doc.diagnostics.iter().filter(|d| d.is_error()).collect();
        assert!(errs.is_empty(), "unexpected errors: {:?}", errs);
        doc.values
    }

    #[test]
    fn anonymous_siblings_get_distinct_auto_ids() {
        let values = eval_ok(
            r#"
            @auto_id
            schema "p" { content: string @text }
            p "a"
            p "b"
            p "c"
            "#,
        );
        let p1 = values.get("p_1").expect("p_1 present");
        let p2 = values.get("p_2").expect("p_2 present");
        let p3 = values.get("p_3").expect("p_3 present");
        assert!(matches!(p1, crate::Value::BlockRef(_)));
        assert!(matches!(p2, crate::Value::BlockRef(_)));
        assert!(matches!(p3, crate::Value::BlockRef(_)));
    }

    #[test]
    fn explicit_id_wins_over_auto_id() {
        let values = eval_ok(
            r#"
            @auto_id
            schema "p" { content: string @text }
            p "a"
            p mine "b"
            p "c"
            "#,
        );
        assert!(values.contains_key("p_1"));
        assert!(values.contains_key("mine"));
        assert!(values.contains_key("p_2")); // counter skips the explicit one
        assert!(!values.contains_key("p_3"));
    }

    #[test]
    fn schemas_without_auto_id_still_collide() {
        // Backwards-compat: opting out keeps today's first-wins behaviour.
        let values = eval_ok(
            r#"
            schema "p" { content: string @text }
            p "a"
            p "b"
            "#,
        );
        // Only one entry under the synthetic scope key.
        let collisions: Vec<_> = values
            .keys()
            .filter(|k| k.contains("__block_p") || k == &"p")
            .collect();
        assert!(
            collisions.len() <= 1,
            "expected at most one entry for colliding anonymous blocks, got {:?}",
            collisions
        );
    }

    #[test]
    fn for_loop_over_block_query_expands_at_retry() {
        let values = eval_ok(
            r#"
@auto_id
schema "marker" { content: string @text }

schema "foo" {
    id: identifier @inline(0)
    port: i64
}

foo alpha { port = 80 }
foo beta  { port = 81 }

for x in (..foo) {
    marker "${x.id}:${x.port}"
}
"#,
        );
        let m1 = values.get("marker_1").expect("marker_1");
        let m2 = values.get("marker_2").expect("marker_2");
        let content = |v: &crate::Value| match v {
            crate::Value::BlockRef(br) => match br.attributes.get("content") {
                Some(crate::Value::String(s)) => s.clone(),
                _ => "<no content>".into(),
            },
            _ => "<not a block>".into(),
        };
        let contents = [content(m1), content(m2)];
        assert!(
            contents.iter().any(|s| s == "alpha:80"),
            "got {:?}",
            contents
        );
        assert!(
            contents.iter().any(|s| s == "beta:81"),
            "got {:?}",
            contents
        );
    }

    #[test]
    fn unresolved_for_loop_iterable_emits_e105() {
        use crate::{parse, ParseOptions};
        // A bare-name iterable that doesn't exist as a let binding, table,
        // or block. Phase 5 leaves it alone (tolerates missing); retry
        // runs in strict mode and emits E105.
        let src = r#"
@auto_id
schema "marker" { content: string @text }
for x in not_a_real_name {
    marker "${x.id}"
}
"#;
        let doc = parse(src, ParseOptions::default());
        let has_e105 = doc
            .diagnostics
            .iter()
            .any(|d| d.code.as_deref() == Some("E105"));
        assert!(
            has_e105,
            "expected E105 diagnostic, got: {:?}",
            doc.diagnostics
                .iter()
                .map(|d| &d.message)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn nested_counters_are_per_parent_scope() {
        let values = eval_ok(
            r#"
            schema "outer" { }
            @auto_id
            schema "p" { content: string @text }
            outer a {
                p "x"
                p "y"
            }
            outer b {
                p "z"
            }
            "#,
        );
        // Each `outer` gets its own counter, so `p_1` shows up inside both
        // without colliding.
        let outer_a = match values.get("a") {
            Some(crate::Value::BlockRef(br)) => br,
            _ => panic!("expected block ref"),
        };
        let outer_b = match values.get("b") {
            Some(crate::Value::BlockRef(br)) => br,
            _ => panic!("expected block ref"),
        };
        assert_eq!(outer_a.children.len(), 2, "a should have two children");
        assert_eq!(outer_b.children.len(), 1, "b should have one child");
        assert_eq!(outer_a.children[0].id.as_deref(), Some("p_1"));
        assert_eq!(outer_a.children[1].id.as_deref(), Some("p_2"));
        assert_eq!(outer_b.children[0].id.as_deref(), Some("p_1"));
    }
}
