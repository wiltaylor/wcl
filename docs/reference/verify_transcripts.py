#!/usr/bin/env python3
"""Rerun the shell transcripts in both documentation trees against a `wcl` binary.

Both trees promise that every example was run. This script holds them to it: it walks
the book (`docs/reference/pages/**/*.wcl`) and the skill (`.claude/skills/wcl/**/*.md`),
replays every `$ ...` command shown in a console block, and compares what the binary
prints with what the page claims.

Each page is replayed in its own temporary directory, in reading order. A code block
that names a file (`filename = "app.wcl"` in the book) is written into that directory
when the reader reaches it, so a later transcript sees the same file the reader did. The
skill never names its files. There, a command that mentions exactly one `.wcl` file the
page has not written is given the nearest unnamed `wcl` block above it, and the result
is labelled `guessed` so a mismatch is read with that in mind.

Pages often show a variant ("change `port` to 70000 and run it again") in prose rather
than as a new file. When an error rendering disagrees, the source lines it quotes are
written back into the file it names and the command is run once more, which recovers
most one-line variants. Variants that add or remove lines cannot be recovered that way,
so a MISMATCH is a lead to read by hand, not a verdict.

A command whose files cannot be found, or that would not terminate (`serve`, `lsp`,
`curl`), is counted as skipped rather than failed.

    python3 docs/reference/verify_transcripts.py --wcl target/debug/wcl
    python3 docs/reference/verify_transcripts.py --wcl target/debug/wcl --page lang_functions -v

Exits 1 when any replayed command disagrees with its page.
"""

from __future__ import annotations

import argparse
import difflib
import os
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BOOK = ROOT / "docs/reference/pages"
SKILL = ROOT / ".claude/skills/wcl"

# Commands that never exit on their own, or reach the network.
NON_TERMINATING = re.compile(r"\bwcl (wdoc )?(serve|lsp)\b|\bcurl\b|\bgit clone\b")
# A usage synopsis (`wcl get <file> <path> [--json]`) is not a command to run, and
# `--version` answers with whatever build ran it.
SYNOPSIS = re.compile(r"(?<!<)<[a-z][\w-]*>|\[--|\[[a-z]+\]|\bwcl --version\b")
FILE_TOKEN =re.compile(r"(?<![\w/.-])([\w][\w./-]*\.wcl)\b")


@dataclass
class Block:
    kind: str  # "file" | "wcl" | "console"
    text: str
    line: int
    name: str | None = None


@dataclass
class Command:
    cmd: str
    expected: str
    line: int
    stdin: str | None = None


@dataclass
class Result:
    page: str
    line: int
    cmd: str
    status: str  # "ok" | "mismatch" | "skipped"
    detail: str = ""
    guessed: bool = False


# ── Extraction ────────────────────────────────────────────────────────────────


def book_blocks(path: Path) -> list[Block]:
    """Code blocks of a wdoc page, in order: `code <lang> { filename = … source = <<'TAG' … TAG }`."""
    lines = path.read_text().splitlines()
    blocks: list[Block] = []
    lang = None
    name = None
    i = 0
    while i < len(lines):
        line = lines[i]
        m = re.match(r"\s*code\s+(\w+)\s*\{", line)
        if m:
            lang, name = m.group(1), None
        m = re.match(r'\s*filename\s*=\s*"([^"]+)"', line)
        if m and lang:
            name = m.group(1)
        m = re.search(r"source\s*=\s*<<'(\w+)'\s*$", line)
        if m and lang:
            tag = m.group(1)
            start = i + 1
            j = start
            while j < len(lines) and lines[j].strip() != tag:
                j += 1
            text = "\n".join(lines[start:j])
            if lang == "console":
                blocks.append(Block("console", text, start + 1))
            elif lang == "wcl":
                blocks.append(Block("file" if name else "wcl", text, start + 1, name))
            elif name:
                blocks.append(Block("file", text, start + 1, name))
            lang, name = None, None
            i = j
        i += 1
    return blocks


