#!/usr/bin/env python3
"""PROTOTYPE — ticket 13: the CSS block vocabulary.

Throwaway. Answers three questions against the REAL 349 rules:

  1. LOSSLESS?  Can `class`/`base`/`font_face`/`media`/`keyframes` express every
     rule in the codebase? Proved by round-trip: CSS -> blocks -> CSS, diffed.
  2. READS BETTER?  What does grouping-by-root actually buy? Measured, and shown
     side by side for the worst case (book_css, 41 flat rules).
  3. DOES THE LINT WORK?  Run the output-scan against a real built site and report
     the true yield / false-positive rate.

No deps (repo convention). Hand-rolled CSS splitter — enough for real stylesheets,
not a spec-compliant parser. The real migration would use tinycss2; this exists to
test the VOCABULARY, not the script.

    python3 run.py                     # extract, convert, round-trip, report
    python3 run.py --lint <site-dir>   # also run the output-scan lint
"""

import os
import re
import sys
import glob
import collections

REPO = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
OUT = os.path.join(os.path.dirname(__file__), "out")


# ── extraction ────────────────────────────────────────────────────────────────

def css_sources():
    """Every place CSS is authored today. Returns [(label, origin, text)]."""
    srcs = []

    # 1. the 27 heredocs in the wdoc stdlib
    for f in sorted(glob.glob(os.path.join(REPO, "crates/wcl_wdoc/lib/*.wcl"))):
        lines = open(f).read().split("\n")
        i = 0
        while i < len(lines):
            m = re.search(r"<<'?(CSS\w*)'?\s*$", lines[i])
            if m:
                tag, j = m.group(1), i + 1
                while j < len(lines) and lines[j].strip() != tag:
                    j += 1
                srcs.append((os.path.basename(f), "heredoc", "\n".join(lines[i + 1:j])))
                i = j
            i += 1

    # 2. theme.rs APPLY + FONT_DEFAULTS (Rust string constants)
    theme = open(os.path.join(REPO, "crates/wcl_wdoc/src/render/theme.rs")).read()
    rust = [l.strip() for l in theme.split("\n")
            if re.match(r"^[.:#a-z\[*][^=]*\{\s*[a-z-]+\s*:", l.strip())
            and not l.strip().startswith("//")]
    if rust:
        srcs.append(("theme.rs", "rust", "\n".join(rust)))

    # 3. the one sibling .css file
    p = os.path.join(REPO, "crates/wcl_wdoc/assets/code-theme.css")
    if os.path.exists(p):
        srcs.append(("code-theme.css", "asset", open(p).read()))

    # 4. DOCUMENT-side CSS. The ticket resolution assumed the migration was
    #    confined to crates/wcl_wdoc; it is not. The docs landing page and every
    #    wskill's book/training template carry their own CSS heredocs — 129 more
    #    rules across 8 of them, in docs/, examples/ and .wad/.
    for pat in ("docs/**/*.wcl", "examples/**/*.wcl", ".wad/**/*.wcl"):
        for f in sorted(glob.glob(os.path.join(REPO, pat), recursive=True)):
            lines = open(f, errors="ignore").read().split("\n")
            i = 0
            while i < len(lines):
                m = re.search(r"<<'?(CSS\w*)'?\s*$", lines[i])
                if m:
                    tag, j = m.group(1), i + 1
                    while j < len(lines) and lines[j].strip() != tag:
                        j += 1
                    srcs.append((os.path.relpath(f, REPO), "document",
                                 "\n".join(lines[i + 1:j])))
                    i = j
                i += 1

    return srcs


# ── a small CSS splitter ──────────────────────────────────────────────────────

def strip_comments(s):
    return re.sub(r"/\*.*?\*/", "", s, flags=re.S)


def split_rules(css):
    """-> [(selector, body, kind)] where kind is 'rule' or 'at'.

    At-rules keep their raw inner text so nesting is preserved."""
    css = strip_comments(css)
    out, buf, depth, i, start = [], "", 0, 0, 0
    sel = ""
    while i < len(css):
        c = css[i]
        if c == "{":
            if depth == 0:
                sel = css[start:i].strip()
                buf_start = i + 1
            depth += 1
        elif c == "}":
            depth -= 1
            if depth == 0:
                body = css[buf_start:i]
                out.append((sel, body, "at" if sel.startswith("@") else "rule"))
                start = i + 1
        i += 1
    return [(s, b, k) for s, b, k in out if s]


