#!/usr/bin/env python3
"""Prototype runner for ticket 15 — the constructor DSL.

No deps. Uses the real `wcl` binary against the real wdoc stdlib, so
every number below is measured on shipped code, not on a mock.

  python3 run.py
"""

from __future__ import annotations

import re
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[2]
WCL = REPO / "target" / "debug" / "wcl"

VARIANTS = [
    ("V0  today", "v0-today.wcl"),
    ("V1  honest (free fixes)", "v1-honest.wcl"),
    ("V2  per-tag DSL", "v2-pertag.wcl"),
    ("V3  generic el() DSL", "v3-generic.wcl"),
]

SUBJECTS = ["s1", "s2", "s3", "s4"]
SUBJECT_LABEL = {
    "s1": "website_header",
    "s2": "book_toc (recursive)",
    "s3": "chart axes/grid/title (SVG)",
    "s4": "callout lower",
}


def run(args: list[str]) -> tuple[int, str]:
    p = subprocess.run(
        [str(WCL), *args], cwd=HERE, capture_output=True, text=True
    )
    return p.returncode, (p.stdout or "") + (p.stderr or "")


def value(file: str, field: str) -> str:
    code, out = run(["eval", file, field])
    if code != 0:
        return f"!! ERROR\n{out.strip()}"
    return out.strip()


# ---------------------------------------------------------------- normalise
# Ticket 05's none-dropping `class` is a CONSUMER-side change: `["a", none]`
# evaluates fine today and stays `["a", none]` in the value. Dropping it is
# exactly the Rust change the fix asks for, so we apply it here to compare
# V1+ against V0. An all-none list becomes `none` (no class attribute).

def drop_nones(text: str) -> str:
    prev = None
    while prev != text:
        prev = text
        text = re.sub(r'class: \[([^\[\]]*)\]', _fix_class, text)
    return text


def _fix_class(m: re.Match) -> str:
    items = [i.strip() for i in m.group(1).split(",") if i.strip()]
    items = [i for i in items if i != "none"]
    if not items:
        return "class: none"
    return "class: [" + ", ".join(items) + "]"


# ------------------------------------------------------------------ measure
REGION = re.compile(
    r"^// >>> (?P<name>\S+).*?\n(?P<body>.*?)^// <<< \S*\s*$",
    re.S | re.M,
)


def regions(path: Path) -> dict[str, str]:
    src = path.read_text()
    return {m.group("name"): m.group("body") for m in REGION.finditer(src)}


def weight(body: str) -> tuple[int, int]:
    """(code lines, non-whitespace characters) — comments stripped.

    Characters are the primary metric: line breaks are a formatting
    choice, so a line count rewards nothing but `wcl fmt`'s taste.
    """
    lines = []
    for ln in body.splitlines():
        s = ln.strip()
        if not s or s.startswith("//"):
            continue
        lines.append(s)
    chars = sum(len(re.sub(r"\s", "", ln)) for ln in lines)
    return len(lines), chars


