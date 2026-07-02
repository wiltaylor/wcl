# Collections and registries

_A registry repo holds wskills under wskills/ and a landing page that discovers, builds, and links every member — no member list to maintain._

Wskills are portable folders, and a \*collection\* (registry) is how a set of them ships
together: a repo with the members under `wskills/<name>/` and a landing page that
discovers them. `wcl init wskill-registry` scaffolds one.


Discovery is file-based, so there is no member list to maintain. The landing's `include`
blocks build each member's projections into sub-sites, and the `included_sites(...)`
builtin exposes the same set to the page as `{ name, href, title, summary }` records for
the card grid:


```wcl
// Build every member book under wskills/<name>/; decks and courses (members
// that ship them) under decks/<name>/ and training/<name>/.
include "../../wskills" { entry = "wdoc/book/main.wcl" }
include "../../wskills" { entry = "wdoc/presentation/main.wcl"  prefix = "decks" }
include "../../wskills" { entry = "wdoc/training/main.wcl"      prefix = "training" }

let wskills = included_sites({ folder: "../../wskills", entry: "wdoc/book/main.wcl" })
```

Because entry-mode discovery keys on the entry file existing, a member that doesn't ship
an optional view simply doesn't appear in that view's list — the registry adapts to each
member's declared artifacts. One build command renders the landing plus every member.
The WCL repo's own docs site works exactly this way over its three wskills.


## Examples

### A registry landing that discovers members

File-based discovery: include builds every member; included_sites feeds the cards. No member list to maintain.

```wcl
include "../../wskills" { entry = "wdoc/book/main.wcl" }
include "../../wskills" { entry = "wdoc/presentation/main.wcl"  prefix = "decks" }

let wskills = included_sites({ folder: "../../wskills", entry: "wdoc/book/main.wcl" })
wdoc_repeater { each = wskills  as = :s
  p $"[**${s.title}**](./${s.href})${match s.summary { none => "", su => $" — ${su}" }}"
}
```

**Expected:** Each member's book builds under wskills/<name>/ and lists on the landing; members shipping a deck also build under decks/<name>/.

## Related

- [The view family](../references/concept_views.md)

- [Attaching a wskill to a registry](../references/process_attaching_to_registry.md)

- [Self-Contained Content](../references/concept_selfcontained.md)

[← Back to SKILL.md](../SKILL.md)
