# Math

LaTeX equations, typeset to self-contained inline SVG by the pure-Rust RaTeX pipeline. Glyph
outlines are embedded as `<path>`s from the bundled KaTeX fonts, so an equation needs no
webfont, no stylesheet and no network — it survives being copied anywhere.

## The `math` block — display equations

The LaTeX source is the block's **label**.

```wcl
math "E = mc^2"

math <<'TEX'
\frac{-b \pm \sqrt{b^2 - 4ac}}{2a}
TEX
```

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `source` | `utf8` | yes | The label slot — the LaTeX. |
| `id` | `identifier?` | no | Explicit HTML id. |
| `class` | `list<utf8>?` | no | Classes, e.g. to recolour or resize the equation. |

**Use a raw heredoc `<<'TEX'` for anything with a backslash.** A backslash is an escape inside
a quoted WCL string. `math "\frac{a}{b}"` is therefore a *parse* error on `\f`, not a math
error. Two spellings work: the raw heredoc, which escapes nothing, and doubled backslashes in
a quoted string (`"\\frac{a}{b}"`). The heredoc reads far better.

## Inline math — `$…$` and `$$…$$`

Inside any patterned string (a `p` body, a `span`, an `li`, a table cell, a `callout` body):

```wcl
p <<'DOC'
The root $x = \frac{-b}{2a}$ flows inline, a display sum $$\sum_{i=1}^{n} i$$ sits
slightly larger but still in the line, and prices like $10 or $20 stay untouched.
DOC
```

| Pattern | Style |
| --- | --- |
| `$…$` | Text style — flows with the surrounding prose. |
| `$$…$$` | Display style — slightly larger, still inline. |

`$$…$$` is *not* a block equation. It stays in the line. Use the `math` block for a centred,
standalone equation.

The `$…$` pattern requires a non-space character at each delimiter, which is what keeps
`$10 or $20` from being read as an equation.

## Colour

Every glyph paints with `currentColor`, so an equation follows the surrounding text colour —
theme, `class`, and the light/dark toggle all just work. The renderer rewrites only the
*default* black; an explicit `\textcolor{…}` keeps its own colour. Recolour an equation
through the surrounding text colour or `\textcolor` — a blanket `path { fill: … }` rule
clobbers both.

## The supported subset

RaTeX covers a KaTeX-shaped subset of math-mode LaTeX. That is wide. Verified working:

- Structure — `^`, `_`, `\frac`, `\sqrt`, `\sqrt[3]`, `\binom`, `\over`, `\left(`…`\right)`,
  `\displaystyle`.
- Big operators — `\sum`, `\prod`, `\int`, `\lim`, with `_` / `^` limits.
- Environments — `matrix`, `pmatrix`, `bmatrix`, `array`, `cases`, `align`, `align*`,
  `gather`, `split`, `equation`, `substack`.
- Symbols — the Greek alphabet, `\pm \times \div \leq \geq \neq \approx \infty`, set and logic
  operators (`\in \forall \exists \subseteq \cup \cap`), arrows (`\rightarrow \Rightarrow
  \leftrightarrow \mapsto \xrightarrow`).
- Fonts and accents — `\mathbb \mathcal \mathbf \mathrm \mathscr \mathfrak \boldsymbol`,
  `\overline \underline \underbrace \hat \vec \tilde \dot`, `\text`, `\operatorname`.
- Spacing and colour — `\,` `\quad` `\qquad` `\phantom` `\raisebox`, `\textcolor` `\color`.
- Macros — `\def` and `\newcommand` inside one equation.

**Not supported** — anything that is document-level LaTeX rather than math: `\usepackage`,
`\label` / `\ref`, `\require`, `\includegraphics`, and drawing environments such as
`tikzpicture`. Each fails as an undefined control sequence or an unknown environment.

## What a failure looks like

**A bad equation never fails the build.** It renders as the literal LaTeX text inside
`<span class="wdoc-math-error">`, with the parser message in the `title` attribute:

```text
ParseError at position 0: Undefined control sequence: \includegraphics
ParseError at position 10: Unexpected end of input in a macro argument
ParseError: No such environment: tikzpicture
```

A red monospace equation in the output therefore means "read the `title`". A build that
succeeded is not evidence that the maths parsed. The layout stage also guards against panics
on pathological input, and degrades the same way.

## `math` is not native — it lowers

The block lowers to `Content::Math { latex, display, id, class }`. LaTeX is a fixed payload:
each backend typesets from it rather than from the block, and `math.rs` is only the
LaTeX-to-SVG leaf they share.

| Target | What you get |
| --- | --- |
| HTML | A centred `<div class="wdoc-math">` around an inline `<svg role="math">`, sized in `em` so it tracks the surrounding font. |
| PDF | The same SVG, drawn natively. |
| Markdown | `$$\n…\n$$` — the LaTeX is kept textual, not rasterised, so a Markdown renderer with math support handles it. |

Long equations scroll horizontally in HTML rather than overflowing the page
(`overflow-x: auto` on `.wdoc-math`).
