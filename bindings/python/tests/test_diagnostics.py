import wcl


def test_severity_values():
    doc = wcl.parse("= invalid")
    assert len(doc.diagnostics) > 0
    for d in doc.diagnostics:
        assert d.severity in ("error", "warning", "info", "hint")


def test_error_has_message():
    doc = wcl.parse("= = =")
    assert doc.has_errors
    for e in doc.errors:
        assert isinstance(e.message, str)
        assert len(e.message) > 0


def test_valid_doc_no_errors():
    doc = wcl.parse("x = 42")
    assert not doc.has_errors
    assert len(doc.errors) == 0


def test_diagnostic_code():
    source = """
schema "cfg" { port: i64 }
cfg { port = "bad" }
"""
    doc = wcl.parse(source)
    coded = [d for d in doc.diagnostics if d.code is not None]
    assert len(coded) > 0


def test_diagnostic_repr():
    doc = wcl.parse("= = =")
    for d in doc.diagnostics:
        r = repr(d)
        assert "Diagnostic(" in r


def test_errors_only_subset():
    # Warnings should not appear in errors
    source = """
@warning
validation "soft check" {
    let x = -1
    check = x > 0
    message = "x not positive"
}
"""
    doc = wcl.parse(source)
    # The warning validation should produce a warning, not an error
    for e in doc.errors:
        assert e.severity == "error"


def test_diagnostics_include_warnings():
    source = """
@warning
validation "soft check" {
    let x = -1
    check = x > 0
    message = "x not positive"
}
"""
    doc = wcl.parse(source)
    warnings = [d for d in doc.diagnostics if d.severity == "warning"]
    assert len(warnings) > 0
