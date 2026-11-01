#!/usr/bin/env python3
"""Self-tests for verify_transcripts.py.

Every liberty the verifier takes with a page is tested twice: a fixture written the way a
correct page writes it must match, and the same fixture with a real difference (a changed
message, a changed number, an extra line) must still fail. A liberty that cannot fail is
a hole in the check, not a rule.

    python3 docs/reference/test_verify_transcripts.py
    python3 -m unittest discover -s docs/reference -p 'test_verify*.py'

The replay tests run against a stand-in `wcl` written into a temporary directory, so they
need no build. The few that need the real renderer use `target/debug/wcl` and are skipped
when it has not been built.
"""

from __future__ import annotations

import os
import stat
import sys
import tempfile
import textwrap
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

import verify_transcripts as v  # noqa: E402

REAL_WCL = v.ROOT / "target/debug/wcl"

# A stand-in binary: `check` prints the file, `wdoc build` writes one page into --out.
FAKE_WCL = """#!/bin/sh
case "$1" in
  check) cat "$2" ;;
  wdoc) mkdir -p "$5" && cp "$3" "$5/index.html" && echo "wrote 1 page" ;;
esac
"""


def env_with(binary_dir: str) -> dict:
    env = dict(os.environ)
    env.update(PATH=binary_dir + os.pathsep + env["PATH"], NO_COLOR="1", COLUMNS="80", TERM="dumb", LC_ALL="C.UTF-8")
    return env


def fake_env(test: unittest.TestCase) -> dict:
    bindir = tempfile.mkdtemp(prefix="wcl-fake-")
    test.addCleanup(lambda: __import__("shutil").rmtree(bindir, ignore_errors=True))
    exe = Path(bindir) / "wcl"
    exe.write_text(FAKE_WCL)
    exe.chmod(exe.stat().st_mode | stat.S_IEXEC)
    return env_with(bindir)


def real_env(test: unittest.TestCase) -> dict:
    bindir = tempfile.mkdtemp(prefix="wcl-real-")
    test.addCleanup(lambda: __import__("shutil").rmtree(bindir, ignore_errors=True))
    os.symlink(REAL_WCL, Path(bindir) / "wcl")
    return env_with(bindir)


def f(name: str, text: str) -> v.Block:
    return v.Block("file", textwrap.dedent(text).strip("\n"), 1, name)


def console(text: str, line: int = 100) -> v.Block:
    return v.Block("console", textwrap.dedent(text).strip("\n"), line)


def statuses(results: list[v.Result]) -> list[str]:
    return [r.status for r in results]


def ran(text: str) -> v.Run:
    return v.Run(text, text, "", 1)


# ── (1) elided absolute paths ─────────────────────────────────────────────────


class ElidedPaths(unittest.TestCase):
    tmp = "/var/tmp/wcl-docs-abc123"

    def check(self, page: str, binary: str) -> bool:
        return v.agrees(v.Command("wcl check x.wcl", page, 1), ran(binary), self.tmp)[0]

    def test_dots_stand_for_the_replay_directory(self):
        self.assertTrue(self.check("in .../lib.wcl", f"in {self.tmp}/lib.wcl"))
        self.assertTrue(self.check("╭─[/…/lib.wcl:1:13]", f"╭─[{self.tmp}/lib.wcl:1:13]"))

    def test_project_directory_stands_for_it(self):
        self.assertTrue(self.check("'/home/you/platform/lib/net.wcl'", f"'{self.tmp}/lib/net.wcl'"))

    def test_a_different_file_still_fails(self):
        self.assertFalse(self.check("in .../other.wcl", f"in {self.tmp}/lib.wcl"))
        self.assertFalse(self.check("'/home/you/platform/lib/net.wcl'", f"'{self.tmp}/net.wcl'"))

    def test_a_name_printed_bare_is_not_an_elided_path(self):
        # Only a path the binary printed from inside the replay directory is rewritten.
        self.assertFalse(self.check("in .../lib.wcl", "in lib.wcl"))
        self.assertFalse(self.check("in /home/you/p/lib.wcl", "in lib.wcl"))

    def test_a_changed_message_still_fails(self):
        self.assertFalse(self.check("declared in .../lib.wcl", f"defined in {self.tmp}/lib.wcl"))

    def test_a_wrapped_temp_path_is_rejoined(self):
        binary = f"  × cycle at '{self.tmp[:12]}-\n  │ {self.tmp[13:]}/lib/net.wcl'"
        self.assertTrue(self.check("  × cycle at '/home/you/p/lib/net.wcl'", binary))
        self.assertFalse(self.check("  × cycle at '/home/you/p/lib/base.wcl'", binary))


