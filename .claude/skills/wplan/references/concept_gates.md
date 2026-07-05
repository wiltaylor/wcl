# Gates are blocks, not lets

_A gate is a block whose `ok` field asserts a condition; `wcl eval gates.wcl gates.<id>.ok` forces it and exits non-zero on failure._

Top-level `let` bindings in WCL are lazy and not addressable by `wcl eval` - a failing `assert` inside an unreferenced let is silent. Gates therefore live as `gate` blocks whose `ok = assert(<condition>, <message>) == none` field is forced by evaluating its path. The justfile's `check` recipe greps gate ids out of gates.wcl and evaluates each, so new gates need no recipe changes.

The planning model owns gates.wcl: keep the five defaults and add project-specific assertions where the schema allows (the template ships commented optional gates as a starting point).

## Related

- [The gated pipeline](../references/concept_pipeline.md)

- [The thirteen default gates](../references/fact_fact_default_gates.md)

- [WCL behaviour verified against the binary (0.29/0.30-alpha)](../references/fact_fact_wcl_notes.md)

[← All concepts](../references/concepts_ref.md) · [← Back to SKILL.md](../SKILL.md)