def decls(body):
    """Normalise a declaration body to a single canonical string."""
    parts = [p.strip() for p in re.split(r";(?![^(]*\))", body) if p.strip()]
    return "; ".join(re.sub(r"\s+", " ", p) for p in parts)


# ── conversion to the proposed block vocabulary ───────────────────────────────

CLASS_RE = re.compile(r"^\.([A-Za-z_][\w-]*)")
TAGCLASS_RE = re.compile(r"^([a-z]+)\.([A-Za-z_][\w-]*)")


def root_of(sel):
    """(root_kind, root_name, tag, remainder) for one selector branch.

    The remainder uses SCSS's `&` convention, which the prototype found the
    vocabulary needs: `nest ".heading-1"` alone cannot say whether it means
    `.parent .heading-1` (descendant) or `.parent.heading-1` (compound), and
    both occur. So a remainder that attaches directly to the parent is written
    `&.heading-1` / `&:hover` / `&::before`; anything else is a descendant.
    """
    s = sel.strip()
    # A tag-qualified root (`table.wdoc-table`) goes to `base`, NOT to a `tag =`
    # field on the class block. The prototype found why: `.wdoc-table`'s own
    # descendants use the BARE class (`.wdoc-table th`), so a block-level tag
    # would wrongly narrow every nested rule too.
    m = CLASS_RE.match(s)
    if not m:
        return ("base", s, None, "")
    name, tag = m.group(1), None
    tail = s[m.end():]
    if tail == "":
        rem = ""
    elif tail[0].isspace() or tail[0] in ">+~":
        rem = tail.strip()                    # descendant / combinator
    else:
        rem = "&" + tail.strip()              # compound / pseudo — attaches directly
    return ("class", name, tag, rem)


def convert(sources):
    """-> (blocks, at_rules). blocks is an ordered dict keyed by (source, kind, name).

    Keyed BY SOURCE on purpose: 46 selectors are declared twice — once by a bundled
    block stylesheet, once again by theme.rs's APPLY re-painting them with
    var(--wdoc-*). That is the documented cascade (`build.rs::site_css` splices the
    theme between library and user rules), so the two must stay two blocks. Merging
    them by name would be the prototype inventing a bug the vocabulary doesn't have.
    """
    blocks = collections.OrderedDict()
    at_blocks = []
    for label, origin, text in sources:
        for sel, body, kind in split_rules(text):
            if kind == "at":
                at_blocks.append((sel, body, label))
                continue
            branches = [b.strip() for b in sel.split(",") if b.strip()]
            roots = [root_of(b) for b in branches]
            # a selector list whose branches disagree on root stays a `base`
            # carrying the whole selector — 20 rules, per the census.
            same = len({(r[0], r[1], r[2]) for r in roots}) == 1
            if same and roots[0][0] == "class":
                rk, name, tag, _ = roots[0]
                rems = [r[3] for r in roots]
                key = (label, "class", name)
                blk = blocks.setdefault(key, {"tag": tag, "css": None, "nest": [], "src": label})
                if tag and not blk["tag"]:
                    blk["tag"] = tag
                if all(r == "" for r in rems):
                    if blk["css"] is None:
                        blk["css"] = decls(body)
                    else:  # same root declared twice within one source
                        blk["nest"].append(("", decls(body)))
                else:
                    blk["nest"].append((rems, decls(body)))
            else:
                key = (label, "base", sel)
                blocks.setdefault(key, {"tag": None, "css": None, "nest": [], "src": label})
                blocks[key]["css"] = decls(body)
    return blocks, at_blocks


def at_to_blocks(at_blocks):
    """@font-face / @media / @keyframes -> their dedicated block forms."""
    out = []
    for sel, body, label in at_blocks:
        head = sel.split(None, 1)
        name = head[1].strip() if len(head) > 1 else ""
        if sel.startswith("@font-face"):
            props = dict()
            for p in re.split(r";(?![^(]*\))", body):
                if ":" in p:
                    k, v = p.split(":", 1)
                    props[k.strip()] = v.strip()
            out.append(("font_face", props.get("font-family", "?").strip("'\""), props, label))
        elif sel.startswith("@media"):
            out.append(("media", name, split_rules("x{}" if not body else body), label))
        elif sel.startswith("@keyframes"):
            out.append(("keyframes", name, split_rules(body), label))
        else:
            out.append(("UNSUPPORTED", sel, body, label))
    return out


