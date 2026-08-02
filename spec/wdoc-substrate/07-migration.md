# 07 — Migration

The in-repo consumers migrating onto the new substrate **is** the proof it works — it is part of the
deliverable, not follow-up work. Nothing gets a compatibility shim.

Source: the map's *Migration sequencing* fog, sharpened by tickets
[05](issues/05-block-type-system.md), [06](issues/06-unit-kinds.md),
[07](issues/07-curator-contract.md), [08](issues/08-wskill-cli.md),
[12](issues/12-template-selection.md), [13](issues/13-css-authoring.md),
[14](issues/14-wdoc-lang-extraction.md) and [15](issues/15-constructor-dsl.md).

## The eight sweeps

| # | sweep | size | mechanical? | source |
|---|---|---|---|---|
| 1 | **Block layer** — 34 page-content blocks become content-IR variants or `@native` | 34 blocks + 4 backends | partly | [02](02-blocks.md) |
| 2 | **Free fixes** — delete 204 dead `: none`; port `class` to none-dropping lists | 204 args + 11 sites | yes | [02](02-blocks.md) §2.10.1 |
| 3 | **`related` flip** — 736 bare edges → ~60–95, then author their reasons | 736 edges | yes, then a human/agent tail | [05](05-wskill.md) §5.3.7 |
| 4 | **Schema + template de-duplication** — ~56 copied files become registry imports | 4 wskills × ~15 files | yes | [05](05-wskill.md) §5.1.6 |
| 5 | **CSS** — 477 rules out of 35 heredocs + `APPLY` + `code-theme.css` | 477 rules | script + hand-finish | [04](04-css.md) §4.7 |
| 6 | **CLI wdoc-Environment** — generic paths learn a document is wdoc's | ~4 call sites | yes | [01](01-language.md) §1.5 |
| 7 | **Constructor port + union rename** — 258 `Element` sites to `el`; 329 sites renamed | 329 sites | yes | [02](02-blocks.md) §2.10 |
| 8 | **Template-layer odds** — `presentation` as a collection template; `sites` tagging | ~2 files | yes | [03](03-templates.md) §3.7.3, §3.8 |

---

## Hard ordering constraints

These are not preferences. Violating one breaks the build or loses data.

**A. Sweep 6 lands *with* [01](01-language.md), not after.** A missing expander becomes a hard error on
demand, and `wcl check` opens with a plain `Environment::new()` (`main.rs:1383`) while the wdoc registry
is threaded separately as a *loader* (`main.rs:33`). Between the language change and this sweep,
`wcl get` / `wcl eval`, the LSP and the editor's open paths **fail loudly on every wdoc document**.

It touches **no `.wcl` at all** — Rust in `crates/wcl` + `crates/wcl_lsp` only — so it contends with
nothing.

**OPEN:** whether "is this document wdoc's?" is decided by the import list or by something explicit.

**B. Sweep 3 runs in four steps, and step 4 is the one that gets forgotten.**

```
1. ship wcl_wskill with `why` OPTIONAL
2. run the flip — `wcl wskill lint --fix` — 736 → ~60–95
3. author the survivors' reasons (author pass, not the curator — it never backfills)
4. tighten `why` to REQUIRED — one line, gated on 3
```

`why` is schema-required in the final state, so **a bare edge will not parse** — the flip cannot run
after the schema tightens. Step 4 is trivially cheap and easy to forget, **and forgetting it leaves
ticket 06's whole finding unenforced.**

**C. The introspection swap ships with [02](02-blocks.md)/[03](03-templates.md)'s renames.** See
[06](06-editor.md) §6.1.1 — the editor's Systems view reads `WdocBlock`, `wdoc_slot`, `Region` and
`wdoc_content` through printed-string comparisons that fail *silently*.

**D. Sweep 5 follows sweep 1.** The CSS vocabulary adds four `@block` types on top of the settled type
system, so it wants to follow rather than precede.

**E. Sweep 7's rename half follows [02](02-blocks.md) §2.9.** Once the unions are generated from WCL,
the name is a one-line change at the source rather than a sweep. The *constructor port* half has no such
constraint.

---

## File contention

This is the part the map could not see until tickets 13 and 15 measured their own scope. **Three sweeps
edit the same files**, and two of them were each believed to be self-contained.