def skill_blocks(path: Path) -> list[Block]:
    """Fenced blocks of a skill page. A fence may be indented under a list item."""
    lines = path.read_text().splitlines()
    blocks: list[Block] = []
    i = 0
    while i < len(lines):
        m = re.match(r"(\s*)```(\w*)\s*$", lines[i])
        if not m:
            i += 1
            continue
        indent, lang = m.group(1), m.group(2)
        j = i + 1
        body = []
        while j < len(lines) and lines[j].strip() != "```":
            body.append(lines[j][len(indent):] if lines[j].startswith(indent) else lines[j].lstrip())
            j += 1
        text = "\n".join(body)
        if lang == "console":
            blocks.append(Block("console", text, i + 2))
        elif lang == "wcl":
            blocks.append(Block("wcl", text, i + 2))
        i = j + 1
    return blocks


def commands(block: Block) -> list[Command]:
    """Split a console block into `$ cmd` + expected output pairs."""
    out: list[Command] = []
    lines = block.text.split("\n")
    i = 0
    while i < len(lines):
        if not lines[i].startswith("$ "):
            i += 1
            continue
        cmd_lines = [lines[i][2:]]
        line_no = block.line + i
        i += 1
        # A heredoc or a backslash continuation carries the command onto later lines.
        heredoc = re.search(r"<<-?'?(\w+)'?", cmd_lines[0])
        if heredoc:
            tag = heredoc.group(1)
            while i < len(lines):
                cmd_lines.append(re.sub(r"^> ?", "", lines[i]))
                i += 1
                if cmd_lines[-1].strip() == tag:
                    break
        # A backslash, or a quote left open, carries it on too.
        while i < len(lines) and (cmd_lines[-1].endswith("\\") or incomplete("\n".join(cmd_lines))):
            cmd_lines.append(re.sub(r"^> ?", "", lines[i]))
            i += 1
        expected = []
        while i < len(lines) and not lines[i].startswith("$ "):
            expected.append(lines[i])
            i += 1
        # A trailing `# …` line is the page talking to the reader between two runs.
        while expected and (not expected[-1].strip() or expected[-1].startswith("# ")):
            expected.pop()
        command = Command("\n".join(cmd_lines), "\n".join(expected), line_no)
        # An interactive repl session shows what was typed after a `wcl> ` prompt. Piped
        # in, the repl prints no prompt and echoes nothing, so the typing becomes stdin.
        if re.match(r"wcl repl\b", command.cmd) and "|" not in command.cmd:
            typed = [line[5:] for line in expected if line.startswith("wcl> ")]
            if typed:
                command.stdin = "\n".join(typed) + "\n"
                command.expected = "\n".join(line for line in expected if not line.startswith("wcl> "))
        out.append(command)
    return out


def incomplete(cmd: str) -> bool:
    """Whether bash would read another line to finish this command."""
    check = subprocess.run(["bash", "-n", "-c", cmd], capture_output=True, text=True)
    return "unexpected end of file" in check.stderr or "unexpected EOF" in check.stderr


# ── Replay ────────────────────────────────────────────────────────────────────


def normalise(text: str, tmp: str) -> list[str]:
    text = text.replace(tmp + "/", "").replace(tmp, ".")
    # Pages write the reader's home directory as /home/you.
    text = text.replace(str(Path.home()), "/home/you")
    lines = [line.rstrip() for line in text.split("\n")]
    while lines and not lines[-1]:
        lines.pop()
    while lines and not lines[0]:
        lines.pop(0)
    return lines


def matches(expected: list[str], actual: list[str], cmd: str = "") -> bool:
    """Exact, with the liberties a page takes when it prints output for a reader.

    - A line holding only `…` or `...` stands for any run of lines, which makes the
      block an excerpt: each line shown need only appear inside an output line.
    - `…` or `...` inside a line (`.../lib.wcl`) stands for any run of characters.
    - A page may re-wrap a long line, so whitespace runs compare equal to line breaks.
    - `ls` and `find` list in an order the filesystem picks, and `ls` into columns.
    """
    if expected == actual:
        return True
    if re.match(r"(ls|find)\b", cmd) and "|" not in cmd:
        return sorted(" ".join(expected).split()) == sorted(" ".join(actual).split())
    flat_exp, flat_act = " ".join(expected).split(), " ".join(actual).split()
    if flat_exp == flat_act:
        return True
    if not any("…" in line or "..." in line for line in expected):
        return False
    excerpt = any(line.strip() in ("…", "...") for line in expected)
    pattern = ""
    for line in expected:
        if line.strip() in ("…", "..."):
            pattern += r"(?:.*\n)*?"
            continue
        parts = re.split(r"\.\.\.|…", line)
        body = ".*".join(re.escape(part) for part in parts)
        pattern += (f".*{body}.*" if excerpt else body) + r"\n"
    return re.fullmatch(pattern, "\n".join(actual) + "\n") is not None


