//! Integration tests for the skill-folder target (`wcl wdoc skill`).

use std::path::Path;

use tempfile::TempDir;
use wcl_wdoc::{BuildError, skill};

/// Write a wdoc fixture, prepending the `import <wdoc.wcl>` line a real
/// document needs.
fn write_fixture(path: impl AsRef<Path>, body: &str) {
    let composed = format!("import <wdoc.wcl>\n{body}");
    std::fs::write(path, composed).expect("write wdoc fixture");
}

/// Build a skill into a fresh temp dir, returning it and the output root.
fn build(body: &str) -> (TempDir, std::path::PathBuf) {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(&src, body);
    let out = tmp.path().join("out");
    match skill(&src, &out, None) {
        Ok(n) => assert!(n >= 1, "at least one page written"),
        Err(BuildError::Io(e, ctx)) => panic!("skill io error: {ctx}: {e}"),
        Err(BuildError::Parse(r)) => panic!("skill parse error: {r:?}"),
        Err(BuildError::Schema(n)) => panic!("skill schema error: {n} violations"),
        Err(BuildError::BadPage(m)) => panic!("skill bad-page error: {m}"),
        Err(BuildError::BadLink(m)) => panic!("skill bad-link error: {m:?}"),
        Err(_) => panic!("skill error"),
    }
    (tmp, out)
}

