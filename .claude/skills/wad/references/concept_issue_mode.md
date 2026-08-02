# Issue mode: wplan compressed for brownfield

_The same schema, gates, briefs and ledger as plan mode - with recon instead of open research, a targeted interview, a mini-PRD of scope fences, and the greenfield bootstraps removed._

Issue mode turns a change request against an existing codebase into a real wplan plan folder
that build mode executes unchanged: open-ended research becomes **codebase reconnaissance**
(plus conditional external research when the change introduces technology the repo doesn't
already use), the interview shrinks to only what recon can't answer, the PRD becomes a
mini-PRD dominated by scope fences, and the greenfield bootstrap specs (spec_000_repo,
spec_010_build) are removed - the repo and build already exist. Everything downstream is
unchanged: same schema, same gates, same `just check` / `just specs`, same `spec/NNN-name`
branches, same status.wcl ledger. Build mode cannot tell an issue plan from a greenfield plan.


Plans live at `plans/<slug>/plan/` inside the target repo (slug like `fix-login-timeout`), so
multiple change plans coexist; hand build mode the specific path. The owns_disjoint gate only
sees within one plan - before creating a plan when another plan in plans/ has unmerged specs
touching overlapping paths, compare their `owns` lists by hand and warn the user.


**When NOT to use issue mode.** A trivial change - single file, no surface/model/contract
impact, an obvious failing-test-first regression test - should just be fixed directly (test
first, then the smallest fix, full suite green). Issue mode starts paying for itself at
multi-file or multi-spec scope, when ownership needs splitting, or when the change touches
anything user-facing. Ceremony disproportionate to the change is how planning pipelines lose
users; say so and fix the trivial thing.


## Related

- [Codebase reconnaissance](../references/process_proc_recon.md) — Codebase reconnaissance supports Issue mode: wplan compressed for brownfield: Replace plan mode's open-ended research with findings complete enough that spec bodies can copy from them and no implementation agent ever explores the codebase blind.

[← Back to SKILL.md](../SKILL.md)
