//! Namespace resolution — flattens namespace declarations and resolves `use` aliases.
//!
//! After this phase, all names in the AST are fully qualified (e.g. `networking::service`)
//! and `DocItem::Namespace` / `DocItem::Use` wrappers have been removed. Downstream phases
//! work with plain qualified strings and do not need to understand namespaces.

use std::collections::{HashMap, HashSet};

use crate::lang::ast::*;
use crate::lang::diagnostic::DiagnosticBag;

/// Alias mappings produced by `use` declarations.
#[derive(Debug, Default, Clone)]
pub struct NamespaceAliases {
    /// Maps alias name → fully qualified name (e.g. "svc" → "networking::service").
    /// Used for schema, block kind, let binding, and general name resolution.
    pub aliases: HashMap<String, String>,
}

/// Resolve namespaces in-place: qualify names, flatten namespace wrappers, resolve `use` aliases.
///
/// Returns the alias map for downstream evaluator/schema resolution.
pub fn resolve(doc: &mut Document, diagnostics: &mut DiagnosticBag) -> NamespaceAliases {
    let mut aliases = NamespaceAliases::default();

    // Step 1: Handle file-level namespace — wraps all subsequent items.
    handle_file_level_namespace(doc);

    // Step 2: Collect known namespace names and their item names (recursive).
    let ns_items = collect_namespace_items(&doc.items, "");

    // Step 3: Recursively flatten namespaces and resolve use declarations.
    let items = std::mem::take(&mut doc.items);
    doc.items = flatten_items(items, "", &ns_items, &mut aliases, diagnostics);

    aliases
}

/// Recursively flatten namespace items with prefix accumulation.
fn flatten_items(
    items: Vec<DocItem>,
    prefix: &str,
    ns_items: &HashMap<String, HashSet<String>>,
    aliases: &mut NamespaceAliases,
    diagnostics: &mut DiagnosticBag,
) -> Vec<DocItem> {
    let mut result = Vec::new();

    for item in items {
        match item {
            DocItem::Namespace(ns) => {
                let ns_path = join_path(&ns.path);
                let new_prefix = if prefix.is_empty() {
                    ns_path
                } else {
                    format!("{}::{}", prefix, ns_path)
                };
                // Recursively flatten the namespace's items with the accumulated prefix
                let flattened =
                    flatten_items(ns.items, &new_prefix, ns_items, aliases, diagnostics);
                result.extend(flattened);
            }
            DocItem::Use(use_decl) => {
                let ns_path = join_path(&use_decl.namespace_path);
                // If we're inside a namespace, the use path might be relative or absolute.
                // Try absolute first, then prefixed.
                let full_ns = if ns_items.contains_key(&ns_path) {
                    ns_path.clone()
                } else if !prefix.is_empty() {
                    let prefixed = format!("{}::{}", prefix, ns_path);
                    if ns_items.contains_key(&prefixed) {
                        prefixed
                    } else {
                        ns_path.clone()
                    }
                } else {
                    ns_path.clone()
                };

                if !ns_items.contains_key(&full_ns) {
                    diagnostics.error_with_code(
                        format!("namespace `{}` not found", ns_path),
                        use_decl
                            .namespace_path
                            .first()
                            .map(|i| i.span)
                            .unwrap_or(use_decl.span),
                        "E121",
                    );
                    continue;
                }
                let defined = &ns_items[&full_ns];
                for target in &use_decl.targets {
                    let qualified_name = format!("{}::{}", full_ns, target.name.name);
                    if !defined.contains(&target.name.name) {
                        diagnostics.error_with_code(
                            format!(
                                "`{}` not found in namespace `{}`",
                                target.name.name, full_ns
                            ),
                            target.name.span,
                            "E120",
                        );
                        continue;
                    }
                    let local_name = target
                        .alias
                        .as_ref()
                        .map(|a| a.name.clone())
                        .unwrap_or_else(|| target.name.name.clone());
                    aliases.aliases.insert(local_name, qualified_name);
                }
            }
            mut other => {
                if !prefix.is_empty() {
                    qualify_doc_item(&mut other, prefix);
                }
                result.push(other);
            }
        }
    }

    result
}

