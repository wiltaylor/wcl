# Writing your own blocks

wdoc's block vocabulary is open. Declare a `@block` type that extends one of the three output
interfaces, give it a `lower` function, and your kind renders like a built-in one.

## The one rule

**A block is rendered exactly one of two ways, and its type says which.**

| Way | Declaration | Who may use it |
| --- | --- | --- |
| A WCL `lower` | `lower = fn(b: MyBlock) -> list<…>` | Anyone. This is how you extend wdoc. |
| Rust dispatch | `@native` | wdoc's own blocks only. |

Declaring **both** fails the build. Declaring **neither** fails the build. A stub `lower` that
returns `[]` while Rust really draws the kind is what the check exists to prevent, so do not
write one to satisfy a type.

`lower` is declared *optional* on each interface, so a native block need not fake a function.
The build check, not the type system, is what makes it exactly one.

## The three output interfaces

An interface says what a block turns **into**. It does not say where the block may be written —
that is decided by the accepts-type on each slot's `@children(...)` field.

| Interface | Lowers to | Where instances are legal |
| --- | --- | --- |
| `ContentBlock` | `list<Content>` | A `page` body, a `card` body, a `node_row` body, a `column` — every `@children(ContentBlock)` slot. |
| `SvgBlock` | `list<Svg>` | A `diagram` or a `container` — every `@children(SvgBlock)` slot. |
| `TermPrimitive` | `list<TermFundamental>` | A `terminal`, or a container widget inside one. |

So `extends ContentBlock` makes a page block; `extends SvgBlock` makes a diagram shape;
`extends TermPrimitive` makes a terminal widget. Extending a *derived* interface counts too:
the wireframe `Widget` extends `SvgBlock`, so a `wf_*` widget is an SVG block.

## Declaring a block

```wcl
@block("status_pill")
type StatusPill extends ContentBlock {
  @doc("The pill's caption.")
  @inline(0) text: utf8
  @doc("Which state the pill reports.")
  state: symbol?
  id: identifier?
  class: list<utf8>?

  lower = fn(p: StatusPill) -> list<Content> {
    let kind = if p.state == :bad { :error } else { :success };
    [ Content::Callout {
        kind: kind,
        heading: p.text,
        body: [],
        id: p.id,
        class: p.class,
      } ]
  }
}
```

Then use it like any other block:

```wcl
page release {
  h1 "Release 1.4"
  status_pill "All checks passed"
  status_pill "Nightly build broken" { state = :bad }
}
```

Four things to note:

- `@block("status_pill")` gives the type its keyword. `@inline(0)` moves a field out of the
  braces and makes it the block's label.
- `lower` is a **field with a default**. The default is the function. An instance may override
  it, but almost never should.
- The interface signature takes `&ContentBlock`; a concrete type narrows the parameter to its
  own type. Function-typed interface fields are structurally compatible, so this is legal.
- The return is a **list**. One block may lower to several nodes.

## The semantic content IR

`Content` is the target-neutral document vocabulary. Every backend — HTML, PDF and Markdown —
matches it exhaustively, so a node you emit renders on all three.

Three properties govern it:

1. **Closed.** There is no `Raw` variant and no generic `Container { role, children }`. A
   concept not in the union is not page content. You keep bespoke *drawings*, through `Drawing`;
   you give up bespoke page *markup*.
2. **One variant per concept.** No `role: symbol` on a shared box.
3. **Semantics in fields, never in strings.** `Heading.level` is a number, not a CSS class.
   `Callout.kind` is a symbol from a declared vocabulary, not a class name matched by hand.

The variants:

