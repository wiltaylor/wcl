use assert_cmd::Command;
use predicates::prelude::*;
use std::io::Write;
use tempfile::TempDir;

fn wcl() -> Command {
    Command::cargo_bin("wcl").expect("wcl binary built")
}

#[test]
fn list_shows_builtin_templates() {
    wcl()
        .args(["init", "--list"])
        .assert()
        .success()
        .stdout(predicate::str::contains("minimal"))
        .stdout(predicate::str::contains("page"))
        .stdout(predicate::str::contains("book"))
        .stdout(predicate::str::contains("presentation"))
        .stdout(predicate::str::contains("wad"));
}

/// Each multi-folder template scaffolds the expected tree, substitutes the
/// project name into the site title, validates with `wcl check`, and builds
/// with `wcl wdoc build`.
#[test]
fn multifolder_templates_scaffold_check_and_build() {
    for template in ["page", "book", "presentation"] {
        let tmp = TempDir::new().expect("mkdir tempdir");
        let dest = tmp.path().join("proj");
        wcl()
            .args(["init", template])
            .arg(&dest)
            .args(["-D", "name=Demo Project", "--defaults"])
            .assert()
            .success();

        // The four-file, three-folder layout.
        for rel in [
            "main.wcl",
            "schema/main.wcl",
            "data/main.wcl",
            "wdoc/main.wcl",
        ] {
            assert!(dest.join(rel).exists(), "{template}: missing {rel}");
        }
        let main = std::fs::read_to_string(dest.join("main.wcl")).expect("read main.wcl");
        assert!(
            main.contains("title = \"Demo Project\""),
            "{template}: site title not substituted: {main}"
        );

        // The generated project validates and builds.
        wcl()
            .arg("check")
            .arg(dest.join("main.wcl"))
            .assert()
            .success();
        wcl()
            .args(["wdoc", "build"])
            .arg(dest.join("main.wcl"))
            .arg("--out")
            .arg(dest.join("_site"))
            .assert()
            .success()
            .stdout(predicate::str::contains("page"));
    }
}

/// The `website` scaffold has its own layout (a custom raw-HTML template +
/// shipped assets), so it gets a dedicated check rather than the shared
/// multi-folder loop above.
#[test]
fn init_website_scaffold_checks_and_builds() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let dest = tmp.path().join("proj");
    wcl()
        .args(["init", "website"])
        .arg(&dest)
        .args(["-D", "name=Acme Co", "--defaults"])
        .assert()
        .success();

    for rel in [
        "main.wcl",
        "theme.wcl",
        "components.wcl",
        "content.wcl",
        "assets/site.css",
        "assets/app.js",
    ] {
        assert!(dest.join(rel).exists(), "website: missing {rel}");
    }
    let main = std::fs::read_to_string(dest.join("main.wcl")).expect("read main.wcl");
    assert!(
        main.contains("title        = \"Acme Co\""),
        "website: site title not substituted: {main}"
    );

    wcl()
        .arg("check")
        .arg(dest.join("main.wcl"))
        .assert()
        .success();
    wcl()
        .args(["wdoc", "build"])
        .arg(dest.join("main.wcl"))
        .arg("--out")
        .arg(dest.join("_site"))
        .assert()
        .success();

    // The build wires the design assets into <head> and slots the page's
    // regions + content into the custom layout.
    let html = std::fs::read_to_string(dest.join("_site/index.html")).expect("read index.html");
    assert!(
        html.contains("assets/site.css") && html.contains("assets/app.js"),
        "website: head assets not linked: {html}"
    );
    assert!(
        html.contains("Build something great"),
        "website: hero component not rendered: {html}"
    );
    assert!(
        dest.join("_site/assets/site.css").exists(),
        "website: assets folder not copied into the build"
    );
}

#[test]
fn init_minimal_with_default_answer() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let dest = tmp.path().join("proj");
    wcl()
        .args(["init", "minimal"])
        .arg(&dest)
        .arg("--defaults")
        .assert()
        .success();
    let main = std::fs::read_to_string(dest.join("main.wcl")).expect("main.wcl written");
    // The `name` property's default flows through `answer("name")`.
    assert!(main.starts_with("// my-project"), "got: {main}");
}

#[test]
fn init_define_overrides_default() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let dest = tmp.path().join("proj");
    wcl()
        .args(["init", "minimal"])
        .arg(&dest)
        .args(["-D", "name=acme", "--defaults"])
        .assert()
        .success();
    let main = std::fs::read_to_string(dest.join("main.wcl")).expect("main.wcl written");
    assert!(main.starts_with("// acme"), "got: {main}");
    // The generated project is itself valid WCL.
    wcl()
        .arg("check")
        .arg(dest.join("main.wcl"))
        .assert()
        .success();
}