@dataclass
class Run:
    combined: str  # stdout and stderr interleaved, as a terminal shows them
    stdout: str  # what a page shows when it leaves the diagnostics out
    stderr: str
    status: int


def run(c: Command, cwd: str, env: dict) -> Run | None:
    def once(stderr) -> subprocess.CompletedProcess:
        return subprocess.run(
            ["bash", "-c", c.cmd],
            cwd=cwd,
            env=env,
            input=c.stdin,
            stdin=None if c.stdin is not None else subprocess.DEVNULL,
            stdout=subprocess.PIPE,
            stderr=stderr,
            text=True,
            timeout=60,
        )

    try:
        combined = once(subprocess.STDOUT)
        alone = once(subprocess.PIPE)
    except subprocess.TimeoutExpired:
        return None
    return Run(combined.stdout, alone.stdout, alone.stderr, combined.returncode)


def agrees(c: Command, ran: Run, tmp: str) -> tuple[bool, list[str]]:
    """Compare a run with the page, which may show stdout alone or both streams.

    Leaving out stderr is how a page skips a diagnostic it has already explained, never
    how it hides a warning: a run that warned must show the warning.
    """
    exp = normalise(c.expected, tmp)
    act = normalise(ran.combined, tmp)
    if matches(exp, act, c.cmd):
        return True, act
    quiet = not re.search(r"^warning:", ran.stderr, re.M)
    if quiet and matches(exp, normalise(ran.stdout, tmp), c.cmd):
        return True, act
    return False, act


SNIPPET_HEADER = re.compile(r"╭─\[([^:\]]+):\d+:\d+\]")
SNIPPET_LINE = re.compile(r"^\s*(\d+) │ ?(.*)$")


def patch_from_snippet(expected: str, tmp: str) -> dict[Path, str]:
    """Write the source lines an error rendering quotes into the file it names.

    Returns the original text of every file changed, so the caller can restore it. Lines
    drawn with a multi-line span marker are left alone: their gutter hides the indent.
    """
    changed: dict[Path, str] = {}
    current: Path | None = None
    for line in expected.split("\n"):
        header = SNIPPET_HEADER.search(line)
        if header:
            current = Path(tmp) / header.group(1)
            if not current.is_file():
                current = None
            continue
        m = SNIPPET_LINE.match(line)
        if not m or current is None:
            continue
        text = m.group(2)
        if re.match(r"[╭├│╰]", text):
            continue
        original = changed.get(current, current.read_text())
        lines = current.read_text().split("\n")
        index = int(m.group(1)) - 1
        if index >= len(lines) or lines[index] == text:
            continue
        lines[index] = text
        changed.setdefault(current, original)
        current.write_text("\n".join(lines))
    return changed