fn read(out: &Path, rel: &str) -> String {
    std::fs::read_to_string(out.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

const SITE: &str = "site s {\n  default_template = :ai_skill\n  \
     skill {\n    name = \"demo-skill\"\n    description = \"A demo skill.\"\n  }\n}\n";

#[test]
fn start_page_becomes_skill_md_with_front_matter() {
    let (_t, out) = build(&format!(
        "{SITE}page overview {{ start = true\n  h1 \"Demo\"\n}}\n"
    ));
    assert!(out.join("SKILL.md").is_file(), "start page → SKILL.md");
    let md = read(&out, "SKILL.md");
    assert!(md.starts_with("---\n"), "front matter fence: {md}");
    assert!(md.contains("name: demo-skill"), "name from skill block");
    assert!(
        md.contains("description: A demo skill."),
        "description from skill block"
    );
    assert!(md.contains("# Demo"), "page body rendered");
}

#[test]
fn other_pages_go_under_references() {
    let (_t, out) = build(&format!(
        "{SITE}page overview {{ start = true\n  h1 \"Demo\"\n}}\npage usage {{\n  h1 \"Usage\"\n}}\n"
    ));
    assert!(out.join("references/usage.md").is_file(), "→ references/");
    assert!(!out.join("usage.md").exists(), "not at the root");
}

#[test]
fn internal_links_resolve_into_skill_layout() {
    let (_t, out) = build(&format!(
        "{SITE}page overview {{ start = true\n  p \"See the [guide](usage).\"\n}}\n\
         page usage {{\n  p \"Back to [overview](overview).\"\n}}\n"
    ));
    // From SKILL.md (root) → references/usage.md
    assert!(
        read(&out, "SKILL.md").contains("(references/usage.md)"),
        "start → reference link"
    );
    // From a reference page → ../SKILL.md
    assert!(
        read(&out, "references/usage.md").contains("(../SKILL.md)"),
        "reference → start link with ../"
    );
}

#[test]
fn extra_front_matter_keys_merge() {
    let (_t, out) = build(&format!(
        "{SITE}page overview {{ start = true\n  \
         @schemaless frontmatter {{\n    version = \"1.0.0\"\n  }}\n  h1 \"Demo\"\n}}\n"
    ));
    let md = read(&out, "SKILL.md");
    assert!(md.contains("name: demo-skill"), "canonical key kept");
    assert!(md.contains("version: 1.0.0"), "extra key merged in");
}

#[test]
fn hyphenated_front_matter_keys_emit_for_skill_spec() {
    // Skill-spec keys are hyphenated (`allowed-tools`, …). A string-literal
    // key in the start page's `@schemaless frontmatter` block emits the
    // hyphenated YAML key verbatim, merged after the canonical fields.
    let (_t, out) = build(&format!(
        "{SITE}page overview {{ start = true\n  \
         @schemaless frontmatter {{\n    \"allowed-tools\" = [\"Bash\", \"Read\"]\n  }}\n  h1 \"Demo\"\n}}\n"
    ));
    let md = read(&out, "SKILL.md");
    assert!(md.contains("name: demo-skill"), "canonical key kept");
    assert!(
        md.contains("allowed-tools:\n  - Bash\n  - Read"),
        "hyphenated skill key merged in: {md}"
    );
}

#[test]
fn front_matter_reads_none_valued_optional() {
    // A frontmatter value that reads a none-valued optional document field
    // (via `??` or `match`) must evaluate to the fallback, not fail the build.
    let (_t, out) = build(&format!(
        "@block(\"meta\") type Meta {{ created: utf8  updated: utf8? }}\n\
         @document type Doc {{ @child(\"meta\") meta: Meta }}\n\
         meta {{ created = \"2026-06-13\" }}\n\
         {SITE}page overview {{ start = true\n  \
         @schemaless frontmatter {{\n    \
         coalesce = $\"${{meta.updated ?? meta.created}}\"\n    \
         matched = $\"${{match meta.updated {{ none => meta.created, u => u }}}}\"\n  }}\n  \
         h1 \"Demo\"\n}}\n"
    ));
    let md = read(&out, "SKILL.md");
    assert!(
        md.contains("coalesce: 2026-06-13"),
        "?? fallback read: {md}"
    );
    assert!(
        md.contains("matched: 2026-06-13"),
        "match fallback read: {md}"
    );
}

#[test]
fn file_blocks_ship_into_their_dir() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    std::fs::write(tmp.path().join("setup.sh"), "#!/bin/sh\necho hi\n").unwrap();
    std::fs::write(tmp.path().join("logo.svg"), "<svg/>\n").unwrap();
    let src = tmp.path().join("doc.wcl");
    write_fixture(
        &src,
        &format!(
            "{SITE}page overview {{ start = true\n  \
             file \"setup.sh\" {{ dir = \"scripts\"  as = \"setup\" }}\n  \
             file \"logo.svg\" {{ dir = \"assets\" }}\n}}\n"
        ),
    );
    let out = tmp.path().join("out");
    if skill(&src, &out, None).is_err() {
        panic!("skill build failed");
    }
    assert!(out.join("scripts/setup.sh").is_file(), "script shipped");
    assert!(out.join("assets/logo.svg").is_file(), "asset shipped");
    // `as` renders a link; the silent file does not.
    let md = read(&out, "SKILL.md");
    assert!(
        md.contains("[setup](scripts/setup.sh)"),
        "linked file: {md}"
    );
    assert!(!md.contains("logo"), "silent file leaves no text");
}

#[test]
fn non_skill_site_is_rejected() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(
        &src,
        "site s {\n  default_template = :webpage\n}\npage p { start = true\n  h1 \"X\"\n}\n",
    );
    let out = tmp.path().join("out");
    match skill(&src, &out, None) {
        Err(BuildError::BadPage(m)) => assert!(m.contains("ai_skill"), "actionable message: {m}"),
        Err(_) => panic!("expected BadPage error"),
        Ok(_) => panic!("non-skill site must error"),
    }
}

#[test]
fn builds_only_the_skill_site_in_a_multi_site_doc() {
    // A `:book` web site and a `:ai_skill` site sharing one page, plus the
    // skill's own start page. `skill` (no `--site`) builds only the skill.
    let body = "site book { default_template = :book }\n\
         site sk { default_template = :ai_skill\n  \
         skill { name = \"sk\"  description = \"D.\" }\n}\n\
         page home { sites = [:sk]  start = true\n  h1 \"Home\"\n  p \"See [shared](shared).\"\n}\n\
         page shared { sites = [:book, :sk]\n  h1 \"Shared\"\n}\n\
         page webonly { sites = [:book]\n  h1 \"Web only\"\n}\n";
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(&src, body);
    let out = tmp.path().join("out");
    let n = match skill(&src, &out, None) {
        Ok(n) => n,
        Err(_) => panic!("skill build failed"),
    };
    assert_eq!(n, 2, "only the skill site's pages (home + shared)");
    assert!(out.join("SKILL.md").is_file(), "skill start page");
    assert!(out.join("references/shared.md").is_file(), "shared page");
    // The web-only `book` site is not built by the skill target.
    assert!(!out.join("references/webonly.md").exists());
    assert!(!out.join("webonly.md").exists());
}

