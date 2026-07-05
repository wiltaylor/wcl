# Build & deploy blocks

| Block | Fields | Notes |
| --- | --- | --- |
| `repository` | `name`, `url`, `summary?`, `primary_language?`, `body?` | local-dev quickstart prose lives in the body or a `:dev` sop |
| `repo_path` | `path`, `description` | nested in `repository` — one row per notable path |
| `pipeline` | `name`, `repo?`, `trigger?`, `summary?`, `body?` | one per workflow |
| `pipeline_stage` | `name?`, `summary?`, `environment?`, `gates[]`, `next[]` | nested in `pipeline`; `next` wires the stage DAG the flowchart draws — branch- and fan-out-safe; `summary` says what the stage does (the pipeline page renders per-stage detail) |
| `release` | `version`, `date?`, `repo?`, `summary?`, `highlights[]`, `url?` | one per shipped version — the Build & deploy releases table; extractor-owned (git tags), never hand-maintained |

A `repo_path` that is really another repository says so: `repo` for one of OUR repos pulled in as a submodule/subtree (the structure diagram links to its page), `external_url` for a submodule outside our control (drawn dashed). This covers monorepos, multi-repo estates, and vendored trees with one vocabulary.

Who populates: pipelines and releases are ideal extractor territory (parse the real CI config / the git tags — see the reference WAD's `extract_ci.py` and `extract_releases.py`); repositories start from `extract_repo.py` and get hand-written descriptions.

## Related

- [The twelve views](../references/concept_twelve_views.md)

[← Back to SKILL.md](../SKILL.md)
