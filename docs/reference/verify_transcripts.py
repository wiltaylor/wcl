#!/usr/bin/env python3
"""Rerun the shell transcripts in both documentation trees against a `wcl` binary.

Both trees promise that every example was run. This script holds them to it: it walks
the book (`docs/reference/pages/**/*.wcl`) and the skill (`.claude/skills/wcl/**/*.md`),
replays every `$ ...` command shown in a console block, and compares what the binary
prints with what the page claims.

Each page is replayed in its own temporary directory, in reading order. A code block
that names a file (`filename = "app.wcl"` in the book, a `` `part.wcl`: `` line above the
fence in the skill) is written into that directory when the reader reaches it, and a later
block with the same name rewrites it, so a later transcript sees the same file the reader
did. The skill mostly leaves its files unnamed. There, a command that mentions exactly one
`.wcl` file the page has not written is given the nearest unnamed `wcl` block above it,
and the result is labelled `guessed` so a mismatch is read with that in mind.

The reader's directory moves with `cd`, and with a project saved into a folder of its own:
when the newest file called `main.wcl` is `media/main.wcl`, `wcl wdoc build main.wcl` runs
in `media/`. An image, video or `file` source that a page names but never prints is given
a placeholder (labelled `placeholder assets`), unless the page's own output reports it
missing.

Pages often show a variant ("change `port` to 70000 and run it again") in prose rather
than as a new file. When an error rendering disagrees, the file it names is rebuilt
around the source lines it quotes and the command is run again: once with each quoted
line written over its number, and once with the lines realigned on the ones the file
already has, which recovers lines a variant added or removed. A variant whose changed
lines the rendering does not quote cannot be recovered, so a MISMATCH is a lead to read
by hand, not a verdict.

A usage block (every `$` line printing nothing, one with a `<placeholder>`) is synopsis
throughout. A command whose files cannot be found, or that would not terminate (`serve`,
`lsp`, `curl`), is counted as skipped rather than failed, and so is a later command in
the same transcript that names no `.wcl` file (`ls`, `find`, `cat`, `echo $?`), since
what it would inspect was never made.

`matches` lists what a comparison forgives. `test_verify_transcripts.py` holds each of
those liberties to catching a real difference.

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
    placeholder: bool = False  # ran beside placeholder media the page never gave


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


# The skill names a multi-file example's files in prose: a line holding only `` `part.wcl`: ``
# directly above the fence.
PROSE_NAME = re.compile(r"^\s*`([\w][\w./-]*)`:\s*$")


def skill_blocks(path: Path) -> list[Block]:
    """Fenced blocks of a skill page. A fence may be indented under a list item.

    A fence whose nearest non-blank line above is `` `name.ext`: `` is that file.
    """
    lines = path.read_text().splitlines()
    blocks: list[Block] = []
    i = 0
    while i < len(lines):
        m = re.match(r"(\s*)```(\w*)\s*$", lines[i])
        if not m:
            i += 1
            continue
        above = next((lines[k] for k in range(i - 1, -1, -1) if lines[k].strip()), "")
        label = PROSE_NAME.match(above)
        name = label.group(1) if label and m.group(2) != "console" else None
        indent, lang = m.group(1), m.group(2)
        j = i + 1
        body = []
        while j < len(lines) and lines[j].strip() != "```":
            body.append(lines[j][len(indent):] if lines[j].startswith(indent) else lines[j].lstrip())
            j += 1
        text = "\n".join(body)
        if lang == "console":
            blocks.append(Block("console", text, i + 2))
        elif name:
            blocks.append(Block("file", text, i + 2, name))
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


def unwrap_messages(text: str) -> str:
    """Join a diagnostic's `× message` back onto one line.

    The renderer wraps the message at the terminal width, continuing it on `│ ` lines, so
    where it breaks depends on how long the paths in it are. The replay directory is never
    as long as the page's, and a page may break a line by hand. Words are rejoined with a
    space; a break straight after a hyphen or a slash (`agent-` / `add6…`, the places the
    renderer may break a path) is rejoined without one.
    """
    out: list[str] = []
    in_message = False
    for line in text.split("\n"):
        cont = re.match(r"\s*│ (.*)$", line)
        if in_message and cont and out:
            rest = cont.group(1).strip()
            out[-1] = out[-1].rstrip() + ("" if out[-1].rstrip()[-1:] in ("-", "/") else " ") + rest
            continue
        in_message = bool(re.match(r"\s*(Error: )?× ", line))
        out.append(line)
    return "\n".join(out)


def normalise(text: str, tmp: str) -> list[str]:
    text = unwrap_messages(text)
    text = text.replace(tmp + "/", "").replace(tmp, ".")
    # Pages write the reader's home directory as /home/you.
    text = text.replace(str(Path.home()), "/home/you")
    lines = [line.rstrip() for line in text.split("\n")]
    while lines and not lines[-1]:
        lines.pop()
    while lines and not lines[0]:
        lines.pop(0)
    return lines


ELLIPSIS = re.compile(r"\.\.\.|…")
# How a page writes an absolute path it has shortened: `.../lib.wcl`, `/…/lib.wcl`, or the
# reader's project directory, `/home/you/platform/lib/net.wcl`.
ELIDED_DIR = r"(?:/?(?:\.\.\.|…)|/home/you/[^/\s'\"`]+)/"


def temp_paths(raw: str, tmp: str) -> set[str]:
    """Relative paths the binary printed as absolute paths inside the replay directory."""
    found = set()
    for m in re.finditer(re.escape(tmp) + r"/([\w./-]+)", unwrap_messages(raw)):
        found.add(m.group(1).rstrip("."))
    return found


def unelide_paths(expected: list[str], rels: set[str]) -> list[str]:
    """Rewrite each elided absolute path on the page to the bare relative path it stands for.

    Only a path the binary actually printed from the replay directory qualifies, and only
    for the same trailing relative path: `.../lib.wcl` becomes `lib.wcl` when the run
    printed `<tmp>/lib.wcl`, and stays as written when it printed `<tmp>/other.wcl`.
    """
    if not rels:
        return expected
    out = []
    for line in expected:
        for rel in sorted(rels, key=len, reverse=True):
            line = re.sub(ELIDED_DIR + re.escape(rel) + r"(?![\w/-])", lambda _m, r=rel: r, line)
        out.append(line)
    return out


def local_path(name: str, tmp: str) -> Path | None:
    """The file in the replay directory that a path written on a page stands for."""
    if not Path(name).is_absolute() and (Path(tmp) / name).is_file():
        return Path(tmp) / name
    m = re.fullmatch(ELIDED_DIR + r"(.+)", name)
    if m and (Path(tmp) / m.group(1)).is_file():
        return Path(tmp) / m.group(1)
    return None


def lists_a_set(cmd: str) -> bool:
    """Whether the order of the output lines is not part of what the page claims.

    `find` walks in an order the filesystem picks, and a trailing `| sort` orders lines in
    the replay's locale (`C.UTF-8`), not the one the page was written in. Either way the
    page claims a set of lines. Only filters that keep lines whole (`sort`, `grep`) may
    follow `find`, and `sort` must be the last stage for the second reading.
    """
    stages = [stage.strip() for stage in cmd.split("|")]
    if re.match(r"find\b", stages[0]) and all(re.match(r"(sort|grep)\b", s) for s in stages[1:]):
        return True
    return len(stages) > 1 and bool(re.fullmatch(r"sort(\s+-[a-zA-Z]+)*", stages[-1]))


def flat_pattern(expected: list[str]) -> str:
    """A regex for an elided block whose lines the page may have re-wrapped.

    Whitespace, line breaks included, is one separator. An ellipsis stands for any run of
    characters inside one line of the binary's output: it may not swallow a line break.
    """
    regex = ""
    previous_gap = True
    for token in " ".join(expected).split():
        gap = token in ("...", "…")
        if regex:
            regex += r"\s*" if gap or previous_gap else r"\s+"
        regex += r"[^\n]*?" if gap else r"[^\n]*?".join(re.escape(p) for p in ELLIPSIS.split(token))
        previous_gap = gap
    return regex


def matches(expected: list[str], actual: list[str], cmd: str = "") -> bool:
    """Exact, with the liberties a page takes when it prints output for a reader.

    - A line holding only `…` or `...` stands for any run of lines, which makes the
      block an excerpt: each line shown need only appear inside an output line.
    - `…` or `...` inside a line (`.../lib.wcl`) stands for any run of characters on
      one output line.
    - A page may re-wrap a long line, so whitespace runs compare equal to line breaks,
      in an elided block as much as in a whole one.
    - In an excerpt, two lines shown one after the other may also sit side by side on one
      output line, separated by nothing but blanks: a page splits a long line of HTML so
      it can be read. Nothing else may come between them.
    - `ls` and `find` list in an order the filesystem picks, and `ls` into columns. A
      trailing `| sort` sorts in whatever locale the page was written in. Either output
      compares as a set of lines (see `lists_a_set`).
    """
    if expected == actual:
        return True
    if lists_a_set(cmd):
        return sorted(" ".join(l.split()) for l in expected) == sorted(" ".join(l.split()) for l in actual)
    if re.match(r"ls\b", cmd) and "|" not in cmd:
        return sorted(" ".join(expected).split()) == sorted(" ".join(actual).split())
    flat_exp, flat_act = " ".join(expected).split(), " ".join(actual).split()
    if flat_exp == flat_act:
        return True
    if not any(ELLIPSIS.search(line) for line in expected):
        return False
    if not any(line.strip() in ("…", "...") for line in expected):
        return re.fullmatch(flat_pattern(expected), "\n".join(actual).strip()) is not None
    pattern = ""
    run_open = False  # inside a run of shown lines, between two `...` lines
    for line in expected:
        if line.strip() in ("…", "..."):
            if run_open:
                pattern += r".*\n"
            pattern += r"(?:.*\n)*?"
            run_open = False
            continue
        body = ".*".join(re.escape(part) for part in ELLIPSIS.split(line.strip()))
        # The next shown line starts on the next output line, or right here after blanks.
        pattern += (r"(?:.*\n.*|[ \t]*)" if run_open else ".*") + body
        run_open = True
    if run_open:
        pattern += r".*\n"
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
    exp = unelide_paths(normalise(c.expected, tmp), temp_paths(ran.combined, tmp))
    act = normalise(ran.combined, tmp)
    if matches(exp, act, c.cmd):
        return True, act
    quiet = not re.search(r"^warning:", ran.stderr, re.M)
    if quiet and matches(exp, normalise(ran.stdout, tmp), c.cmd):
        return True, act
    return False, act


SNIPPET_HEADER = re.compile(r"╭─\[([^\]]+?):\d+:\d+\]")
SNIPPET_LINE = re.compile(r"^\s*(\d+) │ ?(.*)$")
GUTTER = set(" │╭├╰─▶")


def snippet_lines(expected: str, tmp: str) -> dict[Path, dict[int, str]]:
    """The source lines an error rendering quotes, by file and 1-based line number.

    A rendering that draws a multi-line span gives every quoted line the same gutter
    (`╭─▶ `, `│   `, `├─▶ `, or blanks); it is cut off so the line reads as the file does.
    A quoted line the page elided (`...`) says nothing about the file and is dropped.
    """
    quoted: dict[Path, dict[int, str]] = {}
    section: list[tuple[int, str]] = []
    current: Path | None = None

    def flush():
        if current is None:
            return
        width = max((t.index("▶") + 2 for _, t in section if "▶" in t), default=0)
        for n, t in section:
            if width:
                if not all(ch in GUTTER for ch in t[:width]):
                    continue
                t = t[width:]
            if not ELLIPSIS.search(t):
                quoted.setdefault(current, {})[n] = t

    for line in expected.split("\n"):
        header = SNIPPET_HEADER.search(line)
        if header:
            flush()
            section = []
            current = local_path(header.group(1), tmp)
            continue
        m = SNIPPET_LINE.match(line)
        if m and current is not None:
            section.append((int(m.group(1)), m.group(2).rstrip()))
    flush()
    return quoted


def overwrite(lines: list[str], quoted: dict[int, str]) -> list[str]:
    """Each quoted line replaces the line at its number: a variant that changed lines in place."""
    out = list(lines)
    for n, text in sorted(quoted.items()):
        while len(out) < n:
            out.append("")
        out[n - 1] = text
    return out


def realign(lines: list[str], quoted: dict[int, str], tail_inserts: bool) -> list[str] | None:
    """Rebuild a variant that added or removed lines, from the lines it quotes.

    A quoted line that also appears in the current file is an anchor: the difference
    between its two line numbers is how many lines were added above it. Between two
    anchors the variant's lines that are not quoted are the current file's, in order. The
    quoted lines the file lacks are inserted when the gap grew, and lines are dropped when
    it shrank. Of the places a line could anchor, the one that explains its gap with no
    dropped lines wins, then the one that drifts least. Past the last anchor nothing bounds
    the gap, so the quoted lines either insert (`tail_inserts`) or replace in place.
    """
    anchors: list[tuple[int, int]] = []  # (variant index, current index)
    offset, prev_v, last_j = 0, -1, -1
    for n, text in sorted(quoted.items()):
        v = n - 1
        free = sum(1 for k in range(prev_v + 1, v) if k + 1 not in quoted)
        best = None
        for j in range(last_j + 1, len(lines)):
            if lines[j] != text:
                continue
            old = j - last_j - 1
            if free > old:
                continue
            dropped = 0 if old == v - prev_v - 1 else old - free
            cost = (dropped, abs((v - j) - offset), j)
            if best is None or cost < best[0]:
                best = (cost, j)
        if best is not None:
            anchors.append((v, best[1]))
            offset, prev_v, last_j = v - best[1], v, best[1]
    if not anchors:
        return None
    out: list[str] = []
    prev_v, prev_j = -1, -1
    for v, j in anchors:
        gap = range(prev_v + 1, v)
        old = lines[prev_j + 1 : j]
        if len(gap) == len(old):
            out += [quoted.get(k + 1, old[i]) for i, k in enumerate(gap)]
        else:
            free = [k for k in gap if k + 1 not in quoted]
            if len(free) > len(old):
                return None
            fill = iter(old[: len(free)])
            out += [quoted[k + 1] if k + 1 in quoted else next(fill) for k in gap]
        out.append(lines[j])
        prev_v, prev_j = v, j
    rest = lines[prev_j + 1 :]
    if tail_inserts:
        fill = iter(rest)
        for k in range(prev_v + 1, max(quoted)):
            out.append(quoted[k + 1] if k + 1 in quoted else next(fill, ""))
        out += list(fill)
    else:
        out += overwrite(rest, {n - 1 - prev_v: t for n, t in quoted.items() if n - 1 > prev_v})
    return out


def patch_candidates(expected: str, tmp: str) -> list[dict[Path, str]]:
    """Variants of the replay files that would make the quoted source lines true.

    The quote cannot say whether a variant changed lines in place or added and removed
    them, so each reading is offered and the caller keeps whichever reproduces the page.
    """
    quoted = snippet_lines(expected, tmp)
    current = {path: path.read_text() for path in quoted}
    candidates: list[dict[Path, str]] = []
    for build in (
        lambda ls, q: overwrite(ls, q),
        lambda ls, q: realign(ls, q, tail_inserts=True),
        lambda ls, q: realign(ls, q, tail_inserts=False),
    ):
        variant: dict[Path, str] = {}
        for path, by_number in quoted.items():
            lines = current[path].split("\n")
            new = build(lines, by_number)
            if new is not None and new != lines:
                variant[path] = "\n".join(new)
        if variant and variant not in candidates:
            candidates.append(variant)
    return candidates


def is_usage_block(cmds: list[Command]) -> bool:
    """A usage block lists a command's forms, one `$` line each, and prints nothing.

    Its second line is often bare flags (`$ wcl init --list`) that would run as written,
    so the whole block is synopsis once one line carries a placeholder.
    """
    return any(SYNOPSIS.search(c.cmd) for c in cmds) and all(not c.expected.strip() for c in cmds)


# Media a page can name but never print: the binary copies it and, for an image, reads its
# size. A placeholder stands in. The copied name hashes the path, not the bytes.
MEDIA = {
    ".png": b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89"
    b"\x00\x00\x00\rIDATx\x9cc\xf8\x0f\x00\x00\x01\x01\x00\x05\x18\xd8N\x00\x00\x00\x00IEND\xaeB`\x82",
    ".gif": b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff!\xf9\x04\x01\x00\x00\x00\x00"
    b",\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;",
    ".jpg": b"\xff\xd8\xff\xc0\x00\x0b\x08\x00\x01\x00\x01\x01\x01\x11\x00\xff\xd9",
    ".svg": b'<svg xmlns="http://www.w3.org/2000/svg" width="1" height="1"/>\n',
}
MEDIA[".jpeg"] = MEDIA[".jpg"]
for _ext in (".mp4", ".webm", ".ogg", ".ogv", ".mov", ".m4v", ".webp"):
    MEDIA[_ext] = b""
# A local source in a string literal, or the path a `file` block ships.
MEDIA_REF = re.compile(r'"([\w][\w./-]*(?:' + "|".join(re.escape(e) for e in MEDIA) + r'))"')
FILE_REF = re.compile(r'^\s*file\s+"([\w][\w./-]*)"', re.M)


def place_assets(source: Path, root: Path, absent: str) -> list[Path]:
    """Write a placeholder for each local asset a page file names that the page never gives.

    `file` ships its source verbatim, so an empty file stands in for it. A path that the
    page's own output reports missing (`No such file`, `cannot read`) is left missing:
    that error is what the page shows.
    """
    made = []
    text = source.read_text()
    for rel in set(MEDIA_REF.findall(text)) | set(FILE_REF.findall(text)):
        dest = source.parent / rel
        if dest.exists() or rel in absent or root not in dest.resolve().parents:
            continue
        dest.parent.mkdir(parents=True, exist_ok=True)
        dest.write_bytes(MEDIA.get(dest.suffix.lower(), b""))
        made.append(dest)
    return made


def replay(page: str, blocks: list[Block], env: dict, guess: bool) -> list[Result]:
    results: list[Result] = []
    tmp = tempfile.mkdtemp(prefix="wcl-docs-")
    root = Path(tmp).resolve()
    # The lines in which the page reports a file missing, so no placeholder hides them.
    absent = "\n".join(
        line
        for block in blocks
        if block.kind == "console"
        for line in block.text.split("\n")
        if "No such file" in line or "cannot read" in line
    )
    try:
        last_anon: Block | None = None
        last_status: int | None = None
        # The anonymous block each guessed file was last written from.
        guessed_from: dict[str, Block] = {}
        # When each named file was last written, to tell which `main.wcl` the reader means.
        written: dict[str, int] = {}
        placeholders: list[Path] = []
        here = ""  # the reader's directory, relative to the replay root
        for seq, block in enumerate(blocks):
            if block.kind == "file":
                # A later block with the same name is the file as the reader now has it.
                dest = Path(tmp) / block.name
                dest.parent.mkdir(parents=True, exist_ok=True)
                dest.write_text(block.text + "\n")
                guessed_from.pop(block.name, None)
                written[os.path.normpath(block.name)] = seq
                if dest.suffix == ".wcl":
                    placeholders += place_assets(dest, root, absent)
                continue
            if block.kind == "wcl":
                last_anon = block
                continue
            cmds = commands(block)
            usage = is_usage_block(cmds)
            # Once a command in this transcript is skipped for want of its sources, what
            # it would have made (`_site/`, `_md/`, an exit status) is not there either.
            setup_skipped = False
            for c in cmds:
                wanted = [t for t in FILE_TOKEN.findall(c.cmd) if not t.startswith("<")]
                skip = None
                if usage or SYNOPSIS.search(c.cmd):
                    skip = "a synopsis, not a command"
                elif NON_TERMINATING.search(c.cmd):
                    skip = "does not terminate"
                elif c.cmd.strip() == "echo $?" and last_status is None:
                    skip = "the command before it was skipped"
                elif setup_skipped and not wanted:
                    skip = "depends on a command skipped above it"
                if skip:
                    results.append(Result(page, c.line, c.cmd, "skipped", skip))
                    last_status = None
                    setup_skipped = setup_skipped or skip.startswith("depends")
                    continue
                # `cd dir`, alone or before `&&`, moves the reader for the rest of the page.
                cd = re.match(r"cd\s+([\w./-]+)\s*(?:$|&&\s*)", c.cmd)
                if cd:
                    target = os.path.normpath(os.path.join(here, cd.group(1)))
                    if target.startswith("..") or not (Path(tmp) / target).is_dir():
                        results.append(Result(page, c.line, c.cmd, "skipped", "no directory " + cd.group(1)))
                        last_status = None
                        setup_skipped = True
                        continue
                    here = "" if target == "." else target
                    rest = c.cmd[cd.end():].strip()
                    if not rest:
                        last_status = 0
                        ok, act = not c.expected.strip(), []
                        results.append(Result(page, c.line, c.cmd, "ok" if ok else "mismatch", "", False))
                        continue
                    c = Command(rest, c.expected, c.line, c.stdin)
                # A project saved into its own folder (`media/main.wcl`) and then built as
                # `main.wcl` is built from inside that folder: the newest file of that name
                # is the one the reader just saved.
                for t in sorted(set(wanted)):
                    rel = os.path.normpath(t)
                    mine = os.path.normpath(os.path.join(here, rel))
                    same = [p for p in written if p == rel or p.endswith(os.sep + rel)]
                    newer = [p for p in same if written[p] > written.get(mine, -1)]
                    if newer:
                        best = max(newer, key=written.__getitem__)
                        here = best[: -len(rel)].rstrip(os.sep)
                cwd = str(Path(tmp) / here) if here else tmp
                guessed = False
                if c.cmd.strip() == "echo $?":
                    ok = c.expected.strip() == str(last_status)
                    act = [str(last_status)]
                else:
                    missing = [t for t in wanted if not (Path(cwd) / t).exists()]
                    # A command that writes the file itself (`> x.wcl`, `wcl init`) needs nothing.
                    missing = [t for t in missing if not re.search(r">\s*" + re.escape(t), c.cmd)]
                    # A guess is made once, when a file is first needed. A later
                    # unnamed block is as often a fragment of the file as a new one, so
                    # it does not replace a guess; a named block does (see above).
                    if guess and len(set(missing)) == 1 and last_anon is not None and "init" not in c.cmd:
                        dest = Path(cwd) / missing[0]
                        dest.parent.mkdir(parents=True, exist_ok=True)
                        dest.write_text(last_anon.text + "\n")
                        guessed_from[missing[0]] = last_anon
                        placeholders += place_assets(dest, root, absent)
                        missing = []
                    guessed = any(t in guessed_from for t in wanted)
                    if missing:
                        results.append(
                            Result(page, c.line, c.cmd, "skipped", "no source for " + ", ".join(missing))
                        )
                        last_status = None
                        setup_skipped = True
                        continue
                    ran = run(c, cwd, env)
                    if ran is None:
                        results.append(Result(page, c.line, c.cmd, "skipped", "timed out"))
                        last_status = None
                        setup_skipped = True
                        continue
                    last_status = ran.status
                    ok, act = agrees(c, ran, cwd)
                    if not ok:
                        # The page may be showing a variant of the file ("change X and
                        # run it again"). An error rendering quotes the lines it was
                        # given, so rebuild the file around those lines and try again.
                        for variant in patch_candidates(c.expected, cwd):
                            originals = {path: path.read_text() for path in variant}
                            for path, text in variant.items():
                                path.write_text(text)
                            again = run(c, cwd, env)
                            for path, original in originals.items():
                                path.write_text(original)
                            if again is not None:
                                good, seen = agrees(c, again, cwd)
                                if good:
                                    ok, act, last_status = True, seen, again.status
                                    break
                stand_ins = any(Path(cwd).resolve() in p.resolve().parents for p in placeholders)
                if ok:
                    results.append(Result(page, c.line, c.cmd, "ok", guessed=guessed, placeholder=stand_ins))
                else:
                    exp = normalise(c.expected, cwd)
                    diff = "\n".join(difflib.unified_diff(exp, act, "page", "binary", lineterm="", n=2))
                    results.append(Result(page, c.line, c.cmd, "mismatch", diff, guessed, stand_ins))
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
            tag = (" (guessed source)" if r.guessed else "") + (" (placeholder assets)" if r.placeholder else "")
            if r.status == "mismatch":
                print(f"MISMATCH {r.page}:{r.line}{tag}\n  $ {r.cmd}\n{r.detail}\n")
            elif r.status == "skipped" and args.verbose:
                print(f"skipped  {r.page}:{r.line}  $ {r.cmd.splitlines()[0]}  ({r.detail})")
    shutil.rmtree(bindir, ignore_errors=True)
    print(f"{totals['ok']} match, {totals['mismatch']} mismatch, {totals['skipped']} skipped")
    return 1 if totals["mismatch"] else 0


if __name__ == "__main__":
    sys.exit(main())
