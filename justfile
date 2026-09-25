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

# Build the VS Code extension (npm ci + tsc compile in editors/vscode)
[group('build')]
vscode-build:
    cd editors/vscode && npm ci && npm run compile

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

# Run wdoc tests only (unit + integration in crates/wcl_wdoc)
[group('test')]
wdoc-test:
    cargo test -p wcl_wdoc

[private]
require-uv:
    @command -v uv >/dev/null 2>&1 || { echo "uv is required — install it from https://docs.astral.sh/uv/" >&2; exit 1; }

# Deliberately outside the merge bar: every test is at least one live model call,
# so it is billed, non-deterministic and far slower than the rest of the gate.
# Run it when the skill's wording changes, not on every commit.
#
# Depends on cli-build because three of the tests put what the agent wrote through
# target/release/wcl, and a stale binary would grade against the wrong behaviour.
#
# ARGS reach pytest directly, with no `--` separator: `just skill-test -k connection`
# narrows to one test. Passing `--` instead makes pytest read the rest as filenames.
#
# Run the .claude/skills/wcl skill tests against a real agent (billed model calls)
[group('test')]
skill-test *ARGS: require-uv cli-build
    .claude/skills/wcl/test.py {{ARGS}}

# The explicit --target pins the REAL host triple: a prebuilt musl cargo-fuzz
# binary (e.g. from taiki-e/install-action in CI) otherwise defaults to ITS OWN
# build triple, and musl + ASan don't mix.
#
# TARGET is `<crate>:<target>` (e.g. `wcl_wdoc:wdoc_build`); a bare target
# name means `wcl_lang`.
#
# Run one cargo-fuzz target (nightly + cargo-fuzz required); pass extra flags after --
[group('test')]
fuzz-run TARGET *ARGS:
    #!/usr/bin/env bash
    set -euo pipefail
    entry='{{TARGET}}'
    case "$entry" in *:*) crate="${entry%%:*}" t="${entry#*:}" ;; *) crate=wcl_lang t="$entry" ;; esac
    cd "crates/$crate" && cargo +nightly fuzz run --target "$(rustc +nightly -vV | sed -n 's/^host: //p')" "$t" {{ARGS}}

# Outside the merge bar: minutes per target, too slow for every commit. Run it
# before a release or after touching the parser, evaluator or wdoc renderer.
#
# Fuzz every target for SECONDS each with no input cap (default 10 minutes)
[group('test')]
fuzz-soak SECONDS='600': (fuzz-sweep SECONDS '-1')

# Format all code
[group('quality')]
workspace-fmt:
    cargo fmt --all

# Run the CLI: just cli-run check examples/basic.wcl
[group('dev')]
cli-run *ARGS:
    cargo run -p wcl -- {{ARGS}}

# `wdoc serve` watches for changes but rebuilds only on request: press Enter in
# its console (or POST /__wdoc_rebuild) and the open browser tab then reloads.

# Serve examples/wdoc/main.wcl (--site picks one of its four sites; rebuild with Enter)
[group('dev')]
wdoc-serve *ARGS:
    cargo run -p wcl -- wdoc serve examples/wdoc/main.wcl {{ARGS}}

# Serve the wcl.dev landing page (docs/landing/main.wcl); rebuild with Enter
[group('dev')]
docs-serve *ARGS:
    cargo run -p wcl -- wdoc serve docs/landing/main.wcl --addr 127.0.0.1:8137 {{ARGS}}

# Two entries mean two `serve` processes on two ports, so the landing's
# `./reference/` link does not resolve while serving locally.

# Serve the combined reference book (docs/reference/main.wcl); rebuild with Enter
[group('dev')]
docs-serve-ref *ARGS:
    cargo run -p wcl -- wdoc serve docs/reference/main.wcl --addr 127.0.0.1:8138 {{ARGS}}

# The surface comes out of the binary and the stdlib rather than a list, so
# adding a builtin and forgetting the chapter fails here. The script is handed
# the freshly built debug binary, never whatever `wcl` is on PATH.
#
# Check the reference book documents every builtin, subcommand and wdoc block kind
[group('dev')]
docs-coverage: require-uv
    cargo build -p wcl
    uv run docs/reference/coverage.py --wcl target/debug/wcl --check

# Smoke-tests `wcl wdoc build --type markdown`: a folder of .md plus standalone .svg diagrams.
#
# Render the reference book to Markdown under docs/_md/ (gitignored)
[group('dev')]
md-build *ARGS:
    cargo run -p wcl -- wdoc build docs/reference/main.wcl --out docs/_md --type markdown {{ARGS}}

# Render the example and the reference book to PDF under target/pdf/ — smoke-tests `wcl wdoc build --type pdf`
[group('dev')]
wdoc-pdf: (wdoc-pdf-render "examples/wdoc/main.wcl" "target/pdf/examples") (wdoc-pdf-render "docs/reference/main.wcl" "target/pdf/docs")

[private]
wdoc-pdf-render file out:
    cargo run -p wcl -- wdoc build {{file}} --out {{out}} --type pdf

# Run criterion benchmarks
[group('dev')]
workspace-bench:
    cargo bench -p wcl_lang

# Profile each example with `WCL_PROFILE=1 wcl parse`; JSON profile per file to stderr
[group('dev')]
examples-profile:
    @for f in examples/*.wcl; do \
        echo "==> $f" >&2; \
        WCL_PROFILE=1 cargo run -q -p wcl -- parse "$f" >/dev/null; \
        echo >&2; \
    done