#[test]
fn missing_start_page_is_rejected() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(&src, &format!("{SITE}page p {{\n  h1 \"X\"\n}}\n"));
    let out = tmp.path().join("out");
    match skill(&src, &out, None) {
        Err(BuildError::BadPage(_)) => {}
        Err(_) => panic!("expected BadPage error"),
        Ok(_) => panic!("missing start page must error"),
    }
}

#[test]
fn include_fans_out_member_skills() {
    let root = TempDir::new().expect("mkdir tempdir");
    // Two members, each a single `:ai_skill` site at members/<name>/main.wcl.
    for (name, title) in [("alpha", "Alpha"), ("beta", "Beta")] {
        let dir = root.path().join("members").join(name);
        std::fs::create_dir_all(&dir).expect("mkdir member");
        write_fixture(
            dir.join("main.wcl"),
            &format!(
                "site skill {{ default_template = :ai_skill  title = \"{title}\"\n  \
                   skill {{ name = \"{name}\"  description = \"{title} skill\" }}\n}}\n\
                 page index {{ sites = [:skill]  start = true  h1 \"{title}\" }}"
            ),
        );
    }
    // Collection skill doc: no own skill site — just fans members out.
    let collection = root.path().join("collection.wcl");
    write_fixture(
        &collection,
        "include \"members\" { entry = \"main.wcl\"  site = \"skill\" }",
    );
    let out = root.path().join("dist");
    let n = match skill(&collection, &out, None) {
        Ok(n) => n,
        Err(e) => panic!("skill fan-out failed: {}", e.render_plain()),
    };
    assert!(n >= 2, "two member skills written, got {n}");
    assert!(
        out.join("members/alpha/SKILL.md").is_file(),
        "alpha SKILL.md"
    );
    assert!(out.join("members/beta/SKILL.md").is_file(), "beta SKILL.md");
    let md = std::fs::read_to_string(out.join("members/alpha/SKILL.md")).expect("alpha SKILL.md");
    assert!(
        md.contains("name: alpha"),
        "front matter from member skill block: {md}"
    );
    assert!(md.contains("# Alpha"), "member page body rendered");
}

#[test]
fn agent_blocks_emit_agent_files() {
    let (_t, out) = build(
        "site s {\n  default_template = :ai_skill\n  \
           skill {\n    name = \"demo-skill\"\n    description = \"A demo skill.\"\n  }\n}\n\
         agent \"demo-helper\" {\n  \
           description = \"Runs one demo task. Use when dispatching demo work.\"\n  \
           tools = [\"Read\", \"Bash\"]\n  \
           model = \"sonnet\"\n  \
           body {\n    h1 \"Role\"\n    p \"You are the demo helper agent.\"\n  }\n}\n\
         agent \"demo-bare\" {\n  \
           description = \"An agent with no tools list, no model, no body.\"\n}\n\
         page overview { start = true\n  h1 \"Demo\"\n}\n",
    );
    let md = read(&out, "agents/demo-helper.md");
    assert!(md.starts_with("---\n"), "front matter fence: {md}");
    assert!(md.contains("name: demo-helper"), "agent name: {md}");
    assert!(
        md.contains("description: Runs one demo task. Use when dispatching demo work."),
        "agent description: {md}"
    );
    assert!(
        md.contains("tools: \"Read, Bash\""),
        "tools comma-joined: {md}"
    );
    assert!(md.contains("model: sonnet"), "model line: {md}");
    assert!(md.contains("# Role"), "body heading rendered: {md}");
    assert!(
        md.contains("You are the demo helper agent."),
        "body prose rendered: {md}"
    );

    let bare = read(&out, "agents/demo-bare.md");
    assert!(!bare.contains("tools:"), "empty tools omitted: {bare}");
    assert!(!bare.contains("model:"), "absent model omitted: {bare}");
}

