use std::path::Path;

use crate::cli::mutation::{
    block_inline_id, build_span_to_block, parse_path_only, run_selector, split_spec, AssignmentPath,
};
use crate::cli::path::{attr_full_span, block_full_span};
use crate::cli::LibraryArgs;
use crate::lang::ast::{Block, BodyItem};

pub fn run(file: &Path, spec: &str, lib_args: &LibraryArgs) -> Result<(), String> {
    let source = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read {}: {}", file.display(), e))?;

    let (selector_str, action) = split_spec(spec);
    let path = match action {
        Some(a) => Some(parse_path_only(a)?),
        None => None,
    };

    // Parse + evaluate so the selector can run
    let mut options = crate::ParseOptions {
        root_dir: file.parent().unwrap_or(Path::new(".")).to_path_buf(),
        ..Default::default()
    };
    lib_args.apply(&mut options);
    let doc = crate::parse(&source, options);
    if doc.ast.items.is_empty() && doc.has_errors() {
        for d in doc.errors() {
            eprintln!("{}", super::format_diagnostic(d, &doc.source_map, file));
        }
        return Err(format!("parse errors in {}", file.display()));
    }

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

fn walk_path_blocks<'a>(start: &'a Block, path: &AssignmentPath) -> Result<&'a Block, String> {
    let mut current = start;
    for seg in &path.segments[..path.segments.len() - 1] {
        current = current
            .body
            .iter()
            .find_map(|bi| match bi {
                BodyItem::Block(b)
                    if b.kind.name == seg.name
                        && (seg.id.is_none() || block_inline_id(b) == seg.id.as_deref()) =>
                {
                    Some(b)
                }
                _ => None,
            })
            .ok_or_else(|| {
                format!(
                    "nested block '{}' not found in {}",
                    seg.name, current.kind.name
                )
            })?;
    }
    Ok(current)
}
