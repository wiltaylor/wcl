# Generated vs hand-authored data

_Two ownership zones under data/: extractor-owned generated/ files (overwritten wholesale) and human-owned everything else._

Files under `data/generated/` (and any `data/<view>/generated/`) belong to extractor scripts: each script fully overwrites its one output file on every run, and the file's banner names its owner. **Never hand-edit a generated file** — the next run silently reverts you; fix the extractor instead.

Hand-authored data may \*reference\* generated ids (a hand-written relation can point at a script-emitted container) but never reuses them. Generated files are **committed**, not gitignored: the diff→spec workflow materialises old git revisions, and uncommitted generated data would vanish from the old side of every diff.

## Related

- [Extractor scripts](../references/fact_extractor_anatomy.md)

[← Back to SKILL.md](../SKILL.md)
