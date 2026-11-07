#!/usr/bin/env python3
"""Map a changed-files list to the parts of the merge bar it can break.

Read by `just ci::affected`. The changed files arrive on stdin, repo-relative,
separated by newlines, spaces or commas. The plan goes to stdout, one line per
item:

    full <reason>   run the whole gate (`just ci::check`) and nothing else
    pkg <name>      fmt, clippy, rustdoc and tests for this workspace package
    docs            build the docs/ sites and check their URLs
    skip <path>     a file no gate part reads (shown so the plan can be read)

The crates come from `cargo metadata`, not from a list here: a file under a
member's directory selects that member, and every member that depends on it
(normal, dev or build dependency, transitively) comes with it.
"""

import json
import re
import subprocess
import sys

# A change to one of these can change what every crate builds or what the
# gate itself runs, so only the full gate covers it.
WORKSPACE_FILES = {"Cargo.toml", "Cargo.lock", "rust-toolchain.toml", "justfile"}
WORKSPACE_DIRS = (".just/", ".cargo/")

# Read by no gate part (checked with `rg` over crates/: no test or build
# script reaches into them). A file outside every known place runs the full
# gate rather than nothing.
SKIP_DIRS = ("editors/", ".claude/", ".agent/")
SKIP_TOP_LEVEL = re.compile(r"^(LICENSE|[^/]+\.md)$")

# The crates' tests read the examples (crates/*/tests use ../../examples).
EXAMPLES_DIR = "examples/"
DOCS_DIR = "docs/"

# The docs build runs this package's binary, so any change that reaches it
# (its own code or a crate it depends on) can break the docs build.
DOCS_BUILDER = "wcl"


def changed_files(text):
    files = []
    for raw in re.split(r"[\s,]+", text):
        path = raw.strip()
        while path.startswith("./"):
            path = path[2:]
        if path:
            files.append(path)
    return files


def workspace():
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout
    meta = json.loads(out)
    root = meta["workspace_root"].rstrip("/") + "/"
    members = set(meta["workspace_members"])
    by_id = {p["id"]: p for p in meta["packages"] if p["id"] in members}

    # Package directory (repo-relative, with a trailing slash) -> name.
    dirs = {}
    for pkg in by_id.values():
        manifest = pkg["manifest_path"]
        rel = manifest[len(root):] if manifest.startswith(root) else manifest
        pkg_dir = rel.rsplit("/", 1)[0] + "/" if "/" in rel else ""
        dirs[pkg_dir] = pkg["name"]

    # name -> names of the members that depend on it directly.
    dependants = {pkg["name"]: set() for pkg in by_id.values()}
    for node in meta["resolve"]["nodes"]:
        if node["id"] not in by_id:
            continue
        user = by_id[node["id"]]["name"]
        for dep in node["deps"]:
            if dep["pkg"] in by_id:
                dependants[by_id[dep["pkg"]]["name"]].add(user)
    return dirs, dependants


def owner(path, dirs):
    """The member whose directory holds `path` (the deepest one), or None."""
    best = None
    for pkg_dir, name in dirs.items():
        if pkg_dir and path.startswith(pkg_dir):
            if best is None or len(pkg_dir) > len(best[0]):
                best = (pkg_dir, name)
    return best[1] if best else None


def plan(files):
    if not files:
        return [("full", "no changed-files list")]
    for path in files:
        if path in WORKSPACE_FILES or path.startswith(WORKSPACE_DIRS):
            return [("full", f"workspace file changed: {path}")]

    dirs, dependants = workspace()
    selected = set()
    docs = False
    skipped = []
    for path in files:
        name = owner(path, dirs)
        if name:
            selected.add(name)
        elif path.startswith(EXAMPLES_DIR):
            selected.update(dependants)
        elif path.startswith(DOCS_DIR):
            docs = True
        elif path.startswith(SKIP_DIRS) or SKIP_TOP_LEVEL.match(path):
            skipped.append(path)
        else:
            return [("full", f"no rule for {path}")]

    # Close over reverse dependencies.
    todo = list(selected)
    while todo:
        for user in dependants.get(todo.pop(), ()):
            if user not in selected:
                selected.add(user)
                todo.append(user)

    if DOCS_BUILDER in selected:
        docs = True
    items = [("skip", p) for p in skipped]
    items += [("pkg", n) for n in sorted(selected)]
    if docs:
        items.append(("docs", ""))
    return items


def main():
    for kind, arg in plan(changed_files(sys.stdin.read())):
        print(f"{kind} {arg}".rstrip())


if __name__ == "__main__":
    main()
