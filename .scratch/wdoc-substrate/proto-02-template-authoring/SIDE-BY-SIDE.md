# Side by side — the same markup, four ways

The docs website header's menu. Same output in every case:

```html
<ul class="menu">
  <li><a href="index.html">Docs</a></li>
  …
</ul>
```

---

## Today (`crates/wcl_wdoc/lib/website.wcl:155-170`)

```wcl
if len(c.menu) == 0 {
  [ HtmlFundamental::Element {
      tag: "ul", id: none, class: ["menu"], attrs: none,
      children: map(c.pages, fn(p: PageRef) -> HtmlFundamental
        HtmlFundamental::Element {
          tag: "li", id: none, class: none, attrs: none,
          children: [ HtmlFundamental::Element {
            tag: "a", id: none, class: none,
            attrs: [["href", p.href]],
            children: [ HtmlFundamental::Raw { html: p.name } ],
          } ],
        }),
  } ]
} else {
  wdoc_part_menu_tree(c.menu)
}
```

## Model A — external HTML + expressions

```html
{% if site.menu %}
  <ul class="menu">
    {% for m in site.menu %}{{ menu_item(m) }}{% endfor %}
  </ul>
{% else %}
  <ul class="menu">
    {% for p in site.pages %}<li><a href="{{ p.href }}">{{ p.name }}</a></li>{% endfor %}
  </ul>
{% endif %}
```

## Model B — terse WCL element DSL

```wcl
if len(c.menu) == 0 {
  ul(".menu", map(c.pages, fn(p: PageRef) -> El
    li([a({ href: p.href }, [txt(p.name)])])))
} else {
  menu_tree(c.menu)
}
```

## Model C — heredoc with checked slots

```wcl
join([
  html$<<HTML
    <ul class="menu">
    HTML,
  join(map(c.menu, fn(m: MenuEntry) -> Html
    html$<<HTML
      <li><a href="${m.href}">${m.label}</a></li>
      HTML)),
  html$<<HTML
    </ul>
    HTML,
])
```

**A and B are both fine here. C is already broken** — three fragments, none of
them well-formed HTML, nesting inverted against the output.

---

# The acid test — the book's recursive TOC tree

Model C is not shown; it cannot express this without chopping at every level
of the recursion.

## Model A

```html
{% macro toc_tree(entries) %}
  {% if entries %}
    <ul class="book-toc">
      {% for e in entries %}
        <li class="{% if e.children %}book-branch {% if e.active %}open{% endif %}{% endif %}">
          <div class="book-toc-row">
            <span class="book-toc-toggle"></span>
            {% if e.href %}
              <a class="book-chapter {% if e.current %}current{% endif %}" href="{{ e.href }}">{{ e.title }}</a>
            {% else %}
              <span class="book-section">{{ e.title }}</span>
            {% endif %}
          </div>
          {{ toc_tree(e.children) }}
        </li>
      {% endfor %}
    </ul>
  {% endif %}
{% endmacro %}
```

`e.active` is **supplied** — a recursive predicate, which a macro cannot
return. See finding 3.

## Model B

```wcl
let toc_active = fn(e: TocEntry) -> bool
  e.current || any(e.children, toc_active)

let toc_tree = fn(entries: list<TocEntry>) -> list<El>
  when(len(entries) > 0, [
    ul(".book-toc", map(entries, fn(e: TocEntry) -> El
      li(sel_if(len(e.children) > 0, ".book-branch", toc_active(e), ".open"), [
        div(".book-toc-row", [
          span(".book-toc-toggle", []),
          if e.href == "" {
            span(".book-section", [txt(e.title)])
          } else {
            a(sel_if(true, ".book-chapter", e.current, ".current"),
              { href: e.href }, [txt(e.title)])
          },
        ]),
        toc_tree(e.children),
      ]))),
  ])
```

B *can* compute `toc_active` — a genuine advantage, taken at face value. But
look at what conditional classes cost: `sel_if(true, ".book-chapter",
e.current, ".current")` against A's `class="book-chapter {% if e.current
%}current{% endif %}"`.

And B computing it is what keeps it computed **per page**. See finding 3.
