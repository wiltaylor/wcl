# Bundled icon packs

These directories hold the SVG icon packs that `wcl_wdoc` compiles into the binary
(see `../../build.rs`) and exposes through the `iconset` block, the inline `:name:`
handler, and the diagram `icon` block.

Each pack is vendored verbatim from upstream — only the `icons/*.svg` files and the
upstream `LICENSE` are kept. The licence text travels with the files to satisfy the
redistribution terms.

| Pack        | Source                                  | Licence | `pack` value |
|-------------|-----------------------------------------|---------|--------------|
| `lucide/`   | https://github.com/lucide-icons/lucide  | ISC     | `lucide`     |
| `bootstrap/`| https://github.com/twbs/icons           | MIT     | `bootstrap`  |

Icon names are the SVG file stems (e.g. `house.svg` → `house`). Lucide is stroke-based
(`stroke="currentColor"`); Bootstrap is fill-based (`fill="currentColor"`). Both honour
`currentColor`, so the wdoc `class` system's `color` recolours either.

## Updating a pack

```sh
git clone --depth 1 <source-url> /tmp/pack
cp /tmp/pack/icons/*.svg crates/wcl_wdoc/assets/icons/<pack>/
cp /tmp/pack/LICENSE     crates/wcl_wdoc/assets/icons/<pack>/LICENSE
```

The build script re-bundles automatically (`cargo:rerun-if-changed=assets/icons`).
