# Block Visibility

_The @only / @except decorators scope a block by site, template kind, or backend._

Any block can carry an `@only(...)` or `@except(...)` decorator to scope it to a subset of the build — by site name, template kind, or output backend. Use `@only` to render a block \*only\* where the conditions match, and `@except` to render it \*everywhere except\* where they match.


## Axes

Both decorators take the same three optional arguments, each a list of symbols.


| Argument | Matches against |
| --- | --- |
| `sites` | The site name — the label on a `site` block (`site marketing { … }` ⇒ `:marketing`) |
| `templates` | The site's template kind: `:webpage`, `:book`, or `:presentation` |
| `backends` | The output target: `:html`, `:pdf`, `:markdown`, or `:skill` |

> [!NOTE]
> **The skill target names itself**
>
> `wcl wdoc skill` runs the Markdown emitter but reports itself as `:skill`, so a skill folder and a Markdown site can be scoped apart. `@except(backends=[:markdown])` does not hide a block from the skill build — say `[:markdown, :skill]` to hide it from both.

## Examples

```wcl
page home {
  // Only on the `:marketing` site.
  @only(sites=[:marketing]) callout "Promo" { body = "Sign up today!" }

  // Everywhere except the printed PDF.
  @except(backends=[:pdf]) p "Watch the screencast above."

  // Only in slide decks.
  @only(templates=[:presentation]) p "Press → to continue."

  // Only on the web (HTML or Markdown), never the PDF.
  @only(backends=[:html, :markdown]) video { src = "demo.mp4" }
}
```

## Semantics

**Within an axis** the values are OR'd — `sites=[:a, :b]` matches site `a` or `b`. **Across axes** they are AND'd — `@only(sites=[:docs], templates=[:book])` renders only when the site is `docs` \*and\* its template is `book`. A block renders when `@only` (if present) matches **and** `@except` (if present) does \*not\* fully match.


> [!NOTE]
> **Template kind is per-site**
>
> Filtering on `templates` uses the site's `default_template`. A page-level `template` override does not change which template axis a block matches.

## Waiving a block a target can't render

The `backends` axis is also how you answer the build when it refuses a block. A few blocks are implemented for some targets and not others — `markdown_source` previews a page's generated Markdown from inside the HTML build, and `file` ships a file into an output folder a single-file PDF doesn't have. Using one on a target it doesn't cover is a **build error**, naming the kind, the target and the fix:


```text
× `file` has no :pdf implementation (it is native on :html, :markdown, :skill);
  remove the block or waive it here with `@except(backends = [:pdf])`
```

The two halves say different things and the build wants both to agree: a block type's `@native(backends = …)` is a **capability** — what wdoc can render — and `@except(backends = …)` on your instance is your **intent** — that you don't want it there. Waive it and the build proceeds, rendering nothing for that block on that target. The alternative is an `@only` that keeps the block off that target entirely. What you cannot do is nothing: a block a target can't render used to vanish from that output silently, and this is the mechanism that ended it.


```wcl
page overview { start = true
  // Shipped with the skill and linked from the web build; the print PDF
  // has nowhere to put the file, and we're fine with that.
  @except(backends=[:pdf])
  file "src/setup.sh" { dir = "scripts"  as = "run setup" }
}
```

## Related

- [Sites](../references/concept_sites.md)

- [Templates](../references/concept_templates.md)

- [Markdown output](../references/concept_markdown.md)

[← Back to SKILL.md](../SKILL.md)
