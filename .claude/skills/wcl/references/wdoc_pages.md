# Pages

A `page` block declares one rendered HTML page. Each page joins one or more sites and holds the content blocks that make up its body.

## Fields



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
