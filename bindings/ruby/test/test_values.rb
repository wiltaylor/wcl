require_relative "test_helper"

class TestValues < Minitest::Test
  def test_string_value
    doc = Wcl.parse('x = "hello"')
    assert_equal "hello", doc.values["x"]
    assert_kind_of String, doc.values["x"]
  end

  def test_int_value
    doc = Wcl.parse("x = 42")
    assert_equal 42, doc.values["x"]
    assert_kind_of Integer, doc.values["x"]
  end

  def test_float_value
    doc = Wcl.parse("x = 3.14")
    assert_in_delta 3.14, doc.values["x"]
    assert_kind_of Float, doc.values["x"]
  end

  def test_bool_values
    doc = Wcl.parse("a = true\nb = false")
    assert_equal true, doc.values["a"]
    assert_equal false, doc.values["b"]
  end

  def test_null_value
    doc = Wcl.parse("x = null")
    assert_nil doc.values["x"]
  end

  def test_list_value
    doc = Wcl.parse("x = [1, 2, 3]")
    assert_equal [1, 2, 3], doc.values["x"]
    assert_kind_of Array, doc.values["x"]
  end

  def test_nested_list
    doc = Wcl.parse("x = [[1, 2], [3, 4]]")
    assert_equal [[1, 2], [3, 4]], doc.values["x"]
  end

  def test_map_value
    doc = Wcl.parse('x = { a = 1, b = "two" }')
    assert_equal 1, doc.values["x"]["a"]
    assert_equal "two", doc.values["x"]["b"]
    assert_kind_of Hash, doc.values["x"]
  end

  def test_mixed_list
    doc = Wcl.parse('x = [1, "two", true, null]')
    assert_equal [1, "two", true, nil], doc.values["x"]
  end

  def test_empty_list
    doc = Wcl.parse("x = []")
    assert_equal [], doc.values["x"]
  end

  def test_negative_int
    doc = Wcl.parse("x = -42")
    assert_equal(-42, doc.values["x"])
  end

  def test_string_concatenation
    doc = Wcl.parse('x = "hello" + " " + "world"')
    assert_equal "hello world", doc.values["x"]
  end
end