# ── (2) elision and re-wrapping together ──────────────────────────────────────


class ElisionAcrossWraps(unittest.TestCase):
    actual = [
        "warning: gather field 'parts' collides with 'parts' declared by @document 'LibDoc' (lib.wcl) — rename one field",
        "root.wcl: 1 warning",
        "OK",
    ]

    def test_a_wrapped_elided_line_matches(self):
        page = [
            "warning: gather field 'parts' collides with 'parts' declared by",
            "@document 'LibDoc' (...) — rename one field",
            "root.wcl: 1 warning",
            "OK",
        ]
        self.assertTrue(v.matches(page, self.actual))

    def test_a_changed_word_fails(self):
        page = ["warning: gather field 'parts' collides with 'parts' declared by",
                "@document 'OtherDoc' (...) — rename one field", "root.wcl: 1 warning", "OK"]
        self.assertFalse(v.matches(page, self.actual))

    def test_a_changed_number_fails(self):
        page = ["warning: gather field 'parts' ... rename one field", "root.wcl: 2 warnings", "OK"]
        self.assertFalse(v.matches(page, self.actual))

    def test_an_extra_line_fails(self):
        page = ["warning: gather field 'parts' ... rename one field", "root.wcl: 1 warning", "OK"]
        self.assertTrue(v.matches(page, self.actual))
        self.assertFalse(v.matches(page, self.actual + ["extra"]))

    def test_an_ellipsis_does_not_swallow_a_line(self):
        self.assertFalse(v.matches(["first ... last"], ["first a", "whole line", "b last"]))

    def test_wrapped_diagnostic_messages_compare_unwrapped(self):
        page = ["  × block kind 'step' is not allowed at the document root by @document schema",
                "  │ 'Pipeline'"]
        binary = "  × block kind 'step' is not allowed at the document\n  │ root by @document schema 'Pipeline'"
        self.assertTrue(v.matches(v.normalise("\n".join(page), "/x"), v.normalise(binary, "/x")))
        self.assertFalse(v.matches(v.normalise("\n".join(page), "/x"), v.normalise(binary.replace("root", "top"), "/x")))


# ── (3) rebuilding a variant from the lines an error quotes ───────────────────


