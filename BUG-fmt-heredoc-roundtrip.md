# Bug: `wcl fmt` rewrites newline-bearing strings into heredocs that fail to re-parse

Found 2026-06-10 in the `wad` project (`~/dev/wad`) while formatting `wdoc/product.wcl`.
`wcl fmt --in-place` produced output that `wcl parse` then rejects — formatting must never
break a parsing file. It bit twice, in two variants:

## Variant 1: interpolated multi-line string → terminator glued to content

Input (parses and builds fine):

```wcl
callout $"${pe.name}" {
  body = $"${pe.description}\n\n**Goals** — ${pe.goals}\n\n..."
}
```

(authored as a multi-line `$"…"` across several lines). `wcl fmt` rewrote it to:

```wcl
body = $<<INTERP
${pe.description}

**Goals** — ${pe.goals}
...
Enriches ${join(...)}.INTERP
```

The closing `INTERP` is glued to the last content line instead of standing on its own
line, so the next parse fails with `unterminated heredoc starting with '<<INTERP'`.
This output was committed before the next build caught it (wad `c614601`, fixed in
`1aee969`) — `fmt` quietly breaking the tree is the painful part.

## Variant 2: `"\n"` separator string → whitespace heredoc that also fails

After working around variant 1 with `join([...], "\n")`, fmt rewrote the two-character
separator string `"\n"` into:

```wcl
join([...], <<DOC
    
  DOC)
```

— a heredoc whose body is a blank/whitespace line, inside a call argument. That output
fails `wcl parse` too (and even if it parsed, indentation-stripping would change the
string's value).

## Expected

- `fmt` output must always re-parse (round-trip property); ideally `fmt` should verify
  its own output parses before writing.
- Short strings with escapes (`"\n"`) should stay escaped literals — heredoc conversion
  only pays off for genuinely multi-line text, and never inside call arguments.
- When emitting a heredoc, the closing tag must be on its own line; content whose last
  line would swallow the tag needs either a trailing newline or escaped-literal form.

## Repro

wad at `c614601` (`~/dev/wad`): `git show c614601:wdoc/product.wcl` contains the broken
variant-1 output exactly as fmt emitted it. For variant 2, format a file containing
`x = join(["a", "b"], "\n")`.

## Workaround used

Avoid newline-bearing strings in .wcl entirely — the personas callout became a sequence
of separate `p` elements (wad `1aee969`).
