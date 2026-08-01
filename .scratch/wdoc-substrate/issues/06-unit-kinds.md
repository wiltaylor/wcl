# Does the unit-kind vocabulary survive, and what makes it discriminable to an agent?

Type: grilling
Status: open
Blocked by: —

## Question

The vocabulary has collapsed in practice. The `wcl` wskill — the most mature one in the repo —
contains **45 concepts, 2 entities, 18 facts**. Nearly everything became a `concept`.

`unit_decision_guide` exists, ships to the agent as `audience = :both`, and does not prevent this.

The curator (`07-curator-contract`) needs a **target shape to curate toward**. Until it's settled
what the kinds *are* and what makes one correct, the curator has no criterion — so this is upstream
of it.

Decide:

- **Do `concept` / `entity` / `fact` / `procedure` / `index` / `example` / `research` survive as the
  vocabulary?** Note that 45/2/18 is not evidence the kinds are wrong — it might be evidence the
  *guidance* is unenforceable, or that the WCL topic genuinely is mostly concepts. Establish which
  before redesigning anything.
- **What makes a kind discriminable at authoring time?** A definition an agent can apply without
  judgement, or a mechanical test. "An idea the reader must understand" vs "a concrete NAMED thing"
  vs "a container for values" are distinctions that read well and evidently don't bind.
- **Is `related` the right primitive at all?** Untyped, symmetric-in-effect (it renders both ways —
  the target page lists the source under "Referenced by"), and doing double duty as both dependency
  and see-also. Typed relations (`depends-on` / `see-also` / `part-of`) would give the curator
  something to judge; they'd also give an agent more to get wrong.
- **The hub-note problem specifically.** `linking_discipline` names it well: a unit whose body is a
  list of links to its own children — "a menu wearing a page's clothes". Is a hub structurally
  detectable, or only detectable by reading the body? This determines whether the curator needs to
  touch bodies (`07-curator-contract`).
- **`index` vs `related`.** The stated rule is "`related` is meaning; an index is navigation", with
  an explicit prohibition on mirroring index membership into members' `related`. Is that rule
  enforceable, and is the two-mechanism split right?

Keep this **bounded**. The charting decision was that the kinds are *not* the leverage point — the
authoring process is. This ticket exists because the curator needs a target, not to reopen the data
model. If the answer is "the vocabulary stands, here's the mechanical test", that's a good answer.
