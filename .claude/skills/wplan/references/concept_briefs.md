# Self-contained briefs

_Every exported spec .md must stand alone: rules, context and findings are copied in, never referenced._

A brief is executed by an agent that may be a much weaker model than the one writing it. Assume the implementing agent follows instructions literally, cannot research anything, treats ambiguity as permission, and forgets rules that are not in its own brief.

So: project rules are copied verbatim into every brief; each spec's `body` copies in the PRD excerpts, research findings and conventions it relies on; and redundancy across briefs is deliberate. A brief that says "see the PRD" is a broken brief. Repeat the most frequently violated boundary inside the relevant task text too - weak models weigh task text more heavily than rule sections.

## Examples

### A complete spec block

A downstream spec showing dependency-ordered ownership handoff (main.rs stubbed by spec_010, owned here), copied-in research, and runnable acceptance commands.

```wcl
spec spec_030_cli {
  title = "CLI layer"
  objective = "Wire the CLI to the core engine."
  depends_on = [spec_010_build, spec_020_core]
  branch = "spec/030-cli"
  owns = ["src/cli.rs", "src/main.rs"]
  allowed = ["edit src/main.rs", "create src/cli.rs"]
  not_allowed = ["modifying src/core/ internals", "adding dependencies beyond clap"]
  done = ["`demo --help` prints usage"]
  task t1 { text = "Add clap 4 with derive: `cargo add clap --features derive` (research r_clap)." }
  task t2 { text = "Create src/cli.rs with the arg structs; keep main.rs to parsing + dispatch only." }
  accept a1 { check = "Help prints" command = "cargo run -- --help" }
  accept a2 { check = "Tests pass" command = "cargo test" }
  body {
    p "Research finding r_clap: use clap 4 derive API. Subcommands via `#[derive(Subcommand)]`; keep arg structs in cli.rs."
    p "PRD g_fast: sub-second startup is a hard goal - no heavy work before arg parsing."
  }
}
```

**Expected:** wcl check plan.wcl passes (deps resolve, ownership disjoint); just specs emits a standalone brief with frontmatter, worktree instructions and the copied-in context.

## Related

- [Implementation vs verification](../references/concept_role_split.md)

- [Breaking down the specs](../references/process_proc_spec_breakdown.md)

- [Rendering and handoff](../references/process_proc_render_handoff.md)

[← All concepts](../references/concepts_ref.md) · [← Back to SKILL.md](../SKILL.md)
