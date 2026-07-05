# Extractor scripts

Extractors are single-file Python scripts run with `uv run`: a `# /// script` inline-metadata header declares dependencies, so any source of truth — a database, workflow YAML, cargo metadata, an HTTP API — is one dependency line away. Python is the extractor language on purpose: agents write it well and the library ecosystem covers everything. Each script reads one source of truth and fully overwrites one WCL file under `data/generated/`.

```python
#!/usr/bin/env -S uv run
# /// script
# requires-python = ">=3.11"
# dependencies = []            # e.g. ["pyyaml>=6"]
# ///
"""Extract <what> from <source> into data/generated/<name>.wcl."""

from pathlib import Path

WAD_ROOT = Path(__file__).resolve().parents[1]
OUT = WAD_ROOT / "data" / "generated" / "<name>.wcl"

def wcl_str(s: str) -> str:            # the ~5-line emit helper every script copies
    out = s.replace("\\", "\\\\").replace('"', '\\"')
    out = out.replace("\n", "\\n").replace("\t", "\\t").replace("\r", "\\r")
    return f'"{out}"'

# 1. read the source of truth
# 2. map records onto WAD blocks (in-script tables for human naming)
# 3. render lines, sorted, no timestamps
OUT.write_text("\n".join(lines))
```

The eight rules that make extraction safe: uv single-file; **one script, one output file**; full overwrite, never append; generated banner first; deterministic output (sorted, no timestamps — unchanged sources are git-quiet); stable ids derived from source names; an empty result still writes the banner so imports never dangle; output passes `wcl check`. Exit non-zero on failure so `just extract` stops.

When the data doesn't fit the base blocks, declare a **typed extension block** in `schema/extensions.wcl` and have the extractor emit that, with a matching render in the book template — extraction isn't limited to the built-in families (see the extensions worked example). The bundled `extractor_template.py` (skill scripts folder) is the skeleton ready to copy.

## Related

- [Generated vs hand-authored data](../references/concept_generated_vs_hand.md)

- [Write an extractor script](../references/process_writing_extractor.md)

[← Back to SKILL.md](../SKILL.md)
