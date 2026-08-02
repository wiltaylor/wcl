set unstable
set shell := ["bash", "-cu"]

# Where install.sh copies the binary (matches its default; override with WCL_INSTALL_DIR).
bin_dir := env('WCL_INSTALL_DIR', env('HOME') / '.local/bin')

[default, private]
main:
    @just --list

# The merge bar — everything a change must pass before it can land
mod ci '.just/ci'

# Recipes the `ci` module and this justfile both need. A module can't see its
# parent's recipes, so the gate's constituents are defined once in
# .just/shared.just and imported by both.
import '.just/shared.just'

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

# Build the `wcl editor` frontend (editor-ui/ → editor-ui/dist, embedded by build.rs)
[group('build')]
editor-ui-build:
    cd editor-ui && pnpm install && pnpm build

# Build the wcl CLI in release mode (target/release/wcl)
[group('build')]
cli-build:
    cargo build --release --locked -p wcl

# Build and install the wcl CLI where install.sh puts it (WCL_INSTALL_DIR or ~/.local/bin)
[group('build')]
cli-install: cli-build
    install -D -m 755 target/release/wcl {{ bin_dir }}/wcl
    @echo "Installed wcl to {{ bin_dir }}/wcl"

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

# Format all code
[group('quality')]
workspace-fmt:
    cargo fmt --all

# Report book/skill audience coverage per wskill (units kept vs total per
# projection) — informational, not a gate. An excluded unit is otherwise
# invisible: audience defaults to :book (research: :ai), so e.g. a fact
# authored without `audience = :both` silently never reaches the skill.
[group('quality')]
wskill-coverage:
    @for d in docs/wskills/*; do \
        [ -f "$d/wskill.wcl" ] || continue; \
        echo "== $(basename "$d")"; \
        grep -vE '^\s*(//|$)' docs/wskills/coverage.repl \
            | cargo run -q -p wcl -- repl "$d/wskill.wcl" \
            | sed 's/^"//; s/"$//'; \
    done

# Propagate the canonical WAD base schema to .wad/schema/base.wcl
[group('quality')]
wad-schema-sync:
    @{{heredoc}}; heredoc {{wad_template}} WAD_SCHEMA_BASE_WCL > .wad/schema/base.wcl && echo "synced .wad/schema/base.wcl"

# Print the canonical wplan plan schema (the scaffold template's heredoc) to stdout
[private]
wplan-schema-extract:
    @sed -n "/<<'WPLAN_SCHEMA_WCL'/,/^WPLAN_SCHEMA_WCL$/p" crates/wcl/src/scaffold/templates/wplan.wcl | sed '1d;$d'

# Run the CLI: just cli-run -- parse examples/basic.wcl
[group('dev')]
cli-run *ARGS:
    cargo run -p wcl -- {{ARGS}}

# Open the browser editor on this repo (root document: docs/main.wcl, so the preview pane renders the docs site)
[group('dev')]
editor *ARGS:
    cargo run -p wcl -- editor docs/main.wcl --addr 127.0.0.1:8139 {{ARGS}}

# Serve examples/wdoc/main.wcl — hot-reload dev server (--site picks one of its three sites; flags after --). Review comments live in `just editor`'s preview pane.
[group('dev')]
wdoc-serve *ARGS:
    cargo run -p wcl -- wdoc serve examples/wdoc/main.wcl {{ARGS}}

# Serve docs/ in edit mode (landing at /, reference book at /reference/) — click a block to edit it in place. Review comments live in `just editor`'s preview pane (list with `just docs-comments`)
[group('dev')]
docs-serve *ARGS:
    cargo run -p wcl -- wdoc serve docs/main.wcl --addr 127.0.0.1:8137 --edit {{ARGS}}

# Serve the WCL architecture book (.wad/) — hot-reload dev server. Review comments live in `just editor`'s preview pane
[group('dev')]
wad-serve *ARGS:
    cargo run -p wcl -- wdoc serve .wad/wdoc/book/main.wcl --addr 127.0.0.1:8138 {{ARGS}}

# Render the WCL architecture book as AI-consumable Markdown into .wad/_md/ (gitignored)
[group('dev')]
wad-md *ARGS:
    cargo run -p wcl -- wdoc markdown .wad/wdoc/book/main.wcl --out .wad/_md {{ARGS}}

# Derive a change-spec skeleton for the WAD from a reviewed revision: just wad-spec HEAD~3
[group('dev')]
wad-spec REV:
    cargo run -p wcl -- wad spec --from {{REV}} .wad/wad.wcl

# List review @comments left in docs/ via `just editor`'s preview pane (--format json, or `resolve <id>` to delete one)
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
    @agent_names=""; \
    for d in docs/wskills/*; do \
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
            for a in "$stage/agents/"*.md; do \
                an=$(basename "$a" .md); \
                case " $agent_names " in *" $an "*) \
                    echo "agent name collision: $an declared by more than one wskill — refusing to clobber .claude/agents/$an.md"; exit 1;; \
                esac; \
                agent_names="$agent_names $an"; \
            done; \
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
