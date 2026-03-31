import wcl


def test_missing_required_field():
    source = """
schema "config" {
    port: i64
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
    port: i64
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
    port: i64
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
    port: i64 @validate(min=1, max=65535)
}

config {
    port = 99999
}
"""
    doc = wcl.parse(source)
    error_codes = [d.code for d in doc.errors]
    assert "E073" in error_codes, f"expected E073 in {error_codes}"


def test_children_constraint_allows_valid():
    source = """
@children(["endpoint"])
schema "service" {}

service main {
    endpoint health {}
}
"""
    doc = wcl.parse(source)
    containment_errors = [d for d in doc.errors if d.code in ("E095", "E096")]
    assert len(containment_errors) == 0, f"unexpected containment errors: {containment_errors}"


def test_children_constraint_rejects_invalid():
    source = """
@children(["endpoint"])
schema "service" {}

service main {
    middleware auth {}
}
"""
    doc = wcl.parse(source)
    error_codes = [d.code for d in doc.errors]
    assert "E095" in error_codes, f"expected E095 in {error_codes}"


def test_parent_constraint_rejects_at_root():
    source = """
@parent(["service"])
schema "endpoint" {}

endpoint orphan {}
"""
    doc = wcl.parse(source)
    error_codes = [d.code for d in doc.errors]
    assert "E096" in error_codes, f"expected E096 in {error_codes}"


def test_parent_constraint_allows_valid():
    source = """
@parent(["service"])
schema "endpoint" {}

service main {
    endpoint health {}
}
"""
    doc = wcl.parse(source)
    containment_errors = [d for d in doc.errors if d.code in ("E095", "E096")]
    assert len(containment_errors) == 0, f"unexpected containment errors: {containment_errors}"
