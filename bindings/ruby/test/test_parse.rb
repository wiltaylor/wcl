require_relative "test_helper"

class TestParse < Minitest::Test
  include WclTestHelper

  def test_simple_attributes
    doc = Wcl.parse("x = 42\ny = \"hello\"")
    refute doc.has_errors?
    assert_equal 42, doc.values["x"]
    assert_equal "hello", doc.values["y"]
  end

  def test_block
    doc = Wcl.parse("server { port = 8080\n host = \"localhost\" }")
    refute doc.has_errors?
    blocks = doc.blocks_of_type("server")
    assert_equal 1, blocks.size
    assert_equal 8080, blocks[0].get("port")
    assert_equal "localhost", blocks[0].get("host")
  end

  def test_let_binding
    doc = Wcl.parse("let x = 10\nresult = x + 5")
    refute doc.has_errors?
    assert_equal 15, doc.values["result"]
  end

  def test_parse_file
    path = create_tmp_wcl("name = \"from_file\"\nport = 3000")
    doc = Wcl.parse_file(path)
    refute doc.has_errors?
    assert_equal "from_file", doc.values["name"]
    assert_equal 3000, doc.values["port"]
  ensure
    FileUtils.rm_rf(File.dirname(path)) if path
  end

  def test_invalid_syntax_produces_errors
    doc = Wcl.parse("= = =")
    assert doc.has_errors?
    assert doc.errors.size > 0
  end

  def test_parse_options_max_loop_depth
    doc = Wcl.parse(
      "for i in [1,2,3] { entry { v = i } }",
      max_loop_depth: 1
    )
    refute doc.has_errors?
  end

  def test_parse_file_nonexistent
    assert_raises(IOError) do
      Wcl.parse_file("/nonexistent/path.wcl")
    end
  end

  def test_multiple_blocks
    doc = Wcl.parse("server { port = 80 }\nserver { port = 443 }\nclient { timeout = 30 }")
    refute doc.has_errors?
  end

  def test_nested_block
    doc = Wcl.parse("outer { inner { value = 1 } }")
    refute doc.has_errors?
    blocks = doc.blocks_of_type("outer")
    assert_equal 1, blocks.size
  end
end
