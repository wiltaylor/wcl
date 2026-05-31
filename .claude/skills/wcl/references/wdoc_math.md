# Math Equations

`math` typesets a LaTeX equation to inline SVG via the pure-Rust RaTeX toolchain. Glyph outlines embed in the SVG, so equations render the same whether the page is opened locally or served over HTTP — no webfont dependency.

## Block equations

`math "…" {}` (or `math <<TEX … TEX`) renders a centred display equation. Use a raw heredoc for backslash-heavy LaTeX. Previewed in a card:

![diagram](../_wdoc/wdoc_math-diagram-1.svg)

```wcl
math "E = mc^2"

math <<'TEX'
\frac{-b \pm \sqrt{b^2 - 4ac}}{2a}
TEX
```

Its fields:

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `source` | `utf8` | yes | The LaTeX source — the inline label slot. `math "E=mc^2"` or a raw heredoc `math <<'TEX' … TEX`. |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `class` | `list<utf8>` | no | Optional class list (e.g. to recolour or resize the equation). |

## Inline math

Inside any `p` body or `span`, two inline patterns produce math. Live: the mass-energy relation $E = mc^2$ flows with this sentence, and the display form $$\int_0^1 x^2 \, dx$$ sits a touch larger on the same line.

```wcl
$x^2$           // → inline LaTeX (text style — flows with the prose)
$$\int x dx$$   // → display style (slightly bigger, same line)
```

| Pattern | Style |
| --- | --- |
| `$…$` | Text-style inline math — flows with the surrounding prose. |
| `$$…$$` | Display-style inline math — slightly larger, still inline. |

The `$…$` pattern requires a non-space at each delimiter, so currency like `$10 or $20` is left alone.