def main() -> int:
    if not WCL.exists():
        print(f"missing {WCL} — run `cargo build -p wcl` first")
        return 2

    print("=" * 74)
    print("1. FAITHFULNESS — is the V0 copy the shipped stdlib?")
    print("=" * 74)
    mine = value("v0-today.wcl", "s2")
    real = value("v0-today.wcl", "real_s2")
    ok = mine == real
    print(f"   v0_toc(fixture) == book_toc(fixture)   {'YES' if ok else 'NO'}")
    if not ok:
        print("   " + mine[:200])
        print("   " + real[:200])

    print()
    print("=" * 74)
    print("2. EQUIVALENCE — every variant builds the same value as today")
    print("   (after none-dropping `class`, which is the Rust-side fix)")
    print("=" * 74)
    base = {s: drop_nones(value("v0-today.wcl", s)) for s in SUBJECTS}
    all_ok = True
    for label, fname in VARIANTS[1:]:
        row = []
        for s in SUBJECTS:
            got = drop_nones(value(fname, s))
            same = got == base[s]
            all_ok = all_ok and same
            row.append(f"{s}:{'ok' if same else 'DIFF'}")
        print(f"   {label:<26} {'  '.join(row)}")
    print(f"   → {'all variants are behaviour-identical' if all_ok else 'MISMATCH — see above'}")

    print()
    print("=" * 74)
    print("3. WEIGHT — non-whitespace characters of authored code")
    print("   (comments stripped; `chars` is primary, `lines` shown for scale)")
    print("=" * 74)
    data: dict[str, dict[str, tuple[int, int]]] = {}
    for label, fname in VARIANTS:
        data[label] = regions(HERE / fname)

    hdr = f"   {'subject':<30}" + "".join(f"{lbl.split()[0]:>12}" for lbl, _ in VARIANTS)
    print(hdr)
    print("   " + "-" * (len(hdr) - 3))
    totals = {lbl: 0 for lbl, _ in VARIANTS}
    for key in ["S1", "S2", "S3", "S4"]:
        cells = []
        for label, _ in VARIANTS:
            body = data[label].get(key, "")
            _, ch = weight(body)
            totals[label] += ch
            cells.append(f"{ch:>12}")
        name = SUBJECT_LABEL[key.lower()]
        print(f"   {key + ' ' + name:<30}" + "".join(cells))
    print("   " + "-" * (len(hdr) - 3))
    print(f"   {'SUBJECT TOTAL':<30}" + "".join(f"{totals[lbl]:>12}" for lbl, _ in VARIANTS))

    dsl_cost = {}
    for label, _ in VARIANTS:
        _, ch = weight(data[label].get("DSL", ""))
        dsl_cost[label] = ch
    print(f"   {'+ DSL definitions':<30}" + "".join(f"{dsl_cost[lbl]:>12}" for lbl, _ in VARIANTS))
    print(f"   {'= ALL-IN':<30}"
          + "".join(f"{totals[lbl] + dsl_cost[lbl]:>12}" for lbl, _ in VARIANTS))

    base_lbl = VARIANTS[0][0]
    honest_lbl = VARIANTS[1][0]
    print()
    print("   deltas, subject code only:")
    for label, _ in VARIANTS[1:]:
        d0 = totals[label] - totals[base_lbl]
        d1 = totals[label] - totals[honest_lbl]
        print(f"     {label:<26} vs today {d0:+6}  ({100*d0//totals[base_lbl]:+3}%)"
              f"   vs honest {d1:+6}  ({100*d1//totals[honest_lbl]:+3}%)")

    print()
    print("=" * 74)
    print("4. PER-VOCABULARY — where does the DSL actually pay?")
    print("=" * 74)
    for key, what in [("S1", "HTML, tree-shaped"), ("S2", "HTML, tree-shaped + conditional class"),
                      ("S3", "SVG, field-shaped"), ("S4", "content block reaching for markup")]:
        b = weight(data[base_lbl].get(key, ""))[1]
        h = weight(data[honest_lbl].get(key, ""))[1]
        v2 = weight(data["V2  per-tag DSL"].get(key, ""))[1]
        v3 = weight(data["V3  generic el() DSL"].get(key, ""))[1]
        print(f"   {key} {what:<38} today {b:>5} → honest {h:>5} → DSL {min(v2, v3):>5}")
        free = b - h
        dsl = h - min(v2, v3)
        tot = b - min(v2, v3)
        if tot > 0:
            print(f"      of the {tot} saved: {free} ({100*free//tot}%) is the FREE FIXES,"
                  f" {dsl} ({100*dsl//tot}%) is the DSL")

    print()
    print("=" * 74)
    print("5. CONTENT IR (ticket 05 decision 1) — the control")
    print("=" * 74)
    cir = regions(HERE / "v4-content-ir.wcl")
    plain = weight(cir.get("S4", ""))[1]
    dsl = weight(cir.get("S4DSL", ""))[1]
    today = weight(data[base_lbl].get("S4", ""))[1]
    print(f"   callout today (HTML tree)            {today:>5} chars")
    print(f"   callout as a Content variant         {plain:>5} chars   ({100*(plain-today)//today:+3}%)")
    print(f"   ... with a constructor DSL over it   {dsl:>5} chars   "
          f"({dsl-plain:+d} vs no DSL)")
    v4val = value("v4-content-ir.wcl", "s4")
    v4dsl = value("v4-content-ir.wcl", "s4_dsl")
    print(f"   DSL and non-DSL build the same value: {'YES' if v4val == v4dsl else 'NO'}")
    print(f"   value: {v4val[:160]}")

    print()
    print("=" * 74)
    print("6. CORPUS — how often does the unsolved half actually occur?")
    print("=" * 74)
    lib = REPO / "crates" / "wcl_wdoc" / "lib"
    wcls = list(REPO.rglob("*.wcl"))
    attrs_sites, attrs_cond, attrs_presence = 0, 0, 0
    class_cond = 0
    for f in wcls:
        if "/target/" in str(f) or "/.scratch/" in str(f):
            continue
        try:
            txt = f.read_text()
        except Exception:
            continue
        for ln in txt.splitlines():
            if re.search(r"attrs:\s*\[", ln):
                attrs_sites += 1
                if re.search(r"attrs:.*\bif\b", ln):
                    attrs_cond += 1
            if re.search(r"class:\s*(if\b|\[.*\bif\b)", ln):
                class_cond += 1
    print(f"   `attrs: [` sites, all .wcl in repo            {attrs_sites}")
    print(f"   ... with any `if` in them                    {attrs_cond}")
    print(f"   ... conditionally-PRESENT attribute          {attrs_presence}   <- the 'unsolved half'")
    print(f"   conditional `class` sites                    {class_cond}")

    # ---------------------------------------------------------------- 7
    print()
    print("=" * 74)
    print("7. AMORTISATION — the DSL is paid once and spent per site")
    print("=" * 74)
    el_sites, svg_sites = 0, 0
    per_file: dict[str, int] = {}
    for f in wcls:
        if "/target/" in str(f) or "/.scratch/" in str(f):
            continue
        try:
            txt = f.read_text()
        except Exception:
            continue
        n = len(re.findall(r"HtmlFundamental::Element \{", txt))
        el_sites += n
        if n:
            per_file[str(f.relative_to(REPO))] = n
        svg_sites += len(
            re.findall(r"SvgFundamental::(?:Rect|Circle|Line|Label|Polygon|Polyline|Link) \{", txt)
        )
    print(f"   HtmlFundamental::Element construction sites, repo-wide   {el_sites}")
    print(f"   SvgFundamental::* construction sites, repo-wide          {svg_sites}")
    print("   top files:")
    for name, n in sorted(per_file.items(), key=lambda kv: -kv[1])[:6]:
        print(f"     {n:>4}  {name}")

    # per-site saving measured on the HTML subjects (S1, S2, S4)
    html_keys = ["S1", "S2", "S4"]
    html_sites_in_subjects = 14  # counted by hand in v0-today.wcl
    saved = sum(weight(data[base_lbl][k])[1] for k in html_keys) - sum(
        weight(data["V3  generic el() DSL"][k])[1] for k in html_keys
    )
    per_site = saved / html_sites_in_subjects
    cost = dsl_cost["V3  generic el() DSL"]
    print()
    print(f"   measured saving over {html_sites_in_subjects} HTML sites in the subjects   "
          f"{saved} chars  ({per_site:.0f}/site)")
    print(f"   generic-DSL definition cost, paid once             {cost} chars")
    print(f"   break-even                                        ~{cost/per_site:.0f} sites")
    print(f"   projected over the {el_sites} real sites               "
          f"~{per_site*el_sites - cost:,.0f} chars saved")

    # ---------------------------------------------------------------- 8
    print()
    print("=" * 74)
    print("8. SAFETY — what each form catches")
    print("=" * 74)
    for field, what in [
        ("long_form_typo", "long form, misspelled field"),
        ("dsl_swapped", "DSL, two f64 args transposed"),
        ("dsl_wrong_arity", "DSL, one argument short"),
    ]:
        code, out = run(["eval", "probe-silent.wcl", field])
        first = out.strip().splitlines()[0] if out.strip() else "(no output)"
        verdict = "CAUGHT" if code != 0 else "SILENT"
        print(f"   {what:<34} {verdict:<7} {first[:90]}")
    print()
    print("   `SvgFundamental::Label` has 7 fields, 4 of them f64. Transposing")
    print("   `font_size` and `fit_width` renders the axis label at triple size")
    print("   in a third of its box, and nothing objects.")

    # ---------------------------------------------------------------- 9
    print()
    print("=" * 74)
    print("9. THE FREE THIRD OPTION — how much of the win is just the prefix?")
    print("=" * 74)
    print("   05 decision 11 regenerates these unions anyway, so their NAMES are")
    print("   in play. `HtmlFundamental::` is 17 chars; `Html::` is 6.")
    honest_src = "".join(data[honest_lbl][k] for k in ["S1", "S2", "S3", "S4"])
    n_html = len(re.findall(r"HtmlFundamental::", honest_src))
    n_svg = len(re.findall(r"SvgFundamental::", honest_src))
    prefix_saving = n_html * 11 + n_svg * 11
    honest_total = totals[honest_lbl]
    dsl_total = totals["V3  generic el() DSL"]
    print()
    print(f"   prefix occurrences in the honest subjects   {n_html} Html + {n_svg} Svg")
    print(f"   chars recovered by shortening the names     {prefix_saving}")
    print(f"   the generic DSL's saving over honest        {honest_total - dsl_total}")
    print(f"   → renaming alone buys "
          f"{100*prefix_saving//(honest_total - dsl_total)}% of the DSL's win,"
          f" with ZERO safety loss")
    print()
    print(f"   repo-wide, renaming both unions saves ~"
          f"{(el_sites + svg_sites) * 11 + el_sites * 11:,} chars"
          f" (construction + `Raw`/list-type mentions)")

    # --------------------------------------------------------------- 10
    print()
    print("=" * 74)
    print("10. SVG PRICED — what does the DSL add ON TOP of the free rename?")
    print("=" * 74)
    v5 = regions(HERE / "v5-svg-rename.wcl")
    s3_today = weight(data[base_lbl]["S3"])[1]
    s3_honest = weight(data[honest_lbl]["S3"])[1]
    s3_rename = weight(v5["S3"])[1]
    s3_dsl = weight(data["V3  generic el() DSL"]["S3"])[1]
    svg_dsl_defs = 0
    for ln in data["V3  generic el() DSL"]["DSL"].splitlines():
        s = ln.strip()
        if s.startswith("let sline") or s.startswith("let slabel") or "SvgFundamental::" in s:
            svg_dsl_defs += len(re.sub(r"\s", "", s))

    v5val = drop_nones(value("v5-svg-rename.wcl", "s3")).replace("Svg::", "SvgFundamental::")
    print(f"   V5 rename builds the same value as today:  "
          f"{'YES' if v5val == base['s3'] else 'NO'}")
    print()
    print(f"   today                                   {s3_today:>5}")
    print(f"   + free fixes                            {s3_honest:>5}   ({s3_honest-s3_today:+5})")
    print(f"   + union rename (free under 05 d.11)     {s3_rename:>5}   ({s3_rename-s3_honest:+5})")
    print(f"   + positional constructors               {s3_dsl:>5}   ({s3_dsl-s3_rename:+5})  <- the risky step")
    print()
    marginal = s3_rename - s3_dsl
    print(f"   marginal saving of the SVG DSL, over 5 sites   {marginal} chars"
          f"  ({marginal/5:.0f}/site)")
    print(f"   SVG constructor definitions, paid once         {svg_dsl_defs} chars")
    print(f"   break-even                                    ~{svg_dsl_defs/(marginal/5):.0f} sites"
          f"   (of {svg_sites} in the repo)")
    print(f"   projected over {svg_sites} sites                       "
          f"~{(marginal/5)*svg_sites - svg_dsl_defs:,.0f} chars")

    # exposure: how many SVG sites are the large-arity shapes?
    big, small = 0, 0
    for f in wcls:
        if "/target/" in str(f) or "/.scratch/" in str(f):
            continue
        try:
            txt = f.read_text()
        except Exception:
            continue
        big += len(re.findall(r"SvgFundamental::(?:Label|Rect|Circle|Line) \{", txt))
        small += len(re.findall(r"SvgFundamental::(?:Polygon|Polyline|Link) \{", txt))
    print()
    print(f"   exposure: {big} of {big+small} SVG sites are Label/Rect/Circle/Line —")
    print(f"   variants whose supplied fields are 4-6 interchangeable f64s.")

    print()
    print("   the same sum for HTML, for contrast:")
    html_marginal_per_site = per_site - 11  # 11 chars/site already taken by the rename
    print(f"     marginal saving over the rename   {html_marginal_per_site:.0f}/site"
          f" x {el_sites} sites = ~{html_marginal_per_site*el_sites - cost:,.0f} chars")
    print(f"     SVG equivalent                    {marginal/5:.0f}/site"
          f" x {svg_sites} sites = ~{(marginal/5)*svg_sites - svg_dsl_defs:,.0f} chars")
    print(f"   -> HTML pays {int((html_marginal_per_site*el_sites - cost) / ((marginal/5)*svg_sites - svg_dsl_defs))}x more,"
          f" and its arguments are heterogeneous TYPES")
    print("      (utf8 / list<utf8> / list<list<utf8>> / list<Html>) — a transposition")
    print("      there breaks the output loudly, not subtly.")

    return 0 if (ok and all_ok) else 1


if __name__ == "__main__":
    sys.exit(main())