#[test]
fn duplicate_agent_names_fail_the_build() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let src = tmp.path().join("doc.wcl");
    write_fixture(
        &src,
        "site s {\n  default_template = :ai_skill\n  \
           skill { name = \"demo\"  description = \"d\" }\n}\n\
         agent \"dup\" { description = \"a\" }\n\
         agent \"dup\" { description = \"b\" }\n\
         page overview { start = true\n  h1 \"Demo\"\n}\n",
    );
    let out = tmp.path().join("out");
    match skill(&src, &out, None) {
        Err(BuildError::BadPage(m)) => assert!(m.contains("dup"), "names the duplicate: {m}"),
        Ok(n) => panic!("expected duplicate-agent error, wrote {n} pages"),
        Err(e) => panic!(
            "expected duplicate-agent BadPage error, got: {}",
            e.render_plain()
        ),
    }
}

/// Two `:ai_skill` sites in one document (none marked root) each render
/// into their own named subfolder, and document-level agents land once at
/// the output root beside them — the multi-skill wskill layout.
#[test]
fn multiple_skill_sites_render_side_by_side_with_shared_agents() {
    let (_t, out) = build(
        "site alpha {\n  default_template = :ai_skill\n  \
           skill { name = \"alpha\"  description = \"Alpha skill.\" }\n}\n\
         site beta {\n  default_template = :ai_skill\n  \
           skill { name = \"beta\"  description = \"Beta skill.\" }\n}\n\
         agent \"shared-helper\" { description = \"Shared by both skills.\" }\n\
         page a_start { sites = [:alpha]  start = true\n  h1 \"Alpha\"\n}\n\
         page b_start { sites = [:beta]  start = true\n  h1 \"Beta\"\n}\n",
    );
    let a = read(&out, "alpha/SKILL.md");
    assert!(a.contains("name: alpha"), "alpha front matter: {a}");
    let b = read(&out, "beta/SKILL.md");
    assert!(b.contains("name: beta"), "beta front matter: {b}");
    let ag = read(&out, "agents/shared-helper.md");
    assert!(
        ag.contains("name: shared-helper"),
        "shared agent at the root: {ag}"
    );
    assert!(
        !out.join("alpha/agents").exists() && !out.join("beta/agents").exists(),
        "agents are not duplicated into the skill folders"
    );
}

#[test]
fn a_callout_renders_in_the_skill_folder() {
    // The skill target runs the Markdown backend, so the content IR's
    // `Callout` reaches it through the same exhaustive match — the fourth
    // of the four backends.
    let (_t, out) = build(&format!(
        "{SITE}page overview {{ start = true\n  \
           callout \"Heads up\" {{ class = [\"warning\"]  body = \"Careful **here**.\" }}\n}}\n"
    ));
    let md = read(&out, "SKILL.md");
    assert!(md.contains("> [!WARNING]"), "alert keyword: {md}");
    assert!(md.contains("> **Heads up**"), "heading: {md}");
    assert!(md.contains("> Careful **here**."), "body prose: {md}");
}

#[test]
fn nested_component_content_renders_in_the_skill_folder() {
    let (_t, out) = build(&format!(
        "wdoc_component inner {{\n  wdoc_body {{\n    h2 \"Inner frame\"\n    wdoc_content\n  }}\n}}\n\
         wdoc_component outer {{\n  wdoc_body {{\n    inner {{\n      wdoc_content\n    }}\n  }}\n}}\n\
         {SITE}page overview {{ start = true\n  outer {{\n    p \"Nested **payload**.\"\n  }}\n}}\n"
    ));
    let md = read(&out, "SKILL.md");
    assert!(
        md.contains("## Inner frame"),
        "inner component rendered:\n{md}"
    );
    assert!(
        md.contains("Nested **payload**."),
        "the outer content slot was forwarded through the inner component:\n{md}"
    );
}
