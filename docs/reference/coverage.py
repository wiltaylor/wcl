#!/usr/bin/env python3
"""The reference manual's coverage extractor.

Prints one JSON object per line describing the public surface a reader can
reach: every builtin the language exposes, every `wcl` subcommand, and every
wdoc block kind the embedded stdlib declares. Each line carries `path` and
`kind`, which is the contract `/technical-book audit` reads.

    $ uv run docs/reference/coverage.py
    {"path": "list_contains", "kind": "symbol"}
    {"path": "wcl check", "kind": "command"}
    {"path": "callout", "kind": "block"}

`--check` compares that surface against the headings the manual actually
carries and reports what is missing. Use it rather than the audit's own
percentage: the audit matches `h2` section titles only, and this manual
documents a builtin as an `h3` under a semantic group, which reads better and
is invisible to that rule.

`--skill` runs the same surface, plus every long flag of every command,
against the agent skill in `.claude/skills/wcl/` instead: each builtin and
block kind must be named somewhere in the skill's markdown, each command must
have its own section in `lang_cli.md`, and each flag must appear inside the
section of the command that takes it.

`--wcl <path>` picks the binary to ask (default: `wcl` on `PATH`), so CI can
point it at a freshly built `target/debug/wcl`.

Every fact comes from the binary and the crate rather than from a list kept
here, so a builtin added to `wcl_lang` shows up the next time this runs.
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
PAGES = ROOT / "docs" / "reference" / "pages"
STDLIB = ROOT / "crates" / "wcl_wdoc" / "lib"
SKILL = ROOT / ".claude" / "skills" / "wcl"
SKILL_CLI = SKILL / "references" / "language" / "lang_cli.md"

# The `wcl` binary every query runs. `main` replaces it with `--wcl`.
WCL = "wcl"

# Clap adds these to every command; they document themselves.
UNIVERSAL_FLAGS = {"--help", "--version"}

# Reachable only from a host that installs them, so no chapter documents them
# as a builtin of the language. `Builtins the host adds` covers the pair in
# prose instead.
HOST_INSTALLED = {"page_metadata", "__wdoc_slot"}

# `help` is clap's own, and `wcl wdoc` is a group whose two leaves the manual
# documents individually.
NOT_A_COMMAND = {"help", "wdoc"}


def builtins() -> list[str]:
    """Every builtin, straight out of the language's own reflection."""
    out = subprocess.run(
        [WCL, "repl"], input="builtin_names()\n",
        capture_output=True, text=True, check=True,
    ).stdout
    names = re.findall(r'"([^"]+)"', out)
    if not names:
        sys.exit("wcl repl printed no builtin names")
    return sorted(set(names) - HOST_INSTALLED)


def commands() -> list[str]:
    """Every `wcl` subcommand, plus the two `wcl wdoc` leaves."""
    top = subprocess.run(
        [WCL, "--help"], capture_output=True, text=True, check=True
    ).stdout
    block = top.split("Commands:", 1)[1].split("\n\n", 1)[0]
    names = [
        m.group(1) for m in re.finditer(r"^\s{2}(\w[\w-]*)\s{2}", block, re.M)
    ]
    found = [f"wcl {name}" for name in names if name not in NOT_A_COMMAND]

    sub = subprocess.run(
        [WCL, "wdoc", "--help"], capture_output=True, text=True, check=True
    ).stdout
    if "Commands:" in sub:
        block = sub.split("Commands:", 1)[1].split("\n\n", 1)[0]
        found += [
            f"wcl wdoc {m.group(1)}"
            for m in re.finditer(r"^\s{2}(\w[\w-]*)\s{2}", block, re.M)
            if m.group(1) not in NOT_A_COMMAND
        ]
    return sorted(set(found))


def blocks() -> list[str]:
    """Every block kind the embedded wdoc stdlib declares."""
    found = set()
    for path in STDLIB.rglob("*.wcl"):
        # `typedoc.wcl` shows `@block("project_meta")` inside a doc comment
        # explaining what `block_reference` walks. A commented-out kind is an
        # example, not a surface, so comment lines are dropped first.
        source = "\n".join(
            line for line in path.read_text().split("\n")
            if not line.lstrip().startswith("//")
        )
        found |= set(re.findall(r'@(?:block|table)\("([^"]+)"\)', source))
    # The same file declares a literal `@block("<kind>")` placeholder, which
    # is a template rather than a kind anyone writes.
    return sorted(name for name in found if re.fullmatch(r"[a-z][a-z0-9_]*", name))


def headings() -> set[str]:
    """Every heading the manual carries, outside its code samples."""
    found = set()
    for path in PAGES.rglob("*.wcl"):
        text = re.sub(r"<<'(\w+)'\n.*?\n\s*\1", "", path.read_text(), flags=re.S)
        found |= set(re.findall(r'^\s*h[1-6] "(.*?)"', text, re.M))
    return found


def prose_code_spans() -> set[str]:
    """Every `backticked` name in the manual's prose, tables and callouts.

    A block kind gets no heading of its own — `callout` is documented under
    *Callouts*, `p` under *Paragraphs* — so a heading match would report a
    documented kind as missing. What a documented kind does have is its name
    in backticks in the surrounding prose, and a kind nobody mentions has
    nothing anywhere. Code samples are excluded: appearing in someone else's
    example is not being documented.
    """
    found = set()
    for path in PAGES.rglob("*.wcl"):
        text = re.sub(r"<<'(\w+)'\n.*?\n\s*\1", "", path.read_text(), flags=re.S)
        found |= set(re.findall(r"`([a-z][a-z0-9_]*)`", text))
        # A run of numbered siblings is documented as a range — "`h1` through
        # `h6`" — and spelling all six out would be worse prose, so a range
        # counts for both ends and everything between.
        for m in re.finditer(
            r"`([a-z_]+?)(\d)`\s*(?:through|to|and|…|\.\.\.)\s*`\1(\d)`", text
        ):
            stem, lo, hi = m.group(1), int(m.group(2)), int(m.group(3))
            found |= {f"{stem}{n}" for n in range(lo, hi + 1)}
    return found


