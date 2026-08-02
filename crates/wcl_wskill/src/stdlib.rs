//! The wskill **library**: the base schema, the two optional view schemas, and
//! the topic-agnostic wdoc projection templates, embedded in the binary and
//! registered under `wskill/*.wcl` keys plus the public `wskill.wcl` entry
//! point.
//!
//! A wskill's root document opts in with `import <wskill.wcl>`, exactly as it
//! already opts into wdoc with `import <wdoc.wcl>`. A projection entry imports
//! the projection it is (`import <wskill/book.wcl>`) or, when it overrides one
//! of the parts, enumerates the parts it still wants. Mirrors
//! [`wcl_wdoc::schema_registry`].
//!
//! These files used to be **copied** into every wskill folder — the base
//! schema and fourteen templates, ~56 copies across four wskills, policed by
//! two CI diff gates. They were copied so an author could edit any of them;
//! measurement said 9% diverged, most of that two reference-heavy topics.
//! Overriding is now import granularity: a topic that wants its own part
//! doesn't import that part and declares its own. Nothing imported a competing
//! name, so nothing shadows — there is no shadowing mechanism and none is
//! needed.

use std::sync::Once;

use wcl_lang::Registry;

/// The wskill library as a [`Registry`]. Layer it onto wdoc's own rather than
/// using it alone: every part imports `<wdoc.wcl>` (or resolves names a wdoc
/// document supplies), so on its own it resolves nothing.
///
/// Prefer [`install_stdlib`], which is what makes `import <wskill.wcl>`
/// resolve on every document-opening path in the toolchain.
pub fn schema_registry() -> Registry {
    let mut r = Registry::new();
    r.register("wskill.wcl", include_str!("../lib/wskill.wcl"));
    r.register("wskill/prelude.wcl", include_str!("../lib/prelude.wcl"));
    r.register("wskill/schema.wcl", include_str!("../lib/schema.wcl"));
    r.register(
        "wskill/presentation.wcl",
        include_str!("../lib/presentation.wcl"),
    );
    r.register("wskill/training.wcl", include_str!("../lib/training.wcl"));

    // Projections. Each declares a `site` and its pages, so a document is one
    // of them — which is why they are NOT in the prelude.
    r.register("wskill/book.wcl", include_str!("../lib/book.wcl"));

    // Per-unit page components, shared by every projection.
    r.register(
        "wskill/component/common.wcl",
        include_str!("../lib/component/common.wcl"),
    );
    r.register(
        "wskill/component/concept.wcl",
        include_str!("../lib/component/concept.wcl"),
    );
    r.register(
        "wskill/component/entity.wcl",
        include_str!("../lib/component/entity.wcl"),
    );
    r.register(
        "wskill/component/fact.wcl",
        include_str!("../lib/component/fact.wcl"),
    );
    r.register(
        "wskill/component/research.wcl",
        include_str!("../lib/component/research.wcl"),
    );
    r.register(
        "wskill/component/process.wcl",
        include_str!("../lib/component/process.wcl"),
    );
    r.register(
        "wskill/component/type_index.wcl",
        include_str!("../lib/component/type_index.wcl"),
    );
    r.register(
        "wskill/component/skill_md.wcl",
        include_str!("../lib/component/skill_md.wcl"),
    );

    // The book's standalone pages, one part each.
    r.register(
        "wskill/pages/overview.wcl",
        include_str!("../lib/pages/overview.wcl"),
    );
    r.register(
        "wskill/pages/concepts.wcl",
        include_str!("../lib/pages/concepts.wcl"),
    );
    r.register(
        "wskill/pages/entities.wcl",
        include_str!("../lib/pages/entities.wcl"),
    );
    r.register(
        "wskill/pages/facts.wcl",
        include_str!("../lib/pages/facts.wcl"),
    );
    r.register(
        "wskill/pages/processes.wcl",
        include_str!("../lib/pages/processes.wcl"),
    );
    r
}

static INSTALLED: Once = Once::new();

