# assets/

Static files used by this wskill's pages — images, PDFs, datasets, and any other
data file you want to reference from content.

Both projection entry points live at `wdoc/<book|skill>/main.wcl`, so this
wskill-root `assets/` folder is reachable from either as `../../assets/…`. Content
bodies (shared by both projections) reference it with the same path.

## Images

Embed an image in any `body { … }` (concept / entity / fact / process / index) or
page. The source resolves relative to the build entry file and is copied into the
output automatically:

```wcl
image "../../assets/diagram.png" { alt = "Architecture diagram"  width = 640 }
```

## PDFs and other data files

Ship an arbitrary file (PDF, JSON, CSV, …) into the output with the `file` block.
`dir` names the output subfolder; `as` renders a download link (omit `as` to ship
it silently and link it yourself):

```wcl
file "../../assets/report.pdf" { dir = "files"  as = "Download the report (PDF)" }
```

This folder is not generated output — commit the files you add here.