#[test]
fn init_reads_json_answer_file() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let answers = tmp.path().join("answers.json");
    std::fs::write(&answers, r#"{"name":"from-json"}"#).expect("write answers");
    let dest = tmp.path().join("proj");
    wcl()
        .args(["init", "minimal"])
        .arg(&dest)
        .arg("--answers")
        .arg(&answers)
        .arg("--defaults")
        .assert()
        .success();
    let main = std::fs::read_to_string(dest.join("main.wcl")).expect("main.wcl written");
    assert!(main.starts_with("// from-json"), "got: {main}");
}

#[test]
fn init_reads_wcl_answer_file() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let answers = tmp.path().join("answers.wcl");
    std::fs::write(&answers, "name = \"from-wcl\"\n").expect("write answers");
    let dest = tmp.path().join("proj");
    wcl()
        .args(["init", "minimal"])
        .arg(&dest)
        .arg("--answers")
        .arg(&answers)
        .arg("--defaults")
        .assert()
        .success();
    let main = std::fs::read_to_string(dest.join("main.wcl")).expect("main.wcl written");
    assert!(main.starts_with("// from-wcl"), "got: {main}");
}

#[test]
fn init_refuses_nonempty_dest_without_force() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let dest = tmp.path().join("proj");
    std::fs::create_dir_all(&dest).expect("mkdir dest");
    std::fs::write(dest.join("keep.txt"), "x").expect("write file");
    wcl()
        .args(["init", "minimal"])
        .arg(&dest)
        .arg("--defaults")
        .assert()
        .code(4)
        .stderr(predicate::str::contains("not empty"));
    // `--force` writes into the existing directory.
    wcl()
        .args(["init", "minimal"])
        .arg(&dest)
        .args(["--defaults", "--force"])
        .assert()
        .success();
    assert!(dest.join("main.wcl").exists());
}

/// A template folder under `$XDG_DATA_HOME/wcl/templates/<name>` is listed
/// by `--list` and usable by name.
#[test]
fn init_resolves_user_template_from_xdg_data_dir() {
    let xdg = TempDir::new().expect("mkdir tempdir");
    let tdir = xdg.path().join("wcl").join("templates").join("greeting");
    std::fs::create_dir_all(&tdir).expect("mkdir template dir");
    std::fs::write(
        tdir.join("template.wcl"),
        "import <scaffold.wcl>\n\
         property \"who\" { default = \"world\" }\n\
         file \"hello.txt\" { content = $<<H\nHello, ${answer(\"who\")}!\nH\n }\n",
    )
    .expect("write manifest");

    // Listed under the user-templates section.
    wcl()
        .args(["init", "--list"])
        .env("XDG_DATA_HOME", xdg.path())
        .assert()
        .success()
        .stdout(predicate::str::contains("greeting"));

    // Usable by name; the answer is substituted.
    let out = TempDir::new().expect("mkdir tempdir");
    let dest = out.path().join("proj");
    wcl()
        .args(["init", "greeting"])
        .arg(&dest)
        .args(["-D", "who=WCL", "--defaults"])
        .env("XDG_DATA_HOME", xdg.path())
        .assert()
        .success();
    let hello = std::fs::read_to_string(dest.join("hello.txt")).expect("hello.txt written");
    assert_eq!(hello.trim(), "Hello, WCL!");
}

#[test]
fn init_unknown_template_errors() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    wcl()
        .args(["init", "definitely-not-a-template"])
        .arg(tmp.path().join("proj"))
        .arg("--defaults")
        .assert()
        .code(4)
        .stderr(predicate::str::contains("unknown template"));
}

/// The `wad` scaffold: full tree, answer substitution, model + book template
/// validate, and both the HTML and Markdown renders succeed on the empty
/// eleven-chapter book.
#[test]
fn init_wad_scaffold_checks_and_builds() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let dest = tmp.path().join("proj");
    wcl()
        .args(["init", "wad"])
        .arg(&dest)
        .args([
            "-D",
            "system_id=demo",
            "-D",
            "system_name=Demo System",
            "-D",
            "system_summary=A demo estate.",
            "--defaults",
        ])
        .assert()
        .success();

    for rel in [
        "wad.wcl",
        "README.md",
        "justfile",
        ".gitignore",
        "schema/base.wcl",
        "schema/kinds.wcl",
        "schema/extensions.wcl",
        "data/main.wcl",
        "data/overview/main.wcl",
        "data/specs/main.wcl",
        "data/generated/main.wcl",
        "data/generated/repo.wcl",
        "scripts/README.md",
        "scripts/extract_repo.py",
        "wdoc/book/main.wcl",
        "wdoc/pages/systems.wcl",
        "wdoc/pages/domain.wcl",
    ] {
        assert!(dest.join(rel).exists(), "wad: missing {rel}");
    }
    let wad = std::fs::read_to_string(dest.join("wad.wcl")).expect("read wad.wcl");
    assert!(
        wad.contains("wad demo {") && wad.contains("name    = \"Demo System\""),
        "wad: answers not substituted: {wad}"
    );

    wcl()
        .arg("check")
        .arg(dest.join("wad.wcl"))
        .assert()
        .success();
    wcl()
        .arg("check")
        .arg(dest.join("wdoc/book/main.wcl"))
        .assert()
        .success();
    wcl()
        .args(["wdoc", "build"])
        .arg(dest.join("wdoc/book/main.wcl"))
        .arg("--out")
        .arg(dest.join("_site"))
        .assert()
        .success();
    wcl()
        .args(["wdoc", "markdown"])
        .arg(dest.join("wdoc/book/main.wcl"))
        .arg("--out")
        .arg(dest.join("_md"))
        .assert()
        .success();
    // The empty book still renders every chapter landing.
    for page in ["index.html", "system_context.html", "specs_page.html"] {
        assert!(
            dest.join("_site").join(page).exists(),
            "wad: missing {page}"
        );
    }
}

