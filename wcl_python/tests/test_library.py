import os

import wcl


def test_library_builder_build():
    builder = wcl.LibraryBuilder("myapp")
    builder.add_schema_text('schema "config" {\n    port: int\n}\n')
    builder.add_function_stub("greet", [("name", "string")], "string")
    content = builder.build()
    assert 'schema "config"' in content
    assert "declare greet(name: string) -> string" in content


def test_library_builder_with_doc():
    builder = wcl.LibraryBuilder("mylib")
    builder.add_function_stub("process", [("input", "string")], "string", "Process input")
    content = builder.build()
    assert "declare process(input: string) -> string" in content


def test_library_builder_no_return_type():
    builder = wcl.LibraryBuilder("mylib")
    builder.add_function_stub("fire", [("event", "string")])
    content = builder.build()
    assert "declare fire(event: string)" in content
    assert "->" not in content


def test_install_and_uninstall(tmp_lib_dir):
    path = wcl.install_library("test_lib.wcl", 'schema "test" { x: int }')
    assert os.path.exists(path)

    libs = wcl.list_libraries()
    lib_names = [os.path.basename(p) for p in libs]
    assert "test_lib.wcl" in lib_names

    wcl.uninstall_library("test_lib.wcl")
    libs_after = wcl.list_libraries()
    lib_names_after = [os.path.basename(p) for p in libs_after]
    assert "test_lib.wcl" not in lib_names_after


def test_install_via_builder(tmp_lib_dir):
    builder = wcl.LibraryBuilder("builder_test")
    builder.add_schema_text('schema "cfg" { port: int }\n')
    path = builder.install()
    assert os.path.exists(path)
    assert path.endswith("builder_test.wcl")


def test_list_empty_libraries(tmp_lib_dir):
    libs = wcl.list_libraries()
    assert libs == []
