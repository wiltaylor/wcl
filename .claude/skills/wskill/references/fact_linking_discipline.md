# Linking discipline — link sparingly

`related` is the unit-to-unit web. It is tempting to list every id that is even
loosely connected, and every wskill that does ends up with pages whose most
prominent feature is a wall of links. Link what the reader needs **next**, and
nothing else.


## The rules

| Rule | Why |
| --- | --- |
| Keep `related` to about 3-5 ids on a content unit | Past five, the list stops being a recommendation and becomes a directory |
| A link costs two pages, not one | `related` renders both ways: the target page lists this unit under "Referenced by". A careless link adds noise at both ends |
| Link what the reader needs next, not everything relevant | A link is a suggested move, not a citation |
| Restate one clause instead of linking, when the reader must not leave | A page that only makes sense with three tabs open is not [self-contained](../references/concept_selfcontained.md) |
| Never re-explain the linked unit | One fact lives in exactly one unit; two copies drift apart |
| An `index` may pin as many units as its area needs | Indexes ARE navigation — the cap is for content units |

## The hub-note anti-pattern

The commonest failure is a unit whose body is a list of links to its own children —
a "Strings" concept that says strings exist and then links "String literals" and
"String interpolation". It carries no fact of its own, so it is a menu wearing a
page's clothes: the reader clicks through it every time and learns nothing.


Delete the hub and pin its children into an [index](../references/process_building_the_index.md). That is exactly what an index is: a heading in the nav whose members are the real units. The WCL wskill lost two such hubs — a `strings` concept and a `cli` concept that restated the structured CLI reference — and both areas got easier to read.

## `related` is meaning; an index is navigation

Do not mirror an index's membership into each member's `related`. The index already
puts those units side by side in the nav. `related` should say "understanding this
one depends on that one" — a claim the index cannot make.


## Symptoms of over-linking

| Symptom | What it usually means |
| --- | --- |
| A unit links 8+ others | It is not atomic — it is a survey of an area, and wants to be an index |
| Every unit in an area links every other | The `related` lists are mirroring the index; strip them back to real dependencies |
| The same id appears in almost every `related` list | That unit is background, not a next step. Pin it once in the index instead |
| A link with nothing said about it | Either say why the reader would follow it, or drop it |

## Zero links is also a smell

A content unit with no `related` at all is usually misfiled or not yet part of the topic's structure — the exception is a genuinely standalone fact. One or two deliberate links beat both extremes.

## Related

- [Atomic Note](../references/concept_atomic_note.md)

- [Writing style for wskill content](../references/fact_writing_style.md)

- [Building the wskill index](../references/process_building_the_index.md)

[← Back to SKILL.md](../SKILL.md)
