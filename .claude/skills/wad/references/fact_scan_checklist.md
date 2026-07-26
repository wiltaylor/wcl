# Codebase scan checklist (per view)

The probes of the codebase scan, in scan order (cheap, high-signal sources first). Each row:
what to read → what to emit → what to ask the user when the code can't answer. (These are data
— `scan_probe` blocks.)


| View | Look at | Emit | Ask if missing |
| --- | --- | --- | --- |
| overview | README, docs/, ADR folders, manifests (Cargo.toml / package.json / pyproject / go.mod), top-level directory names | wad summary draft, candidate systems/containers, repository (+ structure), existing decisions → adr | Which repositories are in scope, and is there prior architecture documentation anywhere? |
| build_deploy | .github/workflows / .gitlab-ci.yml / Jenkinsfile, Dockerfiles, compose files, k8s manifests, IaC (terraform/, ansible/) | pipeline (+ stages from jobs and needs-edges), environment candidates, infra_node candidates — ideal first extractor | Who approves promotion between environments, and are there environments not represented in the repo? |
| systems | main functions / entry points, service definitions, listen ports, Procfile / systemd units / serverless configs | container per runnable unit (kind, technology), component per major module | Are any of these deployed together as one unit, or is anything here dead code? |
| domain | migrations/, ORM models, schema.sql, protobuf/GraphQL schemas | domain_object (+ fields, relations) and/or a code_item :db_schema — write an extractor rather than hand-copying | Which of these tables/types are core domain vs plumbing? |
| infrastructure | IaC state, cloud config, DNS zones, deployment target names in CI, secret-store references | environment, infra_node (nested via parent), deploy_target rows | What infrastructure was provisioned outside the repo (consoles, ClickOps), and what is shared across environments? |
| personas | auth/RBAC code (role enums, permission tables), issue templates, CODEOWNERS, on-call/rota docs | persona per distinct role + access_grant, raci_entry candidates | Who are the real user populations behind these roles, and how is access actually granted? |
| standards | linter/formatter configs, CONTRIBUTING.md, style docs, CI gate steps | standard (+ rule rows with enforced_by from the CI step that runs it) | Are there unwritten rules reviewers enforce that no tool checks? |
| sysadmin | runbooks/, ops/ docs, Makefile/justfile targets, cron definitions, alerting config | sop per procedure (automation = the target/script that runs it) | What do you actually do when X breaks — walk me through the last incident? |
| systems | route tables, page/component directories in UI code, navigation config | screen per surface (route, nav_to from links); wireframes sketched from the real layout | Which screens matter enough to document, and who uses each? |
| documentation | docs/ folders, static-site configs, README links and badges, publishing workflows (Pages / Netlify / Read the Docs) | doc_item per published artefact (source, built_by from the rendering container, hosted_on from the deploy workflow, url) | Is documentation published anywhere outside the repo — wikis, hosted API docs, internal portals — and who maintains it? |
| overview | everything still empty after the probes above | targeted mini-interview using the interview question bank, restricted to the gaps; unanswered items become open questions in an article, never guesses | Why is the system the way it is? (back-fill the load-bearing decisions as ADRs) |

## Related

- [Document an existing system (the scan)](../references/process_documenting_existing_system.md)

- [Interview question bank (per view)](../references/fact_interview_question_bank.md)

[← Back to SKILL.md](../SKILL.md)
