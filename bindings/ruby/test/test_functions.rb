require_relative "test_helper"

class TestFunctions < Minitest::Test
  def test_int_custom_function
    double = ->(args) { args[0] * 2 }
    doc = Wcl.parse("result = double(21)", functions: { "double" => double })
    refute doc.has_errors?, doc.errors.map(&:message).inspect
    assert_equal 42, doc.values["result"]
  end

  def test_string_custom_function
    greet = ->(args) { "Hello, #{args[0]}!" }
    doc = Wcl.parse('result = greet("World")', functions: { "greet" => greet })
    refute doc.has_errors?, doc.errors.map(&:message).inspect
    assert_equal "Hello, World!", doc.values["result"]
  end

  def test_list_custom_function
    make_list = ->(_args) { [1, 2, 3] }
    doc = Wcl.parse("result = make_list()", functions: { "make_list" => make_list })
    refute doc.has_errors?, doc.errors.map(&:message).inspect
    assert_equal [1, 2, 3], doc.values["result"]
  end

  def test_custom_function_error_handling
    bad_fn = ->(_args) { raise "something went wrong" }
    doc = Wcl.parse("result = bad_fn()", functions: { "bad_fn" => bad_fn })
    assert doc.has_errors?
  end

  def test_multiple_functions
    add = ->(args) { args[0] + args[1] }
    mul = ->(args) { args[0] * args[1] }
    doc = Wcl.parse(
      "a = add(1, 2)\nb = mul(3, 4)",
      functions: { "add" => add, "mul" => mul }
    )
    refute doc.has_errors?, doc.errors.map(&:message).inspect
    assert_equal 3, doc.values["a"]
    assert_equal 12, doc.values["b"]
  end

  def test_custom_function_in_control_flow
    items = ->(_args) { [1, 2, 3] }
    doc = Wcl.parse(
      "for item in items() { entry { value = item } }",
      functions: { "items" => items }
    )
    refute doc.has_errors?, doc.errors.map(&:message).inspect
  end

  def test_function_returning_none
    noop = ->(_args) { nil }
    doc = Wcl.parse("result = noop()", functions: { "noop" => noop })
    refute doc.has_errors?, doc.errors.map(&:message).inspect
    assert_nil doc.values["result"]
  end

  def test_function_returning_bool
    is_even = ->(args) { args[0] % 2 == 0 }
    doc = Wcl.parse("result = is_even(4)", functions: { "is_even" => is_even })
    refute doc.has_errors?, doc.errors.map(&:message).inspect
    assert_equal true, doc.values["result"]
  end
end
