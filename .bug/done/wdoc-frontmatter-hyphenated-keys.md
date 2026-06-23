# wdoc/wcl: cannot author hyphenated frontmatter keys (e.g. `allowed-tools`)

**Resolved (2026-06-16):** `@schemaless` blocks now accept string-literal keys, so
`"allowed-tools" = [...]` parses and emits the hyphenated YAML key verbatim
(verified end-to-end by the WSKILL skill).

**Reported by:** WSKILL skill implementation (2026-06-16)
**Component:** `wcl_lang` parser (`@schemaless` block field keys) + `wcl_wdoc` frontmatter (markdown / `:ai_skill` targets)
**Severity:** moderate (blocks emitting standard Claude/Agent skill frontmatter)
**Type:** feature / enhancement

## Summary

A `@schemaless frontmatter { … }` block can only use **identifier** keys, and WCL
identifiers cannot contain `-`. So there is no way to author a frontmatter key like
`allowed-tools`, `disallowed-tools`, `disable-model-invocation`, `user-invocable`, or
`argument-hint` — exactly the keys the Claude Code / Agent **skill** spec defines for
`SKILL.md`. Claude Code reads the hyphenated form only; the underscore form
(`allowed_tools`) is silently ignored, so a generated skill cannot set these.

This directly contradicts the stdlib doc note at
`crates/wcl_wdoc/lib/templates.wcl:181-188`, which already promises:

> Arbitrary extra front-matter keys (e.g. `allowed-tools`) can still be authored via a
> `frontmatter` block on the start page — they are merged with these.

Today that is not actually possible.

## Repro

Every attempt to express the key fails at parse time:

```wcl
import <wdoc.wcl>
site demo { default_template = :ai_skill  skill { name = "d"  description = "x" } }
page index { start = true
  @schemaless frontmatter {
    "allowed-tools" = ["Bash", "Read"]   // × expected identifier, found string
    `allowed-tools` = ["Bash"]           // × unexpected character '`'
    allowed\-tools  = ["Bash"]           // × unexpected character '\'
    allowed_tools   = ["Bash"]           // parses, but emits `allowed_tools:` (wrong key)
  }
  h1 "D"
}
```

Only the underscore form parses, and it emits the wrong YAML key:

```yaml
---
name: d
description: x
allowed_tools:      # Claude Code ignores this; it wants `allowed-tools`
  - Bash
---
```

## Expected

A way to emit a hyphenated YAML frontmatter key from a `frontmatter` block.

The emit side **already supports this** — `yaml_key` → `scalar_string` →
`is_plain_safe` (`crates/wcl_wdoc/src/markdown/yaml.rs:212-232`) explicitly permits `-`
in a key and would emit `allowed-tools:` bare/unquoted. The only gap is on the **parse
side**: `@schemaless` block field keys must be identifiers.

## Proposed fix (preferred)

Allow a **string-literal key** in `@schemaless` block field position, used verbatim as
the key name:

```wcl
@schemaless frontmatter {
  "allowed-tools" = ["Bash", "Read"]
  "disable-model-invocation" = false
  normal_key = "still fine"
}
```

This is the most general fix, needs no new syntax beyond accepting a string token where
an identifier is currently required (scoped to `@schemaless` blocks, so strict blocks are
unaffected), and makes the existing `templates.wcl` doc note true. `Block::fields()` would
return the string as the field name; `yaml_key` already handles it.

### Alternatives (if a string key is undesirable)

1. A field-rename decorator authorable on a frontmatter field, e.g.
   `@key("allowed-tools") allowed_tools = [...]`, emitting the decorator's name.
2. A wdoc-level known-key map translating a fixed set of underscore keys to their
   hyphenated skill-spec equivalents. (Least preferred — magic, and only covers a fixed
   key set.)

## Impact / why not a workaround

The WSKILL skill is adding first-class blocks for authoring a skill's frontmatter
controls (`allowed-tools`, etc.) from the WCL model. The only workaround is a
post-process pass that rewrites keys in the generated `SKILL.md` (sed/`render.py`), which
defeats the "wdoc emits a correct skill folder directly" contract and is bypassed when a
wskill renders via `wcl wdoc skill` directly (its `justfile` does). A parser-side fix
keeps the model the single source of truth. The skill-frontmatter feature is deferred
until this lands.

## Related

- `.bug/done/wdoc-frontmatter-cannot-read-none-optional.md` — the other frontmatter
  limitation the WSKILL skill hit (none-valued optional reads). Both shape how skills can
  express frontmatter from the model.