def surface() -> list[dict[str, str]]:
    return (
        [{"path": name, "kind": "symbol"} for name in builtins()]
        + [{"path": name, "kind": "command"} for name in commands()]
        + [{"path": name, "kind": "block"} for name in blocks()]
    )


def check() -> int:
    """Report what the product exposes and the manual never names."""
    written = headings()
    # A heading may name more than one thing — `wcl eval and wcl get`, or
    # `base, media, keyframes and font_face`. Split on the joiners so each
    # name is matched on its own.
    named = set()
    for heading in written:
        named.add(heading)
        named |= {
            part.strip(" `")
            for part in re.split(r",| and | / |\bor\b", heading)
            if part.strip()
        }

    spans = prose_code_spans()
    status = 0
    every = surface()
    for kind in ("symbol", "command", "block"):
        want = [s["path"] for s in every if s["kind"] == kind]
        if kind == "block":
            missing = [p for p in want if p not in spans]
            have = len(want) - len(missing)
            pct = 100.0 * have / len(want) if want else 100.0
            print(f"{'ok ' if not missing else 'GAP'} {kind:8} {have:4}/{len(want):<4} {pct:5.1f}%")
            for path in missing:
                print(f"       undocumented: {path}")
            if missing:
                status = 1
            continue
        missing = [p for p in want if p not in named]
        have = len(want) - len(missing)
        pct = 100.0 * have / len(want) if want else 100.0
        mark = "ok " if not missing else "GAP"
        print(f"{mark} {kind:8} {have:4}/{len(want):<4} {pct:5.1f}%")
        for path in missing:
            print(f"       undocumented: {path}")
        if missing:
            status = 1
    return status


def flags(command: str) -> list[str]:
    """Every long flag `wcl <command> --help` lists, bar clap's own."""
    argv = [WCL, *command.split()[1:], "--help"]
    out = subprocess.run(argv, capture_output=True, text=True, check=True).stdout
    options = out.split("Options:", 1)[1] if "Options:" in out else ""
    found = set(re.findall(r"^\s+(?:-\w, )?(--[a-z][a-z0-9-]*)", options, re.M))
    return sorted(found - UNIVERSAL_FLAGS)


def skill_text() -> str:
    """Every markdown file in the skill, concatenated."""
    return "\n".join(p.read_text() for p in sorted(SKILL.rglob("*.md")))


def cli_sections() -> dict[str, str]:
    """`lang_cli.md` split into its `##` sections, keyed by every
    `` `wcl …` `` command the heading names."""
    sections: dict[str, str] = {}
    for chunk in re.split(r"^## ", SKILL_CLI.read_text(), flags=re.M)[1:]:
        heading = chunk.split("\n", 1)[0]
        for name in re.findall(r"`(wcl [a-z][\w -]*?)`", heading):
            sections[name] = chunk
    return sections


def skill_check() -> int:
    """Report what the product exposes and the skill never names."""
    text = skill_text()
    sections = cli_sections()

    def named(name: str) -> bool:
        return re.search(rf"(?<![\w-]){re.escape(name)}(?![\w-])", text) is not None

    every = surface()
    commands_ = [s["path"] for s in every if s["kind"] == "command"]
    rows: list[tuple[str, list[str], list[str]]] = [
        ("symbol", [s["path"] for s in every if s["kind"] == "symbol"], []),
        ("block", [s["path"] for s in every if s["kind"] == "block"], []),
        ("command", commands_, []),
        ("flag", [f"{c} {f}" for c in commands_ for f in flags(c)], []),
    ]
    for kind, want, missing in rows:
        for path in want:
            if kind in ("symbol", "block"):
                ok = named(path)
            elif kind == "command":
                ok = path in sections
            else:
                command, flag = path.rsplit(" ", 1)
                ok = re.search(
                    rf"(?<![\w-]){re.escape(flag)}(?![\w-])",
                    sections.get(command, ""),
                ) is not None
            if not ok:
                missing.append(path)

    status = 0
    for kind, want, missing in rows:
        have = len(want) - len(missing)
        pct = 100.0 * have / len(want) if want else 100.0
        mark = "ok " if not missing else "GAP"
        print(f"{mark} {kind:8} {have:4}/{len(want):<4} {pct:5.1f}%")
        for path in missing:
            print(f"       not in skill: {path}")
        if missing:
            status = 1
    return status


def main() -> None:
    global WCL
    parser = argparse.ArgumentParser(
        description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter
    )
    mode = parser.add_mutually_exclusive_group()
    mode.add_argument(
        "--check", action="store_true",
        help="compare the surface with the manual's headings and report gaps",
    )
    mode.add_argument(
        "--skill", action="store_true",
        help="compare the surface and every CLI flag with the wcl skill and report gaps",
    )
    parser.add_argument(
        "--wcl", default=None, metavar="PATH",
        help="the wcl binary to query (default: `wcl` on PATH)",
    )
    args = parser.parse_args()
    WCL = args.wcl or shutil.which("wcl") or sys.exit("no `wcl` on PATH; pass --wcl <path>")
    if args.check:
        sys.exit(check())
    if args.skill:
        sys.exit(skill_check())
    for entry in surface():
        print(json.dumps(entry))


if __name__ == "__main__":
    main()
