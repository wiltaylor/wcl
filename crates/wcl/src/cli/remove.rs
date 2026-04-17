use std::path::Path;

use crate::cli::mutation::{
    block_inline_id, build_span_to_block, load_parsed, parse_path_only, run_selector, split_spec,
    walk_path_blocks,
};
use crate::cli::path::{attr_full_span, block_full_span};
use crate::cli::LibraryArgs;
use crate::lang::ast::BodyItem;

pub fn run(file: &Path, spec: &str, lib_args: &LibraryArgs) -> Result<(), String> {
    let source = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read {}: {}", file.display(), e))?;

    let (selector_str, action) = split_spec(spec);
    let path = match action {
        Some(a) => Some(parse_path_only(a)?),
        None => None,
    };

    let doc = load_parsed(file, &source, lib_args)?;

    let matches = run_selector(&doc, selector_str)?;
    if matches.is_empty() {
        return Err(format!(
            "selector '{}' matched no blocks in {}",
            selector_str,
            file.display()
        ));
    }

    let span_to_block = build_span_to_block(&doc.ast);

    // Collect (start, end) ranges to erase, then apply in reverse order.
    let mut ranges: Vec<(usize, usize)> = Vec::new();

    for matched in &matches {
        let block_ref = match matched {
            crate::Value::BlockRef(br) => br,
            _ => continue,
        };
        let ast_block = span_to_block.get(&block_ref.span).copied().ok_or_else(|| {
            format!(
                "internal: matched block at span {:?} not found in AST",
                block_ref.span
            )
        })?;

        match &path {
            None => {
                // Remove the whole block
                let (s, e) = block_full_span(&source, ast_block);
                ranges.push((s, e));
            }
            Some(p) => {
                // Walk path through nested child blocks
                let final_block = walk_path_blocks(ast_block, p)?;
                let final_seg = p.last();
                let attr = final_block
                    .body
                    .iter()
                    .find_map(|bi| match bi {
                        BodyItem::Attribute(a) if a.name.name == final_seg.name => Some(a),
                        _ => None,
                    })
                    .ok_or_else(|| {
                        format!(
                            "attribute '{}' not found in {}{}",
                            final_seg.name,
                            final_block.kind.name,
                            block_inline_id(final_block)
                                .map(|s| format!("#{}", s))
                                .unwrap_or_default()
                        )
                    })?;
                let (s, e) = attr_full_span(&source, attr);
                ranges.push((s, e));
            }
        }
    }

    // Apply in reverse order of start
    ranges.sort_by(|a, b| b.0.cmp(&a.0));
    let mut result = source.clone();
    for (s, e) in ranges {
        result.replace_range(s..e, "");
    }

    std::fs::write(file, &result).map_err(|e| format!("cannot write {}: {}", file.display(), e))?;
    println!(
        "removed {} target(s) from {}",
        matches.len(),
        file.display()
    );
    Ok(())
}