| file / tree | 3 `related` | 4 de-dup | 5 CSS | 7 constructor |
|---|:--:|:--:|:--:|:--:|
| `crates/wcl_wdoc/lib/*.wcl` | | | ● 23 stylesheets | ● 57 sites |
| `crates/wcl/src/scaffold/templates/*.wcl` | | ● wskill.wcl heredoc | | ● 66 sites |
| `docs/pages/wcl/landing-parts.wcl` | | | ● 1 heredoc | ● 51 sites |
| `docs/wskills/*/wdoc/book/main.wcl` | | ● | ● | ● |
| `docs/wskills/*/wdoc/training/main.wcl` | | ● | ● | ● 13 sites each |
| `docs/wskills/*/schema/base.wcl` | ● `why` field | ● deleted | | |
| `docs/wskills/*/data/**` | ● 736 edges | | | |
| `.wad/` | | | ● 2 heredocs | |

Two corrections that produced this table:

- **Ticket 13 believed the CSS sweep touched only `crates/wcl_wdoc`.** The prototype found **8 more
  heredocs carrying 129 rules outside the stdlib** — the docs site, all four wskills and WAD. The real
  corpus is **477 rules, not 349**.
- **Ticket 05 believed `Element` construction was 63 sites in three stdlib files.** It is **258
  repo-wide, and only 57 are stdlib.** The largest single file is the docs landing page.

**Suggested resolution:** run sweep 4 (de-duplication) **before** 5 and 7. It deletes ~56 files
outright, so every rule and construction site inside them stops needing migration — the cheapest way to
shrink the other two is to delete their subject.

---

## Suggested order

```
[01 Language] ──┬─ sweep 6  (CLI Environment)          ← constraint A, same change
                │
[02 Blocks]  ───┼─ sweep 1  (block layer)
                ├─ sweep 2  (free fixes)
                └─ introspection swap                   ← constraint C
                        │
[03 Templates] ─────────┼─ sweep 8  (presentation, sites)
                        │
[05 wskill]  ───────────┼─ sweep 4  (de-duplication)    ← shrinks 5 and 7
                        ├─ sweep 3.1 (`why` optional)
                        ├─ sweep 3.2 (`lint --fix`)
                        ├─ sweep 3.3 (author survivors)
                        └─ sweep 3.4 (`why` required)    ← constraint B, DON'T FORGET
                        │
[04 CSS]     ───────────┼─ sweep 5  (477 rules)          ← constraint D
                        │
[02 §2.9 codegen] ──────┴─ sweep 7  (constructors + rename) ← constraint E
```

Sweeps 3 and 4 both edit the wskills but **different files** (`data/**` vs `schema/` + `wdoc/`), so
they can interleave. Sweeps 5 and 7 both edit `wdoc/book/main.wcl` and `wdoc/training/main.wcl` and
should not.

---

## Tooling per sweep

- **Sweep 3** — `wcl wskill lint --fix`, an autofixing lint rule emitting `related_remove` ops. Not a
  bespoke subcommand: `wcl wskill op` already applies ops with per-op rollback and `--dry-run`. The
  survivors arrive as ordinary findings.
- **Sweep 5** — a **throwaway** uv single-file Python script using tinycss2 (the `.wad/scripts/`
  precedent). Output reviewed and committed; **the script is not shipped**. The ~20 selector-list rules
  and the `:root` accent line are hand-finished, plus the schema prune (11 dead `Class` properties, 20
  field uses → `css`).
- **Sweeps 1, 2, 7** — mechanical enough to script, but each wants a human read of the diff. Sweep 1 in
  particular carries a real decision per block (payload-shaped ⇒ leave alone, subtree-shaped ⇒
  `@native`, otherwise ⇒ content-IR variant).
- **Sweeps 4, 6, 8** — hand-done; small.

---

## Verification

Beyond the repo's standing bar (`just workspace-test`, `just workspace-lint`,
`cargo fmt --all -- --check`), each sweep must leave **every in-repo consumer building in every
backend**:

```
docs/            HTML · Markdown · PDF
examples/        HTML · Markdown · PDF
docs/wskills/×4  book · skill · training · presentation
.wad/            HTML · Markdown · PDF
```

Two things to expect rather than debug:

- **The docs PDF build breaks on sweep 1** until `file`-in-PDF is implemented or explicitly waived via
  `@except(backends = [:pdf])`. That is [02](02-blocks.md) §2.7's mechanism working, and it resolves
  the map's incidental defect #4 by forcing someone to state the intent.
- **Sweep 3.2 deletes ~640 edges.** That is the design (a filter, not a wipe), and the pre/post edge
  counts should be recorded in the commit rather than discovered later.

---

## Explicitly not part of this

**Migrating out-of-repo wskills and pages.** They are already broken and need migrating regardless;
that happens as a separate effort after this lands. See [08](08-open.md).
