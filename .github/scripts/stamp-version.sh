#!/usr/bin/env bash
# Stamp a release version over the workspace's `0.0.0` sentinel.
#
#   .github/scripts/stamp-version.sh 0.37.0-alpha
#
# Rewrites exactly these, and fails unless every one of them changed:
#   - `version` in Cargo.toml's [workspace.package]
#   - the `version` of the three path crates in [workspace.dependencies]
#     (a `0.0.0` requirement would reject the stamped crates)
#   - the four workspace members' entries in Cargo.lock, so a `--locked`
#     build still accepts the lockfile
#
# perl rather than `sed -i`, whose in-place flag differs between GNU sed
# (Linux, Git Bash on Windows) and BSD sed (macOS).
set -euo pipefail

v="${1:?usage: stamp-version.sh <version>}"
[[ "$v" =~ ^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?$ ]] || {
  echo "not a semver version: $v" >&2
  exit 1
}

members='wcl|wcl_lang|wcl_lsp|wcl_wdoc'

V="$v" perl -0pi -e '
  $n = s/(\[workspace\.package\]\n(?:[^\[\n][^\n]*\n)*?version = )"0\.0\.0"/$1"$ENV{V}"/;
  $n += s/^((?:wcl_lang|wcl_lsp|wcl_wdoc) = \{ path = "[^"]+", version = )"0\.0\.0"/$1"$ENV{V}"/mg;
  $n == 4 or die "Cargo.toml: expected 4 version fields at 0.0.0, rewrote $n\n";
' Cargo.toml

V="$v" M="$members" perl -0pi -e '
  $n = s/(\[\[package\]\]\nname = "(?:$ENV{M})"\nversion = )"0\.0\.0"/$1"$ENV{V}"/g;
  $n == 4 or die "Cargo.lock: expected 4 workspace members at 0.0.0, rewrote $n\n";
' Cargo.lock

echo "stamped $v"
