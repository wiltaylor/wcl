#!/usr/bin/env -S uv run --script
# /// script
# requires-python = ">=3.11"
# dependencies = ["skillrig>=0.1.1"]
# ///
"""Tests for the wcl skill. Run it directly: ./.claude/skills/wcl/test.py

Two halves. The first tests the router over the closed lists: that the agent opens
the reference owning a question instead of answering from memory, and stops there.
The second asks for WCL that no amount of JSON or YAML habit produces — a
connection graph, a wdoc page, a heredoc whose body would break any other quoting
— and puts what comes back through the real binary. A file that passes `wcl check`
and evaluates to the right value is the sharpest evidence the references were read.

The first half runs anywhere. The second needs `target/release/wcl`, and skips
without it.
"""

import json
import re
import shutil
import subprocess
from pathlib import Path

import pytest
from skillrig import main

REPO_ROOT = Path(__file__).resolve().parents[3]


def opened(result, needle: str) -> bool:
    """True when some tool call named a path containing `needle`."""
    return any(needle in json.dumps(use.input) for use in result.tool_uses)


def wcl(*arguments: str, cwd: Path) -> subprocess.CompletedProcess:
    """Run the repository's own `wcl` against what the agent wrote."""
    binary = REPO_ROOT / "target" / "release" / "wcl"
    found = str(binary) if binary.is_file() else shutil.which("wcl")
    if not found:
        pytest.skip("no wcl binary — build one with `cargo build --release -p wcl`")
    return subprocess.run([found, *arguments], cwd=cwd, capture_output=True, text=True)


def test_takes_the_builtin_from_the_closed_list(run_skill):
    """<never> guess a builtin — lang_builtins.md names every one."""
    result = run_skill(
        "In a WCL field, which builtin turns a list of strings into one string "
        "with a separator between the pieces? Name it and show the call."
    )

    assert result.exit_code == 0, result.stderr[-2000:]
    assert opened(result, "lang_builtins.md"), result.transcript()[-3000:]
    assert re.search(r"\bjoin\s*\(", result.output), result.output[-2000:]


def test_takes_the_cli_flag_from_the_closed_list(run_skill):
    """<never> guess a CLI flag — lang_cli.md names every one."""
    result = run_skill(
        "I have a wdoc document in main.wcl. What is the exact command that "
        "renders it to a folder of Markdown files in ./out?"
    )

    assert result.exit_code == 0, result.stderr[-2000:]
    assert opened(result, "lang_cli.md"), result.transcript()[-3000:]
    assert re.search(r"wcl wdoc build\b", result.output), result.output[-2000:]
    assert re.search(r"--type\s+(markdown|md)\b", result.output), result.output[-2000:]
    assert "--markdown" not in result.output, result.output[-2000:]


def test_gives_every_top_level_field_a_schema(run_skill, judge):
    """<always> a top-level value with no @document type in scope is an error."""
    result = run_skill(
        "Write config.wcl describing one service: its name is api, it listens on "
        "port 8080, and it is enabled. Keep the schema in the same file."
    )

    assert result.exit_code == 0, result.stderr[-2000:]
    assert result.exists("config.wcl"), result.files()
    source = result.read("config.wcl")
    assert "@document" in source, source

    verdict = judge(
        "The WCL file declares a @document type whose fields cover every top-level "
        "field the file writes, and any block it writes has a matching @block type. "
        "Field types are WCL types such as utf8, u16 or bool — not JSON or YAML types.",
        source,
    )
    assert verdict, verdict.reasoning


def test_routes_a_rendering_question_to_the_wdoc_tree(run_skill):
    """The router sends a "what does this render as" question to references/wdoc/."""
    result = run_skill(
        "In wdoc, which block draws a sequence diagram, and does it go at page "
        "level or inside a `diagram` block? Just answer, do not write any files."
    )

    assert result.exit_code == 0, result.stderr[-2000:]
    assert opened(result, "wdoc_sequence_state.md"), result.transcript()[-3000:]
    assert not opened(result, "references/language/lang_"), result.transcript()[-3000:]
    assert "sequence_diagram" in result.output, result.output[-2000:]


def test_writes_a_connection_graph_that_evaluates(run_skill):
    """Connections are three pieces that only `lang_connections.md` names together."""
    result = run_skill(
        "Write deps.wcl: three services called web, db and cache. web depends on "
        "db, and web uses cache. I want to read those edges back out of the file "
        "with `wcl get`, so they have to land in a field."
    )

    assert result.exit_code == 0, result.stderr[-2000:]
    assert result.exists("deps.wcl"), result.files()
    source = result.read("deps.wcl")

    # The declaration, the collecting field, and the arrow statements.
    assert re.search(r"^\s*connection\s+\w+\s*:", source, re.M), source
    field = re.search(r"@connections\(\s*\w+\s*\)\s*(\w+)\s*:", source)
    assert field, source
    assert re.search(r"^\s*web\s*->\s*(db|cache)\b", source, re.M), source

    checked = wcl("check", "deps.wcl", cwd=result.workspace)
    assert checked.returncode == 0, checked.stdout + checked.stderr

    got = wcl("get", "deps.wcl", field.group(1), "--json", cwd=result.workspace)
    assert got.returncode == 0, got.stdout + got.stderr
    edges = json.loads(got.stdout)
    assert len(edges) == 2, edges
    assert {"source", "destination", "kind"} <= set(edges[0]), edges[0]


def test_writes_a_wdoc_page_that_builds(run_skill, tmp_path):
    """A wdoc document is WCL blocks behind one system import, not Markdown."""
    result = run_skill(
        "Write site.wcl: a wdoc site called handbook holding one page titled "
        "'Getting started'. The page needs a paragraph and a Rust code block "
        "that shows the filename src/main.rs above the code."
    )

    assert result.exit_code == 0, result.stderr[-2000:]
    assert result.exists("site.wcl"), result.files()
    source = result.read("site.wcl")
    assert "import <wdoc.wcl>" in source, source
    assert re.search(r"""filename\s*=\s*["']src/main\.rs["']""", source), source

    checked = wcl("check", "site.wcl", cwd=result.workspace)
    assert checked.returncode == 0, checked.stdout + checked.stderr

    out = tmp_path / "rendered"
    built = wcl(
        "wdoc", "build", "site.wcl", "--out", str(out), "--type", "markdown",
        cwd=result.workspace,
    )
    assert built.returncode == 0, built.stdout + built.stderr
    assert list(out.glob("*.md")), sorted(p.name for p in out.iterdir())


PATTERN = r"^\$\{(\w+)\}$"


def test_quotes_a_body_that_would_break_every_other_form(run_skill):
    """`\\$` is an invalid escape in WCL, so only the forms the reference gives work.

    Two of them do — a raw heredoc and a string with the backslashes doubled — so
    this asserts on the value that comes back out, not on which one was written.
    """
    result = run_skill(
        "Write pattern.wcl. It has one top-level field, `pattern`, holding "
        f"exactly this regex and nothing else: {PATTERN}"
    )

    assert result.exit_code == 0, result.stderr[-2000:]
    assert result.exists("pattern.wcl"), result.files()

    checked = wcl("check", "pattern.wcl", cwd=result.workspace)
    assert checked.returncode == 0, checked.stdout + checked.stderr

    got = wcl("get", "pattern.wcl", "pattern", "--json", cwd=result.workspace)
    assert got.returncode == 0, got.stdout + got.stderr
    assert json.loads(got.stdout).strip() == PATTERN, got.stdout


if __name__ == "__main__":
    raise SystemExit(main(__file__))
