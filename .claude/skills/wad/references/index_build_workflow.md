# Build mode — workflow

_Execute a rendered plan: orchestrate implementation, verification, review, fix and merge agents over the spec DAG until every spec is merged and every scenario passes._

Read the roles fact first, then the orchestrator loop - everything else is the loop's sub-runbooks. A run is complete when every spec is :merged AND every scenario passes on trunk, never at the last merge.

## Related

- [The build roles](../references/fact_fact_roles.md)

- [The orchestrator loop](../references/process_proc_orchestrator_loop.md)

- [The runner contract and .wbuild/](../references/fact_fact_runner_contract.md)

- [Dispatching implementation](../references/process_proc_dispatch.md)

- [The verification procedure](../references/process_proc_verify.md)

- [The review procedure](../references/process_proc_review.md)

- [The fix loop and merging](../references/process_proc_fix_and_merge.md)

- [States and status.wcl](../references/fact_fact_states.md)
