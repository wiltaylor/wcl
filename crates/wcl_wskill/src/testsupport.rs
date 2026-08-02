//! The fixture the crate's tests are written against: a miniature but real
//! wskill folder — a registry with two projections, units one-per-file under
//! `data/`, an index pinning them, and a body carrying a view-hidden block.
//!
//! It is written out rather than pointed at a checked-in example because
//! several tests mutate one file of it (a computed `related`, a nested
//! sub-index, a course) and then re-read the model.

use std::path::Path;

/// The root document: schema, registry and topic in one file, exactly as a
/// real `wskill.wcl` carries them.
const ROOT_DOC: &str = r#"import <wdoc.wcl>
import "data/concepts/main.wcl"
import "data/indexes.wcl"
import "data/lessons.wcl"

symbol_set Audience { book ai both }
symbol_set ArtifactKind { book ai_skill }

@document
type Doc {
  @children("topic") topics: list<Topic>
  @children("artifact") artifacts: list<Artifact>
  @children("concept") concepts: list<Concept>
  @children("research") researches: list<Research>
  @children("index") indexes: list<Index>
  @children("lesson") lessons: list<Lesson>
  @children("module") modules: list<Module>
}

@block("topic")
type Topic {
  @inline(0) id: identifier
  name: utf8
}

@block("artifact")
type Artifact {
  @inline(0) id: identifier
  kind: ArtifactKind
  entry: utf8
}

@block("body") @schemaless
type UnitBody {
}

@block("concept")
type Concept {
  @inline(0) id: identifier
  name: utf8
  @default([]) related: list<identifier>
  @default(:book) audience: Audience
  @child("body") body: UnitBody?
}

@block("research")
type Research {
  @inline(0) id: identifier
  name: utf8
  @default(:ai) audience: Audience
}

@block("index")
type Index {
  @inline(0) id: identifier
  name: utf8
  @default([]) related: list<identifier>
  @default(:both) audience: Audience
  @children("index") children: list<Index>?
}

@block("lesson")
type Lesson {
  @inline(0) id: identifier
  title: utf8
  n: u32
}

@block("module")
type Module {
  @inline(0) id: identifier
  title: utf8
  n: u32
  @children("lesson") lessons: list<Lesson>?
}

topic mini {
  name = "Mini"
}

artifact book {
  kind = :book
  entry = "wdoc/book/main.wcl"
}

artifact ai_skill {
  kind = :ai_skill
  entry = "wdoc/skill/main.wcl"
}
"#;

/// Write `text` to `dir/rel`, creating the folders it needs.
pub(crate) fn write(dir: &Path, rel: &str, text: &str) {
    let path = dir.join(rel);
    std::fs::create_dir_all(path.parent().expect("a file has a parent")).unwrap();
    std::fs::write(path, text).unwrap();
}

/// A temp dir holding the fixture wskill. Hold the guard for the test's
/// length — dropping it removes the folder.
pub(crate) fn mini_wskill() -> tempfile::TempDir {
    let td = tempfile::tempdir().unwrap();
    let root = td.path();
    write(root, crate::ROOT_MARKER, ROOT_DOC);
    write(
        root,
        "wdoc/book/main.wcl",
        "import <wdoc.wcl>\n\nsite book {\n  title = \"Mini\"\n  root = true\n}\n\n\
         page index {\n  title = \"Hi\"\n\n  h1 \"Mini\"\n}\n",
    );
    write(
        root,
        "wdoc/skill/main.wcl",
        "import <wdoc.wcl>\n\nsite skill {\n  default_template = :ai_skill\n}\n\n\
         page index {\n  title = \"Hi\"\n\n  h1 \"Mini\"\n}\n",
    );
    write(
        root,
        "data/concepts/main.wcl",
        "import \"./alpha.wcl\"\nimport \"./beta.wcl\"\nimport \"./gamma.wcl\"\n",
    );
    write(
        root,
        "data/concepts/alpha.wcl",
        "concept alpha {\n  name = \"Alpha\"\n  related = [beta]\n\n  body {\n    \
         p \"Everywhere\"\n\n    @except(sites = [:skill])\n    p \"Book only\"\n  }\n}\n",
    );
    write(
        root,
        "data/concepts/beta.wcl",
        "concept beta {\n  name = \"Beta\"\n}\n",
    );
    write(
        root,
        "data/concepts/gamma.wcl",
        "research gamma {\n  name = \"Gamma\"\n}\n",
    );
    write(
        root,
        "data/indexes.wcl",
        "index lang {\n  name = \"Language\"\n  related = [alpha, beta]\n}\n",
    );
    // Present but empty: the course tests fill it in, and an import must
    // resolve from the start.
    write(root, "data/lessons.wcl", "");
    td
}