class SnippetPatch(unittest.TestCase):
    def test_a_span_gutter_is_cut_off(self):
        with tempfile.TemporaryDirectory() as tmp:
            Path(tmp, "p.wcl").write_text("x\n")
            rendering = textwrap.dedent("""\
                    ╭─[p.wcl:2:1]
                  1 │
                  2 │ ╭─▶ stage build {
                  3 │ │     owner = "platform"
                  4 │ ├─▶ }
                    · ╰──── schema violation
                  5 │     }
                    ╰────""")
            quoted = v.snippet_lines(rendering, tmp)[Path(tmp, "p.wcl")]
            self.assertEqual(quoted, {1: "", 2: "stage build {", 3: '  owner = "platform"', 4: "}", 5: "}"})

    def test_an_inserted_line_is_inserted(self):
        lines = ["s one {", "  x = 1", "}", "s two {", "  x = 2", "}", ""]
        quoted = {2: "  x = 1", 3: "  y = 3", 4: "}"}
        self.assertEqual(v.realign(lines, quoted, True), ["s one {", "  x = 1", "  y = 3", "}", "s two {", "  x = 2", "}", ""])
        # Overwriting in place would have eaten the next block's header.
        self.assertEqual(v.overwrite(lines, quoted)[3], "}")

    def test_a_deleted_run_is_dropped(self):
        lines = ["# head", "", "# schema", "type T {}", "", "a = 1", ""]
        quoted = {2: "", 3: "a = 1"}
        self.assertEqual(v.realign(lines, quoted, True), ["# head", "", "a = 1", ""])

    def test_a_line_added_at_the_end(self):
        lines = ["a", "b", ""]
        self.assertIn("a\nb\n\nc", [c[Path("/t/p.wcl")] for c in self._candidates(lines, {3: "", 4: "c"})])

    def _candidates(self, lines, quoted):
        out = []
        for build in (v.overwrite, lambda l, q: v.realign(l, q, True), lambda l, q: v.realign(l, q, False)):
            new = build(lines, quoted)
            if new:
                out.append({Path("/t/p.wcl"): "\n".join(new)})
        return out

    @unittest.skipUnless(REAL_WCL.is_file(), "needs target/debug/wcl")
    def test_replay_recovers_an_added_line_and_still_catches_a_changed_message(self):
        file = f("p.wcl", """
            @block("s") type S { @inline(0) id: identifier  x: i64 }
            @document type D { @children("s") ss: list<S> }
            s one {
              x = 1
            }
            s two {
              x = 2
            }
        """)
        good = """
            $ wcl check p.wcl
            wcl::eval::schema_violation

              × field 'y' is not declared by schema 'S'
               ╭─[p.wcl:5:3]
             4 │   x = 1
             5 │   y = 3
               ·   ──┬──
               ·     ╰── schema violation
             6 │ }
               ╰────

            p.wcl: 1 schema violation
        """
        env = real_env(self)
        self.assertEqual(statuses(v.replay("t", [file, console(good)], env, False)), ["ok"])
        for wrong in (good.replace("is not declared", "is unknown"), good.replace("1 schema", "2 schema")):
            self.assertEqual(statuses(v.replay("t", [file, console(wrong)], env, False)), ["mismatch"])

    @unittest.skipUnless(REAL_WCL.is_file(), "needs target/debug/wcl")
    def test_replay_matches_an_elided_absolute_path(self):
        files = [f("main.wcl", 'import "./lib.wcl"'), f("lib.wcl", "type T { x: bogus }")]
        page = """
            $ wcl check main.wcl
            wcl::parse

              × unknown type 'bogus'
               ╭─[{path}:1:13]
             1 │ type T {{ x: bogus }}
               ·             ──┬──
               ·               ╰── type not declared
               ╰────
        """
        env = real_env(self)
        for path, want in (("/…/lib.wcl", "ok"), ("/home/you/proj/lib.wcl", "ok"), ("/…/main.wcl", "mismatch")):
            got = v.replay("t", files + [console(page.format(path=path))], env, False)
            self.assertEqual(statuses(got), [want], path)


# ── (4) usage blocks ──────────────────────────────────────────────────────────


class UsageBlocks(unittest.TestCase):
    def test_every_line_of_a_usage_block_is_synopsis(self):
        block = console("""
            $ wcl init <template> [dest] [--force]
            $ wcl init --list
        """)
        results = v.replay("t", [block], fake_env(self), False)
        self.assertEqual(statuses(results), ["skipped", "skipped"])

    def test_a_block_with_output_still_runs_its_commands(self):
        blocks = [f("a.wcl", "x = 1"), console("""
            $ wcl --version
            wcl 9.9.9
            $ wcl check a.wcl
            x = 2
        """)]
        self.assertEqual(statuses(v.replay("t", blocks, fake_env(self), False)), ["skipped", "mismatch"])


# ── (5) commands that depend on a skipped one ─────────────────────────────────


class DependentCommands(unittest.TestCase):
    transcript = """
        $ wcl wdoc build main.wcl --out _md
        wrote 1 page
        $ find _md -type f
        _md/index.html
        $ cat _md/index.html
        x = 1
    """

    def test_skipped_with_the_command_that_set_them_up(self):
        results = v.replay("t", [console(self.transcript)], fake_env(self), False)
        self.assertEqual(statuses(results), ["skipped", "skipped", "skipped"])

    def test_they_run_and_still_fail_when_the_setup_ran(self):
        blocks = [f("main.wcl", "x = 1"), console(self.transcript)]
        self.assertEqual(statuses(v.replay("t", blocks, fake_env(self), False)), ["ok", "ok", "ok"])
        wrong = self.transcript.replace("_md/index.html\n", "_md/index.html\n        _md/extra.html\n", 1)
        self.assertEqual(statuses(v.replay("t", [f("main.wcl", "x = 1"), console(wrong)], fake_env(self), False)),
                         ["ok", "mismatch", "ok"])

    def test_a_command_naming_its_own_file_still_runs(self):
        blocks = [f("other.wcl", "y = 2"), console(self.transcript + "\n$ wcl check other.wcl\ny = 3")]
        self.assertEqual(statuses(v.replay("t", blocks, fake_env(self), False))[-1], "mismatch")


