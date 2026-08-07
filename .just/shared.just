# Recipes shared between the root justfile and the `ci` module.
#
# A just module cannot see its parent's recipes, so every recipe the merge bar
# (`.just/ci/mod.just`) depends on lives here and is imported by BOTH the root
# justfile and the module. Each recipe is still defined exactly once, and both
# `just workspace-test` and `just ci::workspace-test` reach it.
#
# Keep this file to the gate's constituents (and their private helpers).
# Everything else belongs in the root justfile.

# Defines a shell function that prints one heredoc's body from a scaffold
# template. Open a recipe line with `{{heredoc}};` then call
# `heredoc <file> <TERM>` as often as you like.
#
# The scaffold heredoc is the canonical copy of the WAD schema, so both the
# sync and the check recipe need to read it out.
# Shared as a *command* rather than as a recipe because a recipe can only be
# consumed for its stdout by shelling out to `just` — which norm 7 forbids and
# which, from inside the ci module, would silently resolve against the PARENT
# justfile. Empty output is a hard error: otherwise a renamed terminator would
# leave every drift check comparing nothing against nothing, and passing.
heredoc := "heredoc() { out=$(sed -n \"/<<'$2'/,/^$2$/p\" \"$1\" | sed '1d;$d'); [ -n \"$out\" ] || { echo \"no heredoc $2 in $1\" >&2; return 1; }; printf '%s\\n' \"$out\"; }"

# The scaffold template whose heredocs are canonical
wad_template := "crates/wcl/src/scaffold/templates/wad.wcl"

# ── External tooling guards ───────────────────────────────────────────────
# So a missing tool reports as a missing tool rather than as a confusing
# build error. pnpm/node are deliberately absent: crates/wcl's build.rs
# already reports their absence itself (and offers WCL_EDITOR_UI_SKIP=1).

[private]
require-cargo:
    @command -v cargo >/dev/null 2>&1 || { echo "cargo is required — install Rust via https://rustup.rs" >&2; exit 1; }

[private]
require-git:
    @command -v git >/dev/null 2>&1 || { echo "git is required — https://git-scm.com" >&2; exit 1; }

[private]
require-uv:
    @command -v uv >/dev/null 2>&1 || { echo "uv is required to run the .wad extractor scripts — https://docs.astral.sh/uv/" >&2; exit 1; }

[private]
require-rustfmt: require-cargo
    @cargo fmt --version >/dev/null 2>&1 || { echo "the rustfmt component is required — run 'rustup component add rustfmt'" >&2; exit 1; }

[private]
require-clippy: require-cargo
    @cargo clippy --version >/dev/null 2>&1 || { echo "the clippy component is required — run 'rustup component add clippy'" >&2; exit 1; }

[private]
require-cargo-fuzz: require-cargo
    @rustc +nightly -vV >/dev/null 2>&1 || { echo "the nightly toolchain is required — run 'rustup toolchain install nightly'" >&2; exit 1; }
    @command -v cargo-fuzz >/dev/null 2>&1 || { echo "cargo-fuzz is required — run 'cargo +nightly install cargo-fuzz'" >&2; exit 1; }

# ── Quality ───────────────────────────────────────────────────────────────

# Check formatting without modifying files
[group('quality')]
fmt-check: require-rustfmt
    cargo fmt --all -- --check

# Lint with clippy (warnings are errors)
[group('quality')]
workspace-lint: require-clippy
    cargo clippy --workspace --all-targets -- -D warnings

# Run all tests (unit + integration)
[group('test')]
workspace-test: require-cargo
    cargo test --workspace

# Bounded fuzz sweep across every target (~15s each, ~75s total)
[group('test')]
fuzz-sweep: require-cargo-fuzz
    @host="$(rustc +nightly -vV | sed -n 's/^host: //p')"; \
    for t in parse eval format_round_trip json_round_trip set_edit_path; do \
        echo "==> fuzz $t" >&2; \
        (cd crates/wcl_lang && cargo +nightly fuzz run --target "$host" "$t" -- -runs=2000 -max_total_time=15) || exit 1; \
    done