/// `include_extractors=no` drops scripts/ and the generated placeholder
/// import, and the result still validates.
#[test]
fn init_wad_without_extractors() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let dest = tmp.path().join("proj");
    wcl()
        .args(["init", "wad"])
        .arg(&dest)
        .args(["-D", "include_extractors=no", "--defaults"])
        .assert()
        .success();

    assert!(
        !dest.join("scripts").exists(),
        "wad: scripts/ not gated off"
    );
    assert!(
        !dest.join("data/generated/repo.wcl").exists(),
        "wad: generated placeholder not gated off"
    );
    let generated_hub = std::fs::read_to_string(dest.join("data/generated/main.wcl"))
        .expect("read generated/main.wcl");
    assert!(
        !generated_hub.contains("import"),
        "wad: generated hub should carry no imports without extractors: {generated_hub}"
    );
    wcl()
        .arg("check")
        .arg(dest.join("wad.wcl"))
        .assert()
        .success();
}

/// The wplan template scaffolds a plan folder that validates, passes its
/// structural gates, and renders both projections (book + agent briefs) —
/// the same bar the old verified tarball guaranteed.
#[test]
fn init_wplan_scaffold_checks_and_builds() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let dest = tmp.path().join("proj").join("plan");
    wcl()
        .args(["init", "wplan"])
        .arg(&dest)
        .arg("--defaults")
        .assert()
        .success();

    for rel in [
        "plan.wcl",
        "justfile",
        "schema/plan_schema.wcl",
        "questions.wcl",
        "research.wcl",
        "research", // empty folder for finding files
        "prd.wcl",
        "surfaces.wcl",
        "scenarios.wcl",
        "contracts.wcl",
        "models.wcl",
        "asbuilt.wcl",
        "signoffs.wcl",
        "specs/spec_000_repo.wcl",
        "specs/spec_010_build.wcl",
        "status.wcl",
        "lessons.wcl",
        "gates.wcl",
        "scripts/extract_plan.py",
        "wdoc/book/main.wcl",
        "wdoc/agent/main.wcl",
    ] {
        assert!(dest.join(rel).exists(), "wplan: missing {rel}");
    }

    wcl()
        .arg("check")
        .arg(dest.join("plan.wcl"))
        .assert()
        .success();
    wcl()
        .arg("check")
        .arg(dest.join("gates.wcl"))
        .assert()
        .success();
    // The structural gates a fresh template must hold green (the full
    // `just check` loop walks every gate; these two prove the seam).
    for gate in ["gates.requirements_covered.ok", "gates.harness_defined.ok"] {
        wcl()
            .arg("eval")
            .arg(dest.join("gates.wcl"))
            .arg(gate)
            .assert()
            .success()
            .stdout(predicate::str::contains("true"));
    }
    wcl()
        .args(["wdoc", "build"])
        .arg(dest.join("wdoc/book/main.wcl"))
        .arg("--out")
        .arg(dest.join("out/book"))
        .assert()
        .success();
    wcl()
        .args(["wdoc", "markdown"])
        .arg(dest.join("wdoc/agent/main.wcl"))
        .arg("--out")
        .arg(dest.join("out/specs"))
        .assert()
        .success();
    assert!(
        dest.join("out/specs/index.md").exists(),
        "wplan: agent briefs index missing"
    );
}

