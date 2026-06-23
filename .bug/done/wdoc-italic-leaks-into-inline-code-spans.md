# wdoc: `_italic_` inline pattern leaks into inline `code` spans; no escape

**Reported by:** WSKILL skill implementation (2026-06-13)
**Component:** `wcl_wdoc` inline-pattern engine
**Severity:** minor (cosmetic, but loses information)

## Summary

Inside an inline code span (a `` `…` `` backtick pattern in a `p`/`span`/table
cell), the `_italic_` pattern is still applied. A token containing an underscore
*pair* — e.g. `reading_long_format` — renders as `reading<i>long</i>format`,
which visually drops the underscores and loses information. There is no escape
that suppresses it.

## Repro

```wcl
import <wdoc.wcl>
site b { default_template = :book  title = "T"  root = true  toc { chapter "x" { page = index } } }
page index { start = true
  p $"A `reading_long_format` B"
}
```

`wcl wdoc build` produces:

```html
<p>A <span class="code">reading<span class="italic">long</span>format</span> B</p>
```

Expected: the contents of an inline code span are verbatim — no emphasis
processing — so it should be `<span class="code">reading_long_format</span>`.

## Escapes don't help

- `"\_"` is rejected by the lexer: `invalid escape '\_'`.
- Producing a literal backslash via `replace(s, "_", "\\_")` renders the
  backslash *and* still italicizes: `reading\<i>long\</i>format`.

## Requested fix (either)

1. Do not run the `_italic_` / `**bold**` patterns inside an inline code span
   (treat code-span contents as verbatim, like fenced code blocks already are).
2. Failing that, honor a backslash escape (`\_`, `\*`) in inline-pattern text so
   authors can opt out.

## Workaround in use

The WSKILL templates wrap id-like values in code spans (correct intent) and the
authoring guidance avoids underscore *pairs* in rendered prose. Fenced `code { … }`
blocks are unaffected (they render verbatim), so multi-line samples are fine.
