# Anatomy of the AI skill

_What `wcl wdoc skill` emits: SKILL.md (frontmatter, boundaries, parameters, index-driven Reference) plus references/, scripts/, assets/._

The AI-skill view renders the wskill into a Claude Code skill folder. Its shape is fixed,
so an agent always knows where to look:


```text
out/skill/
  SKILL.md            # the start page — what the agent loads first
  references/  *.md   # one page per :ai/:both unit + curated index pages
  scripts/            # runnable helpers (from `script` blocks in the skill config)
  assets/             # static data files (from `asset` blocks)
```

SKILL.md is generated from the `skill` block in `wskill.wcl` plus the topic metadata. Its
YAML frontmatter carries the skill `name`, trigger-tuned `description`, tool controls
(`allowed-tools`, `disallowed-tools`, `disable-model-invocation`) and the wskill schema
version. The body then stacks: the topic summary and overview lines, a **Parameters**
table (`skill_param` blocks — what to pass as `$ARGUMENTS`, `$1`, …), the
**boundaries** (`skill_boundary` always/ask/never rules, rendered inside
`<Boundary>` tags so the agent treats them as hard rules), and a **Reference** section
driven by the `:ai`/`:both` indexes — each index becomes a link section pointing into
`references/`.


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

[← Back to SKILL.md](../SKILL.md)
