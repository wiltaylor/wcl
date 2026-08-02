//! The wskill folder format: the `wskill.wcl` marker, and the projections it
//! registers.
//!
//! This is the one place that knows what makes a directory a wskill. It is
//! read straight off the AST with no schema — the registry is metadata about
//! the folder, and a caller asking "is this a wskill?" must not be made to
//! evaluate the whole document to find out.

use std::path::{Path, PathBuf};

use wcl_lang::ast::{Expr, Item};
use wcl_lang::parse_for_edit;

use crate::model::View;

/// The file whose presence marks a directory as a wskill root — the "nearest
/// owning document root" a sidecar, a projection or a graph belongs to.
pub const ROOT_MARKER: &str = "wskill.wcl";

/// One `artifact` block: a projection of the wskill.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub id: String,
    /// The `kind` symbol, bare (`book`, `ai_skill`, `presentation`, …).
    pub kind: String,
    /// The entry document, relative to the wskill root.
    pub entry: String,
}

/// A wskill's `wskill.wcl`, read for identity and projections only.
#[derive(Debug, Clone)]
pub struct Registry {
    /// The `wskill.wcl` itself.
    pub file: PathBuf,
    /// The `topic` block's id.
    pub topic_id: Option<String>,
    /// The `topic` block's `name` — the folder's display label.
    pub topic_name: Option<String>,
    pub artifacts: Vec<Artifact>,
}

impl Registry {
    /// The wskill root at or above `start`: the nearest directory holding a
    /// [`ROOT_MARKER`]. `None` when `start` is not inside a wskill at all.
    ///
    /// The walk is [`wcl_wdoc::owner_root`]'s — "the nearest owning document
    /// root" is a wdoc concept and this crate merely supplies the marker
    /// filename that makes it a *wskill* root.
    pub fn owner_dir(start: &Path) -> Option<PathBuf> {
        let from = if start.is_file() {
            start.parent()?
        } else {
            start
        };
        wcl_wdoc::owner_root(from, ROOT_MARKER)
    }

    /// Read one `wskill.wcl`. A file that doesn't parse yields `None` — the
    /// registry is metadata, and the build step is the authority on errors.
    pub fn read(file: &Path) -> Option<Registry> {
        let src = std::fs::read_to_string(file).ok()?;
        let ast = parse_for_edit(&src, file.display().to_string()).ok()?;
        let mut reg = Registry {
            file: file.to_path_buf(),
            topic_id: None,
            topic_name: None,
            artifacts: Vec::new(),
        };
        for item in &ast.items {
            let Item::Block(b) = item else { continue };
            match b.kind.as_str() {
                "topic" => {
                    reg.topic_id = ast_label(b);
                    reg.topic_name = string_field(b, "name");
                }
                "artifact" => {
                    let (Some(id), Some(kind), Some(entry)) = (
                        ast_label(b),
                        symbol_field(b, "kind"),
                        string_field(b, "entry"),
                    ) else {
                        continue;
                    };
                    reg.artifacts.push(Artifact { id, kind, entry });
                }
                _ => {}
            }
        }
        Some(reg)
    }

    /// The registry's projections as model [`View`]s, each paired with the
    /// site name its entry declares — which is what block visibility
    /// decorators name. `root` is the wskill root the entries are relative
    /// to.
    pub fn views(&self, root: &Path) -> Vec<View> {
        self.artifacts
            .iter()
            .map(|a| View {
                id: a.id.clone(),
                kind: a.kind.clone(),
                entry: a.entry.clone(),
                site: first_site_name(&root.join(&a.entry)),
            })
            .collect()
    }
}

/// The first top-level `site` block's name in a projection entry.
///
/// A plain parse, never an evaluation — the registry is metadata. But the
/// entry does not have to *declare* its site: since the shared projections
/// were embedded, `wdoc/book/main.wcl` is two lines and the `site` arrives
/// with `import <wskill/book.wcl>`. So the walk follows imports depth-first
/// in written order, resolving `"…"` against the importing file's directory
/// and `<…>` through this crate's own embedded library — the only two places
/// a projection's site can come from.
///
/// Bounded rather than cycle-tracked: an entry reaches its site in one hop
/// (the shared projection) or two (a topic's own file that imports one), and a
/// deeper chain is a document to open properly rather than to walk here.
const MAX_SITE_IMPORT_HOPS: usize = 3;

fn first_site_name(entry: &Path) -> Option<String> {
    fn walk(
        src: &str,
        name: String,
        base: Option<&Path>,
        lib: &wcl_lang::Registry,
        depth: usize,
    ) -> Option<String> {
        let ast = parse_for_edit(src, name).ok()?;
        if let Some(site) = ast.items.iter().find_map(|i| match i {
            Item::Block(b) if b.kind == "site" => ast_label(b),
            _ => None,
        }) {
            return Some(site);
        }
        if depth == 0 {
            return None;
        }
        ast.items.iter().find_map(|i| match i {
            Item::Import(imp) if imp.system => {
                // A disk importer, so the key is the path from the registry
                // root — the resolver's own rule, borrowed rather than redone.
                let key = wcl_lang::system_import_key(None, &imp.path);
                walk(lib.get(&key)?, format!("<{key}>"), None, lib, depth - 1)
            }
            Item::Import(imp) => {
                let path = base?.join(&imp.path);
                let text = std::fs::read_to_string(&path).ok()?;
                walk(
                    &text,
                    path.display().to_string(),
                    path.parent(),
                    lib,
                    depth - 1,
                )
            }
            _ => None,
        })
    }

    let src = std::fs::read_to_string(entry).ok()?;
    let lib = crate::schema_registry();
    walk(
        &src,
        entry.display().to_string(),
        entry.parent(),
        &lib,
        MAX_SITE_IMPORT_HOPS,
    )
}

