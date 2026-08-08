//! Introspect the sites an entry document declares — the `site` blocks
//! plus the sub-site members its `include` blocks resolve to. This is the
//! building block a host uses to enumerate every previewable site in a
//! workspace: scan for candidate entry files, call [`entry_site_info`] on
//! each, and nest members under their including entry.

use std::path::{Path, PathBuf};

use crate::build::{collect_pages, is_skill_site_block, open_doc_for_edit, site_name};
use crate::include::{IncludeSpec, resolve_included};
use crate::render::{field_bool, field_utf8, label_string};

/// One `site` block declared by an entry document (or the synthetic
/// default site of a document that declares pages but no `site` block).
pub struct EntrySiteInfo {
    /// The `site` block's name label — the build's `site_filter`. `None`
    /// for an unnamed single site or the synthetic default (build with no
    /// filter).
    pub site: Option<String>,
    /// The site's `title` field, if set.
    pub title: Option<String>,
    /// Whether the site is marked `root = true`.
    pub root: bool,
    /// Whether it is a skill site (`default_template = :ai_skill`) — not
    /// buildable by the HTML target.
    pub skill: bool,
}

/// One `include`-block member: a child sub-site entry document.
pub struct EntryIncludeInfo {
    /// The member's nav/display name (its subdirectory).
    pub name: String,
    /// The member's entry `.wcl`, canonicalized when possible.
    pub entry: PathBuf,
    /// The include's `site` selector for the member build, if narrowing.
    pub site: Option<String>,
}

/// The sites `entry` declares plus its resolved `include` members.
///
/// A document with pages but no `site` block yields one synthetic
/// [`EntrySiteInfo`] with `site: None`. Include blocks that fail to
/// resolve (folder missing, malformed options) are skipped silently —
/// enumeration degrades rather than fails, matching the nav's
/// `read_entry_meta` posture; the build step is the authority on errors.
pub fn entry_site_info(
    entry: &Path,
) -> Result<(Vec<EntrySiteInfo>, Vec<EntryIncludeInfo>), wcl_lang::ParseError> {
    let doc = open_doc_for_edit(entry)?;

    let mut sites: Vec<EntrySiteInfo> = doc
        .blocks()
        .filter(|b| b.kind() == "site")
        .map(|b| EntrySiteInfo {
            site: site_name(&b),
            title: field_utf8(&b, "title"),
            root: field_bool(&b, "root").unwrap_or(false),
            skill: is_skill_site_block(&b),
        })
        .collect();
    if sites.is_empty() {
        let has_pages = collect_pages(&doc).map(|p| !p.is_empty()).unwrap_or(false);
        if has_pages {
            sites.push(EntrySiteInfo {
                site: None,
                title: None,
                root: false,
                skill: false,
            });
        }
    }

    let base_dir = entry.parent();
    let mut includes: Vec<EntryIncludeInfo> = Vec::new();
    for b in doc.blocks().filter(|b| b.kind() == "include") {
        let Some(folder) = label_string(&b) else {
            continue;
        };
        let spec = IncludeSpec {
            folder,
            pattern: field_utf8(&b, "pattern"),
            entry: field_utf8(&b, "entry"),
            site: field_utf8(&b, "site"),
            prefix: field_utf8(&b, "prefix"),
        };
        let Ok(members) = resolve_included(base_dir, &spec) else {
            continue;
        };
        includes.extend(members.into_iter().map(|m| EntryIncludeInfo {
            name: m.name,
            entry: std::fs::canonicalize(&m.src_path).unwrap_or(m.src_path),
            site: m.site,
        }));
    }

    Ok((sites, includes))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    #[test]
    fn sites_and_include_members_enumerate() {
        let td = tempfile::tempdir().unwrap();
        let root = td.path();
        write(
            &root.join("main.wcl"),
            "import <wdoc.wcl>\n\nsite docs {\n  title = \"The Docs\"\n  root = true\n}\n\nsite deck {\n  default_template = :ai_skill\n}\n\npage index {\n  title = \"Hi\"\n  sites = [:docs]\n\n  h1 \"Hi\"\n}\n\ninclude \"members\" {\n  entry = \"main.wcl\"\n}\n",
        );
        write(
            &root.join("members/alpha/main.wcl"),
            "import <wdoc.wcl>\n\npage index {\n  title = \"Alpha\"\n\n  h1 \"Alpha\"\n}\n",
        );

        let (sites, includes) = entry_site_info(&root.join("main.wcl")).unwrap();
        assert_eq!(sites.len(), 2);
        let docs = &sites[0];
        assert_eq!(docs.site.as_deref(), Some("docs"));
        assert_eq!(docs.title.as_deref(), Some("The Docs"));
        assert!(docs.root);
        assert!(!docs.skill);
        let deck = &sites[1];
        assert_eq!(deck.site.as_deref(), Some("deck"));
        assert!(deck.skill);

        assert_eq!(includes.len(), 1);
        assert_eq!(includes[0].name, "alpha");
        assert!(includes[0].entry.ends_with("members/alpha/main.wcl"));

        // The member has pages but no `site` block: one synthetic default.
        let (member_sites, member_includes) = entry_site_info(&includes[0].entry).unwrap();
        assert_eq!(member_sites.len(), 1);
        assert!(member_sites[0].site.is_none());
        assert!(member_includes.is_empty());
    }

    #[test]
    fn missing_include_folder_degrades_to_no_members() {
        let td = tempfile::tempdir().unwrap();
        let entry = td.path().join("main.wcl");
        write(
            &entry,
            "import <wdoc.wcl>\n\npage index {\n  title = \"Hi\"\n\n  h1 \"Hi\"\n}\n\ninclude \"nowhere\" {\n  entry = \"main.wcl\"\n}\n",
        );
        let (sites, includes) = entry_site_info(&entry).unwrap();
        assert_eq!(sites.len(), 1);
        assert!(includes.is_empty());
    }
}
