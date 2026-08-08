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
