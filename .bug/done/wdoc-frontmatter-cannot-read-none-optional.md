# wdoc: frontmatter value that reads a none-valued optional field fails

**Reported by:** WSKILL skill implementation (2026-06-13)
**Component:** `wcl_wdoc` frontmatter evaluation (markdown / `:ai_skill` targets)
**Severity:** moderate (blocks a common pattern)

## Summary

A `@schemaless frontmatter { … }` value that evaluates an optional document
field which happens to be `none` fails the build with:

```
frontmatter field 'X' could not be read — mark the block `@schemaless` so it
accepts arbitrary keys (`@schemaless frontmatter { … }`)
```

The block *is* already `@schemaless`, so the message is misleading — the real
problem is that reading a `none` optional in the frontmatter value position
errors instead of yielding `none` / its `??` fallback.

## Repro

```wcl
import <wdoc.wcl>
// meta.updated is an optional field left unset (so: none)
site skill { default_template = :ai_skill  skill { name = "x"  description = "y" } }
page index { sites = [:skill]  start = true
  @schemaless frontmatter {
    ok   = $"${meta.created}"                      // present  → builds
    bad  = $"${meta.updated ?? meta.created}"      // none lhs → FAILS
    bad2 = $"${match meta.updated { none => meta.created, u => u }}"  // also FAILS
  }
  h1 "Hi"
}
```

`ok` (always-present field) builds; both `bad` forms fail, even though the same
`??` / `match` expressions evaluate fine in a `p` body on the same page.

## Expected

Frontmatter values should evaluate with the same optional/`??`/`match` semantics
as body text — reading a `none` optional should yield `none` (or its fallback),
not error.

## Workaround in use

The WSKILL skill keeps optional reads out of WCL frontmatter and injects
`topic-version` / `generated` into the built `SKILL.md` from its `render.py`
glue script instead (which also lets it stamp the true build date — WCL has no
clock).
