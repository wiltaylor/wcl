# Block Visibility

_The @only / @except decorators scope a block by site, template kind, or backend._

Any block can carry an `@only(...)` or `@except(...)` decorator to scope it to a subset of the build — by site name, template kind, or output backend. Use `@only` to render a block \*only\* where the conditions match, and `@except` to render it \*everywhere except\* where they match.


## Axes

Both decorators take the same three optional arguments, each a list of symbols.


| Argument | Matches against |
| --- | --- |
| `sites` | The site name — the label on a `site` block (`site marketing { … }` ⇒ `:marketing`) |
| `templates` | The site's template kind: `:webpage`, `:book`, or `:presentation` |
| `backends` | The output backend: `:html`, `:pdf`, or `:markdown` |

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
> Filtering on `templates` uses the site's `default_template`. A page-level `template` override does not change which template axis a block matches.

## Related

- [Sites](../references/concept_sites.md)

- [Templates](../references/concept_templates.md)

- [Markdown output](../references/concept_markdown.md)

[← Back to SKILL.md](../SKILL.md)
