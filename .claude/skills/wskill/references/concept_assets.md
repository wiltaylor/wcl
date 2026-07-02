# Assets (images & data files)

_A wskill-root `assets/` folder holds images, PDFs and data files; pages reference them with `image` and `file` blocks via a `../../assets/…` path._

A wskill keeps its static files — images, PDFs, datasets — in an `assets/` folder at the
wskill root, beside `data/` and `wdoc/`. It is committed content, not generated output.
Both projection entry points live at `wdoc/<book|skill>/main.wcl`, so the folder is reachable
from either as `../../assets/…`; because the path resolves relative to the build entry, a unit
`body` (shared by both projections) uses the same reference in each.


Embed an image in any unit `body` or page — the source is copied into the output automatically:

```wcl
image "../../assets/diagram.png" { alt = "Architecture diagram"  width = 640 }
```

For a PDF or any other data file, ship it into the output with a `file` block. `dir` names the
output subfolder and `as` renders a download link (omit `as` to ship the file silently and link
it yourself):


```wcl
file "../../assets/report.pdf" { dir = "files"  as = "Download the report (PDF)" }
```

## Related

- [Separation of Data and Presentation](../references/concept_datapresentation.md)

- [Structured data](../references/concept_structured_data.md)

- [Fact](../references/concept_fact.md)

[← Back to SKILL.md](../SKILL.md)
