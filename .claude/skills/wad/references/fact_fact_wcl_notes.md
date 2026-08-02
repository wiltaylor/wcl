# WCL behaviour verified against the binary (0.29/0.30-alpha)

Everything below was tested while building the template. The installed wcl may have moved past
0.29.0-alpha - re-verify anything that misbehaves and record it in lessons.wcl.


| Observed | Consequence |
| --- | --- |
| Top-level lets are lazy and not eval-addressable | Gates must be blocks; force with wcl eval gates.<id>.ok |
| Eval paths are <document_field>.<label>.<field> | gates.dag_acyclic.ok, statuses.spec_010_build.state; the <kind>.<label> form does not resolve |
| Block lists are not evaluable, even with --json | Orchestrators query per-spec leaves or parse status.wcl textually |
| Multiple lets in a fn body need ; terminators | let a = ...; let b = ...; final expression stands alone |
| + does not concatenate lists | Append with flatten(\[acc, \[item\]\]); concat is strings-only |
| sort_connected does not error on cycles | Cycle detection is the dag_acyclic fold fixpoint, not sort_connected |
| @ref("spec") on list<identifier> works | wcl check reports dangling depends_on ids (exit 2) |
| assert(cond, msg) returns none on success | Gate fields use assert(...) == none to produce the required bool |
| _ in interpolated ids triggers the italic inline pattern | Backtick ids in any prose: $"`${s.id}\`" |
| frontmatter blocks are schemaless; lists render as YAML lists | Markdown briefs carry machine-readable spec/branch/depends_on/owns |
| Repeater-generated pages work in HTML and markdown targets | One page per spec derives from the spec blocks in both projections |
| diagram accepts computed edges from data records | The DAG and surface-map pages derive entirely from block data |
| project on an UNSET optional body errors (user_error) | Guard with a filtered repeater: each = filter(\[x\], fn(v) { v.body != none }) |
| != none works on optional body refs inside filter lambdas | The guard pattern above is the verified conditional-projection idiom |
| Two braceless/inline-labelled wf_\* widgets on one line misparse | One wireframe widget per line; wf_checkbox takes its label inline |
| Inline if in string interpolation works | ${if a.command != none { $": `${a.command}\`" } else { " (manual)" }} |
| @answerable (import <answer.wcl>) works with typed symbol_set status fields (0.30) | prompt/response/status roles + pending/resolved/skipped symbols; kind: symbol? and option children |
| wcl answer follows imports; each answer writes to the declaring file (0.30) | wcl answer plan.wcl --list \| --id <q> --pick <opt> --text ... --skip; pick labels + text compose into the response field |

## Related

- [Gates are blocks, not lets](../references/concept_gates.md)

[← Back to SKILL.md](../SKILL.md)
