# Interface contracts and as-built notes

_Exact signatures crossing spec boundaries, plus verifier-recorded deviations - the two halves of keeping parallel specs semantically compatible._

Ownership stops file conflicts but not semantic ones: a consuming spec's brief is written
before its provider exists, and its agent cannot investigate. A `contract` block pins the
boundary - exact signatures, types, error shapes - owned by the providing spec (implement
verbatim) and copied into every consuming spec's brief (build against exactly, never reach
past). The contract_order gate requires every consumer to transitively depend on its provider,
so the provider is always merged first.


Contracts constrain drift; **as-built notes** report the remainder. When the verifier passes a
spec, it records any deviation downstream specs must know (renamed module, changed command) as
an `asbuilt` row - plain text, verifier-edited like status.wcl, but unlike status.wcl it IS
rendered into dependent briefs. Re-render briefs (just specs) after recording, and before
dispatching each wave, so briefs describe the world as built rather than as planned.


[← Back to SKILL.md](../SKILL.md)
