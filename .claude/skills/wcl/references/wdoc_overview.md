# wdoc Overview

wdoc is WCL's documentation generator. It supports different types of pages and is designed to allow you to present information in wcl files in a human readable format.

## Build & serve

```bash
wcl wdoc build docs/main.wcl --out docs/_site
wcl wdoc serve docs/main.wcl                       # http://127.0.0.1:8080
wcl wdoc serve docs/main.wcl --addr 127.0.0.1:3000 # custom address
wcl wdoc build docs/main.wcl --out _site --site docs   # filter to one site
```

## Multi-site routing

A document can declare several `site` blocks. The site with `root = true` renders at the output root (`/`); other sites render into per-site subdirectories (`/<site>/`). A `chooser` index is auto-generated when no site is rooted.

## Example

```wcl
import <wdoc.wcl>

page index { sites = [:mysite]  start = true
  h1 "My project"
  p "Welcome — see [the docs](docs)."
}
```
