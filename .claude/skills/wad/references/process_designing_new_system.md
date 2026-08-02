# Design a new system (the interview)

## Purpose

Populate a WAD for a system that doesn't exist yet by interviewing the user, one view at a time.

## Prerequisites

- A scaffolded WAD (see the create process)
- The user available to answer questions

## Flowchart

![diagram](../_wdoc/process_designing_new_system-diagram-1.svg)

## Steps

### Step 1: Interview mechanics

Rules for the whole session: work **one view at a time**, in the order below (users before
context; context before drill-down). You may batch several questions per message — number
them `topic.index` style (1.1, 1.2, 2.1 …) so answers can reference them cleanly. Write the
blocks **immediately after each phase** and run `wcl check` before moving on.
**Never invent an answer** — an unknown becomes an open-question line in an `article`, not a
guess. Any decision the user makes mid-interview is captured as an `adr` on the spot.


### Step 2: Frame, decisions, stakeholders (view 1)

Ask the \*overview\* rows of the question bank: purpose, success criteria, quality ranking,
fixed constraints, decisions already made, and who funds/approves/builds/operates. Write
`wad` metadata, `adr`s, `stakeholder`s and `raci_entry` rows.


### Step 3: Personas (view 7)

Every distinct **user** kind (humans, services, AI agents that consume the system), their
goals, and how each gets access. Write `persona` blocks with `access_grant` children. The
builders and operators named in step 2 are stakeholders — do not repeat them here; if
someone appears in both lists, they wear two hats and get one entry in each with different
content (the persona describes their \*usage\*).


### Step 4: Context (views 2–3)

The systems of the solution, the externals they touch, and every connection: direction,
protocol, data, sync/async, plus persona→system usage. Write `system` blocks,
`external_system` stubs, and `relation`s — then render: the context diagram appears, and
misdrawn edges are data bugs to fix now. Classify as you go: only \*functional integrations\*
are externals; hosting/CI/CDN platforms are parked for the infrastructure phase. Follow up
each external with the \*externals\* rows (owner, support, endpoints, API docs, security).


### Step 5: Drill down (view 4)

Per system: containers — the independently-runnable units: services, databases, web apps,
CLIs, queues, batch jobs — with runtime/tech; per significant container: components. Code
items only where a public interface is fixed up front (a DB schema, an API contract) —
everything else is extractor territory once code exists. Then
**spec every user surface completely**: each web screen as a wireframe body, each CLI
command and TUI panel as terminal/TUI mock-ups, all wired with `nav_to` flow — surfaces are
not optional.


### Step 6: Domain (view 10)

The **business objects** of the system — the nouns the business itself uses — and how they
relate: fields that matter, relationships and cardinality, invariants, glossary terms. Write
`domain_object`s (+ `field`/`domain_rel`), and keep the vocabulary in the glossary `fact`;
render to see the ER diagram.


### Step 7: Infrastructure, build, operations (views 5, 6, 8)

Environments and promotion order; where each container runs (nested `infra_node`s +
`deploy_target` rows); repos and pipeline stages with gates; the routine/incident/change
SOPs the team intends to run.


### Step 8: Standards & rules (view 9)

Coding/style standards with what will enforce each; business and regulatory rules.

### Step 9: Read-back

Serve the book and walk every chapter with the user. Corrections loop back into the data
(fix data, never pages). When it survives the walk, record the reviewed revision —
implementation specs come later, decomposed from the WAD by the change workflow.


> [!TIP]
> **Verification**
>
> Every view has content or an explicit open-question marker; the user has walked the rendered book and corrected it.

[← Back to SKILL.md](../SKILL.md)
