# Codebase reconnaissance

## Purpose

Replace plan mode's open-ended research with findings complete enough that spec bodies can copy from them and no implementation agent ever explores the codebase blind.

## Prerequisites

- A scaffolded issue plan that checks green

## Flowchart

![diagram](../_wdoc/process_proc_recon-diagram-1.svg)

## Steps

### Step 1: Read the living WAD first

If the repo carries a WAD (conventionally `.wad/` - or `just wad-md` output beside it), read it before touching raw code: the systems/containers/components decomposition, the relations, and the domain objects ARE the affected-code map's skeleton, and view 4's code items name the modules recon would otherwise grep for. **Verify, don't trust**: spot-check the blocks you rely on against the code; where the WAD and reality disagree, that is a finding - fix the WAD data on the spot when the correction is small (hand-authored files only), or note it for the maintenance sweep. No WAD in the repo? Continue - and consider suggesting doc mode to the user once the issue lands.

### Step 2: Read the project context file

If `plans/project-context.md` exists (a previous plan wrote it), read it: durable repo knowledge - stack and versions, exact build/test/lint commands with their last observed baseline, conventions, landmines. **Verify, don't trust**: re-run the commands you'll rely on and spot-check one convention; update entries that drifted (edit in place, don't regenerate). Where it is accurate, the r_commands and r_conventions items shrink to a verification note that cites it - but still copy the verified content into the finding files, since briefs must stand alone. If it doesn't exist, create it at the END of recon from the minimum-set findings: only what you actually ran and observed, kept concise.

### Step 3: The minimum recon set

Create these `research` items for every issue (ids are suggestions):

**r_commands - build/test/lint commands.** Find how this repo is actually built and tested (justfile, Makefile, csproj/sln, package.json scripts, CI config). Run them; record the exact commands and their current pass/fail state on trunk. A red baseline changes everything - surface it to the user before planning (you cannot gate a fix on "full suite green" if the suite was already red).

**r_map - affected-code map.** Locate the code the issue touches: entry points, the call path, data structures, config. Use grep/glob plus reading; `git log --follow` and `git blame` on the suspect files often reveal the change that introduced a bug. Record file paths, key symbols, and how they connect - this becomes the heart of every spec body.

**r_conventions - local conventions.** How does \*this\* codebase name things, structure tests, handle errors, format code? Where do new tests for this area live? Copy short representative snippets into the finding.

**r_repro - reproduction (bugs only).** Reproduce the bug as a runnable command - ideally a failing test invocation - and record it verbatim with the observed vs expected output. This later becomes the regression test's contract. If reproduction fails, do not guess at the cause: raise an interview question with what you tried.

Add issue-specific items beyond these as needed (e.g. r_schema for a DB-touching change, r_api for an external dependency). External research (library docs, upstream issues) is allowed when the issue demands it - same finding mechanism; check installed wskill research pages (`.claude/skills/*/references/research_*.md`, then the same glob under `$HOME/.claude/skills/`) before the web, and cite a reused page in the finding.

### Step 4: Capture each finding

```wcl
finding r_repro {
  summary = "Login hangs on slow LDAP; failing test reproduces it."
  body {
    p "Run: `dotnet test --filter LdapTimeoutTests` - observed: hang >30s; expected: timeout error at 5s."
  }
}
```

For each item: a `research` block in research.wcl (`status`, `findings_file`), a finding file at `research/<id>.wcl` containing a `finding` block (`summary` plus a `body` of `p` paragraphs and code where detail warrants), and an import line in plan.wcl - literally `import "./research/<id>.wcl"`, next to the existing imports. Mark the research row `:done` only when the finding stands alone. Consult the wcl skill rather than inventing syntax for anything fancier than paragraphs.

### Step 5: Recon discipline

Read code before forming theories; record what the code \*does\*, not what the issue report says it does - disagreements between the two are interview questions. Timebox rabbit holes: if a line of investigation stops informing the spec breakdown, mark where you stopped and move on. Note landmines for spec authors: shared files many features touch (DI registration, route tables, project files), generated code, areas with no test coverage.

> [!TIP]
> **Verification**
> The research_done gate passes; every finding stands alone; bugs have a verbatim reproduction or an open interview question.

[← Back to SKILL.md](../SKILL.md)
