//! Fixtures shared by the editor modules' own test suites.
//!
//! Each module tests its own behaviour through its inner functions (which
//! take a [`Workspace`], not a router), so the only thing worth sharing is
//! the *documents* those tests are written against and the two-line setup
//! that puts one on disk. Anything used by a single module stays in that
//! module.

use std::path::Path;
use std::sync::Arc;

use wcl_lang::{Span, ast};

use super::{EditorState, Workspace};

/// A minimal one-page wdoc site.
pub(super) const SITE_DOC: &str = "import <wdoc.wcl>\n\nsite docs {\n  title = \"The Docs\"\n  root = true\n}\n\npage index {\n  title = \"Hi\"\n\n  h1 \"Hello preview\"\n}\n";

/// A one-page site whose body holds two paragraphs — the fixture for block
/// editing, visibility toggles and preview invalidation.
pub(super) const BODY_DOC: &str = "import <wdoc.wcl>\n\nsite docs {\n  title = \"The Docs\"\n  root = true\n}\n\npage index {\n  title = \"Hi\"\n\n  p \"First paragraph\"\n\n  p \"Second paragraph\"\n}\n";

/// A document with a user schema kind (`thing`) plus an `edit_object`
/// button targeting one of its instances.
pub(super) const OBJECT_DOC: &str = "import <wdoc.wcl>\n\n\
    @document\ntype Doc {\n  @children(\"thing\") things: list<Thing>\n}\n\n\
    @block(\"thing\")\ntype Thing {\n  @inline(0) name: utf8\n  note: utf8?\n}\n\n\
    site docs {\n  title = \"The Docs\"\n  root = true\n}\n\n\
    thing \"alpha\" {\n  note = \"first\"\n}\n\n\
    thing \"beta\" {\n  note = \"second\"\n}\n\n\
    page index {\n  title = \"Hi\"\n\n  h1 \"Hello\"\n\n  edit_object {\n    kind = \"thing\"\n    target = \"alpha\"\n  }\n}\n";

/// A temp dir holding `main.wcl`, plus a workspace over it. The guard must
/// outlive the workspace — hold it for the length of the test.
pub(super) fn workspace_with(doc: &str) -> (tempfile::TempDir, Workspace) {
    let td = tempfile::tempdir().unwrap();
    std::fs::write(td.path().join("main.wcl"), doc).unwrap();
    let ws = Workspace::at(td.path());
    (td, ws)
}

/// A full editor state over `dir` — for the preview module, the only one
/// that needs the scratch tree and the session map.
pub(super) fn state_at(dir: &Path) -> Arc<EditorState> {
    state_with_review(dir, None)
}

pub(super) fn state_with_review(
    dir: &Path,
    review: Option<wcl_wdoc::Handshake>,
) -> Arc<EditorState> {
    Arc::new(EditorState {
        ws: Workspace::at(dir),
        preview: crate::preview::Preview::new().unwrap(),
        sessions: super::preview::Sessions::default(),
        review,
    })
}

/// The span of the first block in `text` satisfying `pred`, found with the
/// editor's own recursive [`super::find_block`] descent.
pub(super) fn span_of(text: &str, pred: impl Fn(&ast::Block) -> bool) -> Span {
    let src = wcl_lang::parse_for_edit(text, "t").unwrap();
    super::find_block(&src.items, &pred)
        .expect("no block matched")
        .span
}

