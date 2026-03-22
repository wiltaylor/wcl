use std::path::Path;

use crate::path::line_span_of;
use wcl_core::ast::*;

/// Find a table by name in the document, searching top-level and inside blocks.
fn find_table<'a>(doc: &'a Document, name: &str) -> Option<&'a Table> {
    for item in &doc.items {
        if let DocItem::Body(bi) = item {
            if let Some(t) = find_table_in_body_item(bi, name) {
                return Some(t);
            }
        }
    }
    None
}

fn find_table_in_body_item<'a>(bi: &'a BodyItem, name: &str) -> Option<&'a Table> {
    match bi {
        BodyItem::Table(t) => {
            if table_id(t).as_deref() == Some(name) {
                return Some(t);
            }
        }
        BodyItem::Block(block) => {
            for child in &block.body {
                if let Some(t) = find_table_in_body_item(child, name) {
                    return Some(t);
                }
            }
        }
        _ => {}
    }
    None
}

fn table_id(table: &Table) -> Option<String> {
    table.inline_id.as_ref().map(|id| match id {
        InlineId::Literal(lit) => lit.value.clone(),
        InlineId::Interpolated(parts) => parts
            .iter()
            .filter_map(|p| match p {
                StringPart::Literal(s) => Some(s.clone()),
                _ => None,
            })
            .collect::<Vec<_>>()
            .join(""),
    })
}

/// Evaluate a condition against a table row, using the original source text to extract cell values.
fn eval_row_condition_with_source(
    source: &str,
    condition_str: &str,
    columns: &[ColumnDecl],
    row: &TableRow,
) -> Result<bool, String> {
    let mut synthetic = String::new();
    for (i, col) in columns.iter().enumerate() {
        if i < row.cells.len() {
            let cell_span = row.cells[i].span();
            let cell_text = &source[cell_span.start..cell_span.end];
            synthetic.push_str(&format!("let {} = {}\n", col.name.name, cell_text));
        }
    }
    synthetic.push_str(&format!("__result = ({})\n", condition_str));

    let opts = wcl::ParseOptions::default();
    let doc = wcl::parse(&synthetic, opts);
    if doc.has_errors() {
        let msgs: Vec<String> = doc.errors().iter().map(|d| d.message.clone()).collect();
        return Err(format!("condition evaluation error: {}", msgs.join("; ")));
    }
    match doc.values.get("__result") {
        Some(wcl::Value::Bool(b)) => Ok(*b),
        Some(other) => Err(format!(
            "condition did not evaluate to bool, got: {:?}",
            other
        )),
        None => Err("condition did not produce a result".to_string()),
    }
}

pub fn run_insert(file: &Path, table_name: &str, values: &str) -> Result<(), String> {
    let source = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read {}: {}", file.display(), e))?;

    let file_id = wcl_core::FileId(0);
    let (doc, diags) = wcl_core::parse(&source, file_id);
    if diags.has_errors() {
        for d in diags.diagnostics() {
            if d.is_error() {
                eprintln!("error: {}", d.message);
            }
        }
        return Err(format!("parse errors in {}", file.display()));
    }

    let table =
        find_table(&doc, table_name).ok_or_else(|| format!("table '{}' not found", table_name))?;

    if table.import_expr.is_some() {
        return Err("cannot insert rows into an imported table".to_string());
    }

    // Determine insertion point and indentation
    let (insert_pos, indent) = if let Some(last_row) = table.rows.last() {
        let (_, line_end) = line_span_of(&source, last_row.span);
        // Detect indentation from the last row
        let (line_start, _) = line_span_of(&source, last_row.span);
        let line = &source[line_start..last_row.span.start];
        let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
        (line_end, indent)
    } else if let Some(last_col) = table.columns.last() {
        let (_, line_end) = line_span_of(&source, last_col.span);
        let (line_start, _) = line_span_of(&source, last_col.span);
        let line = &source[line_start..last_col.span.start];
        let indent: String = line.chars().take_while(|c| c.is_whitespace()).collect();
        (line_end, indent)
    } else {
        return Err("table has no columns; cannot determine insertion point".to_string());
    };

    // Format the new row: | val1 | val2 |
    let new_row = format!("{}| {} |\n", indent, values);

    let mut result = String::with_capacity(source.len() + new_row.len());
    result.push_str(&source[..insert_pos]);
    result.push_str(&new_row);
    result.push_str(&source[insert_pos..]);

    // Validate by re-parsing
    let (_, check_diags) = wcl_core::parse(&result, wcl_core::FileId(0));
    if check_diags.has_errors() {
        let msgs: Vec<String> = check_diags
            .diagnostics()
            .iter()
            .filter(|d| d.is_error())
            .map(|d| d.message.clone())
            .collect();
        return Err(format!(
            "inserted row produces parse errors: {}",
            msgs.join("; ")
        ));
    }

    std::fs::write(file, &result).map_err(|e| format!("cannot write {}: {}", file.display(), e))?;
    println!(
        "inserted row into table '{}' in {}",
        table_name,
        file.display()
    );
    Ok(())
}

