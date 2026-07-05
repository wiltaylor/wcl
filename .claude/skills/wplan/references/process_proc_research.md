# Doing the research

## Purpose

Leave nothing for implementation agents to figure out - weak models doing mid-build research is the primary failure mode this pipeline exists to prevent.

## Prerequisites

- Interview complete (questions_closed passes)

## Flowchart

![diagram](../_wdoc/process_proc_research-diagram-1.svg)

## Steps

### Step 1: List the research items

While interviewing, keep a running list: library/crate selection, API shapes, version constraints, algorithms, file-format details, platform quirks. Record each as a `research` block in research.wcl with `findings_file` naming its finding file.

### Step 2: One finding file per item

```wcl
finding r_clap {
  summary = "Use clap 4 with derive feature."
  body { p "clap 4.x derive API is the standard. Add with: `cargo add clap --features derive`." }
}
```

Create research/<id>.wcl per item and add its import line to plan.wcl. Separate files mean items can be researched in parallel (by you, subagents, or across sessions) without merge conflicts.

### Step 3: Research NOW, as the strong model

Check installed wskill research pages BEFORE the web: glob `.claude/skills/*/references/research_*.md` (repo level), then `ls $HOME/.claude/skills/*/references/research_*.md 2>/dev/null` (user level). Grep the headline and `**Question:**` lines for the item's keywords; on a hit, read that skill's `references/index_research.md` for the full menu, verify the page's `Researched:` date / `applies to:` version is still current, reuse what applies, and cite it in the finding body (`p "Source: wskill <name>, references/research_<id>.md (researched <date>)."`). Web-search only the gaps; on a conflict with current official docs, trust the docs and flag the wskill page as stale. Set :done only when the finding contains enough detail that a spec can copy it in and an agent needs nothing else.

### Step 4: Promote durable findings

After the items are :done, offer the user promotion: a finding that is durable and project-independent (a library's API shape, a format's gotcha, a platform quirk) belongs in the matching topic wskill as a `research` block, so the next plan starts from it instead of re-researching — the wskill skill's `capturing_research` runbook does the authoring. Project-specific conclusions stay in this plan's research/ folder.

> [!TIP]
> **Verification**
> The research_done gate passes and every finding file is imported from plan.wcl.

[← All processes](../references/processes_ref.md) · [← Back to SKILL.md](../SKILL.md)