/// A miniature wskill: a gathered `topic`, `concept` units one-per-file
/// under `data/concepts/` with a `main.wcl` aggregator, and an `index`
/// pinning them.
pub(super) fn write_mini_wskill(root: &Path) {
    std::fs::write(
        root.join("main.wcl"),
        "import <wdoc.wcl>\nimport \"data/concepts/main.wcl\"\nimport \"data/indexes.wcl\"\n\n\
         @document\ntype Doc {\n  @children(\"topic\") topics: list<Topic>\n  @children(\"concept\") concepts: list<Concept>\n  @children(\"index\") indexes: list<Index>\n}\n\n\
         @block(\"topic\")\ntype Topic {\n  @inline(0) id: identifier\n}\n\n\
         @block(\"concept\")\ntype Concept {\n  @inline(0) id: identifier\n  name: utf8\n}\n\n\
         @block(\"index\")\ntype Index {\n  @inline(0) id: identifier\n  name: utf8\n  related: list<identifier>?\n}\n\n\
         topic mini {}\n\n\
         site book {\n  title = \"Mini\"\n  root = true\n  toc {\n    chapter \"Overview\" {\n      page = index\n    }\n  }\n}\n\n\
         page index {\n  title = \"Hi\"\n\n  h1 \"Mini\"\n}\n",
    )
    .unwrap();
    std::fs::create_dir_all(root.join("data/concepts")).unwrap();
    std::fs::write(
        root.join("data/concepts/main.wcl"),
        "import \"./alpha.wcl\"\nimport \"./beta.wcl\"\n",
    )
    .unwrap();
    std::fs::write(
        root.join("data/concepts/alpha.wcl"),
        "concept alpha {\n  name = \"Alpha\"\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("data/concepts/beta.wcl"),
        "concept beta {\n  name = \"Beta\"\n}\n",
    )
    .unwrap();
    std::fs::write(
        root.join("data/indexes.wcl"),
        "index lang {\n  name = \"Language\"\n  related = [alpha, beta]\n}\n",
    )
    .unwrap();
}

/// [`write_mini_wskill`] with a nested sub-index: `alpha` pinned at the top
/// level, `beta` inside `lang_sub`.
pub(super) fn write_mini_wskill_nested(root: &Path) {
    write_mini_wskill(root);
    let main = std::fs::read_to_string(root.join("main.wcl")).unwrap();
    let main = main.replace(
        "@block(\"index\")\ntype Index {\n  @inline(0) id: identifier\n  name: utf8\n  related: list<identifier>?\n}",
        "@block(\"index\")\ntype Index {\n  @inline(0) id: identifier\n  name: utf8\n  related: list<identifier>?\n  @children(\"index\") children: list<Index>?\n}",
    );
    std::fs::write(root.join("main.wcl"), main).unwrap();
    std::fs::write(
        root.join("data/indexes.wcl"),
        "index lang {\n  name = \"Language\"\n  related = [alpha]\n\n  index lang_sub {\n    name = \"Sub\"\n    related = [beta]\n  }\n}\n",
    )
    .unwrap();
}

/// [`write_mini_wskill`] with a training course: `lesson` blocks ordered by
/// `n`.
pub(super) fn write_mini_wskill_training(root: &Path) {
    write_mini_wskill(root);
    let main = std::fs::read_to_string(root.join("main.wcl")).unwrap();
    let main = main
        .replace(
            "  @children(\"index\") indexes: list<Index>\n}",
            "  @children(\"index\") indexes: list<Index>\n  @children(\"lesson\") lessons: list<Lesson>\n}",
        )
        .replace(
            "@block(\"index\")",
            "@block(\"lesson\")\ntype Lesson {\n  @inline(0) id: identifier\n  title: utf8\n  n: u32\n}\n\n@block(\"index\")",
        );
    std::fs::write(root.join("main.wcl"), main).unwrap();
    std::fs::write(
        root.join("data/lessons.wcl"),
        "lesson first { title = \"First\"  n = 1u32 }\n\nlesson second { title = \"Second\"  n = 2u32 }\n",
    )
    .unwrap();
    let main = std::fs::read_to_string(root.join("main.wcl")).unwrap();
    std::fs::write(
        root.join("main.wcl"),
        main.replace(
            "import \"data/indexes.wcl\"",
            "import \"data/indexes.wcl\"\nimport \"data/lessons.wcl\"",
        ),
    )
    .unwrap();
}
