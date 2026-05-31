# Pages

A `page` block declares one rendered HTML page. Each page joins one or more sites and holds the content blocks that make up its body.

## Fields

| Property | Type | Required | Description |
| --- | --- | --- | --- |
| `name` | `identifier` | yes | The page name (the inline label); becomes the output filename (`<name>.html` / `.md`). |
| `id` | `identifier` | no | Optional explicit HTML id. |
| `title` | `utf8` | no | Human-readable page title; sets the browser tab title (`<title>`). Falls back to the page name. |
| `template` | `symbol` | no | Template to wrap the page in; overrides the site's `default_template`. |
| `sites` | `list<symbol>` | no | Named sites this page belongs to; absent ⇒ every site. |
| `start` | `bool` | no | Mark this page as the site's start page (served at `/`). |

#### Child blocks

| Slot | Accepts | Multiple | Description |
| --- | --- | --- | --- |
| `frontmatter` | `frontmatter` | no | Optional YAML front matter for the Markdown target (Markdown only). |
| `children` | `WdocBlock` | yes | The page's content blocks. |

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
> Page names are unique per site, so two different sites can each have a page called index. A page with no sites field is shared with every site.

## Cross-page links

Inside any `p` or `span`, write a markdown-style link where the URL is a bare page name for an in-site link, or `site_name:page_name` for a cross-site link. Links to unknown pages are build errors, so renaming a page can't silently break navigation.

```wcl
p "See [the about page](about) or jump to [the docs](docs:index)."
```
