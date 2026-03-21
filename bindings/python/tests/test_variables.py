import wcl


def test_variable_basic():
    doc = wcl.parse("port = PORT", variables={"PORT": 8080})
    assert not doc.has_errors, f"errors: {[e.message for e in doc.errors]}"
    assert doc.values["port"] == 8080


def test_variable_overrides_let():
    doc = wcl.parse("let x = 2\nresult = x", variables={"x": 99})
    assert not doc.has_errors, f"errors: {[e.message for e in doc.errors]}"
    assert doc.values["result"] == 99


def test_variable_types():
    doc = wcl.parse(
        "vs = s\nvi = i\nvf = f\nvb = b\nvn = n",
        variables={"s": "hello", "i": 42, "f": 3.14, "b": True, "n": None},
    )
    assert not doc.has_errors, f"errors: {[e.message for e in doc.errors]}"
    assert doc.values["vs"] == "hello"
    assert doc.values["vi"] == 42
    assert doc.values["vf"] == 3.14
    assert doc.values["vb"] is True
    assert doc.values["vn"] is None


def test_variable_list():
    doc = wcl.parse("result = items", variables={"items": [1, 2, 3]})
    assert not doc.has_errors, f"errors: {[e.message for e in doc.errors]}"
    assert doc.values["result"] == [1, 2, 3]


def test_variable_no_variables_backwards_compat():
    doc = wcl.parse("x = 42")
    assert not doc.has_errors
    assert doc.values["x"] == 42
