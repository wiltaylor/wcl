# File ownership

_Each path has exactly one owning spec; disjoint ownership - not luck - is what makes parallel agents merge-safe._

`owns` lists the paths (files or directories) a spec may create or modify; the `owns_disjoint` gate rejects any path claimed twice. When a file is created early but finished later (src/main.rs stubbed by the build spec, completed by the CLI spec), the **last spec to touch it owns it**; the earlier spec mentions the creation under `allowed`. This is safe because the two specs are dependency-ordered and never run concurrently.

Known limitation: the gate compares strings exactly - it does not expand globs, so `src/` and `src/core/` do not collide as far as the gate can see. Keep ownership at consistent granularity (prefer one directory per spec) and check mixed file/directory overlaps yourself.

## Related

- [The spec DAG and build waves](../references/concept_dag_waves.md)

- [The thirteen default gates](../references/fact_fact_default_gates.md)

[← All concepts](../references/concepts_ref.md) · [← Back to SKILL.md](../SKILL.md)