pub fn run_remove(file: &Path, table_name: &str, condition: &str) -> Result<(), String> {
    let source = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read {}: {}", file.display(), e))?;

    let file_id = wcl_core::FileId(0);
    let (doc, diags) = wcl_core::parse(&source, file_id);
    if diags.has_errors() {
        for d in diags.diagnostics() {
            if d.is_error() {
                eprintln!("error: {}", d.message);
            }
        }
        return Err(format!("parse errors in {}", file.display()));
    }

    let table =
        find_table(&doc, table_name).ok_or_else(|| format!("table '{}' not found", table_name))?;

    if table.columns.is_empty() {
        return Err("table has no columns; cannot evaluate condition".to_string());
    }

    // Find rows matching the condition (collect spans to remove)
    let mut spans_to_remove: Vec<(usize, usize)> = Vec::new();
    let mut match_count = 0;
    for row in &table.rows {
        let matches = eval_row_condition_with_source(&source, condition, &table.columns, row)?;
        if matches {
            spans_to_remove.push(line_span_of(&source, row.span));
            match_count += 1;
        }
    }

    if match_count == 0 {
        println!(
            "no rows matched condition '{}' in table '{}'",
            condition, table_name
        );
        return Ok(());
    }

    // Remove spans backward to preserve offsets
    let mut result = source.clone();
    spans_to_remove.sort_by(|a, b| b.0.cmp(&a.0));
    for (start, end) in &spans_to_remove {
        result = format!("{}{}", &result[..*start], &result[*end..]);
    }

    std::fs::write(file, &result).map_err(|e| format!("cannot write {}: {}", file.display(), e))?;
    println!(
        "removed {} row(s) from table '{}' in {}",
        match_count,
        table_name,
        file.display()
    );
    Ok(())
}

