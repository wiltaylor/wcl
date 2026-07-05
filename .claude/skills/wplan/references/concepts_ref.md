# wplan — concepts

Each concept has its own page. This is the index.

- [**The gated pipeline**](../references/concept_pipeline.md) — Five phases - interview, research, PRD, spec breakdown, render - each blocked by checkable gates.

- [**Gates are blocks, not lets**](../references/concept_gates.md) — A gate is a block whose `ok` field asserts a condition; `wcl eval gates.wcl gates.<id>.ok` forces it and exits non-zero on failure.

- [**The spec DAG and build waves**](../references/concept_dag_waves.md) — Spec-level `depends_on` forms a validated DAG; waves of independent specs build in parallel on their own branches and merge in dependency order.

- [**File ownership**](../references/concept_ownership.md) — Each path has exactly one owning spec; disjoint ownership - not luck - is what makes parallel agents merge-safe.

- [**Self-contained briefs**](../references/concept_briefs.md) — Every exported spec .md must stand alone: rules, context and findings are copied in, never referenced.

- [**Surfaces**](../references/concept_surfaces.md) — Every screen, command and endpoint is a typed contract - elements, required states, interactions - defined during the PRD and delivered by exactly one spec.

- [**Usage scenarios**](../references/concept_scenarios.md) — End-to-end action/expect walks through the finished application - the system-level definition of done that survives all the per-spec merges.

- [**Interface contracts and as-built notes**](../references/concept_contracts.md) — Exact signatures crossing spec boundaries, plus verifier-recorded deviations - the two halves of keeping parallel specs semantically compatible.

- [**Data models**](../references/concept_data_models.md) — Every stored or shared entity as a typed contract - fields, validation, persistence - defined by exactly one spec and copied into every brief that touches it.

- [**Phase signoffs**](../references/concept_signoffs.md) — Every phase ends in an explicit :done or :not_applicable - the two-tier check makes silent skipping impossible without blocking a fresh template.

- [**Implementation vs verification**](../references/concept_role_split.md) — Implementation agents build; a separate verification agent judges, records status, and controls merges - so agents never mark their own work done.

- [**The project context file**](../references/concept_project_context.md) — Durable, repo-level knowledge - stack, commands, conventions, landmines - kept at plans/project-context.md, verified (not regenerated) by each plan, and copied into briefs like any finding.

- [**The lessons loop**](../references/concept_lessons_loop.md) — lessons.wcl captures what each run teaches; durable lessons flow back into this wskill's template and content.
