# wdoc: `include` needs a site selector, a non-recursive `entry` mode, skill-target fan-out, and richer `included_sites`

**Reported by:** wskill collection work (2026-06-19)
**Component:** `wcl_wdoc` include — `crates/wcl_wdoc/src/include.rs` (`resolve_included`, `walk_files`, `IncludedSite`), `crates/wcl_wdoc/lib/include.wcl` (`IncludeSites`, `IncludeEntry`), the `included_sites(...)` builtin, and the build / skill pipelines
**Severity:** enhancement (workarounds exist, all of them hacks)

## Summary

The `include` block (build other wdoc documents from a folder into sub-site
subdirectories) is exactly what a **collection of wskills** wants: one
`wcl wdoc build` should render the landing page plus every member's book, and one
`wcl wdoc skill` should render every member's skill. But the current design fits
only *flat* folders of *single-site, HTML* documents, and a wskill member is none of
those. Four gaps:

1. **The scan is fully recursive and matches on filename only.** `walk_files`
   recurses every subdirectory and `resolve_included` matches the `pattern` glob
   against the bare filename. A wskill member is a whole project — the book lives at
   `<member>/wdoc/book/main.wcl`, a *skill* entry is also named `main.wcl`, there's a
   rendered `out/` tree, and the self-hosting member even bundles whole scaffold
   template trees through symlinks. So `pattern = "main.wcl"` matches the book **and**
   the skill **and** every bundled/rendered `main.wcl`, each becoming a junk sub-site
   named e.g. `wskill/skill/assets/scaffold/wdoc/book`. The only way to get one clean
   match per member today is to plant a uniquely-named shim file (`book.wcl`) at each
   member root — a hack.

2. **No way to pick which site to render.** A member declares two sites — a
   `:book` (HTML) and an `:ai_skill`. `include` always builds the whole document, so
   it can't say "for the collection landing, build each member's `book` site only."
   `wcl wdoc build`/`wcl wdoc skill` already take `--site`; `include` should too.