# ── emitting the WCL block forms ──────────────────────────────────────────────

def q(s):
    return '"' + s.replace("\\", "\\\\").replace('"', '\\"') + '"'


def wrap(text, indent, width=88):
    """Wrap a declaration string across lines the way an author would.

    Escapes `"` — `content: "▸"` is real CSS in book_css, and a raw declaration
    living inside a WCL string literal has to escape it. A spec detail the
    prototype surfaced: either escape, or let `css` take a raw heredoc.
    """
    text = text.replace("\\", "\\\\").replace('"', '\\"')
    parts = [p.strip() for p in text.split(";") if p.strip()]
    lines, cur = [], ""
    for p in parts:
        piece = p + ";"
        if cur and len(cur) + len(piece) + 1 > width:
            lines.append(cur)
            cur = piece
        else:
            cur = (cur + " " + piece).strip()
    if cur:
        lines.append(cur)
    return ("\n" + " " * indent).join(lines)


PAINT = ("fill", "stroke", "stroke-width", "opacity")


def emit_blocks(blocks, ats):
    lines = ["// GENERATED BY THE TICKET-13 PROTOTYPE — throwaway, not for commit.",
             "// The real migration hand-finishes selector lists and the :root accent line.",
             ""]
    cur_src = None
    for (src, kind, name), b in blocks.items():
        if src != cur_src:
            lines.append(f"// ── from {src} " + "─" * max(0, 56 - len(src)))
            lines.append("")
            cur_src = src
        head = f'{kind} {q(name)} {{'
        lines.append(head)
        if b["tag"]:
            lines.append(f'  tag = {q(b["tag"])}')
        if b["css"]:
            # decision 3: SVG paint stays a typed field
            rest = []
            for d in [x.strip() for x in b["css"].split(";") if x.strip()]:
                k = d.split(":", 1)[0].strip()
                if kind == "class" and k in PAINT:
                    v = d.split(":", 1)[1].strip()
                    lines.append(f'  {k.replace("-", "_")} = {q(v)}')
                else:
                    rest.append(d)
            if rest:
                lines.append(f'  css = "{wrap("; ".join(rest) + ";", 9)}"')
        for rems, body in b["nest"]:
            frag = ", ".join(rems) if isinstance(rems, list) else rems
            lines.append(f'  nest {q(frag)} {{ css = "{wrap(body + ";", 4)}" }}')
        lines.append("}")
        lines.append("")
    for kind, name, payload, label in ats:
        if kind == "font_face":
            lines.append(f'font_face {q(name)} {{')
            for k, v in payload.items():
                if k == "font-family":
                    continue
                lines.append(f'  {k.replace("font-", "").replace("-", "_")} = {q(v)}')
            lines.append("}")
            lines.append("")
        elif kind in ("media", "keyframes"):
            lines.append(f'{kind} {q(name)} {{')
            for sel, body, _k in payload:
                rk, rn, tag, rem = root_of(sel.split(",")[0])
                lines.append(f'  {rk} {q(rn)} {{ css = "{wrap(decls(body) + ";", 4)}" }}'
                             if not rem else
                             f'  {rk} {q(rn)} {{ nest {q(rem)} {{ css = "{decls(body)};" }} }}')
            lines.append("}")
            lines.append("")
        else:
            lines.append(f"// !! UNSUPPORTED at-rule from {label}: {name}")
    return "\n".join(lines)


# ── round-trip: blocks back to CSS ────────────────────────────────────────────

