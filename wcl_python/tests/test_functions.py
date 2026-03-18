import wcl


def test_int_custom_function():
    def double(args):
        return args[0] * 2

    doc = wcl.parse("result = double(21)", functions={"double": double})
    assert not doc.has_errors, [d.message for d in doc.errors]
    assert doc.values["result"] == 42


def test_string_custom_function():
    def greet(args):
        return f"Hello, {args[0]}!"

    doc = wcl.parse('result = greet("World")', functions={"greet": greet})
    assert not doc.has_errors, [d.message for d in doc.errors]
    assert doc.values["result"] == "Hello, World!"


def test_list_custom_function():
    def make_list(args):
        return [1, 2, 3]

    doc = wcl.parse("result = make_list()", functions={"make_list": make_list})
    assert not doc.has_errors, [d.message for d in doc.errors]
    assert doc.values["result"] == [1, 2, 3]


def test_custom_function_error_handling():
    def bad_fn(args):
        raise ValueError("something went wrong")

    doc = wcl.parse("result = bad_fn()", functions={"bad_fn": bad_fn})
    # The function error should propagate as a diagnostic
    assert doc.has_errors


def test_multiple_functions():
    def add(args):
        return args[0] + args[1]

    def mul(args):
        return args[0] * args[1]

    doc = wcl.parse(
        "a = add(1, 2)\nb = mul(3, 4)",
        functions={"add": add, "mul": mul},
    )
    assert not doc.has_errors, [d.message for d in doc.errors]
    assert doc.values["a"] == 3
    assert doc.values["b"] == 12


def test_custom_function_in_control_flow():
    def items(args):
        return [1, 2, 3]

    doc = wcl.parse(
        "for item in items() { entry { value = item } }",
        functions={"items": items},
    )
    assert not doc.has_errors, [d.message for d in doc.errors]


def test_function_returning_none():
    def noop(args):
        return None

    doc = wcl.parse("result = noop()", functions={"noop": noop})
    assert not doc.has_errors, [d.message for d in doc.errors]
    assert doc.values["result"] is None


def test_function_returning_bool():
    def is_even(args):
        return args[0] % 2 == 0

    doc = wcl.parse("result = is_even(4)", functions={"is_even": is_even})
    assert not doc.has_errors, [d.message for d in doc.errors]
    assert doc.values["result"] is True
