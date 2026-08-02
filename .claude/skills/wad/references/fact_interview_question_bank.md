# Interview question bank (per view)

The questions the design interview asks, in interview order. Each answer becomes the block in
its \*maps to\* column immediately — write, `wcl check`, then move on. Optional rows are asked
only when relevant. (These are data — `interview_q` blocks — so tooling can walk them.)


## Overview / decisions

| Question | Maps to |  |
| --- | --- | --- |
| What is the system, in one sentence — and what business problem makes it worth building? | `wad.summary / wad.description` |  |
| How will you know it succeeded? (measurable criteria if possible) | `wad.description / article :overview` |  |
| Rank the qualities that matter: performance, availability, security, cost, time-to-market. | `article :overview (drives later trade-off ADRs)` |  |
| What is already fixed — mandated stack, compliance regime, budget, deadline, existing contracts? | `adr (one per constraint that forecloses options)` |  |
| What decisions have already been made, and why? | `adr (status :accepted, dated)` |  |

## Personas

| Question | Maps to |  |
| --- | --- | --- |
| Who USES the system — every distinct kind of user, including service accounts and AI agents that consume it? (Not the people who build or operate it — those are view-1 stakeholders.) | `persona (kind :human / :service / :ai_agent); builders/operators → stakeholder` |  |
| For each persona: what are they trying to get done? What volume of them do you expect? | `persona.goals / persona.body` |  |
| What SECURITY CONTROL gates each persona's access — SSO, approved signup, admin invite — and who approves? (Freely installing the software is not access control: no control means no access_grant blocks at all.) | `persona access_grant (method, approver) — or nothing` |  |
| For each persona: what are the typical things they do with the system, end to end? Pick a selection broad enough to exercise most of their usage — each becomes a step-by-step flow (and doubles as a user-acceptance test). | `use_case (steps, from -> to flow, preconditions, outcome)` |  |

## System context

| Question | Maps to |  |
| --- | --- | --- |
| Which systems make up the solution — the boxes an outsider would see — with one line each? | `system` |  |
| Which third-party systems does it talk to over a boundary to do its job (identity, payments, data feeds, notifications…)? Libraries it links against don't count (they're container technology), and where it is hosted/built comes later, in infrastructure. | `external_system (stub) + relation; hosting/CI/CDN → infra_node (view 5); linked libraries → container.technology` |  |
| For each connection: which direction, what protocol, what data crosses, sync or async? | `relation (kind, protocol, data)` |  |
| Which persona uses which system/container? | `relation (persona → container, kind :uses)` |  |

## External systems

| Question | Maps to |  |
| --- | --- | --- |
| Per external system: who owns/runs it (vendor, team), and how critical is an outage to you? | `external_system.vendor / .criticality` |  |
| Who do you call when it breaks — support contact and escalation path? | `support_contact` |  |
| Connection details per environment: URLs, IPs, queue names, auth method? | `endpoint (kind url/ip/config)` |  |
| Where do the API docs / OpenAPI specs / integration references live? | `api_ref` |  |
| What data classification crosses the boundary, and where do the secrets live? | `security_note` |  |

## Systems (C4)

| Question | Maps to |  |
| --- | --- | --- |
| Per system: what are the independently deployable/runnable units — services, databases, web apps, CLIs, queues, batch jobs — with runtime/language for each? | `container (kind, technology)` |  |
| Inside each significant container: what are the major parts and their responsibilities? | `component` |  |
| Is any public interface fixed up front — a DB schema, an API contract, a key protocol? (Code items are otherwise extractor territory once code exists.) | `code_item (:db_schema / :api)` | _optional_ |
| For each user-facing container: every screen/page, what is on it, and how the screens flow into each other. Spec them out completely — wireframe each one. | `screen (wireframe body, nav_to)` |  |
| For each CLI container: every command and subcommand, its arguments and flags, with a description of each — plus a terminal mock-up of the important outputs. | `cli_command (+ cli_arg / cli_flag children) per command; screen (terminal body) only for output mock-ups` |  |
| For each TUI container: every screen/panel, its widgets, and the navigation between them — mock each with terminal/TUI blocks. | `screen per panel (terminal/tui body, nav_to)` |  |

## Domain

| Question | Maps to |  |
| --- | --- | --- |
| What are the business objects in this system — the nouns the business itself uses when talking about it? | `domain_object` |  |
| Per object: which fields matter, and how do objects relate (one-to-many, references)? | `field + domain_rel children` |  |
| What invariants or lifecycle rules must always hold? | `domain_object.body / standard (kind :business)` |  |
| Which terms does the team use that an outsider would misread? | `term` |  |

## Infrastructure

| Question | Maps to |  |
| --- | --- | --- |
| Which environments exist (local, CI, dev, staging, prod, DR) and in what promotion order? | `environment (order, promotes_to)` |  |
| Where does each container run per environment — cloud/k8s/VMs/serverless/SaaS — and what is shared between environments? | `infra_node (parent nesting, environments) + deploy_target` |  |
| What are the availability/scaling arrangements, and what gets backed up, how? | `infra_node.notes / article :infrastructure` | _optional_ |

## Build & deploy

| Question | Maps to |  |
| --- | --- | --- |
| One repo or many? What lives where inside them? | `repository (+ repo_path rows)` |  |
| What CI runs on a change, in what stages, and what gates promotion to each environment? Who approves? | `pipeline (+ stages: gates, next, environment)` |  |
| How are versions numbered, where are artifacts stored, and what shipped in past releases? (Releases are extractor territory — git tags / the platform's releases API.) | `release (via an extractor) + pipeline.body / article :build_deploy` |  |
| How does a new developer get from clone to running tests? | `sop (kind :dev) or repository.body` |  |

## Documentation

| Question | Maps to |  |
| --- | --- | --- |
| What will the project publish to be read — docs site, rendered book, agent skill, README — and for each: where is the source authored, what builds it, where is it hosted, and which personas is it for? | `doc_item (source, built_by, hosted_on, url, audience)` |  |

## System admin

| Question | Maps to |  |
| --- | --- | --- |
| What routine operations exist — deploys, secret rotation, restores, user admin? | `sop (kind :operations / :runbook)` |  |
| What happens when it breaks at 3am — alerting destination, incident steps, escalation? | `sop (kind :incident)` |  |

## Standards & rules

| Question | Maps to |  |
| --- | --- | --- |
| What coding/style standards apply, and what enforces each (CI, linter, review)? | `standard (kind :coding / :style) + rule (enforced_by)` |  |

[← Back to SKILL.md](../SKILL.md)