# Fail when .wad/schema/base.wcl drifts from the scaffold heredoc (runs in ci)
[group('quality')]
wad-schema-check:
    @{{heredoc}}; diff <(heredoc {{wad_template}} WAD_SCHEMA_BASE_WCL) .wad/schema/base.wcl >/dev/null \
        || { echo "wad schema drift: .wad/schema/base.wcl — run 'just wad-schema-sync'"; exit 1; }

# Validate the WCL architecture document (.wad/) — model + book template
[group('quality')]
wad-check: require-cargo
    cargo run -p wcl -- check .wad/wad.wcl
    cargo run -p wcl -- check .wad/wdoc/book/main.wcl

# Needs uv and full git history. releases.wcl is exempt: it derives from git
# tags, and the release pipeline creates a tag AFTER the commit CI builds from,
# so it can never be fresh at gate time — the keep-current sweep picks new
# releases up instead.
#
# NOT side-effect-free, unlike the rest of the gate: it re-runs the extractors,
# so it REWRITES the tracked files under .wad/data/generated/ (leaving them
# rewritten when it fails), and restores releases.wcl with `git checkout`,
# discarding uncommitted edits to that one file. Freshness can't be judged
# without regenerating, and the git tree is the comparison.

# Fail when .wad/data/generated/ is stale by re-running every extractor — rewrites those tracked files (runs in ci)
[group('quality')]
wad-extract-check: require-git wad-extract
    @if [ -n "$(git status --porcelain -- .wad/data/generated ':(exclude).wad/data/generated/releases.wcl')" ]; then \
        git --no-pager diff -- .wad/data/generated ':(exclude).wad/data/generated/releases.wcl' | head -80; \
        echo "wad generated-data drift: run 'just wad-extract' and commit the result"; \
        exit 1; \
    fi
    @git checkout -q -- .wad/data/generated/releases.wcl 2>/dev/null || true
    @echo "wad-extract-check OK — .wad/data/generated is fresh (releases.wcl exempt: tag-derived)"

# Scaffold the shipped wplan template into target/ and run its structural gates (runs in ci)
[group('quality')]
wplan-template-check: require-cargo
    rm -rf target/wplan-template-check
    cargo run -p wcl -- init wplan target/wplan-template-check --defaults
    cargo run -p wcl -- check target/wplan-template-check/plan.wcl
    cargo run -p wcl -- check target/wplan-template-check/gates.wcl
    @for g in $(grep -oE '^gate [a-z_0-9]+' target/wplan-template-check/gates.wcl | cut -d' ' -f2 | grep -v '^signoffs_complete$'); do \
        printf 'gate %-24s ' "$g"; \
        cargo run -q -p wcl -- eval target/wplan-template-check/gates.wcl "gates.$g.ok" || exit 1; \
    done

# ── Builds the gate depends on ────────────────────────────────────────────

# wcl.dev is two documents: the landing page at `/` and the combined
# reference book at `/reference/`. These are the SAME two invocations, in the
# same order, that .github/workflows/deploy-site.yml runs — this recipe is a
# gate part precisely so the gate and the deploy cannot drift.
#
# Order matters: build the landing first. A build tree is self-contained and
# relocatable by design, and `wdoc build` never wipes its `--out` dir. The
# second invocation therefore writes into the first one's output safely. No
# staging dir, and nothing is copied.
#
# The wipe up front is this recipe's own. A build never deletes a page that
# no longer exists, so a removed page would linger as orphaned HTML.

# Build the project's docs/ sites into docs/_site/ (gitignored)
[group('dev')]
docs-build *ARGS: require-cargo
    rm -rf docs/_site
    cargo run -p wcl -- wdoc build docs/landing/main.wcl --out docs/_site {{ARGS}}
    cargo run -p wcl -- wdoc build docs/reference/main.wcl --out docs/_site/reference {{ARGS}}

# The output dir is wiped first, for the same reason as `docs-build`.

# Build the WCL architecture book (.wad/) into .wad/_site/ (gitignored)
[group('dev')]
wad-build *ARGS: require-cargo
    rm -rf .wad/_site
    cargo run -p wcl -- wdoc build .wad/wdoc/book/main.wcl --out .wad/_site {{ARGS}}

# Run every WAD extractor script (rewrites .wad/data/generated/), then re-validate
[group('dev')]
wad-extract: require-uv && wad-check
    for s in .wad/scripts/extract_*.py; do echo "==> $s" >&2; uv run "$s"; done