def replay(page: str, blocks: list[Block], env: dict, guess: bool) -> list[Result]:
    results: list[Result] = []
    tmp = tempfile.mkdtemp(prefix="wcl-docs-")
    try:
        last_anon: Block | None = None
        last_status: int | None = None
        for block in blocks:
            if block.kind == "file":
                dest = Path(tmp) / block.name
                dest.parent.mkdir(parents=True, exist_ok=True)
                dest.write_text(block.text + "\n")
                continue
            if block.kind == "wcl":
                last_anon = block
                continue
            for c in commands(block):
                skip = None
                if SYNOPSIS.search(c.cmd):
                    skip = "a synopsis, not a command"
                elif NON_TERMINATING.search(c.cmd):
                    skip = "does not terminate"
                elif c.cmd.strip() == "echo $?" and last_status is None:
                    skip = "the command before it was skipped"
                if skip:
                    results.append(Result(page, c.line, c.cmd, "skipped", skip))
                    last_status = None
                    continue
                guessed = False
                if c.cmd.strip() == "echo $?":
                    ok = c.expected.strip() == str(last_status)
                    act = [str(last_status)]
                else:
                    wanted = [t for t in FILE_TOKEN.findall(c.cmd) if not t.startswith("<")]
                    missing = [t for t in wanted if not (Path(tmp) / t).exists()]
                    # A command that writes the file itself (`> x.wcl`, `wcl init`) needs nothing.
                    missing = [t for t in missing if not re.search(r">\s*" + re.escape(t), c.cmd)]
                    if guess and len(missing) == 1 and last_anon is not None and "init" not in c.cmd:
                        dest = Path(tmp) / missing[0]
                        dest.parent.mkdir(parents=True, exist_ok=True)
                        dest.write_text(last_anon.text + "\n")
                        guessed = True
                        missing = []
                    if missing:
                        results.append(
                            Result(page, c.line, c.cmd, "skipped", "no source for " + ", ".join(missing))
                        )
                        last_status = None
                        continue
                    ran = run(c, tmp, env)
                    if ran is None:
                        results.append(Result(page, c.line, c.cmd, "skipped", "timed out"))
                        last_status = None
                        continue
                    last_status = ran.status
                    ok, act = agrees(c, ran, tmp)
                    if not ok:
                        # The page may be showing a variant of the file ("change X and
                        # run it again"). An error rendering quotes the lines it was
                        # given, so write those lines back into the file and try again.
                        patched = patch_from_snippet(c.expected, tmp)
                        if patched:
                            again = run(c, tmp, env)
                            for path, original in patched.items():
                                path.write_text(original)
                            if again is not None:
                                last_status = again.status
                                ok, act = agrees(c, again, tmp)
                if ok:
                    results.append(Result(page, c.line, c.cmd, "ok", guessed=guessed))
                else:
                    exp = normalise(c.expected, tmp)
                    diff = "\n".join(difflib.unified_diff(exp, act, "page", "binary", lineterm="", n=2))
                    results.append(Result(page, c.line, c.cmd, "mismatch", diff, guessed))
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    return results


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--wcl", default=str(ROOT / "target/debug/wcl"), help="binary to replay against")
    ap.add_argument("--page", action="append", default=[], help="only pages whose path contains this")
    ap.add_argument("--book-only", action="store_true")
    ap.add_argument("--skill-only", action="store_true")
    ap.add_argument("-v", "--verbose", action="store_true", help="list skipped commands too")
    args = ap.parse_args()

    binary = Path(args.wcl).resolve()
    if not binary.is_file():
        sys.exit(f"no wcl binary at {binary}")
    bindir = tempfile.mkdtemp(prefix="wcl-bin-")
    os.symlink(binary, Path(bindir) / "wcl")
    env = dict(os.environ)
    env.update(PATH=bindir + os.pathsep + env["PATH"], NO_COLOR="1", COLUMNS="80", TERM="dumb", LC_ALL="C.UTF-8")

    # The book names every file it shows, so only the skill's commands are given a guess.
    pages: list[tuple[str, list[Block], bool]] = []
    if not args.skill_only:
        for p in sorted(BOOK.rglob("*.wcl")):
            pages.append((str(p.relative_to(ROOT)), book_blocks(p), False))
    if not args.book_only:
        for p in sorted(SKILL.rglob("*.md")):
            pages.append((str(p.relative_to(ROOT)), skill_blocks(p), True))
    if args.page:
        pages = [page for page in pages if any(f in page[0] for f in args.page)]

    totals = {"ok": 0, "mismatch": 0, "skipped": 0}
    for name, blocks, guess in pages:
        for r in replay(name, blocks, env, guess):
            totals[r.status] += 1
            tag = " (guessed source)" if r.guessed else ""
            if r.status == "mismatch":
                print(f"MISMATCH {r.page}:{r.line}{tag}\n  $ {r.cmd}\n{r.detail}\n")
            elif r.status == "skipped" and args.verbose:
                print(f"skipped  {r.page}:{r.line}  $ {r.cmd.splitlines()[0]}  ({r.detail})")
    shutil.rmtree(bindir, ignore_errors=True)
    print(f"{totals['ok']} match, {totals['mismatch']} mismatch, {totals['skipped']} skipped")
    return 1 if totals["mismatch"] else 0


if __name__ == "__main__":
    sys.exit(main())
