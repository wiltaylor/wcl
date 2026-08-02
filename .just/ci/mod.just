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
#
# One part is not side-effect-free: `wad-extract-check` regenerates the tracked
# files under .wad/data/generated/, because freshness can't be judged without
# re-running the extractors. See its own doc comment.

# Everything a change must pass before it can merge: fmt, lint, tests, WAD schema drift, artifact freshness, doc builds
[group('ci')]
check: fmt-check workspace-lint workspace-test wskill-lint wskill-check wcl-refcheck wad-schema-check wad-check wad-extract-check wad-facts-check wad-build wplan-template-check skills-check docs-build
