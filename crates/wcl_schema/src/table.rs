//! Table column type and decorator validation (Spec Section 18.7).
//!
//! Validates that each cell in a table row matches the declared column type,
//! and enforces any `@validate` constraints on column decorators.

use wcl_core::ast::*;
use wcl_core::diagnostic::DiagnosticBag;

use crate::schema::{
    expr_to_value, get_validate_constraints, string_lit_to_string, validate_constraints,
    value_type_label,
};
use crate::types::{check_type, type_name};

/// Validate all tables in the document: check cell types and column decorator constraints.
pub fn validate_tables(doc: &Document, diagnostics: &mut DiagnosticBag) {
    validate_items(&doc.items, diagnostics);
}

fn validate_items(items: &[DocItem], diagnostics: &mut DiagnosticBag) {
    for item in items {
        match item {
            DocItem::Body(BodyItem::Table(table)) => validate_table(table, diagnostics),
            DocItem::Body(BodyItem::Block(block)) => {
                validate_body_items(&block.body, diagnostics);
            }
            _ => {}
        }
    }
}

fn validate_body_items(items: &[BodyItem], diagnostics: &mut DiagnosticBag) {
    for item in items {
        match item {
            BodyItem::Table(table) => validate_table(table, diagnostics),
            BodyItem::Block(block) => validate_body_items(&block.body, diagnostics),
            _ => {}
        }
    }
}

fn validate_table(table: &Table, diagnostics: &mut DiagnosticBag) {
    let col_count = table.columns.len();

    for (row_idx, row) in table.rows.iter().enumerate() {
        // Check cell count matches column count
        if row.cells.len() != col_count {
            diagnostics.error(
                format!(
                    "table row {} has {} cells but {} columns are declared",
                    row_idx + 1,
                    row.cells.len(),
                    col_count
                ),
                row.span,
            );
            continue;
        }

        // Validate each cell against its column
        for (cell_expr, col) in row.cells.iter().zip(table.columns.iter()) {
            // Try to resolve the cell expression to a value
            if let Some(val) = expr_to_value(cell_expr) {
                // Type check against column type
                if !check_type(&val, &col.type_expr) {
                    diagnostics.error_with_code(
                        format!(
                            "type mismatch in table column '{}' row {}: expected {}, got {}",
                            col.name.name,
                            row_idx + 1,
                            type_name(&col.type_expr),
                            value_type_label(&val),
                        ),
                        cell_expr.span(),
                        "E071",
                    );
                }

                // Apply @validate constraints from column decorators
                if let Some(constraints) = get_validate_constraints(&col.decorators) {
                    validate_constraints(
                        &val,
                        &constraints,
                        &format!("{}[row {}]", col.name.name, row_idx + 1),
                        cell_expr.span(),
                        diagnostics,
                    );
                }
            }
        }
    }

    // Validate @table_index decorators
    validate_table_index(table, diagnostics);
}

/// Extract string values from a list expression (e.g. `["a", "b"]`).
fn extract_string_list(expr: &Expr) -> Option<Vec<String>> {
    match expr {
        Expr::List(items, _) => {
            let mut result = Vec::new();
            for item in items {
                match item {
                    Expr::StringLit(s) => result.push(string_lit_to_string(s)),
                    _ => return None,
                }
            }
            Some(result)
        }
        _ => None,
    }
}

/// Extract a bool from an expression.
fn extract_bool(expr: &Expr) -> Option<bool> {
    match expr {
        Expr::BoolLit(b, _) => Some(*b),
        _ => None,
    }
}

