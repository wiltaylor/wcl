require_relative "test_helper"

class TestVariables < Minitest::Test
  def test_variable_basic
    doc = Wcl.parse("port = PORT", variables: {"PORT" => 8080})
    refute doc.has_errors?, "errors: #{doc.errors.map(&:message)}"
    assert_equal 8080, doc.values["port"]
  end

  def test_variable_overrides_let
    doc = Wcl.parse("let x = 2\nresult = x", variables: {"x" => 99})
    refute doc.has_errors?, "errors: #{doc.errors.map(&:message)}"
    assert_equal 99, doc.values["result"]
  end

  def test_variable_types
    doc = Wcl.parse(
      "vs = s\nvi = i\nvf = f\nvb = b\nvn = n",
      variables: {"s" => "hello", "i" => 42, "f" => 3.14, "b" => true, "n" => nil}
    )
    refute doc.has_errors?, "errors: #{doc.errors.map(&:message)}"
    assert_equal "hello", doc.values["vs"]
    assert_equal 42, doc.values["vi"]
    assert_in_delta 3.14, doc.values["vf"]
    assert_equal true, doc.values["vb"]
    assert_nil doc.values["vn"]
  end

  def test_variable_list
    doc = Wcl.parse("result = items", variables: {"items" => [1, 2, 3]})
    refute doc.has_errors?, "errors: #{doc.errors.map(&:message)}"
    assert_equal [1, 2, 3], doc.values["result"]
  end

  def test_no_variables_backwards_compat
    doc = Wcl.parse("x = 42")
    refute doc.has_errors?
    assert_equal 42, doc.values["x"]
  end
end
