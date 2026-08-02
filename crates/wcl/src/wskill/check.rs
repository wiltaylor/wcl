use std::path::Path;

use super::WSKILL_OK;
use super::support::{discover, open_graph, render_view, report, scratch};

/// Run `wcl wskill check [<entry>]`: read each registry through the model,
/// resolve every declared artifact entry, build it into scratch space, and
/// report the model nodes each projection reaches.
pub(crate) fn run(entry: &Path) -> u8 {
    let collection = match discover(entry) {
        Ok(collection) => collection,
        Err(error) => return report(error),
    };
    let scratch = match scratch("create check staging directory") {
        Ok(dir) => dir,
        Err(error) => return report(error),
    };

    let mut artifacts = 0usize;
    for (root_n, root) in collection.roots.iter().enumerate() {
        let graph = match open_graph(root) {
            Ok(graph) => graph,
            Err(error) => return report(error),
        };
        println!("==> {}", root.display());
        let total = graph.units.len() + graph.index_levels().count();
        for (view_n, view) in graph.views.iter().enumerate() {
            let out = scratch
                .path()
                .join(root_n.to_string())
                .join(view_n.to_string());
            if let Err(error) = render_view(&graph, view, &out) {
                return report(error);
            }
            let visible = graph.units.iter().filter(|u| u.shows_in(view)).count()
                + graph
                    .index_levels()
                    .filter(|index| index.shows_in(view))
                    .count();
            println!("coverage {}: {visible}/{total} nodes", view.id);
            artifacts += 1;
        }
    }
    println!(
        "checked {artifacts} artifact{} across {} wskill{}",
        plural_suffix(artifacts),
        collection.roots.len(),
        plural_suffix(collection.roots.len())
    );
    WSKILL_OK
}

fn plural_suffix(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}
