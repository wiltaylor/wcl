import wcl


def test_string_value():
    doc = wcl.parse('x = "hello"')
    assert doc.values["x"] == "hello"
    assert isinstance(doc.values["x"], str)


def test_int_value():
    doc = wcl.parse("x = 42")
    assert doc.values["x"] == 42
    assert isinstance(doc.values["x"], int)


def test_float_value():
    doc = wcl.parse("x = 3.14")
    assert doc.values["x"] == 3.14
    assert isinstance(doc.values["x"], float)


def test_bool_values():
    doc = wcl.parse("a = true\nb = false")
    assert doc.values["a"] is True
    assert doc.values["b"] is False


def test_null_value():
    doc = wcl.parse("x = null")
    assert doc.values["x"] is None


def test_list_value():
    doc = wcl.parse("x = [1, 2, 3]")
    assert doc.values["x"] == [1, 2, 3]
    assert isinstance(doc.values["x"], list)


def test_nested_list():
    doc = wcl.parse("x = [[1, 2], [3, 4]]")
    assert doc.values["x"] == [[1, 2], [3, 4]]


def test_map_value():
    doc = wcl.parse('x = { a = 1, b = "two" }')
    assert doc.values["x"]["a"] == 1
    assert doc.values["x"]["b"] == "two"
    assert isinstance(doc.values["x"], dict)


def test_mixed_list():
    doc = wcl.parse('x = [1, "two", true, null]')
    assert doc.values["x"] == [1, "two", True, None]


def test_empty_list():
    doc = wcl.parse("x = []")
    assert doc.values["x"] == []


def test_negative_int():
    doc = wcl.parse("x = -42")
    assert doc.values["x"] == -42


def test_string_concatenation():
    doc = wcl.parse('x = "hello" + " " + "world"')
    assert doc.values["x"] == "hello world"