| Group | Variants |
| --- | --- |
| Prose | `Heading { level text id class }`, `Paragraph { text id class }`, `List { items style start id class }`, `Table { rows header caption id class }`, `Code { source language filename id class }`, `Callout { kind heading body icon id class }`, `Columns { columns id class }` |
| Media | `Image { source alt caption width height id class }`, `Video { source poster title width height caption id class }`, `File { path label id class }`, `Math { latex display id class }`, `Drawing { shapes width height caption desc id class }`, `Terminal { lines title id class }` |
| Apparatus | `Toc { entries title id class }`, `Footnotes { notes title id class }`, `ChapterHeader { title kicker subtitle reading_time updated version id class }` |
| Presentation | `Fragment { body id class }`, `SpeakerNotes { body id }` |

Supporting record types: `ContentListItem { text blocks }`, `ContentTocEntry { depth title target
number }`, `ContentFootnote { marker text }`. `CalloutKind` is
`:note :info :tip :warning :error :success`.

`class` survives as a **style hint** only, and `id` as an anchor target. Neither may carry
meaning a backend has to parse back out.

Prose fields such as `Paragraph.text` and `ContentListItem.text` run through the inline-pattern
engine, so `**bold**`, links and `:icons:` work inside them.

**Name the fields.** The IR keeps the long record literal on purpose: WCL checks argument arity
but never argument types, so a positional constructor over interchangeable fields renders
silently wrong.

## The `Svg` union

A diagram shape lowers to these:

```wcl
union Svg {
  Rect     { x y width height rx fill stroke id class }
  Circle   { cx cy r fill stroke id class }
  Line     { x1 y1 x2 y2 stroke dash id class }
  Label    { content x y font_size fit_width fit_height fill id class }
  Polygon  { points fill stroke id class }
  Polyline { points stroke dash id class }
  Link     { href children }
}
```

Coordinates are the diagram's user units. A shape's lowering reads its own `x` / `y` and
positions itself, so a custom shape is placed by the author like a `rect`.

## The `TermFundamental` union

A terminal widget lowers to two variants only:

```wcl
union TermFundamental {
  Text     { content: utf8  row: i64  col: i64  fg: utf8?  bg: utf8?  bold: bool? }
  Children { row: i64  col: i64 }
}
```

`Text` is a styled run. `Children` marks a container's content origin, where the renderer draws
the block's child widgets. Lay a widget out from its **own** `(1, 1)`; the renderer offsets it
by the widget's placement.

## The HTML element vocabulary

A lowering may also return `Html` fundamentals. Use them for HTML-first designs — landing pages,
custom site chrome — where the semantic IR has no matching concept:

| Variant | Meaning |
| --- | --- |
| `Element { tag id class attrs children }` | A generic, recursive element. |
| `Paragraph { id class spans }` | A paragraph of escaped runs. |
| `Table { id class header rows }` | Escaped-cell table. |
| `Inline { text }` | A prose run through the inline-pattern engine. |
| `Icon { name class }` | A resolved icon from the declared iconsets. |
| `Raw { html }` | Verbatim, unescaped HTML. |
| `Head { children }` | Hoisted into the page `<head>` when returned at the top level. |
| `Style { name }` | A named structured `style` block, rendered in place. |
| `Highlighted { source language }`, `Math { latex display }` | Leaves computed in Rust. |
| `Blocks { blocks slot owner fallback }` | Typed placement of authored blocks. Templates only. |

The `el` family is the shorthand, each constructor being its long form with the field names
dropped:

```wcl
el(tag, cls, kids)          // Html::Element { tag, class, children }
ela(tag, cls, attrs, kids)  // … with attrs, a list of [name, value] pairs
eli(tag, id, cls, kids)     // … with an explicit id
raw(html)  inl(text)  icon(name, cls)  para(cls, spans)  css_style(name)
```

`ela` and `eli` take the **same number of arguments**, so arity — the one positional mistake WCL
catches — cannot separate them. Calling one where you meant the other drops the id or the attrs
silently. Pick by what you are passing, and use the long form when an element needs both.

An empty `class` or `attrs` list emits no attribute at all, so `el("li", [], kids)` renders
`<li>`.

