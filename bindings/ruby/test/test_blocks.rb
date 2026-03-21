require_relative "test_helper"

class TestBlocks < Minitest::Test
  def test_blocks
    doc = Wcl.parse("server { port = 80 }\nclient { timeout = 30 }")
    blocks = doc.blocks
    assert_equal 2, blocks.size
    kinds = blocks.map(&:kind).to_set
    assert_equal Set.new(["server", "client"]), kinds
  end

  def test_blocks_of_type
    doc = Wcl.parse("server { port = 80 }\nclient { timeout = 30 }\nserver { port = 443 }")
    servers = doc.blocks_of_type("server")
    assert_equal 2, servers.size
    servers.each { |s| assert_equal "server", s.kind }
  end

  def test_blocks_of_type_empty
    doc = Wcl.parse("server { port = 80 }")
    clients = doc.blocks_of_type("client")
    assert_equal [], clients
  end

  def test_blockref_attributes
    doc = Wcl.parse("server { port = 8080\n host = \"localhost\" }")
    blocks = doc.blocks
    assert_equal 1, blocks.size
    attrs = blocks[0].attributes
    assert_equal 8080, attrs["port"]
    assert_equal "localhost", attrs["host"]
  end

  def test_blockref_get
    doc = Wcl.parse("server { port = 8080 }")
    block = doc.blocks[0]
    assert_equal 8080, block.get("port")
    assert_nil block.get("missing")
  end

  def test_blockref_id
    doc = Wcl.parse("server main { port = 8080 }")
    block = doc.blocks[0]
    assert_equal "main", block.id
  end

  def test_blockref_no_id
    doc = Wcl.parse("server { port = 8080 }")
    block = doc.blocks[0]
    assert_nil block.id
  end

  def test_blockref_children
    doc = Wcl.parse("outer { inner { value = 1 } }")
    block = doc.blocks[0]
    children = block.children
    assert_equal 1, children.size
    assert_equal "inner", children[0].kind
    assert_equal 1, children[0].get("value")
  end

  def test_blockref_decorators
    doc = Wcl.parse("@deprecated(\"use v2\")\nserver main { port = 8080 }")
    block = doc.blocks[0]
    decorators = block.decorators
    assert_equal 1, decorators.size
    assert_equal "deprecated", decorators[0].name
  end

  def test_blockref_has_decorator
    doc = Wcl.parse("@deprecated(\"use v2\")\nserver main { port = 8080 }")
    block = doc.blocks[0]
    assert block.has_decorator?("deprecated")
    refute block.has_decorator?("nonexistent")
  end

  def test_blockref_inspect
    doc = Wcl.parse("server main { port = 80 }")
    block = doc.blocks[0]
    assert_includes block.inspect, "BlockRef"
    assert_includes block.inspect, "server"
    assert_includes block.inspect, "main"
  end

  def test_decorator_args
    doc = Wcl.parse("@deprecated(\"use v2\")\nserver main { port = 80 }")
    block = doc.blocks[0]
    dec = block.decorators[0]
    assert_equal "deprecated", dec.name
    assert_kind_of Hash, dec.args
  end

  def test_decorator_inspect
    doc = Wcl.parse("@deprecated\nserver { port = 80 }")
    block = doc.blocks[0]
    dec = block.decorators[0]
    assert_includes dec.inspect, "Decorator(@deprecated)"
  end
end
