import pytest

import wcl


def test_block_selection():
    doc = wcl.parse("service { port = 8080 }\nservice { port = 9090 }\ndatabase { port = 5432 }")
    result = doc.query("service")
    assert isinstance(result, list)
    assert len(result) == 2


def test_projection():
    doc = wcl.parse("service { port = 8080 }\nservice { port = 9090 }")
    result = doc.query("service | .port")
    assert result == [8080, 9090]


def test_filter():
    doc = wcl.parse("service { port = 8080 }\nservice { port = 9090 }")
    result = doc.query("service | .port > 8500")
    assert isinstance(result, list)
    assert len(result) == 1


def test_invalid_query_raises():
    doc = wcl.parse("x = 1")
    with pytest.raises(ValueError):
        doc.query("")


def test_single_block_query():
    doc = wcl.parse("database { port = 5432 }")
    result = doc.query("database")
    assert isinstance(result, list)
    assert len(result) == 1


def test_no_matching_blocks():
    doc = wcl.parse("server { port = 80 }")
    result = doc.query("database")
    assert result == []


def test_projection_string():
    doc = wcl.parse('service { name = "web" }\nservice { name = "api" }')
    result = doc.query("service | .name")
    assert result == ["web", "api"]
