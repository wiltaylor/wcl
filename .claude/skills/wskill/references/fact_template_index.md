# Template — index

An **index** is curated navigation: a top-level entry in the book nav whose members are the
unit pages it pins in `related`. In the book a \*nav\* index (no `body`) is a heading whose
children are those pages; a \*content\* index (with a `body`) renders as its own page. In the
skill a `:both` index becomes a section of SKILL.md's Reference. Indexes may nest one level.


## Skeleton

```wcl
// data/indexes.wcl — all indexes usually live in one file, since they are the nav
index <id> {
  name     = "<Area name>"
  summary  = "<One sentence: what this area covers.>"
  audience = :both
  related  = [<unit ids, in reading order>]

  index <child_id> {                 // optional: one level of nesting
    name    = "<Sub-area>"
    summary = "<One sentence.>"
    related = [<unit ids>]
  }
}
```

## Fields

| Field | What goes in it |
| --- | --- |
| `name` | The area as it should read in the nav — short, no verb |
| `related` | The member units, in the order a reader should meet them. Unlike a content unit, an index has no link cap: pinning is its whole job |
| `audience` | `:both` puts the area in SKILL.md's Reference too; `:ai` makes it skill-only navigation |
| `body` | Omit it for a plain nav heading. Add it only when the area needs its own prose page (a Quick Start, an orientation) |
| nested `index` | One level renders. A deeper nest is not projected |

A unit that is in no index is reachable by link but invisible in the nav — pinning is the last
step of the [capture loop](../references/process_adding_content.md).


## Filled example

```wcl
index authoring {
  name     = "Authoring"
  summary  = "Capturing knowledge: decomposition, classification, style, and the capture loop."
  audience = :both
  related  = [decomposing_information, unit_decision_guide, writing_style, linking_discipline]

  index templates {
    name    = "Unit templates"
    summary = "A fill-out skeleton for every unit kind."
    related = [template_concept, template_entity, template_fact, template_procedure]
  }
}
```

## Done when

- The name reads as a nav heading, not a sentence.
- Members are in reading order, not the order you happened to write them.
- Every unit in the wskill is pinned by exactly one index (a second pin is a duplicate nav entry).
- It has a `body` only if it genuinely needs a page of its own.

[← Back to SKILL.md](../SKILL.md)