# ── (6) a later block with the same name ──────────────────────────────────────


class RewrittenFiles(unittest.TestCase):
    def test_the_reader_sees_the_latest_version(self):
        blocks = [
            f("a.wcl", "x = 1"),
            console("$ wcl check a.wcl\nx = 1"),
            f("a.wcl", "x = 2"),
            console("$ wcl check a.wcl\nx = 2"),
        ]
        self.assertEqual(statuses(v.replay("t", blocks, fake_env(self), False)), ["ok", "ok"])

    def test_the_old_version_no_longer_matches(self):
        blocks = [f("a.wcl", "x = 1"), f("a.wcl", "x = 2"), console("$ wcl check a.wcl\nx = 1")]
        self.assertEqual(statuses(v.replay("t", blocks, fake_env(self), False)), ["mismatch"])


# ── (7) files the skill names in prose ────────────────────────────────────────


class SkillProseNames(unittest.TestCase):
    page = textwrap.dedent("""\
        `part.wcl`:

        ```wcl
        server extra { host = "from-import" }
        ```

        Then some prose, and a fragment:

        ```wcl
        fragment = true
        ```

        ```console
        $ wcl check part.wcl
        server extra { host = "from-import" }
        ```
        """)

    def blocks(self, text: str) -> list[v.Block]:
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp, "page.md")
            path.write_text(text)
            return v.skill_blocks(path)

    def test_a_prose_label_names_the_block(self):
        kinds = [(b.kind, b.name) for b in self.blocks(self.page)]
        self.assertEqual(kinds, [("file", "part.wcl"), ("wcl", None), ("console", None)])

    def test_the_named_file_is_written_and_not_guessed(self):
        results = v.replay("t", self.blocks(self.page), fake_env(self), True)
        self.assertEqual(statuses(results), ["ok"])
        self.assertFalse(results[0].guessed)

    def test_different_content_still_fails(self):
        wrong = self.page.replace('$ wcl check part.wcl\nserver extra { host = "from-import" }',
                                  '$ wcl check part.wcl\nserver extra { host = "elsewhere" }')
        self.assertEqual(statuses(v.replay("t", self.blocks(wrong), fake_env(self), True)), ["mismatch"])

    def test_an_unlabelled_block_is_still_a_guess(self):
        page = "```wcl\nx = 1\n```\n\n```console\n$ wcl check y.wcl\nx = 1\n```\n"
        results = v.replay("t", self.blocks(page), fake_env(self), True)
        self.assertEqual(statuses(results), ["ok"])
        self.assertTrue(results[0].guessed)


# ── (8) and (10): order-free listings ─────────────────────────────────────────


class Listings(unittest.TestCase):
    page = ["_site/index.html", "_site/home.html", "_site/_wdoc/favicon.svg"]
    binary = ["_site/_wdoc/favicon.svg", "_site/home.html", "_site/index.html"]

    def test_find_in_any_order(self):
        self.assertTrue(v.matches(self.page, self.binary, "find _site -type f"))
        self.assertTrue(v.matches(self.page, self.binary, "find _site -type f | grep -v woff2"))

    def test_a_trailing_sort_in_any_locale(self):
        self.assertTrue(v.matches(self.page, self.binary, "find _site -type f -not -name '*.woff2' | sort"))
        self.assertTrue(v.matches(["b", "a"], ["a", "b"], "grep -o x out.txt | sort"))

    def test_an_extra_or_missing_or_renamed_file_fails(self):
        cmd = "find _site -type f | sort"
        self.assertFalse(v.matches(self.page, self.binary + ["_site/extra.html"], cmd))
        self.assertFalse(v.matches(self.page, self.binary[:-1], cmd))
        self.assertFalse(v.matches(self.page, [b.replace("home", "house") for b in self.binary], cmd))

    def test_order_still_matters_without_find_or_a_trailing_sort(self):
        self.assertFalse(v.matches(["b", "a"], ["a", "b"], "cat list.txt"))
        self.assertFalse(v.matches(["b", "a"], ["a", "b"], "sort list.txt | head"))


# ── (9) an excerpt of one long output line ────────────────────────────────────


