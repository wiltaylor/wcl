# wcl: want a WCL-level document diff (for change records)

**Reported by:** WAD skill implementation (2026-06-13)
**Component:** `wcl` CLI (new subcommand) / `wcl_lang` document view
**Severity:** enhancement (a git-diff workaround exists)

## Summary

WAD's change-record workflow (PRD §9) needs to know which **model entities**
changed between two branches. There is no WCL-aware diff, so WAD parses the git
unified diff, maps changed line-ranges to enclosing top-level blocks, and emits a
`change_item` per touched entity. This is shallow: a whole-file reformat looks
like "everything changed", and field-level edits inside a block are reported only
at block granularity.

## Requested

A `wcl diff <old.wcl> <new.wcl>` (or `wcl diff --base <ref>`) that compares the
evaluated document views and reports added / removed / modified **entities and
fields** by path, ideally as JSON:

```json
[
  {"op":"modified","entity":"domain_entity:task","field":"fields.due_date","kind":"added"},
  {"op":"added","entity":"spec:impl_due_dates"}
]
```

This would let the change-record generator be precise (per-field `change_item`s)
and be robust to formatting-only churn.

## Workaround in use

`scripts/wad.py change` runs `git diff -U0 <base>...HEAD -- data/`, parses hunk
headers for new-file line ranges, and maps each changed line to its enclosing
`kind "label" {` block by brace-depth tracking. Deliberately not a real WCL
parse — see the PRD note to keep git-diff parsing shallow and file this request.
