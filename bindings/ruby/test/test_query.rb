require_relative "test_helper"

class TestQuery < Minitest::Test
  def test_block_selection
    doc = Wcl.parse("service { port = 8080 }\nservice { port = 9090 }\ndatabase { port = 5432 }")
    result = doc.query("service")
    assert_kind_of Array, result
    assert_equal 2, result.size
  end

  def test_projection
    doc = Wcl.parse("service { port = 8080 }\nservice { port = 9090 }")
    result = doc.query("service | .port")
    assert_equal [8080, 9090], result
  end

  def test_filter
    doc = Wcl.parse("service { port = 8080 }\nservice { port = 9090 }")
    result = doc.query("service | .port > 8500")
    assert_kind_of Array, result
    assert_equal 1, result.size
  end

  def test_invalid_query_raises
    doc = Wcl.parse("x = 1")
    assert_raises(Wcl::ValueError) do
      doc.query("")
    end
  end

  def test_single_block_query
    doc = Wcl.parse("database { port = 5432 }")
    result = doc.query("database")
    assert_kind_of Array, result
    assert_equal 1, result.size
  end

  def test_no_matching_blocks
    doc = Wcl.parse("server { port = 80 }")
    result = doc.query("database")
    assert_equal [], result
  end

  def test_projection_string
    doc = Wcl.parse("service { name = \"web\" }\nservice { name = \"api\" }")
    result = doc.query("service | .name")
    assert_equal ["web", "api"], result
  end
end
