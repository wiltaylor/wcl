# Anatomy of the AI skill

_What `wcl wdoc skill` emits: SKILL.md (frontmatter, boundaries, parameters, index-driven Reference) plus references/, scripts/, assets/._

The AI-skill view renders the wskill into a Claude Code skill folder. Its shape is fixed,
so an agent always knows where to look:


```text
out/skill/
  SKILL.md            # the start page — what the agent loads first
  references/  *.md   # one page per :ai/:both unit + curated index pages
    research_*.md     # captured research findings (fixed, glob-able names)
    index_research.md # the research menu, when findings exist
  scripts/            # runnable helpers (from `script` blocks in the skill config)
  assets/             # static data files (from `asset` blocks)
```

SKILL.md follows the canonical Claude-skill format (the meta-skill spec), so it lints
clean against that toolchain's validator. Its YAML frontmatter carries the skill `name`
(the topic **id** — lowercase, equal to the install directory), the trigger-tuned
`description`, tool controls (`allowed-tools`, `disable-model-invocation`) and a
`metadata` map holding `wskill_schema_version`. The body then stacks: the topic summary
and overview lines inside `<overview>` tags, the invocation parameters (`skill_param`
blocks) as `<variables>` bullets, the `skill_boundary` always/ask/never rules inside
`<boundaries>` tags, a **Reference** section driven by the `:ai`/`:both` indexes — each
index becomes a link section pointing into `references/` — a **Research** section listing
captured `research` findings with their checked dates, and the shipped **Views**.


Because the Reference section is index-driven, curating the `:ai` indexes IS designing
the agent's entry points. A unit that no index pins is still reachable via `related`
links, but the indexes are what the agent scans first.


## Examples

### Installing the skill into a repo

After rendering, copy the skill projection into a repo's .claude/skills/.

```console
$ cp -r docs/wskills/git/out/skill .claude/skills/git
```

**Expected:** out/skill/ is copied to .claude/skills/git/ (use the name from the rendered SKILL.md frontmatter).

## Related

- [The view family](../references/concept_views.md)

- [Audience control](../references/concept_audience_control.md)

- [Setting up AI skill generation](../references/process_setup_ai_skill.md)

- [Building and installing the AI skill](../references/process_installing_the_skill.md)

- [Capturing research into a wskill](../references/process_capturing_research.md)

[← Back to SKILL.md](../SKILL.md)
