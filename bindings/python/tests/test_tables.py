import wcl


def test_table_inline_columns():
    doc = wcl.parse("""
        table users {
            name : string
            age  : i64
            | "Alice" | 30 |
            | "Bob"   | 25 |
        }
    """)
    assert not doc.has_errors, f"errors: {doc.errors}"


def test_table_schema_ref_colon():
    doc = wcl.parse("""
        table users : user_row {
            | "Alice" | 30 |
        }
    """)
    # Should parse without E002 errors (schema may not exist but parsing succeeds)
    parse_errors = [e for e in doc.errors if "E002" in str(e)]
    assert len(parse_errors) == 0, f"parse errors: {parse_errors}"


def test_table_schema_decorator():
    doc = wcl.parse("""
        @schema("user_row")
        table users {
            | "Alice" | 30 |
        }
    """)
    parse_errors = [e for e in doc.errors if "E002" in str(e)]
    assert len(parse_errors) == 0, f"parse errors: {parse_errors}"


def test_table_schema_ref_plus_inline_columns_e092():
    doc = wcl.parse("""
        table users : user_row {
            name : string
            | "Alice" |
        }
    """)
    e092_errors = [e for e in doc.errors if "E092" in str(e)]
    assert len(e092_errors) == 1, f"expected E092, got: {doc.errors}"


def test_table_type_mismatch_e071():
    doc = wcl.parse("""
        table users {
            name : string
            port : i64
            | "web" | "bad" |
        }
    """)
    e071_errors = [e for e in doc.errors if "E071" in str(e)]
    assert len(e071_errors) > 0, f"expected E071, got: {doc.errors}"


def test_import_table_csv_syntax():
    """Verify import_table with assignment syntax parses correctly."""
    doc = wcl.parse("""
        table users = import_table("data.csv")
    """)
    # Will error on file not found, but should NOT have parse errors
    parse_errors = [e for e in doc.errors if "E002" in str(e)]
    assert len(parse_errors) == 0, f"parse errors: {parse_errors}"


def test_import_table_named_args_parse():
    """Verify import_table with named args parses without errors."""
    doc = wcl.parse("""
        val = import_table("data.csv", headers=false, columns=["a", "b"])
    """)
    # Will error on file not found, but should NOT have parse errors
    parse_errors = [e for e in doc.errors if "E002" in str(e)]
    assert len(parse_errors) == 0, f"parse errors: {parse_errors}"
