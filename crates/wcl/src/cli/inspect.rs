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
        let file_id = crate::lang::FileId(0);
        let (doc, _diags) = crate::lang::parse(&source, file_id);
        println!("{:#?}", doc);
    }

    if show_hir {
        let options = crate::ParseOptions {
            root_dir: file.parent().unwrap_or(Path::new(".")).to_path_buf(),
            ..Default::default()
        };
        let doc = crate::parse(&source, options);
        println!("=== Evaluated Values ===");
        for (key, val) in &doc.values {
            println!("{} = {}", key, val);
        }
    }

    if show_scopes {
        let file_id = crate::lang::FileId(0);
        let (doc, _diags) = crate::lang::parse(&source, file_id);
        let mut evaluator = crate::Evaluator::new();
        let _ = evaluator.evaluate(&doc);

        println!("=== Scope Tree ===");
        let scopes = evaluator.scopes();
        for scope in scopes.all_scopes() {
            let parent_str = match scope.parent {
                Some(p) => format!("parent=Scope({})", p.0),
                None => "root".to_string(),
            };
            println!("Scope({}) [{:?}] {}", scope.id.0, scope.kind, parent_str);
            for (name, entry) in &scope.entries {
                let val_str = match &entry.value {
                    Some(v) => format!("{}", v),
                    None => "<unevaluated>".to_string(),
                };
                println!("  {} [{:?}] = {}", name, entry.kind, val_str);
            }
        }
    }

    if show_deps {
        let file_id = crate::lang::FileId(0);
        let (doc, _diags) = crate::lang::parse(&source, file_id);
        let mut evaluator = crate::Evaluator::new();
        let _ = evaluator.evaluate(&doc);

        println!("=== Dependency Graph ===");
        let scopes = evaluator.scopes();
        for scope in scopes.all_scopes() {
            for (name, entry) in &scope.entries {
                if !entry.dependencies.is_empty() {
                    let mut deps: Vec<&String> = entry.dependencies.iter().collect();
                    deps.sort();
                    println!(
                        "{} -> {}",
                        name,
                        deps.iter()
                            .map(|d| d.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
            }
        }
    }

    Ok(())
}
