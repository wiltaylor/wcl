# Codebase reconnaissance

Recon replaces wplan's open-ended research. Its output is finding files complete enough that spec bodies can copy from them and no implementation agent ever needs to explore the codebase blind. External research (library docs, upstream issues) is still allowed when the issue demands it — same finding mechanism. Before hitting the web, check installed wskill research pages (`.claude/skills/*/references/research_*.md`, then the same glob under `$HOME/.claude/skills/`) — grep the headline and `**Question:**` lines for the topic, verify the page's `Researched:` date, and cite it in the finding when reused (same rule as wplan's research procedure).

## Before investigating: the project context file

If `plans/project-context.md` exists (a previous plan wrote it), read it first. It holds the durable repo knowledge — stack and versions, exact build/test/lint commands with their last observed baseline, conventions, landmines. **Verify, don't trust**: re-run the commands you'll rely on and spot-check one convention; update entries that drifted (edit in place, don't regenerate the file). Where it's accurate, recon items r_commands and r_conventions shrink to a verification note that cites it — copy the verified content into the finding files as usual, since briefs must still stand alone.

If it doesn't exist, create it at the END of recon from the minimum-set findings: only what you actually ran and observed, kept concise. This is the one artifact that outlives the plan.

## The minimum recon set

Create these `research` items for every issue (ids are suggestions):

**r_commands — build/test/lint commands.** Find how this repo is actually built and tested (justfile, Makefile, csproj/sln + `dotnet test`, package.json scripts, CI config). Run them; record the exact commands and their current pass/fail state on trunk. A red baseline changes everything — surface it to the user before planning (you cannot gate a fix on "full suite green" if the suite was already red).

**r_map — affected-code map.** Locate the code the issue touches: entry points, the call path, data structures, config. Use grep/glob plus reading; `git log --follow` and `git blame` on the suspect files often reveal the change that introduced a bug and who/what else touches this area. Record file paths, key symbols, and how they connect — this becomes the heart of every spec body.

**r_conventions — local conventions.** How does *this* codebase name things, structure tests, handle errors, format code? Where do new tests for this area live? Which patterns must the change imitate? Copy short representative snippets into the finding.

**r_repro — reproduction (bugs only).** Reproduce the bug as a runnable command — ideally a failing test invocation, else a curl/CLI/script sequence — and record it verbatim with the observed vs expected output. This later becomes the regression test's contract. If reproduction fails, do not guess at the cause: raise an interview question with what you tried.

Add issue-specific items beyond these as needed (e.g. r_schema for a DB-touching change, r_api for an external dependency).

## Capturing a finding

For each item: a `research` block in research.wcl (`status`, `findings_file`), a finding file at `research/<id>.wcl` containing a `finding` block (`summary` plus a `body` of `p` paragraphs and code where detail warrants), and an import line in plan.wcl — literally `import "./research/<id>.wcl"`, next to the existing imports. Mark the research row `:done` only when the finding stands alone. Style follows the wplan skill's examples; consult the wcl skill rather than inventing syntax for anything fancier than paragraphs.

## Recon discipline

- Read code before forming theories; record what the code *does*, not what the issue report says it does — disagreements between the two are interview questions.
- Timebox rabbit holes: if a line of investigation stops informing the spec breakdown, mark where you stopped and move on.
- Note landmines for spec authors: shared files many features touch (DI registration, route tables, project files), generated code, areas with no test coverage.
