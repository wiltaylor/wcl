set unstable
set shell := ["bash", "-cu"]

# Where install.sh copies the binary (matches its default; override with WCL_INSTALL_DIR).
bin_dir := env('WCL_INSTALL_DIR', env('HOME') / '.local/bin')

[default, private]
main:
    @just --list

# Build the workspace
[group('build')]
workspace-build:
    cargo build --workspace

# Build the VS Code extension (npm install + tsc compile in editors/vscode)
[group('build')]
vscode-build:
    cd editors/vscode && npm install && npm run compile

# Package the VS Code extension as a .vsix (uses @vscode/vsce via npx)
[group('build')]
vscode-package: vscode-build
    cd editors/vscode && npx --yes @vscode/vsce package

# Install the freshly-built .vsix into VS Code (requires `code` on PATH)
[group('build')]
vscode-install: vscode-package
    cd editors/vscode && code --install-extension "$(ls -t wcl-vscode-*.vsix | head -1)" --force

# Build the wcl CLI in release mode (target/release/wcl)
[group('build')]
cli-build:
    cargo build --release --locked -p wcl

# Build and install the wcl CLI where install.sh puts it (WCL_INSTALL_DIR or ~/.local/bin)
[group('build')]
cli-install: cli-build
    install -D -m 755 target/release/wcl {{ bin_dir }}/wcl
    @echo "Installed wcl to {{ bin_dir }}/wcl"

# Run all tests (unit + integration)
[group('test')]
workspace-test:
    cargo test --workspace

# Run wdoc tests only (unit + integration in crates/wcl_wdoc)
[group('test')]
wdoc-test:
    cargo test -p wcl_wdoc

# Run one cargo-fuzz target (nightly + cargo-fuzz required); pass extra flags after --.
# The explicit --target pins the REAL host triple: a prebuilt musl cargo-fuzz
# binary (e.g. from taiki-e/install-action in CI) otherwise defaults to ITS OWN
# build triple, and musl + ASan don't mix.
[group('test')]
fuzz-run TARGET *ARGS:
    cd crates/wcl_lang && cargo +nightly fuzz run --target "$(rustc +nightly -vV | sed -n 's/^host: //p')" {{TARGET}} {{ARGS}}

# Bounded fuzz sweep across every target (~15s each, ~75s total)
[group('test')]
fuzz-sweep:
    @host="$(rustc +nightly -vV | sed -n 's/^host: //p')"; \
    for t in parse eval format_round_trip json_round_trip set_edit_path; do \
        echo "==> fuzz $t" >&2; \
        (cd crates/wcl_lang && cargo +nightly fuzz run --target "$host" "$t" -- -runs=2000 -max_total_time=15) || exit 1; \
    done

# Format all code
[group('quality')]
workspace-fmt:
    cargo fmt --all