/// If the first non-trivia item is a file-level `namespace foo::bar`, wrap all subsequent items
/// in a braced namespace and convert the file-level form.
fn handle_file_level_namespace(doc: &mut Document) {
    let ns_idx = doc
        .items
        .iter()
        .position(|item| matches!(item, DocItem::Namespace(ns) if ns.file_level));

    if let Some(idx) = ns_idx {
        if let DocItem::Namespace(mut ns) = doc.items.remove(idx) {
            let remaining: Vec<DocItem> = doc.items.drain(idx..).collect();
            ns.items = remaining;
            ns.file_level = false;
            doc.items.push(DocItem::Namespace(ns));
        }
    }
}

/// Recursively collect the set of item names defined within each namespace.
/// Keys are full namespace paths like "foo" or "foo::bar".
fn collect_namespace_items(items: &[DocItem], prefix: &str) -> HashMap<String, HashSet<String>> {
    let mut map: HashMap<String, HashSet<String>> = HashMap::new();
    for item in items {
        if let DocItem::Namespace(ns) = item {
            let ns_path = join_path(&ns.path);
            let full_path = if prefix.is_empty() {
                ns_path.clone()
            } else {
                format!("{}::{}", prefix, ns_path)
            };
            let entry = map.entry(full_path.clone()).or_default();
            for inner in &ns.items {
                if let Some(name) = doc_item_name(inner) {
                    entry.insert(name);
                }
            }
            // Recurse into nested namespaces
            let nested = collect_namespace_items(&ns.items, &full_path);
            for (k, v) in nested {
                map.entry(k).or_default().extend(v);
            }
        }
    }
    map
}

/// Extract the "name" of a doc item (the identifier that gets qualified).
fn doc_item_name(item: &DocItem) -> Option<String> {
    match item {
        DocItem::Body(body) => body_item_name(body),
        DocItem::ExportLet(el) => Some(el.name.name.clone()),
        DocItem::FunctionDecl(fd) => Some(fd.name.name.clone()),
        DocItem::ReExport(re) => Some(re.name.name.clone()),
        _ => None,
    }
}

/// Extract the "name" of a body item.
fn body_item_name(item: &BodyItem) -> Option<String> {
    match item {
        BodyItem::Block(b) => Some(b.kind.name.clone()),
        BodyItem::Schema(s) => string_lit_text(&s.name),
        BodyItem::Table(_t) => None,
        BodyItem::LetBinding(lb) => Some(lb.name.name.clone()),
        BodyItem::MacroDef(m) => Some(m.name.name.clone()),
        BodyItem::StructDef(s) => string_lit_text(&s.name),
        BodyItem::SymbolSetDecl(ss) => Some(ss.name.name.clone()),
        BodyItem::DecoratorSchema(ds) => string_lit_text(&ds.name),
        _ => None,
    }
}

/// Extract plain text from a StringLit (only if all parts are literal).
fn string_lit_text(s: &StringLit) -> Option<String> {
    let mut out = String::new();
    for part in &s.parts {
        match part {
            StringPart::Literal(text) => out.push_str(text),
            StringPart::Interpolation(_) => return None,
        }
    }
    Some(out)
}

/// Qualify all names within a doc item by prepending the namespace prefix.
fn qualify_doc_item(item: &mut DocItem, ns: &str) {
    match item {
        DocItem::Body(body) => qualify_body_item(body, ns),
        DocItem::ExportLet(el) => {
            el.name.name = format!("{}::{}", ns, el.name.name);
        }
        DocItem::FunctionDecl(fd) => {
            fd.name.name = format!("{}::{}", ns, fd.name.name);
        }
        DocItem::ReExport(re) => {
            re.name.name = format!("{}::{}", ns, re.name.name);
        }
        DocItem::Import(_) | DocItem::Namespace(_) | DocItem::Use(_) => {}
    }
}

/// Qualify names within a body item.
fn qualify_body_item(item: &mut BodyItem, ns: &str) {
    match item {
        BodyItem::Block(b) => {
            b.kind.name = format!("{}::{}", ns, b.kind.name);
            for child in &mut b.body {
                qualify_body_item(child, ns);
            }
        }
        BodyItem::Schema(s) => {
            qualify_string_lit(&mut s.name, ns);
        }
        BodyItem::LetBinding(lb) => {
            lb.name.name = format!("{}::{}", ns, lb.name.name);
        }
        BodyItem::MacroDef(m) => {
            m.name.name = format!("{}::{}", ns, m.name.name);
        }
        BodyItem::StructDef(s) => {
            qualify_string_lit(&mut s.name, ns);
        }
        BodyItem::SymbolSetDecl(ss) => {
            ss.name.name = format!("{}::{}", ns, ss.name.name);
        }
        BodyItem::DecoratorSchema(ds) => {
            qualify_string_lit(&mut ds.name, ns);
        }
        BodyItem::Table(_)
        | BodyItem::Attribute(_)
        | BodyItem::MacroCall(_)
        | BodyItem::ForLoop(_)
        | BodyItem::Conditional(_)
        | BodyItem::Validation(_) => {}
    }
}

