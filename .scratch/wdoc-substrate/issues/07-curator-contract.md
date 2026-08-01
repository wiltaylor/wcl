# The curator contract — what may it change, and what gates it?

Type: grilling
Status: open
Blocked by: 06

## Question

Settled while charting: authors write freely and messily; a **dedicated curator pass** owns the
graph, **edits directly**, and is gated on render + validate. An advisory curator would just be
`linking_discipline` again — guidance with no teeth.

What's open is its contract.

- **Structure only, or bodies too?** This is the sharp one. A hub note's defect is its *content* — it
  carries no fact of its own. A purely structural curator can flag "this unit's body is 80% links"
  but the fix is a rewrite. Options: structure-only and hubs get reported; bodies in scope with the
  render gate as the guard; or a split where the curator restructures and dispatches body rewrites.
- **What ops does it have?** Candidates: merge two units, split one, demote a hub to an `index`,
  promote an index to a unit, prune a link, re-pin, re-kind (concept → fact), delete an orphan, move
  between files. Which are safe to automate, which need a human? Note the editor already has
  span-addressed AST mutation ops (`/api/block/ops`, `/api/nav/op`, `replace_block_by_span`,
  `remove_field`, `set_or_remove_decorator`, `ensure_import`) — establish whether the curator reuses
  that op vocabulary or needs its own.
- **When does it run?** After every authoring session, on demand, on a schedule, as a pre-commit
  gate, or as a phase of the authoring process itself?
- **What exactly gates it?** "Render + validate" needs teeth. Does it mean the four projections all
  still build, the graph lint passes, a diff review, or a before/after structural comparison? A
  curator that can delete a unit needs a stronger gate than one that only prunes links.
- **How is a bad curation caught?** It edits directly, so a wrong merge is a data loss. Git is the
  backstop — is that enough, or does it need a dry-run / journal / revert path?
- **Is it one agent or several?** One curator holding the whole graph, or per-index curators with
  bounded working sets? Scale matters: the `wcl` wskill is 65 units and 69 data files.

Blocked by `06-unit-kinds`: the curator needs a target shape to curate toward.
Feeds `08-wskill-cli`: the CLI surface exists to serve this contract.