**The trade-off:** `Html` nodes render fully in HTML. The Markdown and PDF walkers flatten them.
Headings stay headings and `Inline` text keeps its markers, but chrome such as a backdrop or an
icon badge drops away. Prefer `Content` when a concept exists there. Reach for `Html` when it
does not.

## Recursion

A lowering may return **another custom kind's variant**. The renderer recurses until only
fundamentals are left. This runs on every backend, not only in HTML, so a block built out of
your other blocks reaches every target.

The recursion is depth-limited to 32 levels. A lowering that returns its own kind never
terminates and hits that limit.

## `@native`

`@native` says wdoc implements the block in Rust. It is not available to you:

```
type 'X' declares `@native`, but wdoc implements no dispatch for "x" —
only wdoc's own blocks can be native; a user block is rendered by its `lower`
```

The decorator takes an optional backend list — `@native(backends = [:html, :markdown])`
— naming the targets whose Rust dispatch handles the kind. Bare `@native` means all three
(`:html`, `:pdf`, `:markdown`). The declaration is cross-checked against wdoc's
dispatch registry **both ways**: a target claimed but not implemented, or implemented but not
claimed, fails the build.

Using a native block on a target its `backends` exclude is a build error too. Waive it per
instance with the visibility system's backend axis — the decorator goes **before** the block:

```wcl
page overview {
  // Shipped with the web build; the print PDF has nowhere to put a file.
  @except(backends = [:pdf])
  file "src/setup.sh" { dir = "scripts"  as = "run setup" }
}
```

Capability says *cannot*; author intent says *do not want to*. The build refuses until the two
agree.

## Errors you will meet

| Message | Cause |
| --- | --- |
| `declares neither a lower nor @native` | A type extending an output interface with no rendering. |
| `declares both a lower and @native` | Pick one. |
| `wdoc implements no dispatch for "…"` | `@native` on a user block. |
| `carries @native but is not a renderable block` | The type extends no output interface. |
| `re-declares the built-in kind "…"` | Your `@block` name collides with a Rust-dispatched kind (`table`, `list`, `image`, `terminal`, `diagram`, `map`, `tilemap`, `card`, `tree`, and the rest). The schema and its `lower` would be ignored, so pick another name. |
| `lowered to a malformed content node` | A `Content` variant with a missing required field or an out-of-range number. |

An evaluation error inside a lowering does not abort the document. The block renders as nothing,
the first error is captured, and the build reports it and exits non-zero.

## The other way: components

Not every new block needs a lowering. A `wdoc_component` declares reusable markup with named
slots, and an instance of it is validated like any other block:

```wcl
wdoc_component metric_card {
  wdoc_slot label
  wdoc_slot value
  wdoc_slot status { default = "note" }
  wdoc_body {
    callout $"${label}" { class = [status]  body = $"Currently at **${value}%**" }
  }
}

page dash {
  metric_card { label = "CPU"  value = 42  status = "warning" }
}
```

Use a component when the block is a fixed arrangement of existing blocks. Write a `lower` when
the block needs computation — geometry, branching, or a shape the markup cannot express.

## Gotchas

- **The interface picks the output, the slot picks the placement.** Extending `ContentBlock`
  does not make a block legal in a diagram, and no interface says "page-level".
- **`lower` is a field default, not a method.** Write `lower = fn(b: MyBlock) -> list<Content>`
  inside the type body.
- **Return a list**, even for one node.
- **Name the record fields** in `Content` and `Svg` literals. WCL checks arity, not types.
- **`@native` is wdoc's, not yours.**
- **Do not reuse a built-in kind name.** Your declaration would be silently ignored, so wdoc
  refuses it instead.
- **A custom kind reached by a wdoc container is not laid out by it.** A wireframe container, for
  example, arranges the built-in widgets only; your widget renders as a standalone shape.
- **Recursion is capped at 32 levels.**
