# State vocabularies

| Set | States | Notes |
| --- | --- | --- |
| Question | :open -> :answered \| :dropped | :open is the default; record answers verbatim; :dropped needs a why in answer |
| Research | :open -> :in_progress -> :done \| :blocked | PRD blocked until all :done |
| Spec | :todo -> :in_progress -> :implemented -> :verified -> :reviewed -> :merged \| :blocked | :implemented = agent reports done; :verified = mechanical verification passed; :reviewed = strong-model code review approved; merge only :reviewed specs whose deps are :merged |
| Surface state kinds | :empty :loading :error :populated :custom | Screens must cover the first four (surface_states gate); :custom for extras |
| Signoff | :pending -> :done \| :not_applicable | :not_applicable needs a note saying why; check-full blocks while any is :pending |

## Related

- [Implementation vs verification](../references/concept_role_split.md)

[← All facts](../references/facts_ref.md) · [← Back to SKILL.md](../SKILL.md)