/// The wskill scaffold writes the topic's own files ONLY: the base schema,
/// the optional-view schemas and the shared wdoc templates ship embedded in
/// the binary (`crates/wcl_wskill/lib/`) and arrive by import. All four
/// projections still build from what is written.
#[test]
fn init_wskill_scaffolds_entries_only_and_builds_every_projection() {
    let tmp = TempDir::new().expect("mkdir tempdir");
    let dest = tmp.path().join("topic");
    wcl()
        .args(["init", "wskill"])
        .arg(&dest)
        .args([
            "-D",
            "topic_id=demo",
            "-D",
            "topic_name=Demo",
            "-D",
            "include_presentation=yes",
            "-D",
            "include_training=yes",
            "--defaults",
        ])
        .assert()
        .success();

    // What the scaffold no longer copies. Each of these was a verbatim copy
    // in every wskill folder before the library was embedded.
    for rel in [
        "schema/base.wcl",
        "schema/presentation.wcl",
        "schema/training.wcl",
        "wdoc/component/common.wcl",
        "wdoc/component/skill_md.wcl",
        "wdoc/pages/overview.wcl",
    ] {
        assert!(
            !dest.join(rel).exists(),
            "wskill: {rel} should come from the embedded library, not a copy"
        );
    }
    // …and what it still writes, because it is this topic's own.
    for rel in ["wskill.wcl", "schema/kinds.wcl", "schema/extensions.wcl"] {
        assert!(dest.join(rel).exists(), "wskill: missing {rel}");
    }
    // The book entry is an entry: the topic's model plus the shared template.
    let book = std::fs::read_to_string(dest.join("wdoc/book/main.wcl")).expect("read book main");
    assert!(
        book.contains("import <wskill/book.wcl>") && book.contains("import \"../../wskill.wcl\""),
        "wskill: book entry does not import the shared template: {book}"
    );

    // A content index must project in both link-bearing views. Keep this in
    // the scaffold test because the skill entry is topic-owned generated code,
    // while the book entry delegates to the embedded template.
    writeln!(
        std::fs::OpenOptions::new()
            .append(true)
            .open(dest.join("data/reference/reference.wcl"))
            .expect("open reference data"),
        "\nconcept alpha {{\n  name = \"Alpha\"\n  summary = \"First.\"\n  audience = :both\n  related = [{{ id: \"beta\", why: \"Beta follows Alpha.\" }}]\n}}\n\n\
         concept beta {{\n  name = \"Beta\"\n  summary = \"Second.\"\n  audience = :both\n}}\n\n\
         index guided {{\n  name = \"Guided area\"\n  audience = :both\n  related = [alpha, beta]\n  body {{ p \"Guidance.\" }}\n}}"
    )
    .expect("append bodied index");

    for rel in [
        "wskill.wcl",
        "wdoc/book/main.wcl",
        "wdoc/skill/main.wcl",
        "wdoc/presentation/main.wcl",
        "wdoc/training/main.wcl",
    ] {
        wcl().arg("check").arg(dest.join(rel)).assert().success();
    }

    for (cmd, entry, out) in [
        ("build", "wdoc/book/main.wcl", "out/book"),
        ("skill", "wdoc/skill/main.wcl", "out/skill"),
        ("build", "wdoc/presentation/main.wcl", "out/presentation"),
        ("build", "wdoc/training/main.wcl", "out/training"),
    ] {
        wcl()
            .args(["wdoc", cmd])
            .arg(dest.join(entry))
            .arg("--out")
            .arg(dest.join(out))
            .assert()
            .success();
    }
    assert!(
        dest.join("out/skill/SKILL.md").exists(),
        "wskill: the skill projection wrote no SKILL.md"
    );
    assert!(
        dest.join("out/book/index_guided.html").exists(),
        "wskill: the book projection wrote no bodied-index page"
    );
    assert!(
        dest.join("out/skill/references/index_guided.md").exists(),
        "wskill: the skill projection wrote no bodied-index page"
    );
    let alpha_skill = std::fs::read_to_string(dest.join("out/skill/references/concept_alpha.md"))
        .expect("read rendered skill unit");
    assert!(
        alpha_skill.contains("Beta follows Alpha."),
        "wskill: the skill projection dropped a related-edge reason"
    );

    writeln!(
        std::fs::OpenOptions::new()
            .append(true)
            .open(dest.join("data/reference/reference.wcl"))
            .expect("open reference data"),
        "\nindex nav_only {{\n  name = \"Navigation only\"\n  audience = :book\n}}"
    )
    .expect("append bodyless index");
    writeln!(
        std::fs::OpenOptions::new()
            .append(true)
            .open(dest.join("data/training/main.wcl"))
            .expect("open training data"),
        "\nlesson invalid_link {{\n  title = \"Invalid link\"\n  n = 99\n  related = [{{ id: \"nav_only\", why: \"The navigation-only index cannot provide a lesson page.\" }}]\n}}"
    )
    .expect("append invalid training edge");
    wcl()
        .args(["wdoc", "build"])
        .arg(dest.join("wdoc/training/main.wcl"))
        .arg("--out")
        .arg(dest.join("out/invalid-training"))
        .assert()
        .failure()
        .stderr(predicate::str::contains("bodyless index `nav_only`"));
}
