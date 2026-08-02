# External-system blocks

> [!WARNING]
> **Integrations, not platforms — and not libraries**
>
> An external system is a third party the design integrates with **as part of its function** — an API it calls, an identity provider, a payment gateway, a data feed. Platforms that \*host, build, or ship\* the system (a cloud, a CI service, a CDN, a git host, a package registry the build pulls from) are **infrastructure** (`infra_node`, view 5), not externals. Linked or vendored **libraries and frameworks** (an SDL2, a web framework, a JSON parser) are not externals either — they are part of the container that links them: name them in its `technology` field. Test: does the \*running system\* talk to it over a boundary to do its job? If it's compiled in, it's technology; if it runs/builds/ships the system, it's infra. A WAD with no externals is fine — a self-contained system honestly has none.

| Block | Fields | Notes |
| --- | --- | --- |
| `external_system` | `name`, `summary`, `kind`, `boundary?`, `vendor?`, `url?`, `criticality?`, `body?` | one per integration; the four child families below nest inside it |
| `support_contact` | `name`, `role?`, `channel`, `details?` | who to call and how |
| `endpoint` | `name`, `environment?`, `kind` (url/ip/config/queue…), `value`, `notes?` | connection details, per environment when they differ |
| `api_ref` | `name`, `kind` (openapi/docs/repo…), `location`, `notes?` | integration references |
| `security_note` | `title`, `detail`, `severity?` | what to watch at the boundary |

Never invent contacts or security posture — these come from the user or the vendor, and the interview asks for them explicitly.

## Related

- [The twelve views](../references/concept_twelve_views.md)

[← Back to SKILL.md](../SKILL.md)
