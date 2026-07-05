# Phase signoffs

_Every phase ends in an explicit :done or :not_applicable - the two-tier check makes silent skipping impossible without blocking a fresh template._

Empty questions, surfaces and scenarios pass their gates vacuously - correct for a backend-only project, but it means a rushed session could skip the surfaces pass on a UI app and stay green. Six `signoff` rows (interview, research, surfaces, scenarios, data_model, prd) ship as :pending; each must be resolved with the user - :done, or :not_applicable with a note saying why.

Enforcement is two-tier: `just check` (the structural gates) stays green on a fresh template and after every edit, while `just check-full` adds the signoffs gate and `just render` requires it - so nothing is handed to agents while a phase skip is unrecorded. Resolve each signoff at its phase's end, not in a batch at the finish.

## Related

- [The gated pipeline](../references/concept_pipeline.md)

- [The plan/ justfile recipes](../references/fact_fact_cli.md)

[← All concepts](../references/concepts_ref.md) · [← Back to SKILL.md](../SKILL.md)
