# Recipes shared between the root justfile and the `ci` module.
#
# A just module cannot see its parent's recipes, so every recipe the merge bar
# (`.just/ci/mod.just`) depends on lives here and is imported by BOTH the root
# justfile and the module. Each recipe is still defined exactly once, and both
# `just workspace-test` and `just ci::workspace-test` reach it.
#
# Keep this file to the gate's constituents (and their private helpers).
# Everything else belongs in the root justfile.

# ── External tooling guards ───────────────────────────────────────────────
# So a missing tool reports as a missing tool rather than as a confusing
# build error.

[private]
require-cargo:
    @command -v cargo >/dev/null 2>&1 || { echo "cargo is required — install Rust via https://rustup.rs" >&2; exit 1; }

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

# Every cargo command in the gate runs `--locked`: a Cargo.lock that is out of
# date with the manifests fails the gate instead of being silently rewritten.

# The two `-A` flags exempt the documentation-coverage lints that Cargo.toml
# enables workspace-wide. They are a backlog (~2550 items when switched on),
# not a standard we currently meet, so gating on them would mean gating on
# nothing — `-D warnings` would fail every run and the ~200 other clippy
# lints would stop being enforced. `-A` after `-D warnings` overrides it for
# these two only; a plain `cargo clippy` still shows them.
#
# Delete both flags once the backlog is cleared — that is the whole switch.
#
# Lint with clippy (warnings are errors)
[group('quality')]
workspace-lint: require-clippy
    cargo clippy --locked --workspace --all-targets -- -D warnings -A missing_docs -A clippy::missing_docs_in_private_items

# Build the API docs with rustdoc warnings as errors (broken intra-doc links)
[group('quality')]
workspace-doc: require-cargo
    RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps --workspace

# `--no-fail-fast` so one failing crate does not hide failures in the rest,
# which matters most on the Windows and macOS legs where a run is slow.
#
# Run all tests (unit + integration)
[group('test')]
workspace-test: require-cargo
    cargo test --locked --workspace --no-fail-fast

# One fuzz target per line in `targets`, as `<crate dir>:<target>`, so adding
# a target is a one-line change that merges cleanly. Each crate under
# `crates/` keeps its own cargo-fuzz project in `fuzz/`.
#
# Bounded fuzz sweep across every target (~15s each)
[group('test')]
fuzz-sweep: require-cargo-fuzz
    #!/usr/bin/env bash
    set -euo pipefail
    targets=(
        wcl_lang:parse
        wcl_lang:eval
        wcl_lang:eval_fields
        wcl_lang:format_round_trip
        wcl_lang:json_round_trip
        wcl_lang:set_edit_path
        wcl_wdoc:wdoc_build
    )
    host="$(rustc +nightly -vV | sed -n 's/^host: //p')"
    for entry in "${targets[@]}"; do
        crate="${entry%%:*}"
        t="${entry#*:}"
        echo "==> fuzz $crate/$t" >&2
        (cd "crates/$crate" && cargo +nightly fuzz run --target "$host" "$t" -- -runs=2000 -max_total_time=15)
    done

# ── Builds the gate depends on ────────────────────────────────────────────

# wcl.dev is two documents: the landing page at `/` and the combined
# reference book at `/reference/`. .github/workflows/deploy-site.yml builds
# the site by running this recipe (through `docs-relative-urls`) — it is a
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
    cargo run --locked -p wcl -- wdoc build docs/landing/main.wcl --out docs/_site {{ARGS}}
    cargo run --locked -p wcl -- wdoc build docs/reference/main.wcl --out docs/_site/reference {{ARGS}}

# A build tree is relocatable: every URL in it is relative, so the tree works
# from whatever directory it is served out of. The book relies on it — it is
# deployed under `/reference/`, a directory the build was never told about.
#
# This part is deliberately a grep and nothing more. Resolving every target in
# shell would reimplement the walk the Rust tests already do
# (`crates/wcl_wdoc/tests/build.rs`, `crates/wcl/tests/wdoc.rs`), over a fixture
# book, both halves of the property, in a language that can say what it means.
# What those tests cannot see is a human typing an absolute link into a real
# page, so that is the one thing this part looks for — on the two real sites.
#
# It rides `docs-build`, and deploy-site.yml runs it too: the deploy triggers
# on a docs push to `main` without waiting for CI, so it checks for itself.
#
# Two patterns, because a page carries URLs in two vocabularies. The first is
# the `href` / `src` attribute. The second is a CSS `url(…)` — the `src:` of an
# `@font-face`, and `background-image` / `mask` / `cursor` / `@import` on the
# same shape for free. #279 added it: the stylesheet is inlined into every
# page, so three dangling font URLs sat on the live landing page while an
# attribute-only scan called the site clean.
#
# The CSS pattern takes any `url(` and asks only whether the target is anchored
# at `/`. That reaches one page listing CSS as an example, where the quotes come
# out as `&#39;` and the pattern therefore misses the quoted form but not the
# bare one. A bare absolute URL in a listing is worth a second look anyway, and
# the grep is meant to be read when it fires.
#
# Only the root-absolute half is here. Whether a URL has a file behind it is the
# walk's question, and the walk answers it — attribute and stylesheet alike, on
# both halves, over the fixture book, in Rust, where it can say what it means
# (`crates/wcl_wdoc/tests/support/relocatable.rs`).

# Check the built docs/ sites for root-absolute URLs (they must all be relative)
[group('dev')]
docs-relative-urls: docs-build
    @hits="$(grep -rnoE --include='*.html' \
        -e '(href|src)="/[^"]*"' \
        -e "url\([\"']?/[^)]*\)" \
        docs/_site || true)"; \
    if [ -n "$hits" ]; then \
        echo "root-absolute URLs in the built site — every URL must be relative:" >&2; \
        echo "$hits" >&2; \
        exit 1; \
    fi
