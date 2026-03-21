import wcl


def test_blocks():
    doc = wcl.parse("server { port = 80 }\nclient { timeout = 30 }")
    blocks = doc.blocks()
    assert len(blocks) == 2
    kinds = {b.kind for b in blocks}
    assert kinds == {"server", "client"}


def test_blocks_of_type():
    doc = wcl.parse("server { port = 80 }\nclient { timeout = 30 }\nserver { port = 443 }")
    servers = doc.blocks_of_type("server")
    assert len(servers) == 2
    for s in servers:
        assert s.kind == "server"


def test_blocks_of_type_empty():
    doc = wcl.parse("server { port = 80 }")
    clients = doc.blocks_of_type("client")
    assert clients == []


def test_blockref_attributes():
    doc = wcl.parse('server { port = 8080\n host = "localhost" }')
    blocks = doc.blocks()
    assert len(blocks) == 1
    attrs = blocks[0].attributes
    assert attrs["port"] == 8080
    assert attrs["host"] == "localhost"


def test_blockref_get():
    doc = wcl.parse("server { port = 8080 }")
    block = doc.blocks()[0]
    assert block.get("port") == 8080
    assert block.get("missing") is None


def test_blockref_id():
    doc = wcl.parse("server main { port = 8080 }")
    block = doc.blocks()[0]
    assert block.id == "main"


def test_blockref_no_id():
    doc = wcl.parse("server { port = 8080 }")
    block = doc.blocks()[0]
    assert block.id is None



def test_blockref_children():
    doc = wcl.parse("outer { inner { value = 1 } }")
    block = doc.blocks()[0]
    children = block.children
    assert len(children) == 1
    assert children[0].kind == "inner"
    assert children[0].get("value") == 1


def test_blockref_decorators():
    doc = wcl.parse('@deprecated("use v2")\nserver main { port = 8080 }')
    block = doc.blocks()[0]
    decorators = block.decorators
    assert len(decorators) == 1
    assert decorators[0].name == "deprecated"


def test_blockref_has_decorator():
    doc = wcl.parse('@deprecated("use v2")\nserver main { port = 8080 }')
    block = doc.blocks()[0]
    assert block.has_decorator("deprecated")
    assert not block.has_decorator("nonexistent")


def test_blockref_repr():
    doc = wcl.parse("server main { port = 80 }")
    block = doc.blocks()[0]
    assert "BlockRef" in repr(block)
    assert "server" in repr(block)
    assert "main" in repr(block)


def test_decorator_args():
    doc = wcl.parse('@deprecated("use v2")\nserver main { port = 80 }')
    block = doc.blocks()[0]
    dec = block.decorators[0]
    assert dec.name == "deprecated"
    args = dec.args
    assert isinstance(args, dict)


def test_decorator_repr():
    doc = wcl.parse("@deprecated\nserver { port = 80 }")
    block = doc.blocks()[0]
    dec = block.decorators[0]
    assert "Decorator(@deprecated)" in repr(dec)