/// Make `import <wskill.wcl>` resolve everywhere in this process, by layering
/// [`schema_registry`] onto [`wcl_wdoc::schema_registry`].
///
/// Idempotent and cheap to call again, so call it rather than wonder: every
/// entry point that may open a wskill document does (the `wcl` binary in
/// `main`, and every [`Graph`](crate::Graph) loader here). `wcl_wdoc` builds
/// its own loaders, so a document opened through it — a build, an editor save,
/// an LSP diagnostic — cannot be handed a registry any other way.
pub fn install_stdlib() {
    INSTALLED.call_once(|| wcl_wdoc::install_stdlib(schema_registry()));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every registered part parses, and every system import inside one names
    /// a key that exists — in this registry or in wdoc's. A typo in an
    /// importer-relative path (`<wdoc.wcl>` from `wskill/component/`, which
    /// resolves to `wskill/component/wdoc.wcl`) would otherwise only surface
    /// as a build failure in a wskill.
    #[test]
    fn every_part_parses_and_its_system_imports_resolve() {
        install_stdlib();
        let all = wcl_wdoc::schema_registry();
        for (key, src) in registered() {
            wcl_lang::parse_for_edit(&src, key.clone())
                .unwrap_or_else(|e| panic!("{key} does not parse: {e}"));
            for target in system_imports(&src) {
                // The language's own rule, so this test cannot disagree with
                // the loader about what `<../../wdoc.wcl>` resolves to.
                let resolved = wcl_lang::system_import_key(Some(&key), &target);
                assert!(
                    all.get(&resolved).is_some(),
                    "{key}: `import <{target}>` resolves to `{resolved}`, which is not registered"
                );
            }
        }
    }

    /// The parts named by the acceptance criteria are all there, under the
    /// keys the wskills and the scaffold import.
    #[test]
    fn registers_the_schema_and_every_shared_template() {
        let r = schema_registry();
        for key in [
            "wskill.wcl",
            "wskill/prelude.wcl",
            "wskill/schema.wcl",
            "wskill/presentation.wcl",
            "wskill/training.wcl",
            "wskill/book.wcl",
            "wskill/component/common.wcl",
            "wskill/component/skill_md.wcl",
            "wskill/pages/overview.wcl",
        ] {
            assert!(r.get(key).is_some(), "missing registry key {key}");
        }
    }

    /// The embedded schema and book projection support both edge forms,
    /// reciprocal dedupe, and linkable content indexes without local copies.
    #[test]
    fn embedded_book_supports_reasoned_edges_and_linkable_indexes() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        crate::testsupport::write(
            root,
            crate::ROOT_MARKER,
            "import <wdoc.wcl>\n\
             import <wskill.wcl>\n\
             import \"./schema/kinds.wcl\"\n\
             import \"./data/main.wcl\"\n\n\
             schema_version = \"1.4.0\"\n\n\
             topic demo {\n  name = \"Demo\"\n  summary = \"A demo.\"\n  created = \"1970-01-01\"\n}\n\n\
             wcl.wskill::skill {\n}\n\n\
             artifact book {\n  kind = :book\n  entry = \"wdoc/book/main.wcl\"\n}\n",
        );
        crate::testsupport::write(
            root,
            "schema/kinds.wcl",
            "namespace wcl.wskill\n\n\
             symbol_set EntityKind { software }\n\
             symbol_set ArtifactKind { book ai_skill presentation training }\n",
        );
        crate::testsupport::write(
            root,
            "data/main.wcl",
            "concept alpha {\n  name = \"Alpha\"\n  summary = \"The first idea.\"\n  \
             related = [{ id: \"beta\", why: \"Beta puts the idea into practice.\" }, { id: \"nested\", why: \"The nested guide explains Alpha.\" }]\n  \
             body { p \"Alpha's body.\" }\n}\n\n\
             concept beta {\n  name = \"Beta\"\n  summary = \"The second idea.\"\n  \
             related = [alpha]\n}\n\n\
             index start {\n  name = \"Start here\"\n  related = [alpha, beta]\n}\n\n\
             index area {\n  name = \"Area guide\"\n  summary = \"A guided area.\"\n  related = [alpha]\n  body { p \"Area body.\" }\n\n  \
               index nested {\n    name = \"Nested guide\"\n    related = [beta]\n    body { p \"Nested body.\" }\n  }\n}\n",
        );
        crate::testsupport::write(
            root,
            "wdoc/book/main.wcl",
            "import \"../../wskill.wcl\"\nimport <wskill/book.wcl>\n",
        );

        install_stdlib();
        let root_doc = wcl_wdoc::open_doc_for_edit(&root.join(crate::ROOT_MARKER))
            .expect("root document opens");
        let related = root_doc
            .blocks()
            .find(|b| b.kind() == "concept")
            .and_then(|b| b.field("related"))
            .expect("alpha.related");
        assert!(
            related.value().is_ok(),
            "alpha.related: {:?}",
            related.value()
        );

        let graph = crate::Graph::open(root).expect("model loads");
        assert_eq!(graph.units.len(), 2);
        assert_eq!(graph.units[0].id, "alpha");
        assert_eq!(graph.units[0].related[0].id, "beta");
        assert_eq!(
            graph.units[0].related[0].why.as_deref(),
            Some("Beta puts the idea into practice.")
        );
        assert_eq!(graph.indexes.len(), 2);

        // The book entry is a document in its own right: it must open and
        // validate, since that is what `wcl wdoc build` does to it.
        install_stdlib();
        let doc =
            wcl_wdoc::open_doc_for_edit(&root.join("wdoc/book/main.wcl")).expect("book opens");
        let errs = doc.schema_errors();
        assert!(errs.is_empty(), "book entry has schema errors: {errs:?}");
        assert!(
            doc.blocks().any(|b| b.kind() == "site"),
            "the book part contributed no site"
        );

        let out = root.join("out/book");
        if let Err(error) = wcl_wdoc::build(&root.join("wdoc/book/main.wcl"), &out, None) {
            error.report();
            panic!("book builds");
        }
        let alpha =
            std::fs::read_to_string(out.join("concept_alpha.html")).expect("alpha page rendered");
        assert!(
            alpha.contains("Beta puts the idea into practice."),
            "annotated edge reason did not render: {alpha}"
        );
        assert_eq!(
            alpha
                .matches("<p>- <a class=\"link\" href=\"concept_beta.html\">Beta</a>")
                .count(),
            1,
            "a reciprocal edge must not repeat under Referenced by"
        );
        assert!(
            alpha.contains("The nested guide explains Alpha."),
            "a related edge to a bodied nested index must render its reason"
        );
        assert!(
            alpha.contains("href=\"index_nested.html\""),
            "a bodied nested index must be linkable as a related target"
        );
        assert!(
            out.join("index_area.html").is_file(),
            "a top-level bodied index must get a page"
        );
        assert!(
            out.join("index_nested.html").is_file(),
            "a nested bodied index must get a page"
        );

        crate::testsupport::write(
            root,
            "data/main.wcl",
            "concept alpha {\n  name = \"Alpha\"\n  summary = \"The first idea.\"\n  related = [start]\n}\n\n\
             index start {\n  name = \"Start here\"\n  audience = :ai\n  related = [alpha]\n}\n",
        );
        match wcl_wdoc::build(
            &root.join("wdoc/book/main.wcl"),
            &root.join("out/bodyless"),
            None,
        ) {
            Err(wcl_wdoc::BuildError::Eval(report)) => assert!(
                format!("{report:?}").contains("bodyless index `start`"),
                "the build error should identify the bodyless related target"
            ),
            Err(error) => {
                error.report();
                panic!("a bodyless related target should be an evaluation error");
            }
            Ok(_) => panic!("a related edge to a bodyless index must fail the build"),
        }
    }

    fn registered() -> Vec<(String, String)> {
        let r = schema_registry();
        let mut out: Vec<(String, String)> = r
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        out.sort();
        out
    }

    /// The `import <...>` targets in `src`, ignoring comment lines.
    fn system_imports(src: &str) -> Vec<String> {
        src.lines()
            .map(str::trim_start)
            .filter(|l| l.starts_with("import <"))
            .filter_map(|l| l["import <".len()..].split('>').next())
            .map(str::to_string)
            .collect()
    }
}
