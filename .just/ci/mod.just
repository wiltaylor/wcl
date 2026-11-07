# The merge bar: everything a change must pass before it can land.
#
# Recipes run from the repository root (`working-directory`), not from this
# folder, so every path below is repo-relative like the root justfile's.
# The parts themselves live in ../shared.just — imported here AND by the root
# justfile, because a module cannot see its parent's recipes and each recipe
# must still be defined exactly once.

set shell := ["bash", "-cu"]
set working-directory := '../..'

import '../shared.just'

[default, private]
main:
    @just --list ci

# `fuzz-sweep` is deliberately NOT part of `check`: it needs the nightly
# toolchain plus cargo-fuzz, which most machines don't have. CI runs it as its
# own job (`just ci::fuzz-sweep`), and it is guarded so a machine without the
# tooling says so instead of failing obscurely.

# Everything a change must pass before it can merge: fmt, lint, API docs, tests, doc builds
[group('ci')]
check: fmt-check workspace-lint workspace-doc workspace-test docs-build docs-relative-urls

# The affected-tests selector for card validation. TERRARIUM_CHANGED_FILES
# holds the changed files (repo-relative; newlines, spaces or commas between
# them). affected.py turns them into a plan from `cargo metadata`: the crates
# under the changed paths plus every workspace crate that depends on them get
# fmt, clippy, rustdoc and tests; the docs sites are built when docs/ changed
# or the change reaches the `wcl` binary that builds them. An empty list, or a
# workspace file (Cargo.toml, Cargo.lock, rust-toolchain.toml, justfile,
# .just/), runs the full `check` instead. `check` stays the merge bar: the
# merge queue runs it in full on every batch.
#
# Run the merge-bar parts the files in TERRARIUM_CHANGED_FILES can break
[group('ci')]
affected: require-rustfmt require-clippy
    #!/usr/bin/env bash
    set -euo pipefail
    command -v python3 >/dev/null 2>&1 || { echo "python3 is required for ci::affected" >&2; exit 1; }
    plan="$(printf '%s' "${TERRARIUM_CHANGED_FILES:-}" | python3 .just/ci/affected.py)"
    echo "ci::affected plan:" >&2
    printf '%s\n' "$plan" | sed 's/^/  /' >&2
    if full="$(printf '%s\n' "$plan" | sed -n 's/^full //p')" && [ -n "$full" ]; then
        echo "==> full gate ($full)" >&2
        exec "{{ just_executable() }}" ci::check
    fi
    pkgs=()
    while read -r kind arg; do
        [ "$kind" = pkg ] && pkgs+=(-p "$arg")
    done <<< "$plan"
    if [ ${#pkgs[@]} -gt 0 ]; then
        echo "==> fmt-check ${pkgs[*]}" >&2
        cargo fmt --check "${pkgs[@]}"
        echo "==> clippy ${pkgs[*]}" >&2
        cargo clippy --locked "${pkgs[@]}" --all-targets -- {{ lint_flags }}
        echo "==> rustdoc ${pkgs[*]}" >&2
        RUSTDOCFLAGS="-D warnings" cargo doc --locked --no-deps "${pkgs[@]}"
        echo "==> test ${pkgs[*]}" >&2
        cargo test --locked "${pkgs[@]}" --no-fail-fast
    fi
    if printf '%s\n' "$plan" | grep -qx docs; then
        echo "==> docs build and relative URL check" >&2
        "{{ just_executable() }}" ci::docs-relative-urls
    fi
    if [ ${#pkgs[@]} -eq 0 ] && ! printf '%s\n' "$plan" | grep -qx docs; then
        echo "==> nothing in the merge bar reads these files" >&2
    fi