pub fn run_update(
    file: &Path,
    table_name: &str,
    condition: &str,
    set_expr: &str,
) -> Result<(), String> {
    let source = std::fs::read_to_string(file)
        .map_err(|e| format!("cannot read {}: {}", file.display(), e))?;

    let file_id = wcl_core::FileId(0);
    let (doc, diags) = wcl_core::parse(&source, file_id);
    if diags.has_errors() {
        for d in diags.diagnostics() {
            if d.is_error() {
                eprintln!("error: {}", d.message);
            }
        }
        return Err(format!("parse errors in {}", file.display()));
    }

    let table =
        find_table(&doc, table_name).ok_or_else(|| format!("table '{}' not found", table_name))?;

    if table.columns.is_empty() {
        return Err("table has no columns; cannot evaluate condition".to_string());
    }

    // Parse set assignments: "col1 = val1, col2 = val2"
    let assignments: Vec<(&str, &str)> = set_expr
        .split(',')
        .map(|s| {
            let s = s.trim();
            s.split_once('=')
                .map(|(k, v)| (k.trim(), v.trim()))
                .ok_or_else(|| format!("invalid set assignment: '{}'", s))
        })
        .collect::<Result<Vec<_>, _>>()?;

    // Map column names to indices
    let col_indices: Vec<(usize, &str)> = assignments
        .iter()
        .map(|(col_name, val)| {
            let idx = table
                .columns
                .iter()
                .position(|c| c.name.name == *col_name)
                .ok_or_else(|| {
                    format!("column '{}' not found in table '{}'", col_name, table_name)
                })?;
            Ok((idx, *val))
        })
        .collect::<Result<Vec<_>, String>>()?;

    // Collect replacements: (span_start, span_end, new_value) for matching rows
    let mut replacements: Vec<(usize, usize, String)> = Vec::new();
    let mut match_count = 0;

    for row in &table.rows {
        let matches = eval_row_condition_with_source(&source, condition, &table.columns, row)?;
        if matches {
            match_count += 1;
            for &(col_idx, new_val) in &col_indices {
                if col_idx < row.cells.len() {
                    let cell_span = row.cells[col_idx].span();
                    replacements.push((cell_span.start, cell_span.end, new_val.to_string()));
                }
            }
        }
    }

    if match_count == 0 {
        println!(
            "no rows matched condition '{}' in table '{}'",
            condition, table_name
        );
        return Ok(());
    }

    // Apply replacements backward to preserve offsets
    replacements.sort_by(|a, b| b.0.cmp(&a.0));
    let mut result = source.clone();
    for (start, end, new_val) in &replacements {
        result = format!("{}{}{}", &result[..*start], new_val, &result[*end..]);
    }

    std::fs::write(file, &result).map_err(|e| format!("cannot write {}: {}", file.display(), e))?;
    println!(
        "updated {} row(s) in table '{}' in {}",
        match_count,
        table_name,
        file.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write_temp(content: &str) -> tempfile::NamedTempFile {
        let mut f = tempfile::NamedTempFile::new().unwrap();
        f.write_all(content.as_bytes()).unwrap();
        f
    }

    #[test]
    fn test_insert_row() {
        let content = r#"table users {
    name : string
    age  : int
    | "alice" | 25 |
    | "bob"   | 30 |
}
"#;
        let f = write_temp(content);
        run_insert(f.path(), "users", "\"charlie\" | 35").unwrap();
        let result = std::fs::read_to_string(f.path()).unwrap();
        assert!(result.contains("\"charlie\""));
        assert!(result.contains("35"));
        // Verify it still parses
        let (_, diags) = wcl_core::parse(&result, wcl_core::FileId(0));
        assert!(
            !diags.has_errors(),
            "parse errors after insert: {:?}",
            diags.diagnostics()
        );
    }

    #[test]
    fn test_remove_row() {
        let content = r#"table users {
    name : string
    age  : int
    | "alice" | 25 |
    | "bob"   | 30 |
}
"#;
        let f = write_temp(content);
        run_remove(f.path(), "users", "name == \"bob\"").unwrap();
        let result = std::fs::read_to_string(f.path()).unwrap();
        assert!(!result.contains("bob"));
        assert!(result.contains("alice"));
        let (_, diags) = wcl_core::parse(&result, wcl_core::FileId(0));
        assert!(
            !diags.has_errors(),
            "parse errors after remove: {:?}",
            diags.diagnostics()
        );
    }

    #[test]
    fn test_update_row() {
        let content = r#"table users {
    name : string
    age  : int
    | "alice" | 25 |
    | "bob"   | 30 |
}
"#;
        let f = write_temp(content);
        run_update(f.path(), "users", "name == \"alice\"", "age = 26").unwrap();
        let result = std::fs::read_to_string(f.path()).unwrap();
        assert!(result.contains("26"));
        assert!(!result.contains("25"));
        assert!(result.contains("alice"));
        let (_, diags) = wcl_core::parse(&result, wcl_core::FileId(0));
        assert!(
            !diags.has_errors(),
            "parse errors after update: {:?}",
            diags.diagnostics()
        );
    }

    #[test]
    fn test_remove_no_match() {
        let content = r#"table users {
    name : string
    | "alice" |
}
"#;
        let f = write_temp(content);
        // Should succeed but not modify the file
        run_remove(f.path(), "users", "name == \"nobody\"").unwrap();
        let result = std::fs::read_to_string(f.path()).unwrap();
        assert!(result.contains("alice"));
    }

    #[test]
    fn test_insert_into_empty_table() {
        let content = r#"table users {
    name : string
    age  : int
}
"#;
        let f = write_temp(content);
        run_insert(f.path(), "users", "\"alice\" | 25").unwrap();
        let result = std::fs::read_to_string(f.path()).unwrap();
        assert!(result.contains("\"alice\""));
        let (_, diags) = wcl_core::parse(&result, wcl_core::FileId(0));
        assert!(
            !diags.has_errors(),
            "parse errors: {:?}",
            diags.diagnostics()
        );
    }

    #[test]
    fn test_table_not_found() {
        let content = "x = 42\n";
        let f = write_temp(content);
        assert!(run_insert(f.path(), "users", "\"alice\" | 25").is_err());
    }
}
