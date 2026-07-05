# WAD vocabularies (kinds.wcl)

The symbol_sets the kind fields draw from — extend them in the instance's `schema/kinds.wcl` (add members; don't rename, data references them):

| Set | Members |
| --- | --- |
| ContainerKind | service web_app spa cli library db queue cache batch_job function mobile_app desktop_app editor_plugin gateway static_site other |
| ExternalKind | saas api registry identity datastore ci cdn payment partner ai_service other |
| RelationKind | sync_api async_msg reads writes reads_writes publishes subscribes uses depends_on authenticates triggers hosts other |
| InfraKind | cloud region network vpc cluster namespace vm container_runtime physical_host paas_service ci_runner registry workstation storage edge other |
| EnvKind | local ci dev test staging prod dr other |
| AdrStatus | proposed accepted rejected superseded deprecated |
| SpecStatus | planning in_progress complete abandoned |
| PersonaKind | human ai_agent service |
| SopKind | operations incident change runbook release dev |
| StandardKind | coding style business security process documentation |
| DomainRelKind | has_one has_many references extends uses |
| CodeItemKind | module_graph db_schema class_diagram api other |
| ApiParamLoc | path query header cookie |
| Criticality | low medium high critical |
| DocKind | site book skill reference readme guide changelog other |
| WadSection | overview context externals systems infrastructure build_deploy documentation personas sysadmin standards domain specs |

There is deliberately no RaciRole set — a `raci_entry` row carries four id-lists, which renders the matrix directly.

## Related

- [WAD folder layout](../references/fact_wad_layout.md)

[← Back to SKILL.md](../SKILL.md)
