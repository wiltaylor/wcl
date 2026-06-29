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

# Run one cargo-fuzz target (nightly + cargo-fuzz required); pass extra flags after --
[group('test')]
fuzz-run TARGET *ARGS:
    cd crates/wcl_lang && cargo +nightly fuzz run {{TARGET}} {{ARGS}}

# Bounded fuzz sweep across every target (~15s each, ~75s total)
[group('test')]
fuzz-sweep:
    @for t in parse eval format_round_trip json_round_trip set_edit_path; do \
        echo "==> fuzz $t" >&2; \
        (cd crates/wcl_lang && cargo +nightly fuzz run "$t" -- -runs=2000 -max_total_time=15) || exit 1; \
    done

# Format all code
[group('quality')]
workspace-fmt:
    cargo fmt --all

# Lint with clippy (warnings are errors)
[group('quality')]
workspace-lint:
    cargo clippy --workspace --all-targets -- -D warnings

# Full CI gate: fmt-check + workspace-lint + workspace-test + docs-build
[group('quality')]
ci: fmt-check workspace-lint workspace-test docs-build

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

# Build the WCL architecture book (.wad/) into .wad/_site/ (gitignored)
[group('dev')]
wad-build *ARGS:
    cargo run -p wcl -- wdoc build .wad/wdoc/book/main.wcl --out .wad/_site {{ARGS}}

# List review @comments left in docs/ via `just docs-serve` (--format json, or `resolve <id>` to delete one)
[group('dev')]
docs-comments *ARGS:
    cargo run -p wcl -- wdoc comments docs/main.wcl {{ARGS}}

# Render the project's docs/ site to Markdown under docs/_md/ (gitignored) —
# smoke-tests `wcl wdoc markdown` (folder of .md + standalone .svg diagrams)
[group('dev')]
md-build *ARGS:
    cargo run -p wcl -- wdoc markdown docs/main.wcl --out docs/_md {{ARGS}}

# Build the WCL authoring skill into .claude/skills/wcl/ (committed) from the
# `wcl` wskill (docs/wskills/wcl) — smoke-tests `wcl wdoc skill`
[group('dev')]
skill-build *ARGS:
    cargo run -p wcl -- wdoc skill docs/wskills/wcl/wdoc/skill/main.wcl --out .claude/skills/wcl {{ARGS}}

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
