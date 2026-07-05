# Living capability specs

Each wissue plan is self-contained, but the *knowledge* it produces shouldn't die with it. Capability specs make repeat work against the same subsystem compound instead of starting cold: a living statement of current behaviour per capability, updated by each plan's deltas when it merges.

Everything here is optional-but-recommended for the first issue against a repo and expected from the second issue onward.

## The capability layer

`plans/capabilities/<capability>.md` — one file per capability the repo's issues have touched (`auth.md`, `csv-export.md`, `search.md`). Seeded **incrementally**: never spec the whole legacy system up front, only the capability the current issue touches, and only what recon actually established.

Format (keep it this simple):

```markdown
# Auth

Purpose: session-based login against LDAP with local fallback.

## Requirements

- WHEN a user submits valid credentials THE SYSTEM SHALL create a session valid for 24h.
- IF LDAP is unreachable THEN THE SYSTEM SHALL fall back to local accounts and log a warning.
- WHEN a session expires THE SYSTEM SHALL redirect to /login preserving the return URL.

## Notes

- Entry points: src/Auth/LdapAuthenticator.cs, src/Auth/SessionMiddleware.cs
- Tests: tests/Auth/
```

Requirements are EARS SHALL statements describing **current behaviour** (see spec_shapes.md for the patterns). The Notes section carries the durable pointers recon would otherwise re-derive.

## The delta discipline

A plan never rewrites a capability file directly. Instead, at mini-PRD time, write `plans/<slug>/capability-deltas.md` stating only what this change does to each touched capability:

```markdown
# Capability deltas — fix-login-timeout

## auth

### MODIFIED
- WHEN a user submits valid credentials THE SYSTEM SHALL create a session valid for 24h.
  → WHEN a user submits valid credentials THE SYSTEM SHALL create a session valid for 24h, waiting up to the configured LDAP timeout before failing.

### ADDED
- IF LDAP responds slower than the configured limit THEN THE SYSTEM SHALL fail the login with a timeout error, not hang.

### REMOVED
(none)
```

Rules:
- Use the three headers `### ADDED` / `### MODIFIED` (old line `→` new line) / `### REMOVED` (line + one-line why). Match requirements by their exact text.
- If the capability file doesn't exist yet, the recon step seeds it first (current behaviour), then the delta is written against it — a delta against nothing is just a rewrite in disguise.
- The deltas are part of the mini-PRD approval: the user approves the future behaviour statement along with goals and specs.
- The plan's prd.wcl requirements and the delta lines should agree — the delta is the same content organised per capability; keep the wording identical where they overlap.

## Merging on completion

When wbuild finishes the plan (all specs merged, scenarios green), its completion procedure applies the deltas: edit each `plans/capabilities/<capability>.md` — append ADDED lines, replace MODIFIED lines, delete REMOVED lines — then stamp the deltas file with `Merged into capabilities on <date>.` at the top. A deltas file without that stamp means an unfinished merge: reconcile before trusting the capability files.

**The known failure mode** (learned from OpenSpec, which pioneered this model): unmerged deltas rot the living specs. The merge is cheap — do it at completion, not "later". If capability files and reality have visibly drifted (recon contradicts them), fix them during recon and note the correction as a finding.
