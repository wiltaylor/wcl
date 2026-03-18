import wcl


def test_missing_required_field():
    source = """
schema "config" {
    port: int
    host: string
}

config {
    port = 8080
}
"""
    doc = wcl.parse(source)
    error_codes = [d.code for d in doc.errors]
    assert "E070" in error_codes, f"expected E070 in {error_codes}"


def test_type_mismatch():
    source = """
schema "config" {
    port: int
}

config {
    port = "not_a_number"
}
"""
    doc = wcl.parse(source)
    error_codes = [d.code for d in doc.errors]
    assert "E071" in error_codes, f"expected E071 in {error_codes}"


def test_valid_schema_no_errors():
    source = """
schema "config" {
    port: int
    host: string
}

config {
    port = 8080
    host = "localhost"
}
"""
    doc = wcl.parse(source)
    schema_errors = [d for d in doc.errors if d.code in ("E070", "E071", "E072")]
    assert len(schema_errors) == 0, f"unexpected schema errors: {schema_errors}"


def test_closed_schema_unknown_field():
    source = """
@closed
schema "strict" {
    name: string
}

strict {
    name = "ok"
    extra = 123
}
"""
    doc = wcl.parse(source)
    error_codes = [d.code for d in doc.errors]
    assert "E072" in error_codes, f"expected E072 in {error_codes}"


def test_constraint_violation():
    source = """
schema "config" {
    port: int @validate(min=1, max=65535)
}

config {
    port = 99999
}
"""
    doc = wcl.parse(source)
    error_codes = [d.code for d in doc.errors]
    assert "E073" in error_codes, f"expected E073 in {error_codes}"