/// Validate `@table_index` decorators on a table.
///
/// Checks that:
/// 1. Each column name in `columns` matches an actual column declaration (E090).
/// 2. If `unique=true`, indexed column values are unique across rows (E091).
fn validate_table_index(table: &Table, diagnostics: &mut DiagnosticBag) {
    for decorator in &table.decorators {
        if decorator.name.name != "table_index" {
            continue;
        }

        // Extract `columns` parameter
        let columns_arg = decorator
            .args
            .iter()
            .find_map(|arg| match arg {
                DecoratorArg::Named(ident, expr) if ident.name == "columns" => Some(expr),
                _ => None,
            })
            .or_else(|| {
                // Fall back to positional arg at index 0
                decorator.args.first().and_then(|arg| match arg {
                    DecoratorArg::Positional(expr) => Some(expr),
                    _ => None,
                })
            });

        let column_names = match columns_arg.and_then(extract_string_list) {
            Some(names) => names,
            None => continue, // Can't extract columns; skip validation
        };

        // Extract `unique` parameter (default false)
        let unique = decorator
            .args
            .iter()
            .find_map(|arg| match arg {
                DecoratorArg::Named(ident, expr) if ident.name == "unique" => extract_bool(expr),
                _ => None,
            })
            .unwrap_or(false);

        // 1. Verify each column name exists in the table
        let table_col_names: Vec<&str> =
            table.columns.iter().map(|c| c.name.name.as_str()).collect();
        let mut valid_col_indices: Vec<usize> = Vec::new();
        let mut all_valid = true;

        for col_name in &column_names {
            if let Some(idx) = table_col_names.iter().position(|&n| n == col_name) {
                valid_col_indices.push(idx);
            } else {
                diagnostics.error_with_code(
                    format!(
                        "@table_index references column '{}' which does not exist in the table",
                        col_name
                    ),
                    decorator.span,
                    "E090",
                );
                all_valid = false;
            }
        }

        // 2. If unique=true and all columns are valid, check uniqueness
        if unique && all_valid && !valid_col_indices.is_empty() {
            let mut seen: std::collections::HashSet<Vec<String>> = std::collections::HashSet::new();

            for (row_idx, row) in table.rows.iter().enumerate() {
                if row.cells.len() != table.columns.len() {
                    continue; // Skip rows with wrong cell count
                }

                let key: Vec<String> = valid_col_indices
                    .iter()
                    .map(|&col_idx| {
                        expr_to_value(&row.cells[col_idx])
                            .map(|v| format!("{:?}", v))
                            .unwrap_or_else(|| "<expr>".to_string())
                    })
                    .collect();

                if !seen.insert(key) {
                    let col_desc = column_names.join(", ");
                    diagnostics.error_with_code(
                        format!(
                            "duplicate value in @table_index(unique=true) for column(s) [{}] at row {}",
                            col_desc,
                            row_idx + 1,
                        ),
                        row.span,
                        "E091",
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wcl_core::span::Span;
    use wcl_core::trivia::Trivia;

    fn ds() -> Span {
        Span::dummy()
    }

    fn mk_ident(name: &str) -> Ident {
        Ident {
            name: name.to_string(),
            span: ds(),
        }
    }

    fn mk_column(name: &str, type_expr: TypeExpr) -> ColumnDecl {
        ColumnDecl {
            decorators: vec![],
            name: mk_ident(name),
            type_expr,
            trivia: Trivia::empty(),
            span: ds(),
        }
    }

    fn mk_table(columns: Vec<ColumnDecl>, rows: Vec<TableRow>) -> Table {
        Table {
            decorators: vec![],
            partial: false,
            inline_id: None,
            schema_ref: None,
            columns,
            rows,
            import_expr: None,
            trivia: Trivia::empty(),
            span: ds(),
        }
    }

    fn mk_row(cells: Vec<Expr>) -> TableRow {
        TableRow { cells, span: ds() }
    }

    fn mk_doc_with_table(table: Table) -> Document {
        Document {
            items: vec![DocItem::Body(BodyItem::Table(table))],
            trivia: Trivia::empty(),
            span: ds(),
        }
    }

    fn mk_string_expr(s: &str) -> Expr {
        Expr::StringLit(StringLit {
            parts: vec![StringPart::Literal(s.into())],
            span: ds(),
        })
    }

    #[test]
    fn valid_table_passes() {
        let table = mk_table(
            vec![
                mk_column("name", TypeExpr::String(ds())),
                mk_column("port", TypeExpr::Int(ds())),
            ],
            vec![mk_row(vec![
                mk_string_expr("web"),
                Expr::IntLit(8080, ds()),
            ])],
        );
        let doc = mk_doc_with_table(table);
        let mut diags = DiagnosticBag::new();
        validate_tables(&doc, &mut diags);
        assert!(!diags.has_errors());
    }

    #[test]
    fn type_mismatch_in_table_cell() {
        let table = mk_table(
            vec![mk_column("port", TypeExpr::Int(ds()))],
            vec![mk_row(vec![mk_string_expr("not_a_number")])],
        );
        let doc = mk_doc_with_table(table);
        let mut diags = DiagnosticBag::new();
        validate_tables(&doc, &mut diags);
        assert!(diags.has_errors());
        assert_eq!(diags.error_count(), 1);
    }

    #[test]
    fn wrong_cell_count_errors() {
        let table = mk_table(
            vec![
                mk_column("a", TypeExpr::Int(ds())),
                mk_column("b", TypeExpr::Int(ds())),
            ],
            vec![mk_row(vec![Expr::IntLit(1, ds())])], // only 1 cell for 2 columns
        );
        let doc = mk_doc_with_table(table);
        let mut diags = DiagnosticBag::new();
        validate_tables(&doc, &mut diags);
        assert!(diags.has_errors());
    }

    #[test]
    fn column_validate_constraint_enforced() {
        let mut col = mk_column("port", TypeExpr::Int(ds()));
        col.decorators.push(Decorator {
            name: mk_ident("validate"),
            args: vec![
                DecoratorArg::Named(mk_ident("min"), Expr::IntLit(1, ds())),
                DecoratorArg::Named(mk_ident("max"), Expr::IntLit(65535, ds())),
            ],
            span: ds(),
        });
        let table = mk_table(
            vec![col],
            vec![
                mk_row(vec![Expr::IntLit(0, ds())]),     // below min
                mk_row(vec![Expr::IntLit(8080, ds())]),  // valid
                mk_row(vec![Expr::IntLit(70000, ds())]), // above max
            ],
        );
        let doc = mk_doc_with_table(table);
        let mut diags = DiagnosticBag::new();
        validate_tables(&doc, &mut diags);
        assert_eq!(diags.error_count(), 2); // row 1 below min, row 3 above max
    }

    #[test]
    fn nested_table_in_block_validated() {
        let table = mk_table(
            vec![mk_column("x", TypeExpr::Int(ds()))],
            vec![mk_row(vec![mk_string_expr("wrong")])],
        );
        let block = Block {
            decorators: vec![],
            partial: false,
            kind: mk_ident("outer"),
            inline_id: None,
            labels: vec![],
            body: vec![BodyItem::Table(table)],
            trivia: Trivia::empty(),
            span: ds(),
        };
        let doc = Document {
            items: vec![DocItem::Body(BodyItem::Block(block))],
            trivia: Trivia::empty(),
            span: ds(),
        };
        let mut diags = DiagnosticBag::new();
        validate_tables(&doc, &mut diags);
        assert!(diags.has_errors());
        assert_eq!(diags.error_count(), 1);
    }

    #[test]
    fn empty_table_passes() {
        let table = mk_table(vec![], vec![]);
        let doc = mk_doc_with_table(table);
        let mut diags = DiagnosticBag::new();
        validate_tables(&doc, &mut diags);
        assert!(!diags.has_errors());
    }

    fn mk_table_with_decorators(
        decorators: Vec<Decorator>,
        columns: Vec<ColumnDecl>,
        rows: Vec<TableRow>,
    ) -> Table {
        Table {
            decorators,
            partial: false,
            inline_id: None,
            schema_ref: None,
            columns,
            rows,
            import_expr: None,
            trivia: Trivia::empty(),
            span: ds(),
        }
    }

    fn mk_table_index_decorator(columns: Vec<&str>, unique: bool) -> Decorator {
        let mut args = vec![DecoratorArg::Named(
            mk_ident("columns"),
            Expr::List(columns.into_iter().map(mk_string_expr).collect(), ds()),
        )];
        if unique {
            args.push(DecoratorArg::Named(
                mk_ident("unique"),
                Expr::BoolLit(true, ds()),
            ));
        }
        Decorator {
            name: mk_ident("table_index"),
            args,
            span: ds(),
        }
    }

    #[test]
    fn table_index_valid_column_passes() {
        let table = mk_table_with_decorators(
            vec![mk_table_index_decorator(vec!["name"], false)],
            vec![
                mk_column("name", TypeExpr::String(ds())),
                mk_column("port", TypeExpr::Int(ds())),
            ],
            vec![mk_row(vec![
                mk_string_expr("web"),
                Expr::IntLit(8080, ds()),
            ])],
        );
        let doc = mk_doc_with_table(table);
        let mut diags = DiagnosticBag::new();
        validate_tables(&doc, &mut diags);
        assert!(!diags.has_errors());
    }

    #[test]
    fn table_index_missing_column_errors() {
        let table = mk_table_with_decorators(
            vec![mk_table_index_decorator(vec!["missing"], false)],
            vec![mk_column("name", TypeExpr::String(ds()))],
            vec![mk_row(vec![mk_string_expr("web")])],
        );
        let doc = mk_doc_with_table(table);
        let mut diags = DiagnosticBag::new();
        validate_tables(&doc, &mut diags);
        assert!(diags.has_errors());
        assert_eq!(diags.error_count(), 1);
    }

    #[test]
    fn table_index_unique_with_unique_values_passes() {
        let table = mk_table_with_decorators(
            vec![mk_table_index_decorator(vec!["name"], true)],
            vec![mk_column("name", TypeExpr::String(ds()))],
            vec![
                mk_row(vec![mk_string_expr("alice")]),
                mk_row(vec![mk_string_expr("bob")]),
                mk_row(vec![mk_string_expr("carol")]),
            ],
        );
        let doc = mk_doc_with_table(table);
        let mut diags = DiagnosticBag::new();
        validate_tables(&doc, &mut diags);
        assert!(!diags.has_errors());
    }

    #[test]
    fn table_index_unique_with_duplicates_errors() {
        let table = mk_table_with_decorators(
            vec![mk_table_index_decorator(vec!["name"], true)],
            vec![mk_column("name", TypeExpr::String(ds()))],
            vec![
                mk_row(vec![mk_string_expr("alice")]),
                mk_row(vec![mk_string_expr("bob")]),
                mk_row(vec![mk_string_expr("alice")]), // duplicate
            ],
        );
        let doc = mk_doc_with_table(table);
        let mut diags = DiagnosticBag::new();
        validate_tables(&doc, &mut diags);
        assert!(diags.has_errors());
        assert_eq!(diags.error_count(), 1);
    }

    #[test]
    fn table_index_multi_column_unique() {
        let table = mk_table_with_decorators(
            vec![mk_table_index_decorator(vec!["host", "port"], true)],
            vec![
                mk_column("host", TypeExpr::String(ds())),
                mk_column("port", TypeExpr::Int(ds())),
            ],
            vec![
                mk_row(vec![mk_string_expr("web"), Expr::IntLit(80, ds())]),
                mk_row(vec![mk_string_expr("web"), Expr::IntLit(443, ds())]), // same host, different port — OK
                mk_row(vec![mk_string_expr("api"), Expr::IntLit(80, ds())]), // different host, same port — OK
                mk_row(vec![mk_string_expr("web"), Expr::IntLit(80, ds())]), // duplicate tuple
            ],
        );
        let doc = mk_doc_with_table(table);
        let mut diags = DiagnosticBag::new();
        validate_tables(&doc, &mut diags);
        assert!(diags.has_errors());
        assert_eq!(diags.error_count(), 1);
    }

    #[test]
    fn any_type_accepts_all_cells() {
        let table = mk_table(
            vec![mk_column("val", TypeExpr::Any(ds()))],
            vec![
                mk_row(vec![Expr::IntLit(42, ds())]),
                mk_row(vec![mk_string_expr("hello")]),
                mk_row(vec![Expr::BoolLit(true, ds())]),
            ],
        );
        let doc = mk_doc_with_table(table);
        let mut diags = DiagnosticBag::new();
        validate_tables(&doc, &mut diags);
        assert!(!diags.has_errors());
    }
}
