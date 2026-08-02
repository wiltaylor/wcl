# Pages

_The `page` block: id, title, the sites it joins, and the start page._

A `page` block declares one rendered HTML page. Each page joins one or more sites (its `sites`
field) and holds the content blocks that make up its body. A page's `title` sets its
heading-bar / navigation label, and `start = true` marks the document entry page.


## A page

```wcl
import <wdoc.wcl>

page index { sites = [:mysite]  start = true
  h1 "My project"
  p "A short intro."
}
```

> [!NOTE]
> **Per-site page names**
>
> Page names are unique per site, so two different sites can each have a page called `index`. Once a document declares more than one site, every page must name its `sites` — a page carrying none is a build error, and a shared page says so with `sites = [:docs, :blog]`. A single-site document needs no `sites` field anywhere.

## Cross-page links

Inside any `p` or `span`, write a markdown-style link where the URL is a bare page name for an
in-site link, or `site_name:page_name` for a cross-site link. Links to unknown pages are build
errors, so renaming a page can't silently break navigation.


```wcl
p "See [the about page](about) or jump to [the docs](docs:index)."
```

## Block reference

A `page` block: one rendered page — its id, title, the sites it joins, the start-page flag, and the content blocks that make up its body.

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | `identifier` | yes | The page name (the inline label); becomes the output filename (`<name>.html` / `.md`). |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `title` | `utf8` | no | Human-readable page title; sets the browser tab title (`<title>`). Falls back to the page name. |
| `template` | `symbol` | no | Template to wrap the page in; overrides the site's `default_template`. |
| `sites` | `list<symbol>` | no | Named sites this page belongs to; required in a multi-site document. |
| `start` | `bool` | no | Mark this page as the site's start page (served at `/`). |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `frontmatter` | `frontmatter` | no | Optional YAML front matter for the Markdown target (Markdown only). |
| `children` | `ContentBlock` | yes | The page's content blocks. |

## Examples

### A minimal page in a site

The smallest useful document: import the wdoc library, declare one start page, and give it a heading, prose, and a code block.

```wcl
import <wdoc.wcl>

page index { sites = [:mysite]  start = true
  h1 "My project"
  p "A short intro. See [the docs](docs)."
  code wcl {
    source = <<'WCL'
let greeting = "hello"
WCL
  }
}
```

**Expected:** One page named index renders to index.html as the site's start page, with a heading, a paragraph, and a fenced code block.

## Related

- [Sites](../references/concept_sites.md) — Sites supports Pages: The `site` block: one output target — template, title, theme, multi-site routing, and full-text search.

[← Back to SKILL.md](../SKILL.md)
