# Living capability specs

_Cross-plan behavioural memory: one EARS statement of current behaviour per capability under plans/capabilities/, updated by each plan's deltas when it merges._

Each issue plan is self-contained, but the \*knowledge\* it produces shouldn't die with it. Capability specs make repeat work against the same subsystem compound instead of starting cold. `plans/capabilities/<capability>.md` - one file per capability the repo's issues have touched (`auth.md`, `csv-export.md`) - holds a purpose line, EARS SHALL statements describing **current behaviour**, and a Notes section with the durable pointers recon would otherwise re-derive (entry points, test locations). Seeded **incrementally**: never spec the whole legacy system up front, only the capability the current issue touches, and only what recon actually established. Optional-but-recommended for the first issue against a repo; expected from the second issue onward.

**The delta discipline.** A plan never rewrites a capability file directly. At mini-PRD time, write `plans/<slug>/capability-deltas.md` stating only what this change does to each touched capability, under three headers: `### ADDED` / `### MODIFIED` (old line, arrow, new line) / `### REMOVED` (line + one-line why). Match requirements by their exact text; keep the wording identical to prd.wcl where they overlap. If the capability file doesn't exist yet, recon seeds it first (current behaviour) - a delta against nothing is just a rewrite in disguise. The user approves the deltas together with the mini-PRD.

**Merging on completion.** When build mode finishes the plan (all specs merged, scenarios green), its completion procedure applies the deltas - append ADDED lines, replace MODIFIED lines, delete REMOVED lines - then stamps the deltas file `Merged into capabilities on <date>.` at the top. A deltas file without that stamp means an unfinished merge: reconcile before trusting the capability files. **The known failure mode** (learned from OpenSpec, which pioneered this model): unmerged deltas rot the living specs. The merge is cheap - do it at completion, not "later". If capability files and reality have visibly drifted, fix them during recon and note the correction as a finding.

Capability specs answer "what does this subsystem DO" in EARS terms; the repo's WAD answers "how is it BUILT". They complement rather than compete - recon reads both, and build-mode completion updates both.

## Related

- [Issue mode: wplan compressed for brownfield](../references/concept_issue_mode.md)

- [The project context file](../references/concept_project_context.md)

- [The issue pipeline](../references/process_proc_issue_pipeline.md)

- [The orchestrator loop](../references/process_proc_orchestrator_loop.md)

[← Back to SKILL.md](../SKILL.md)
