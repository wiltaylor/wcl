use std::path::Path;

use crate::cli::mutation::{
    build_span_to_block, line_indent, load_parsed, parse_assignment, run_selector, split_spec,
    walk_path_blocks,
};
use crate::cli::LibraryArgs;
use crate::lang::ast::{Block, BodyItem};

pub fn run(file: &Path, spec: &str, lib_args: &LibraryArgs) -> Result<(), String> {
    let source = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read {}: {}", file.display(), e))?;

    let (selector_str, action) = split_spec(spec);
    let action =
        action.ok_or_else(|| "missing '~> .path = value' assignment in spec".to_string())?;
    let (path, rhs) = parse_assignment(action)?;

    let doc = load_parsed(file, &source, lib_args)?;

    // Run the selector against the evaluated document.
    let matches = run_selector(&doc, selector_str)?;
    if matches.is_empty() {
        return Err(format!(
            "selector '{}' matched no blocks in {}",
            selector_str,
            file.display()
        ));
    }

    // Build span -> &Block lookup
    let span_to_block = build_span_to_block(&doc.ast);

    // Collect (start, end, replacement) edits, then apply in reverse order.
    let mut edits: Vec<(usize, usize, String)> = Vec::new();

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

        let current = walk_path_blocks(ast_block, &path)?;

        let final_seg = path.last();
        // Find existing attribute
        let existing = current.body.iter().find_map(|bi| match bi {
            BodyItem::Attribute(a) if a.name.name == final_seg.name => Some(a),
            _ => None,
        });

        if let Some(attr) = existing {
            let val_span = attr.value.span();
            edits.push((val_span.start, val_span.end, rhs.clone()));
        } else {
            // Insert a new `name = rhs` line just before the block's closing `}`
            let block_end = current.span.end;
            // current.span.end is the byte just after `}`. Find the `}` itself.
            let close_byte = source[..block_end]
                .rfind('}')
                .ok_or_else(|| "could not locate closing '}' for block".to_string())?;
            let indent = derive_inner_indent(&source, current);
            let insertion = format!("{}{} = {}\n", indent, final_seg.name, rhs);
            // Find start of the line containing the closing brace, so we
            // insert above it cleanly.
            let line_start = source[..close_byte].rfind('\n').map(|n| n + 1).unwrap_or(0);
            edits.push((line_start, line_start, insertion));
        }
    }

    // Apply edits in reverse order of start position
    edits.sort_by(|a, b| b.0.cmp(&a.0));
    let mut result = source.clone();
    for (start, end, replacement) in edits {
        result.replace_range(start..end, &replacement);
    }

    std::fs::write(file, &result).map_err(|e| format!("cannot write {}: {}", file.display(), e))?;
    println!(
        "set {} block(s) in {} ({})",
        matches.len(),
        file.display(),
        action
    );
    Ok(())
}

/// Compute the indentation that should be used for items inside `block`.
/// Reads the indent of an existing body item if available; otherwise indents
/// 4 spaces past the block's opening line.
fn derive_inner_indent(source: &str, block: &Block) -> String {
    if let Some(first) = block.body.first() {
        let span = match first {
            BodyItem::Attribute(a) => a.span,
            BodyItem::Block(b) => b.span,
            BodyItem::LetBinding(lb) => lb.span,
            BodyItem::Table(t) => t.span,
            _ => block.span,
        };
        return line_indent(source, span.start);
    }
    let outer = line_indent(source, block.span.start);
    format!("{}    ", outer)
}
