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

set -eu

REPO="wiltaylor/wcl"
SOURCE_BUILD="cargo install --git https://github.com/wiltaylor/wcl -p wcl --locked"

VERSION="${WCL_VERSION:-}"
BIN_DIR="${WCL_INSTALL_DIR:-$HOME/.local/bin}"
PRE=0

err() { printf 'error: %s\n' "$1" >&2; exit 1; }

usage() {
  sed -n '2,15p' "$0" | sed 's/^# \{0,1\}//'
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

# Pull the first "tag_name": "..." out of a GitHub API JSON response.
first_tag() { sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n1; }

# ── Detect platform ─────────────────────────────────────────────────────────
os="$(uname -s)"
arch="$(uname -m)"
case "$arch" in
  x86_64|amd64) arch="x86_64" ;;
esac
case "$os" in
  Linux)
    [ "$arch" = "x86_64" ] || err "no prebuilt binary for Linux/$arch — build from source:
  $SOURCE_BUILD"
    suffix="linux-x86_64" ;;
  Darwin)
    err "no prebuilt macOS binary yet — build from source:
  $SOURCE_BUILD" ;;
  *)
    err "unsupported platform: $os/$arch — build from source:
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
url="https://github.com/$REPO/releases/download/$tag/$asset"

# ── Download + install ──────────────────────────────────────────────────────
printf 'Installing wcl %s to %s\n' "$ver" "$BIN_DIR"

tmp="$(mktemp)"
trap 'rm -f "$tmp"' EXIT INT TERM
download_file "$url" "$tmp" || err "download failed: $url
The release may not exist or may lack a $suffix asset. See https://github.com/$REPO/releases"

chmod +x "$tmp"
mkdir -p "$BIN_DIR"
mv "$tmp" "$BIN_DIR/wcl"
trap - EXIT INT TERM

printf 'Installed: %s\n' "$("$BIN_DIR/wcl" --version 2>/dev/null || echo "$BIN_DIR/wcl")"

# ── PATH hint ───────────────────────────────────────────────────────────────
case ":$PATH:" in
  *":$BIN_DIR:"*) ;;
  *) printf '\n%s is not on your PATH. Add it, e.g.:\n  export PATH="%s:$PATH"\n' "$BIN_DIR" "$BIN_DIR" ;;
esac