/// A block's first inline label — how every wskill block is named
/// (`concept alpha`, `artifact book`). Naming a block is a language fact, so
/// the reader is `wcl_lang::edit`'s; the crate reads it under this name.
pub(crate) use wcl_lang::edit::block_label as ast_label;

fn field_expr<'a>(b: &'a wcl_lang::ast::Block, name: &str) -> Option<&'a Expr> {
    b.items.iter().find_map(|it| match it {
        Item::Field(f) if f.name == name => Some(&f.expr),
        _ => None,
    })
}

fn string_field(b: &wcl_lang::ast::Block, name: &str) -> Option<String> {
    match field_expr(b, name)? {
        Expr::Utf8(s) | Expr::Ascii(s) => Some(s.clone()),
        _ => None,
    }
}

fn symbol_field(b: &wcl_lang::ast::Block, name: &str) -> Option<String> {
    match field_expr(b, name)? {
        Expr::Symbol(s) => Some(s.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testsupport::write;

    #[test]
    fn reads_topic_and_artifacts_and_resolves_site_names() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        write(
            root,
            ROOT_MARKER,
            "topic demo {\n  name = \"Demo Topic\"\n}\n\n\
             artifact book {\n  kind = :book\n  entry = \"wdoc/book/main.wcl\"\n  output = \"out/book\"\n}\n\n\
             artifact ai_skill {\n  kind = :ai_skill\n  entry = \"wdoc/skill/main.wcl\"\n}\n",
        );
        write(
            root,
            "wdoc/book/main.wcl",
            "import <wdoc.wcl>\n\nsite handbook {\n  root = true\n}\n",
        );

        let reg = Registry::read(&root.join(ROOT_MARKER)).expect("registry");
        assert_eq!(reg.topic_id.as_deref(), Some("demo"));
        assert_eq!(reg.topic_name.as_deref(), Some("Demo Topic"));
        assert_eq!(reg.artifacts.len(), 2);
        assert_eq!(reg.artifacts[0].kind, "book");
        assert_eq!(reg.artifacts[0].entry, "wdoc/book/main.wcl");

        let views = reg.views(root);
        // The site name comes from the projection entry…
        assert_eq!(views[0].site.as_deref(), Some("handbook"));
        assert_eq!(views[0].site_name(), "handbook");
        // …and falls back to the artifact id when the entry is missing.
        assert_eq!(views[1].site, None);
        assert_eq!(views[1].site_name(), "ai_skill");
    }

    /// A projection entry that imports its site instead of declaring one —
    /// the shape every wskill has since the shared templates were embedded.
    /// Both hops matter: through the library (`<wskill/book.wcl>` declares
    /// `site book`) and through a disk import.
    #[test]
    fn a_site_reached_by_import_still_names_the_view() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        write(
            root,
            ROOT_MARKER,
            "topic demo {\n  name = \"Demo\"\n}\n\n\
             artifact book {\n  kind = :book\n  entry = \"wdoc/book/main.wcl\"\n}\n\n\
             artifact ai_skill {\n  kind = :ai_skill\n  entry = \"wdoc/skill/main.wcl\"\n}\n",
        );
        write(
            root,
            "wdoc/book/main.wcl",
            "import \"../../wskill.wcl\"\nimport <wskill/book.wcl>\n",
        );
        write(root, "wdoc/skill/main.wcl", "import \"./site.wcl\"\n");
        write(root, "wdoc/skill/site.wcl", "site skill {\n}\n");

        let views = Registry::read(&root.join(ROOT_MARKER))
            .expect("registry")
            .views(root);
        assert_eq!(views[0].site.as_deref(), Some("book"));
        assert_eq!(views[1].site.as_deref(), Some("skill"));
    }

    #[test]
    fn owner_dir_walks_up_to_the_marker() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        write(root, ROOT_MARKER, "topic demo {\n  name = \"D\"\n}\n");
        write(root, "wdoc/book/main.wcl", "");
        let owner = Registry::owner_dir(&root.join("wdoc/book")).expect("owner");
        assert_eq!(owner, std::fs::canonicalize(root).unwrap());
        // From a file, too.
        assert_eq!(
            Registry::owner_dir(&root.join("wdoc/book/main.wcl")).expect("owner"),
            std::fs::canonicalize(root).unwrap()
        );
        // Outside a wskill there is no owner.
        let other = tempfile::tempdir().unwrap();
        assert_eq!(Registry::owner_dir(other.path()), None);
    }
}
