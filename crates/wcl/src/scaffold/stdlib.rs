//! The scaffold standard library, embedded in the binary and registered
//! under `scaffold/*.wcl` keys plus the public `scaffold.wcl` entry
//! point. A template opts in with an explicit `import <scaffold.wcl>`
//! line, which pulls in the prelude; the prelude pulls in every other
//! part via importer-relative system imports (`import <core.wcl>` →
//! `scaffold/core.wcl`). Mirrors `wcl_wdoc::schema_registry`.

use wcl_lang::Registry;

pub(crate) fn schema_registry() -> Registry {
    let mut r = Registry::new();
    r.register("scaffold.wcl", include_str!("lib/scaffold.wcl"));
    r.register("scaffold/prelude.wcl", include_str!("lib/prelude.wcl"));
    r.register("scaffold/core.wcl", include_str!("lib/core.wcl"));
    r
}
