#!/bin/sh
# install.sh — install the `wcl` CLI from a GitHub release.
#
#   curl -fsSL https://wcl.dev/install.sh | sh                       # latest stable
#   curl -fsSL https://wcl.dev/install.sh | sh -s -- --pre           # latest pre-release
#   curl -fsSL https://wcl.dev/install.sh | sh -s -- --version 0.16.0-alpha
#
# WCL is pre-release only for now, so use --pre (or --version) — a plain run
# targets stable, which does not exist yet.
#
# Options / environment:
#   --version <X>   install version X (e.g. 0.16.0-alpha); or set WCL_VERSION
#   --pre           install the newest pre-release
#   --bin-dir <dir> install into <dir> (default: $WCL_INSTALL_DIR or ~/.local/bin)
#   --help          show this help
#
# Prebuilt binaries: Linux x86_64 (glibc or musl), macOS aarch64 and x86_64.
# Every download is checked against the release's SHA256SUMS before it is
# installed; a missing or mismatched checksum aborts the install.

set -eu

REPO="wiltaylor/wcl"
SOURCE_BUILD="cargo install --git https://github.com/wiltaylor/wcl -p wcl --locked"

VERSION="${WCL_VERSION:-}"
BIN_DIR="${WCL_INSTALL_DIR:-$HOME/.local/bin}"
PRE=0
# Where release assets are fetched from. Only the tests point it elsewhere
# (a local file server standing in for GitHub).
RELEASES_URL="${WCL_RELEASES_URL:-https://github.com/$REPO/releases}"

err() { printf 'error: %s\n' "$1" >&2; exit 1; }

usage() {
  sed -n '2,19p' "$0" | sed 's/^# \{0,1\}//'
  exit "${1:-0}"
}

# ── Parse args ──────────────────────────────────────────────────────────────
while [ $# -gt 0 ]; do
  case "$1" in
    --version) [ $# -ge 2 ] || err "--version needs an argument"; VERSION="$2"; shift 2 ;;
    --version=*) VERSION="${1#--version=}"; shift ;;
    --pre) PRE=1; shift ;;
    --bin-dir) [ $# -ge 2 ] || err "--bin-dir needs an argument"; BIN_DIR="$2"; shift 2 ;;
    --bin-dir=*) BIN_DIR="${1#--bin-dir=}"; shift ;;
    -h|--help) usage 0 ;;
    -*) err "unknown option: $1 (try --help)" ;;
    *) [ -z "$VERSION" ] || err "unexpected argument: $1"; VERSION="$1"; shift ;;
  esac
done

# ── HTTP helper (curl or wget) ──────────────────────────────────────────────
if command -v curl >/dev/null 2>&1; then
  http_get()      { curl -fsSL "$1"; }
  download_file() { curl -fsSL -o "$2" "$1"; }
elif command -v wget >/dev/null 2>&1; then
  http_get()      { wget -qO- "$1"; }
  download_file() { wget -qO "$2" "$1"; }
else
  err "need curl or wget on PATH"
fi

# ── SHA-256 helper ──────────────────────────────────────────────────────────
# Checked up front, so a machine that cannot verify never downloads at all.
if command -v sha256sum >/dev/null 2>&1; then
  sha256() { sha256sum "$1" | cut -d' ' -f1; }
elif command -v shasum >/dev/null 2>&1; then
  sha256() { shasum -a 256 "$1" | cut -d' ' -f1; }
else
  err "need sha256sum or shasum on PATH to verify the download"
fi

# Pull the first "tag_name": "..." out of a GitHub API JSON response.
first_tag() { sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1; }

# glibc or musl? A glibc binary does not start on musl (Alpine), so musl
# systems get the statically linked build.
is_musl() {
  ls /lib/ld-musl-* >/dev/null 2>&1 && return 0
  ldd --version 2>&1 | grep -qi musl
}

# ── Detect platform ─────────────────────────────────────────────────────────
os="$(uname -s)"
arch="$(uname -m)"
case "$arch" in
  x86_64|amd64) arch="x86_64" ;;
  arm64|aarch64) arch="aarch64" ;;
esac
case "$os/$arch" in
  Linux/x86_64)
    if is_musl; then suffix="linux-x86_64-musl"; else suffix="linux-x86_64"; fi ;;
  Darwin/aarch64) suffix="macos-aarch64" ;;
  Darwin/x86_64)  suffix="macos-x86_64" ;;
  *)
    err "no prebuilt binary for $os/$arch — build from source:
  $SOURCE_BUILD" ;;
esac

# ── Resolve version ─────────────────────────────────────────────────────────
if [ -n "$VERSION" ]; then
  tag="v${VERSION#v}"
elif [ "$PRE" -eq 1 ]; then
  tag="$(http_get "https://api.github.com/repos/$REPO/releases" | first_tag)"
  [ -n "$tag" ] || err "could not find any release for $REPO"
else
  tag="$(http_get "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null | first_tag || true)"
  [ -n "$tag" ] || err "no stable release published yet.
WCL is pre-release only for now — re-run with --pre to get the newest pre-release:
  curl -fsSL https://wcl.dev/install.sh | sh -s -- --pre
See $( printf 'https://github.com/%s/releases' "$REPO" )"
fi

ver="${tag#v}"
asset="wcl-${ver}-${suffix}"
base="$RELEASES_URL/download/$tag"

# ── Download + verify + install ─────────────────────────────────────────────
printf 'Installing wcl %s to %s\n' "$ver" "$BIN_DIR"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT INT TERM
download_file "$base/$asset" "$tmp/$asset" || err "download failed: $base/$asset
The release may not exist or may lack a $suffix asset. See https://github.com/$REPO/releases"
download_file "$base/SHA256SUMS" "$tmp/SHA256SUMS" || err "download failed: $base/SHA256SUMS
Releases before checksums were published cannot be verified; build from source instead:
  $SOURCE_BUILD"

# SHA256SUMS lines are `<hex>  <file>`; match the file name exactly.
expected="$(awk -v f="$asset" '$2 == f || $2 == "*" f { print $1; exit }' "$tmp/SHA256SUMS")"
[ -n "$expected" ] || err "SHA256SUMS has no entry for $asset — refusing to install"
actual="$(sha256 "$tmp/$asset")"
[ "$expected" = "$actual" ] || err "checksum mismatch for $asset — refusing to install
  expected $expected
  got      $actual"

chmod +x "$tmp/$asset"
mkdir -p "$BIN_DIR"
mv "$tmp/$asset" "$BIN_DIR/wcl"

printf 'Installed: %s\n' "$("$BIN_DIR/wcl" --version 2>/dev/null || echo "$BIN_DIR/wcl")"

# ── PATH hint ───────────────────────────────────────────────────────────────
# shellcheck disable=SC2016 # `$PATH` is printed literally, for the user to paste.
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) printf '\n%s is not on your PATH. Add it, e.g.:\n  export PATH="%s:$PATH"\n' "$BIN_DIR" "$BIN_DIR" ;;
esac