def blocks_to_css(blocks, ats):
    """What wdoc would emit from the block forms. Used for the lossless check."""
    rules = []
    for (src, kind, name), b in blocks.items():
        base = ("." + name) if kind == "class" else name
        if b["tag"]:
            base = b["tag"] + base
        if b["css"]:
            rules.append((base, b["css"]))
        for rems, body in b["nest"]:
            branches = rems if isinstance(rems, list) else [rems]
            sel = ", ".join(
                base if f == "" else
                (base + f[1:]) if f.startswith("&") else       # compound / pseudo
                (base + " " + f)                               # descendant / combinator
                for f in branches)
            rules.append((sel, body))
    for kind, name, payload, _label in ats:
        if kind == "font_face":
            rules.append(("@font-face", "; ".join(f"{k}: {v}" for k, v in payload.items())))
        elif kind in ("media", "keyframes"):
            for sel, body, _k in payload:
                rules.append((f"@{kind[:5]} {name} :: {sel}", decls(body)))
    return rules


def canon(sel):
    s = re.sub(r"\s*([>+~,])\s*", r"\1", sel.strip())
    return re.sub(r"\s+", " ", s)


def original_rules(sources):
    out = []
    for label, origin, text in sources:
        for sel, body, kind in split_rules(text):
            if kind == "at":
                head = sel.split(None, 1)
                nm = head[1].strip() if len(head) > 1 else ""
                if sel.startswith("@font-face"):
                    out.append(("@font-face", decls(body)))
                else:
                    for s2, b2, _k in split_rules(body):
                        out.append((f"@{sel.split()[0][1:6]} {nm} :: {s2}", decls(b2)))
            else:
                out.append((sel, decls(body)))
    return out


# ── the output-scan lint (decision 5) ─────────────────────────────────────────

def authored_classes(*roots):
    """`class "name" { … }` blocks — stdlib (css-classes.wcl et al) AND the
    document's own. The prototype's first lint run flagged 192 names, most of
    them the docs site's own `lp-*` / `quiz-*` classes: a lint that reads only
    the stylesheet CSS treats every user class as a typo. The real lint must
    union both sources."""
    names = set()
    for r in roots:
        for f in glob.glob(os.path.join(r, "**", "*.wcl"), recursive=True):
            for m in re.finditer(r'^\s*class\s+"([^"]+)"', open(f, errors="ignore").read(), re.M):
                names.add(m.group(1))
    return names


def run_lint(site_dir, blocks, ats):
    html_files = glob.glob(os.path.join(site_dir, "**", "*.html"), recursive=True)
    if not html_files:
        return None
    used = collections.defaultdict(set)
    for f in html_files:
        s = open(f, errors="ignore").read()
        for m in re.finditer(r'class="([^"]*)"', s):
            for c in m.group(1).split():
                used[c].add(os.path.relpath(f, site_dir))
    defined = set()
    for sel, _body in blocks_to_css(blocks, ats):
        for c in re.findall(r"\.([A-Za-z_][\w-]*)", sel):
            defined.add(c)
    defined |= authored_classes(os.path.join(REPO, "crates/wcl_wdoc/lib"),
                                os.path.join(REPO, "docs"))
    return html_files, used, defined


# ── report ────────────────────────────────────────────────────────────────────

