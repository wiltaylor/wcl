# The spec DAG and build waves

_Spec-level \`depends_on\` forms a validated DAG; waves of independent specs build in parallel on their own branches and merge in dependency order._

Dependencies are spec-level only (`depends_on = [spec_a, spec_b]`). `wcl check` validates every id via `@ref("spec")`, and the `dag_acyclic` gate rejects cycles with a Kahn-style fixpoint (the `sort_connected` builtin does not error on cycles, so it cannot serve as the detector).

The build order is derived, not authored: specs whose dependencies are all merged form the next **wave**, and specs within a wave run in parallel. The book's DAG page and the exported `index.md` both show the waves. Each spec is implemented on its own branch (`spec/NNN-name`) in a git worktree at `.tree/<branch>` (gitignored by spec_000); branches merge only after verification, in dependency order.

## Related

- [File ownership](../references/concept_ownership.md)

- [The thirteen default gates](../references/fact_fact_default_gates.md)

- [Breaking down the specs](../references/process_proc_spec_breakdown.md)

- [Verifying and reviewing a spec](../references/process_proc_verify_spec.md)

[← All concepts](../references/concepts_ref.md) · [← Back to SKILL.md](../SKILL.md)
