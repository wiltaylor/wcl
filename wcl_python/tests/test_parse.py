import wcl


def test_simple_attributes():
    doc = wcl.parse("x = 42\ny = \"hello\"")
    assert not doc.has_errors
    assert doc.values["x"] == 42
    assert doc.values["y"] == "hello"


def test_block():
    doc = wcl.parse('server { port = 8080\n host = "localhost" }')
    assert not doc.has_errors
    # Blocks are stored with __block_ prefix in values
    blocks = doc.blocks_of_type("server")
    assert len(blocks) == 1
    assert blocks[0].get("port") == 8080
    assert blocks[0].get("host") == "localhost"


def test_let_binding():
    doc = wcl.parse("let x = 10\nresult = x + 5")
    assert not doc.has_errors
    assert doc.values["result"] == 15


def test_parse_file(tmp_wcl_file):
    path = tmp_wcl_file('name = "from_file"\nport = 3000')
    doc = wcl.parse_file(path)
    assert not doc.has_errors
    assert doc.values["name"] == "from_file"
    assert doc.values["port"] == 3000


def test_invalid_syntax_produces_errors():
    doc = wcl.parse("= = =")
    assert doc.has_errors
    assert len(doc.errors) > 0


def test_parse_options_max_loop_depth():
    doc = wcl.parse(
        "for i in [1,2,3] { entry { v = i } }",
        max_loop_depth=1,
    )
    # Should work fine since nesting depth is 1
    assert not doc.has_errors


def test_parse_file_nonexistent():
    import pytest

    with pytest.raises(IOError):
        wcl.parse_file("/nonexistent/path.wcl")


def test_multiple_blocks():
    doc = wcl.parse(
        'server { port = 80 }\nserver { port = 443 }\nclient { timeout = 30 }'
    )
    assert not doc.has_errors


def test_nested_block():
    doc = wcl.parse("outer { inner { value = 1 } }")
    assert not doc.has_errors
    blocks = doc.blocks_of_type("outer")
    assert len(blocks) == 1