# Lint with clippy (warnings are errors)
[group('quality')]
workspace-lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Print the canonical wskill base schema (the scaffold template's heredoc) to stdout
[private]
wskill-schema-extract:
    @sed -n "/<<'WSK_SCHEMA_BASE_WCL'/,/^WSK_SCHEMA_BASE_WCL$/p" crates/wcl/src/scaffold/templates/wskill.wcl | sed '1d;$d'

# Propagate the canonical wskill base schema to every wskill under docs/wskills/
[group('quality')]
wskill-schema-sync:
    @for d in docs/wskills/*; do \
        [ -d "$d/schema" ] || continue; \
        just wskill-schema-extract > "$d/schema/base.wcl"; \
        echo "synced $d/schema/base.wcl"; \
    done

# Fail when a live wskill base.wcl drifts from the scaffold heredoc (runs in ci)
[group('quality')]
wskill-schema-check:
    @for d in docs/wskills/*; do \
        [ -d "$d/schema" ] || continue; \
        diff <(just wskill-schema-extract) "$d/schema/base.wcl" >/dev/null \
            || { echo "wskill schema drift: $d/schema/base.wcl — run 'just wskill-schema-sync'"; exit 1; }; \
    done

# Fail when the wskill scaffold's topic-agnostic wdoc templates drift from the
# live reference implementation (docs/wskills/wskill) — improvements to the
# meta wskill must be back-ported so new scaffolds don't strand on stale
# templates (runs in ci). wdoc/skill/main.wcl is exempt: its skill description
# is topic-tuned in the live instance.
[group('quality')]
wskill-template-check:
    @for pair in \
        WSK_WDOC_COMPONENT_COMMON_WCL:component/common.wcl \
        WSK_WDOC_COMPONENT_SKILL_MD_WCL:component/skill_md.wcl \
        WSK_WDOC_COMPONENT_CONCEPT_WCL:component/concept.wcl \
        WSK_WDOC_COMPONENT_ENTITY_WCL:component/entity.wcl \
        WSK_WDOC_COMPONENT_FACT_WCL:component/fact.wcl \
        WSK_WDOC_COMPONENT_RESEARCH_WCL:component/research.wcl \
        WSK_WDOC_COMPONENT_PROCESS_WCL:component/process.wcl \
        WSK_WDOC_COMPONENT_TYPE_INDEX_WCL:component/type_index.wcl \
        WSK_WDOC_PAGES_OVERVIEW_WCL:pages/overview.wcl \
        WSK_WDOC_PAGES_REFERENCE_WCL:pages/reference.wcl \
        WSK_WDOC_PAGES_SKILL_LINKS_WCL:pages/skill_links.wcl \
        WSK_WDOC_PAGES_CONCEPTS_WCL:pages/concepts.wcl \
        WSK_WDOC_PAGES_ENTITIES_WCL:pages/entities.wcl \
        WSK_WDOC_PAGES_FACTS_WCL:pages/facts.wcl \
        WSK_WDOC_PAGES_PROCESSES_WCL:pages/processes.wcl \
        WSK_WDOC_BOOK_MAIN_WCL:book/main.wcl \
    ; do \
        term="${pair%%:*}"; path="${pair#*:}"; \
        diff <(sed -n "/<<'$term'/,/^$term$/p" crates/wcl/src/scaffold/templates/wskill.wcl | sed '1d;$d' | sed -z 's/\n*$/\n/') \
             <(sed -z 's/\n*$/\n/' "docs/wskills/wskill/wdoc/$path") >/dev/null \
            || { echo "wskill template drift: docs/wskills/wskill/wdoc/$path vs scaffold heredoc $term — back-port one side"; exit 1; }; \
    done

# Print the canonical WAD base schema (the scaffold template's heredoc) to stdout
[private]
wad-schema-extract:
    @sed -n "/<<'WAD_SCHEMA_BASE_WCL'/,/^WAD_SCHEMA_BASE_WCL$/p" crates/wcl/src/scaffold/templates/wad.wcl | sed '1d;$d'

# Propagate the canonical WAD base schema to .wad/schema/base.wcl
[group('quality')]
wad-schema-sync:
    @just wad-schema-extract > .wad/schema/base.wcl && echo "synced .wad/schema/base.wcl"

# Fail when .wad/schema/base.wcl drifts from the scaffold heredoc (runs in ci)
[group('quality')]
wad-schema-check:
    @diff <(just wad-schema-extract) .wad/schema/base.wcl >/dev/null \
        || { echo "wad schema drift: .wad/schema/base.wcl — run 'just wad-schema-sync'"; exit 1; }

# Validate the WCL architecture document (.wad/) — model + book template
[group('quality')]
wad-check:
    cargo run -p wcl -- check .wad/wad.wcl
    cargo run -p wcl -- check .wad/wdoc/book/main.wcl

# Fail when .wad/data/generated/ is stale — re-runs every extractor and requires a quiet git tree after (runs in ci; needs uv and full git history).
# releases.wcl is exempt: it derives from git tags, and the release pipeline creates a tag AFTER the commit CI builds from, so it can never be
# fresh at gate time — the keep-current sweep picks new releases up instead.
[group('quality')]
wad-extract-check: wad-extract
    @if [ -n "$(git status --porcelain -- .wad/data/generated ':(exclude).wad/data/generated/releases.wcl')" ]; then \
        git --no-pager diff -- .wad/data/generated ':(exclude).wad/data/generated/releases.wcl' | head -80; \
        echo "wad generated-data drift: run 'just wad-extract' and commit the result"; \
        exit 1; \
    fi
    @git checkout -q -- .wad/data/generated/releases.wcl 2>/dev/null || true
    @echo "wad-extract-check OK — .wad/data/generated is fresh (releases.wcl exempt: tag-derived)"

# Fail when the wad wskill's hand-reflected fact tables lag the WAD schema version (runs in ci)
[group('quality')]
wad-facts-check:
    @V=$(grep -m1 'schema_version = ' crates/wcl/src/scaffold/templates/wad.wcl | grep -oE '[0-9]+\.[0-9]+\.[0-9]+'); \
    grep -q "hand-reflected from schema $V" docs/wskills/wad/data/reference/facts.wcl \
        || { echo "wad wskill facts drift: docs/wskills/wad/data/reference/facts.wcl must say 'hand-reflected from schema $V' — re-reflect the fact tables against the current WAD schema, then update the header"; exit 1; }
    @echo "wad-facts-check OK"

# Print the canonical wplan plan schema (the scaffold template's heredoc) to stdout
[private]
wplan-schema-extract:
    @sed -n "/<<'WPLAN_SCHEMA_WCL'/,/^WPLAN_SCHEMA_WCL$/p" crates/wcl/src/scaffold/templates/wplan.wcl | sed '1d;$d'

# Scaffold the shipped wplan template into target/ and run its structural gates (runs in ci)
[group('quality')]
wplan-template-check:
    rm -rf target/wplan-template-check
    cargo run -p wcl -- init wplan target/wplan-template-check --defaults
    cargo run -p wcl -- check target/wplan-template-check/plan.wcl
    cargo run -p wcl -- check target/wplan-template-check/gates.wcl
    @for g in $(grep -oE '^gate [a-z_0-9]+' target/wplan-template-check/gates.wcl | cut -d' ' -f2 | grep -v '^signoffs_complete$'); do \
        printf 'gate %-24s ' "$g"; \
        cargo run -q -p wcl -- eval target/wplan-template-check/gates.wcl "gates.$g.ok" || exit 1; \
    done

# Full CI gate: fmt-check + workspace-lint + workspace-test + schema drift checks + doc builds
[group('quality')]
ci: fmt-check workspace-lint workspace-test wskill-schema-check wskill-template-check wad-schema-check wad-check wad-extract-check wad-facts-check wad-build wplan-template-check docs-build

# Run the CLI: just cli-run -- parse examples/basic.wcl
[group('dev')]
cli-run *ARGS:
    cargo run -p wcl -- {{ARGS}}

# Serve examples/wdoc/main.wcl in comment mode — click a block to leave a review note (--site picks one of its three sites; flags after --)
[group('dev')]
wdoc-serve *ARGS:
    cargo run -p wcl -- wdoc serve examples/wdoc/main.wcl --comment {{ARGS}}

# Serve docs/ in comment + edit mode (landing at /, reference book at /reference/) — click a block to leave a review note (list with `just docs-comments`) or edit it in place
[group('dev')]
docs-serve *ARGS:
    cargo run -p wcl -- wdoc serve docs/main.wcl --addr 127.0.0.1:8137 --comment --edit {{ARGS}}

# Build the project's docs/ site into docs/_site/ (gitignored)
[group('dev')]
docs-build *ARGS:
    cargo run -p wcl -- wdoc build docs/main.wcl --out docs/_site {{ARGS}}

# Serve the WCL architecture book (.wad/) in comment mode — hot reload, click a block to leave a review note
[group('dev')]
wad-serve *ARGS:
    cargo run -p wcl -- wdoc serve .wad/wdoc/book/main.wcl --addr 127.0.0.1:8138 --comment {{ARGS}}

# Build the WCL architecture book (.wad/) into .wad/_site/ (gitignored).
# The output dir is wiped first: a build never deletes pages that no longer
# exist, so removed entities would otherwise linger as orphaned HTML.
[group('dev')]
wad-build *ARGS:
    rm -rf .wad/_site
    cargo run -p wcl -- wdoc build .wad/wdoc/book/main.wcl --out .wad/_site {{ARGS}}

# Render the WCL architecture book as AI-consumable Markdown into .wad/_md/ (gitignored)
[group('dev')]
wad-md *ARGS:
    cargo run -p wcl -- wdoc markdown .wad/wdoc/book/main.wcl --out .wad/_md {{ARGS}}

# Run every WAD extractor script (rewrites .wad/data/generated/), then re-validate
[group('dev')]
wad-extract: && wad-check
    for s in .wad/scripts/extract_*.py; do echo "==> $s" >&2; uv run "$s"; done

# Derive a change-spec skeleton for the WAD from a reviewed revision: just wad-spec HEAD~3
[group('dev')]
wad-spec REV:
    cargo run -p wcl -- wad spec --from {{REV}} .wad/wad.wcl

# List review @comments left in docs/ via `just docs-serve` (--format json, or `resolve <id>` to delete one)
[group('dev')]
docs-comments *ARGS:
    cargo run -p wcl -- wdoc comments docs/main.wcl {{ARGS}}

# Render the project's docs/ site to Markdown under docs/_md/ (gitignored) —
# smoke-tests `wcl wdoc markdown` (folder of .md + standalone .svg diagrams)
[group('dev')]
md-build *ARGS:
    cargo run -p wcl -- wdoc markdown docs/main.wcl --out docs/_md {{ARGS}}

# Install every wskill's AI skill into .claude/skills/<name>/ (committed) —
# discovery matches the registry (skill entry file presence under docs/wskills/*),
# so a new wskill is picked up with no list to maintain; each target folder is
# replaced wholesale so removed pages don't linger. Smoke-tests `wcl wdoc skill`.
[group('dev')]
skills-install:
    @mkdir -p .claude/agents target/skills-stage && rm -rf target/skills-stage/*
    @for d in docs/wskills/*; do \
        [ -f "$d/wdoc/skill/main.wcl" ] || continue; \
        name=$(basename "$d"); \
        echo "==> $name" >&2; \
        stage="target/skills-stage/$name"; \
        cargo run -q -p wcl -- wdoc skill "$d/wdoc/skill/main.wcl" --out "$stage"; \
        for md in "$stage/SKILL.md" "$stage"/*/SKILL.md; do \
            [ -f "$md" ] || continue; \
            sd=$(dirname "$md"); \
            sn=$(sed -n 's/^name: *//p' "$md" | head -1 | sed 's/^"//; s/"$//'); \
            rm -rf ".claude/skills/$sn"; \
            cp -r "$sd" ".claude/skills/$sn"; \
        done; \
        if [ -d "$stage/agents" ]; then \
            echo "    agents: $(ls "$stage/agents")" >&2; \
            cp "$stage/agents/"*.md .claude/agents/; \
        fi; \
    done

# Render the example and the docs to PDF under target/pdf/ — smoke-tests `wcl wdoc pdf`
[group('dev')]
wdoc-pdf: (wdoc-pdf-render "examples/wdoc/main.wcl" "target/pdf/examples") (wdoc-pdf-render "docs/main.wcl" "target/pdf/docs")

[private]
wdoc-pdf-render file out:
    cargo run -p wcl -- wdoc pdf {{file}} --out {{out}}

# Run criterion benchmarks
[group('dev')]
workspace-bench:
    cargo bench -p wcl_lang

# Profile each example with `wcl parse --profile`; JSON profile per file to stderr
[group('dev')]
examples-profile:
    @for f in examples/*.wcl; do \
        echo "==> $f" >&2; \
        cargo run -q -p wcl -- parse --profile "$f" >/dev/null; \
        echo >&2; \
    done

[private]
fmt-check:
    cargo fmt --all -- --check
