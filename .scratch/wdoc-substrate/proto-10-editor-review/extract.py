#!/usr/bin/env python3
"""Extract the wskill unit graph at a git rev. Rough — regex over WCL, good
enough for a prototype. Emits {units, indexes} JSON on stdout."""
import json
import re
import subprocess
import sys

UNIT_KINDS = ("concept", "fact", "entity", "process", "procedure", "reference",
              "example", "research", "lesson", "slide", "index")

rev = sys.argv[1]
root = sys.argv[2] if len(sys.argv) > 2 else "docs/wskills/wcl"

files = subprocess.run(["git", "ls-tree", "-r", "--name-only", rev, root],
                       capture_output=True, text=True).stdout.split()
files = [f for f in files if f.endswith(".wcl")]

open_re = re.compile(r"^\s*(%s)\s+([A-Za-z_][A-Za-z0-9_]*)\s*\{" % "|".join(UNIT_KINDS))
units, indexes = [], []

for path in files:
    src = subprocess.run(["git", "show", f"{rev}:{path}"],
                         capture_output=True, text=True).stdout
    lines = src.split("\n")
    i = 0
    while i < len(lines):
        m = open_re.match(lines[i])
        if not m:
            i += 1
            continue
        kind, uid = m.group(1), m.group(2)
        # walk to the matching brace, ignoring braces inside strings/heredocs
        depth, j, buf = 0, i, []
        in_heredoc = None
        while j < len(lines):
            ln = lines[j]
            if in_heredoc:
                if ln.strip() == in_heredoc:
                    in_heredoc = None
            else:
                hd = re.search(r"\$?<<'?([A-Z_]+)'?", ln)
                if hd:
                    in_heredoc = hd.group(1)
                stripped = re.sub(r'"(\\.|[^"\\])*"', '""', ln)
                depth += stripped.count("{") - stripped.count("}")
            buf.append(ln)
            j += 1
            if depth == 0 and j > i:
                break
        body = "\n".join(buf)
        name = re.search(r'\bname\s*=\s*"([^"]*)"', body)
        title = re.search(r'\btitle\s*=\s*"([^"]*)"', body)
        summary = re.search(r'\bsummary\s*=\s*"((?:\\.|[^"\\])*)"', body)
        audience = re.search(r"\baudience\s*=\s*:(\w+)", body)
        rel = re.search(r"\brelated\s*=\s*\[([^\]]*)\]", body, re.S)
        related = [r.strip() for r in rel.group(1).split(",")] if rel else []
        related = [r for r in related if re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", r)]
        has_body = bool(re.search(r"^\s*body\s*\{", body, re.M))
        rec = {
            "kind": kind, "id": uid,
            "name": (name or title).group(1) if (name or title) else uid,
            "summary": summary.group(1) if summary else "",
            "audience": audience.group(1) if audience else "both",
            "related": related, "has_body": has_body,
            "file": path, "line": i + 1,
            "words": len(re.findall(r"\w+", body)),
        }
        if kind == "index":
            rec["pinned"] = related
            rec["related"] = []
            indexes.append(rec)
        else:
            units.append(rec)
        i = i + 1

json.dump({"rev": rev, "units": units, "indexes": indexes}, sys.stdout, indent=1)
