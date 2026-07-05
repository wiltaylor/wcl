# math

A `math` block typesets a LaTeX equation to inline SVG via the pure-Rust RaTeX toolchain. Glyph outlines embed in the SVG, so equations render the same whether the page is opened locally or served — no webfont dependency.

```wcl
math "E = mc^2"
math "\\frac{-b \\pm \\sqrt{b^2 - 4ac}}{2a}\n"
```

$$
E = mc^2
$$

$$
\frac{-b \pm \sqrt{b^2 - 4ac}}{2a}
$$

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `source` | `utf8` | yes | The LaTeX source — the inline label slot. `math "E=mc^2"` or a raw heredoc `math <<'TEX' … TEX`. |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional class list (e.g. to recolour or resize the equation). |

## Block equations

`math "…" {}` (or `math <<TEX … TEX`) renders a centred display equation. Use a raw heredoc for backslash-heavy LaTeX.

## Inline math

Inside any `p` body or `span`, two inline patterns produce math: `$…$` is text-style (flows with the prose) and `$$…$$` is display-style (slightly larger, still inline). The `$…$` pattern requires a non-space at each delimiter, so currency stays untouched. See [formatting](../references/concept_formatting.md).

```wcl
p <<'DOC'
  The quadratic root $x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}$ flows inline with the prose, while a
  display-style sum $$\sum_{i=1}^{n} i = \frac{n(n+1)}{2}$$ sits slightly larger but still in the
  line.
DOC
```

The quadratic root $x = \frac{-b \pm \sqrt{b^2 - 4ac}}{2a}$ flows inline with the prose, while a display-style sum $$\sum_{i=1}^{n} i = \frac{n(n+1)}{2}$$ sits slightly larger but still in the line.


| Pattern | Style |
| --- | --- |
| $…$ | Text-style inline math — flows with the surrounding prose. |
| $$…$$ | Display-style inline math — slightly larger, still inline. |

## Related

- [diagram](../references/fact_diagrams.md)

[← Back to SKILL.md](../SKILL.md)