3. **`include` only embeds for the HTML build.** Per `lib/include.wcl`, the
   markdown/pdf/**skill** targets define `included_sites` (so documents parse) but do
   **not** embed sub-sites. So there's no way to fan a collection's *skills* out — one
   `wcl wdoc skill <collection> --out dist/skills` should drop each member into
   `dist/skills/<name>/` (SKILL.md + references/), the natural `.claude/skills/<name>/`
   layout.

4. **`included_sites` returns only `{ name, href }`.** `name` is a folder name, so
   the collection landing can't show each member's real title/summary without a
   separately hand-maintained member list.

## Requested

Four changes, ideally together (they compose into the collection use case below):

### 1. A non-recursive `entry` mode on `include`

Add an `entry: utf8?` field to `IncludeSites` as an alternative to `pattern`: a
**relative path within each immediate subdirectory** of `folder`. Require exactly one
of `pattern` / `entry`.

`entry` semantics (no recursion): read only the immediate subdirectories of
`folder`; for each `<sub>`, if `<folder>/<sub>/<entry>` is a file, it's a sub-site
with `name = <sub>`, built into `<prefix>/<sub>/`. Subdirs without the file are
skipped. This kills gap 1 outright — `out/`, `_wdoc/`, and bundled trees are never
scanned, the entry can sit at a fixed deep path, and the sub-site name stays the clean
member folder.

```wcl
// member books live at wskills/<m>/main.wcl
include "../../wskills" { entry = "main.wcl" }
//   wskills/ls/main.wcl      → <out>/wskills/ls/
//   wskills/wskill/main.wcl  → <out>/wskills/wskill/
//   (wskills/ls/out/**, wskills/wskill/skill/assets/** — never scanned)
```

(Keep `pattern` for genuinely flat folders of single-file sites.)

### 2. A `site` selector on `include` (and `included_sites`)

Add `site: utf8?` to `IncludeSites`. When set, the recursive build of each sub-site
passes `--site <name>` (build only that named site). `included_sites(...)` takes the
same `entry` / `site` arguments so nav and built output stay aligned.

```wcl
include "../../wskills" { entry = "main.wcl"  site = "book" }   // each member's :book site only
```

### 3. `include` participates in the skill (and markdown) build target

Make the `include` block embed sub-sites for `wcl wdoc skill` the same way it does
for `wcl wdoc build`: recurse `wdoc skill` per included entry (honouring the `site`
selector) and write each into `<out>/<name>/`. Then:

```bash
wcl wdoc skill collection-skill.wcl --out dist/skills
#   → dist/skills/ls/SKILL.md + references/ , dist/skills/wskill/SKILL.md + references/
```

is a one-command "render every member skill into the .claude/skills/<name>/ layout".
(Markdown could embed too, for completeness; PDF can stay a no-op.)

### 4. Richer `included_sites` records

Have `resolve_included` read each entry's selected `site { title = … }` (templates
already carry `title`) and return it on the record, so `IncludeEntry` becomes
`{ name, href, title, summary? }`. Optionally add an optional `summary` /
`description` field to the `:webpage` / `:book` site templates that gets surfaced.
Markdown/pdf (which don't embed) can fall back `title → name`. This lets a landing
render real links with no hand-maintained member list:

```wcl
wdoc_repeater { each = included_sites("../../wskills", entry = "main.wcl", site = "book")  as = :s
  p $"**[${s.title}](${s.href})** — ${s.summary}"
}
```

## The consumer shape this unlocks (wskill collections)

Each member becomes **one multi-site document** (`wskills/<m>/main.wcl`) declaring
`site book { default_template = :book }` + `site skill { default_template = :ai_skill }`
over the same imported model. Then the whole collection is two commands and zero
shims / zero hand-maintained member lists:

```wcl
// wskill-registry/wdoc/page/main.wcl  (landing)
include "../../wskills" { entry = "main.wcl"  site = "book" }
site landing { root = true  default_template = :webpage … }
page index { sites = [:landing]  …  // links via included_sites(…, site="book") }
```

```bash
wcl wdoc build  wskill-registry/wdoc/page/main.wcl --out out/site     # landing + every member book
wcl wdoc skill  wskill-registry/wdoc/skill.wcl     --out dist/skills   # every member skill, .claude/skills/<name>/ layout
```

## Code touch-points

- `crates/wcl_wdoc/src/include.rs`: branch `resolve_included` on `entry` vs `pattern`
  (the `entry` arm is a single `read_dir` over immediate dirs, no `walk_files`);
  thread a `site: Option<String>` onto `IncludedSite` and into the per-entry build
  recursion (`crate::build`); read the entry site's `title` for `IncludedSite`.
- `crates/wcl_wdoc/lib/include.wcl`: add `entry`, `site` to `IncludeSites`
  (exactly-one-of `entry`/`pattern`); add `title` / optional `summary` to
  `IncludeEntry`; update the doc comment + the `pattern`-only examples.
- The `included_sites` builtin: accept `entry` / `site`; return the richer record.
- The **skill** build pipeline: embed included sub-sites (today only the HTML build +
  dev server do). Keep the cycle check.
- Tests in `crates/wcl_wdoc/tests/build.rs`: `entry` immediate-subdir scan; `site`
  selector picks one site; deep/`out/`/symlinked trees are NOT scanned in `entry`
  mode; skill fan-out writes `<out>/<name>/SKILL.md`; `included_sites` title/summary.

## Workarounds in use (all undesirable)

- A `book.wcl` re-export shim at each member root to give the recursive filename glob
  one clean, unique match (extra file per member; still recursive, still fragile).
- A per-member glob loop in the justfile (`for m in wskills/*/; do wcl wdoc build
  "$m/wdoc/book/main.wcl" …; done`) instead of one include-driven build.
- A hand-maintained `_members.wcl` (one `member` block per wskill) to get rich
  landing links, because `included_sites` lacks title/summary.
- Per-member `wcl wdoc skill` + `cp out/skill .claude/skills/<name>` to install, with
  no way to fan out in one command.
