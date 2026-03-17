use std::path::Path;

pub fn run(
    file: &Path,
    show_ast: bool,
    show_hir: bool,
    show_scopes: bool,
    show_deps: bool,
) -> Result<(), String> {
    let source = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read {}: {}", file.display(), e))?;

    // If no flag given, default to showing the AST.
    let default_ast = !show_ast && !show_hir && !show_scopes && !show_deps;

    if show_ast || default_ast {
        let file_id = wcl_core::FileId(0);
        let (doc, _diags) = wcl_core::parse(&source, file_id);
        println!("{:#?}", doc);
    }

    if show_hir {
        let mut options = wcl::ParseOptions::default();
        options.root_dir = file.parent().unwrap_or(Path::new(".")).to_path_buf();
        let doc = wcl::parse(&source, options);
        println!("=== Evaluated Values ===");
        for (key, val) in &doc.values {
            println!("{} = {}", key, val);
        }
    }

    if show_scopes || show_deps {
        println!("// scope/dep inspection not yet implemented");
    }

    Ok(())
}