def main():
    os.makedirs(OUT, exist_ok=True)
    srcs = css_sources()
    orig = original_rules(srcs)
    blocks, at_raw = convert(srcs)
    ats = at_to_blocks(at_raw)

    print("PROTOTYPE — ticket 13: the CSS block vocabulary")
    print("=" * 72)
    by_origin = collections.Counter(o for _l, o, _t in srcs)
    print(f"\nsources: {len(srcs)}  ({dict(by_origin)})")
    print(f"rules parsed out of them: {len(orig)}")

    # ── Q1: lossless? ────────────────────────────────────────────────────────
    back = blocks_to_css(blocks, ats)
    o = collections.Counter((canon(s), d) for s, d in orig)
    n = collections.Counter((canon(s), d) for s, d in back)
    lost = list((o - n).elements())
    gained = list((n - o).elements())

    print("\n" + "-" * 72)
    print("Q1  LOSSLESS?   CSS -> blocks -> CSS, diffed on (selector, declarations)")
    print("-" * 72)
    print(f"  original rules   {len(orig)}")
    print(f"  round-tripped    {len(back)}")
    print(f"  lost             {len(lost)}")
    print(f"  spurious         {len(gained)}")
    if lost:
        print("\n  rules the vocabulary did NOT reproduce:")
        for s, d in lost[:25]:
            print(f"    {s[:64]}")
            print(f"        {d[:100]}")
        if len(lost) > 25:
            print(f"    … and {len(lost)-25} more")
    else:
        print("\n  ✓ every rule round-trips — the vocabulary is lossless on the real corpus")

    # ── Q2: reads better? ────────────────────────────────────────────────────
    kinds = collections.Counter(k for _s, k, _n in blocks)
    nested = sum(len(b["nest"]) for b in blocks.values())
    print("\n" + "-" * 72)
    print("Q2  READS BETTER?   what grouping-by-root buys")
    print("-" * 72)
    print(f"  {len(orig)} flat rules  ->  {len(blocks) + len(ats)} blocks "
          f"({nested} of them nested inside a parent)")
    for k, v in kinds.most_common():
        print(f"    {k:12} {v}")
    for k, v in collections.Counter(a[0] for a in ats).most_common():
        print(f"    {k:12} {v}")
    top = sorted(blocks.items(), key=lambda kv: -(len(kv[1]["nest"]) + 1))[:8]
    print("\n  biggest consolidations (rules folded into one block):")
    for (_s, k, name), b in top:
        print(f"    {k} {name:26} {len(b['nest']) + (1 if b['css'] else 0)} rules")

    # ── artifacts ────────────────────────────────────────────────────────────
    wcl = emit_blocks(blocks, ats)
    open(os.path.join(OUT, "all.wcl"), "w").write(wcl)

    book = [(l, o_, t) for l, o_, t in srcs if l == "templates.wcl"]
    if book:
        bk = max(book, key=lambda x: len(x[2]))
        b_blocks, b_at = convert([bk])
        b_ats = at_to_blocks(b_at)
        open(os.path.join(OUT, "book.before.css"), "w").write(bk[2].strip())
        open(os.path.join(OUT, "book.after.wcl"), "w").write(emit_blocks(b_blocks, b_ats))
        print(f"\n  worst case, book_css: {len(split_rules(bk[2]))} flat rules "
              f"-> {len(b_blocks) + len(b_ats)} blocks")
        print("    out/book.before.css   vs   out/book.after.wcl")

    print(f"\n  wrote out/all.wcl  ({len(wcl.splitlines())} lines)")

    # ── Q3: the lint ─────────────────────────────────────────────────────────
    site = None
    if "--lint" in sys.argv:
        site = sys.argv[sys.argv.index("--lint") + 1]
    else:
        for cand in ("docs/_site",
                     "/tmp/claude-1000/-home-wil-orca-wcl/"
                     "be1f759f-871f-49bf-96a5-de59266870db/scratchpad/docs_site"):
            p = cand if os.path.isabs(cand) else os.path.join(REPO, cand)
            if os.path.isdir(p):
                site = p
                break

    print("\n" + "-" * 72)
    print("Q3  DOES THE LINT WORK?   output-scan against a real built site")
    print("-" * 72)
    if not site:
        print("  (no built site found — run `just docs-build`, or pass --lint <dir>)")
        return
    res = run_lint(site, blocks, ats)
    if not res:
        print(f"  (no .html under {site})")
        return
    files, used, defined = res
    undef = sorted(k for k in used if k not in defined)
    unused = sorted(k for k in defined if k not in used)
    print(f"  scanned {len(files)} pages under {os.path.relpath(site, REPO) if site.startswith(REPO) else site}")
    print(f"  distinct class names in the rendered HTML   {len(used)}")
    print(f"  distinct class names the blocks define      {len(defined)}")
    print(f"\n  USED, NO RULE ({len(undef)}) — the typo check:")
    for c in undef:
        where = sorted(used[c])
        print(f"    {c:34} {len(where):>4} page(s)   e.g. {where[0]}")
    print(f"\n  RULE, NO USE ({len(unused)}) — the dead-code check:")
    for i in range(0, len(unused), 4):
        print("    " + "  ".join(f"{c:26}" for c in unused[i:i + 4]))
    print("\n  NOTE: the docs site exercises one template set. A name flagged here")
    print("  may be live in the book/deck/website. That is the waiver question the")
    print("  ticket deliberately left open.")


if __name__ == "__main__":
    main()
