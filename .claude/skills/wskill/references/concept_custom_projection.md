# Custom projections (schema extension modules)

_Model domain data — a language's builtins, a CLI surface — as typed blocks in a schema module, then project them into generated reference pages._

When a topic has recurring domain data the four unit kinds don't capture — the builtin
functions of a language, the subcommand tree of a CLI, a keybinding table — model it as
its OWN schema and project it, instead of flattening it into prose facts. The pattern is
a \*schema extension module\*: a `.wcl` file declaring typed `@block`s plus a merging
`@document` that gathers them.


```wcl
// schema/keybindings.wcl — a topic-specific extension module
namespace wcl.wskill

@block("keybinding")
type Keybinding {
  @inline(0) id: identifier
  keys:    utf8
  action:  utf8
  context: utf8?
}

// Merges with the base @document — `keybindings` gathers only where imported.
@document
type KeybindingDoc {
  @children("keybinding") keybindings: list<Keybinding>
}
```

Import the module from `wskill.wcl`, author instances in `data/`, then add a render to
each projection template — typically a `wdoc_repeater` generating one page (or one table
row) per instance. Because imported `@document` schemas MERGE with the base, the
extension composes cleanly with everything else.


The wcl wskill is the worked example at scale: its `schema/builtins.wcl` and
`schema/cli.wcl` modules capture 80+ builtin functions and the whole `wcl` subcommand
tree as data, projected into generated per-function and per-command reference pages.
Two standard modules ship with every wskill — the [presentation](../references/concept_presentation_view.md)
and [training](../references/concept_training_view.md) views use exactly this mechanism.


## Examples

### A typed custom block

Declare an @block plus a @document that gathers it; imported documents merge with the base. Then add a render to both template sets.

```wcl
@block("keybinding")
type Keybinding {
  @inline(0) id: identifier
  keys:     utf8
  action:   utf8
  context:  utf8?
}

@document
type Extensions {
  @children("keybinding") keybindings: list<Keybinding>
}
```

## Related

- [Structured data](../references/concept_structured_data.md)

- [Components: one look for every unit](../references/concept_components_look_feel.md)

- [Creating a schema extension](../references/process_creating_schema_extension.md)

[← Back to SKILL.md](../SKILL.md)
