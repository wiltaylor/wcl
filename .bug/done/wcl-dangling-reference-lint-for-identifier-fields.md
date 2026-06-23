# wcl: relationship-integrity lint for plain identifier reference fields

**Reported by:** WAD skill implementation (2026-06-13)
**Component:** `wcl` CLI (`check` / a lint mode) + `wcl_lang`
**Severity:** enhancement (a Python workaround exists)

## Summary

`wcl check` validates that **connection** operands name real blocks, which is
great. But a field typed `identifier` (or `utf8`) that is *semantically* a
reference to another entity is not checked — there's no way to declare "this
field's value must be the id of an existing block of kind K". WAD has several such
fields (`flow.entry_screen`, `flow_transition.from/to_screen`,
`procedure_step.actor`, `change_item.entity_id`, `external_endpoint.environment`)
and has to validate them in a Python lint pass.

## Requested

Either:

1. A field decorator that declares a referential constraint, e.g.
   `@ref("screen") entry_screen: identifier`, checked by `wcl check`; or
2. A machine-readable `wcl check --refs` / lint mode that reports dangling
   identifier references given such declarations; or
3. Allow `&BlockType`-by-id resolution so these fields can be real typed
   references rather than bare identifiers.

Any of these would let dangling-reference integrity live in `wcl` instead of in
each tool's glue.

## Workaround in use

`scripts/wad.py lint` extracts the model's entity ids and the reference fields
(via the JSON-extraction trick), then checks each reference against the id set,
reporting `dangling-reference` findings. Works, but duplicates knowledge of which
fields are references.