class ExcerptSplitForReading(unittest.TestCase):
    binary = ['<main>', '<img class="wdoc-image" src="_wdoc/hero.png" alt="The team" width="480" /><p>x</p>', '</main>']

    def test_a_line_split_for_reading_matches(self):
        page = ["...", '<img class="wdoc-image" src="_wdoc/hero.png"', '     alt="The team" width="480" />', "..."]
        self.assertTrue(v.matches(page, self.binary))

    def test_lines_on_successive_output_lines_still_match(self):
        self.assertTrue(v.matches(["...", "<main>", "<img class", "..."], self.binary))

    def test_something_between_the_halves_fails(self):
        page = ["...", '<img class="wdoc-image"', 'alt="The team" width="480" />', "..."]
        self.assertFalse(v.matches(page, self.binary))

    def test_a_changed_value_fails(self):
        page = ["...", '<img class="wdoc-image" src="_wdoc/hero.png"', '     alt="The team" width="640" />', "..."]
        self.assertFalse(v.matches(page, self.binary))


# ── (11) placeholder media ────────────────────────────────────────────────────


class PlaceholderMedia(unittest.TestCase):
    source = 'image "assets/hero.png" { }\nvideo "assets/intro.mp4" { poster = "assets/t.jpg" }\nfile "src/setup.sh" { }\nimage "https://example.com/x.png" { }\n'

    def test_named_but_never_given_assets_get_placeholders(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = Path(tmp, "main.wcl")
            src.write_text(self.source)
            made = {p.relative_to(tmp).as_posix() for p in v.place_assets(src, Path(tmp).resolve(), "")}
            self.assertEqual(made, {"assets/hero.png", "assets/intro.mp4", "assets/t.jpg", "src/setup.sh"})
            self.assertTrue(Path(tmp, "assets/hero.png").read_bytes().startswith(b"\x89PNG"))

    def test_an_asset_the_page_reports_missing_stays_missing(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = Path(tmp, "main.wcl")
            src.write_text(self.source)
            absent = "copy assets/hero.png -> _site/_wdoc/image-hero-8572175c.png: No such file or directory"
            v.place_assets(src, Path(tmp).resolve(), absent)
            self.assertFalse(Path(tmp, "assets/hero.png").exists())

    def test_a_result_beside_a_placeholder_is_labelled(self):
        blocks = [f("main.wcl", 'image "assets/hero.png" { }'), console("$ wcl wdoc build main.wcl --out _o\nwrote 2 pages")]
        results = v.replay("t", blocks, fake_env(self), False)
        self.assertEqual(statuses(results), ["mismatch"])
        self.assertTrue(results[0].placeholder)


# ── (12) the reader's directory ───────────────────────────────────────────────


class WorkingDirectory(unittest.TestCase):
    def test_cd_moves_the_reader(self):
        blocks = [f("media/main.wcl", "inner"), console("$ cd media\n$ wcl check main.wcl\ninner")]
        self.assertEqual(statuses(v.replay("t", blocks, fake_env(self), False)), ["ok", "ok"])

    def test_a_project_folder_saved_last_is_where_its_file_is_built(self):
        blocks = [
            f("main.wcl", "outer"),
            console("$ wcl check main.wcl\nouter"),
            f("media/main.wcl", "inner"),
            console("$ wcl wdoc build main.wcl --out _site\nwrote 1 page\n$ cat _site/index.html\ninner"),
        ]
        self.assertEqual(statuses(v.replay("t", blocks, fake_env(self), False)), ["ok", "ok", "ok"])

    def test_the_wrong_file_still_fails(self):
        blocks = [f("main.wcl", "outer"), f("media/main.wcl", "inner"), console("$ wcl check main.wcl\nouter")]
        self.assertEqual(statuses(v.replay("t", blocks, fake_env(self), False)), ["mismatch"])

    def test_a_root_file_saved_last_brings_the_reader_back(self):
        blocks = [f("media/main.wcl", "inner"), f("main.wcl", "outer"), console("$ wcl check main.wcl\nouter")]
        self.assertEqual(statuses(v.replay("t", blocks, fake_env(self), False)), ["ok"])

    def test_cd_into_a_missing_directory_is_skipped(self):
        results = v.replay("t", [console("$ cd nowhere\n$ ls\nx")], fake_env(self), False)
        self.assertEqual(statuses(results), ["skipped", "skipped"])


if __name__ == "__main__":
    unittest.main()