/// Prepend namespace to a StringLit by modifying the first literal part.
fn qualify_string_lit(s: &mut StringLit, ns: &str) {
    if let Some(StringPart::Literal(ref mut text)) = s.parts.first_mut() {
        *text = format!("{}::{}", ns, text);
    } else {
        s.parts.insert(0, StringPart::Literal(format!("{}::", ns)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lang::span::{FileId, Span};
    use crate::lang::trivia::Trivia;

    fn dummy_span() -> Span {
        Span::new(FileId(0), 0, 0)
    }

    fn make_ident(name: &str) -> Ident {
        Ident {
            name: name.to_string(),
            span: dummy_span(),
        }
    }

    fn make_string_lit(text: &str) -> StringLit {
        StringLit {
            parts: vec![StringPart::Literal(text.to_string())],
            span: dummy_span(),
        }
    }

    fn make_schema(name: &str) -> BodyItem {
        BodyItem::Schema(Schema {
            decorators: vec![],
            name: make_string_lit(name),
            fields: vec![],
            variants: vec![],
            trivia: Trivia::default(),
            span: dummy_span(),
        })
    }

    fn make_block(kind: &str) -> BodyItem {
        BodyItem::Block(Block {
            decorators: vec![],
            partial: false,
            kind: make_ident(kind),
            inline_id: None,
            arrow_target: None,
            inline_args: vec![],
            body: vec![],
            text_content: None,
            trivia: Trivia::default(),
            span: dummy_span(),
        })
    }

    fn make_ns(path: &[&str], items: Vec<DocItem>) -> DocItem {
        DocItem::Namespace(NamespaceDecl {
            path: path.iter().map(|s| make_ident(s)).collect(),
            items,
            file_level: false,
            trivia: Trivia::default(),
            span: dummy_span(),
        })
    }

    fn make_use(ns_path: &[&str], targets: Vec<UseTarget>) -> DocItem {
        DocItem::Use(UseDecl {
            namespace_path: ns_path.iter().map(|s| make_ident(s)).collect(),
            targets,
            trivia: Trivia::default(),
            span: dummy_span(),
        })
    }

    #[test]
    fn test_braced_namespace_qualifies_names() {
        let mut doc = Document {
            items: vec![make_ns(
                &["net"],
                vec![
                    DocItem::Body(make_schema("service")),
                    DocItem::Body(make_block("endpoint")),
                ],
            )],
            trivia: Trivia::default(),
            span: dummy_span(),
        };

        let mut diags = DiagnosticBag::new();
        let _aliases = resolve(&mut doc, &mut diags);

        assert!(!diags.has_errors(), "{:?}", diags.diagnostics());
        assert_eq!(doc.items.len(), 2);

        if let DocItem::Body(BodyItem::Schema(s)) = &doc.items[0] {
            assert_eq!(string_lit_text(&s.name).unwrap(), "net::service");
        } else {
            panic!("expected Schema");
        }

        if let DocItem::Body(BodyItem::Block(b)) = &doc.items[1] {
            assert_eq!(b.kind.name, "net::endpoint");
        } else {
            panic!("expected Block");
        }
    }

    #[test]
    fn test_file_level_namespace() {
        let mut doc = Document {
            items: vec![
                DocItem::Namespace(NamespaceDecl {
                    path: vec![make_ident("myns")],
                    items: vec![],
                    file_level: true,
                    trivia: Trivia::default(),
                    span: dummy_span(),
                }),
                DocItem::Body(make_schema("thing")),
                DocItem::Body(make_block("widget")),
            ],
            trivia: Trivia::default(),
            span: dummy_span(),
        };

        let mut diags = DiagnosticBag::new();
        let _aliases = resolve(&mut doc, &mut diags);

        assert!(!diags.has_errors(), "{:?}", diags.diagnostics());
        assert_eq!(doc.items.len(), 2);

        if let DocItem::Body(BodyItem::Schema(s)) = &doc.items[0] {
            assert_eq!(string_lit_text(&s.name).unwrap(), "myns::thing");
        } else {
            panic!("expected Schema, got {:?}", doc.items[0]);
        }
    }

    #[test]
    fn test_use_creates_aliases() {
        let mut doc = Document {
            items: vec![
                make_ns(&["net"], vec![DocItem::Body(make_schema("service"))]),
                make_use(
                    &["net"],
                    vec![UseTarget {
                        name: make_ident("service"),
                        alias: Some(make_ident("svc")),
                        span: dummy_span(),
                    }],
                ),
            ],
            trivia: Trivia::default(),
            span: dummy_span(),
        };

        let mut diags = DiagnosticBag::new();
        let aliases = resolve(&mut doc, &mut diags);

        assert!(!diags.has_errors(), "{:?}", diags.diagnostics());
        assert_eq!(
            aliases.aliases.get("svc"),
            Some(&"net::service".to_string())
        );
    }

    #[test]
    fn test_use_unknown_namespace_errors() {
        let mut doc = Document {
            items: vec![make_use(
                &["unknown"],
                vec![UseTarget {
                    name: make_ident("thing"),
                    alias: None,
                    span: dummy_span(),
                }],
            )],
            trivia: Trivia::default(),
            span: dummy_span(),
        };

        let mut diags = DiagnosticBag::new();
        let _aliases = resolve(&mut doc, &mut diags);

        assert!(diags.has_errors());
        let errs = diags.diagnostics();
        assert!(errs[0].code.as_deref() == Some("E121"));
    }

    #[test]
    fn test_use_unknown_target_errors() {
        let mut doc = Document {
            items: vec![
                make_ns(&["net"], vec![DocItem::Body(make_schema("service"))]),
                make_use(
                    &["net"],
                    vec![UseTarget {
                        name: make_ident("nonexistent"),
                        alias: None,
                        span: dummy_span(),
                    }],
                ),
            ],
            trivia: Trivia::default(),
            span: dummy_span(),
        };

        let mut diags = DiagnosticBag::new();
        let _aliases = resolve(&mut doc, &mut diags);

        assert!(diags.has_errors());
        let errs = diags.diagnostics();
        assert!(errs[0].code.as_deref() == Some("E120"));
    }

    #[test]
    fn test_nested_namespace_qualifies_deeply() {
        let mut doc = Document {
            items: vec![make_ns(
                &["foo"],
                vec![make_ns(&["bar"], vec![DocItem::Body(make_schema("item"))])],
            )],
            trivia: Trivia::default(),
            span: dummy_span(),
        };

        let mut diags = DiagnosticBag::new();
        let _aliases = resolve(&mut doc, &mut diags);

        assert!(!diags.has_errors(), "{:?}", diags.diagnostics());
        assert_eq!(doc.items.len(), 1);

        if let DocItem::Body(BodyItem::Schema(s)) = &doc.items[0] {
            assert_eq!(string_lit_text(&s.name).unwrap(), "foo::bar::item");
        } else {
            panic!("expected Schema, got {:?}", doc.items[0]);
        }
    }

    #[test]
    fn test_namespace_path_syntax() {
        // namespace foo::bar { schema "item" {} }
        let mut doc = Document {
            items: vec![make_ns(
                &["foo", "bar"],
                vec![DocItem::Body(make_schema("item"))],
            )],
            trivia: Trivia::default(),
            span: dummy_span(),
        };

        let mut diags = DiagnosticBag::new();
        let _aliases = resolve(&mut doc, &mut diags);

        assert!(!diags.has_errors(), "{:?}", diags.diagnostics());
        if let DocItem::Body(BodyItem::Schema(s)) = &doc.items[0] {
            assert_eq!(string_lit_text(&s.name).unwrap(), "foo::bar::item");
        } else {
            panic!("expected Schema");
        }
    }

    #[test]
    fn test_use_with_nested_namespace() {
        let mut doc = Document {
            items: vec![
                make_ns(
                    &["foo"],
                    vec![make_ns(&["bar"], vec![DocItem::Body(make_schema("thing"))])],
                ),
                make_use(
                    &["foo", "bar"],
                    vec![UseTarget {
                        name: make_ident("thing"),
                        alias: None,
                        span: dummy_span(),
                    }],
                ),
            ],
            trivia: Trivia::default(),
            span: dummy_span(),
        };

        let mut diags = DiagnosticBag::new();
        let aliases = resolve(&mut doc, &mut diags);

        assert!(!diags.has_errors(), "{:?}", diags.diagnostics());
        assert_eq!(
            aliases.aliases.get("thing"),
            Some(&"foo::bar::thing".to_string())
        );
    }
}
