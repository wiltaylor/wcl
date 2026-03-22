"""Tests for WCL language features: macros, for loops, if/else, inline args,
partial let, and symbol sets."""

import wcl


# ── Macros ────────────────────────────────────────────────────────────────────


def test_attribute_macro_inject():
    """Attribute macro with inject adds attributes to the decorated block."""
    doc = wcl.parse("""
        macro @add_env(env) {
            inject {
                environment = env
            }
        }

        @add_env("production")
        server web {
            port = 8080
        }
    """)
    assert not doc.has_errors, f"errors: {[e.message for e in doc.errors]}"
    servers = doc.blocks_of_type("server")
    assert len(servers) == 1
    assert servers[0].get("port") == 8080
    assert servers[0].get("environment") == "production"


def test_attribute_macro_multiple_params():
    """Attribute macro with multiple parameters."""
    doc = wcl.parse("""
        macro @configure(p, h) {
            inject {
                port = p
                host = h
            }
        }

        @configure(8080, "localhost")
        server web {
            name = "web"
        }
    """)
    assert not doc.has_errors, f"errors: {[e.message for e in doc.errors]}"
    servers = doc.blocks_of_type("server")
    assert len(servers) == 1
    assert servers[0].get("port") == 8080
    assert servers[0].get("host") == "localhost"
    assert servers[0].get("name") == "web"


def test_attribute_macro_default_param():
    """Attribute macro with a default parameter value."""
    doc = wcl.parse("""
        macro @set_env(env = "staging") {
            inject {
                environment = env
            }
        }

        @set_env()
        server web {
            port = 8080
        }
    """)
    assert not doc.has_errors, f"errors: {[e.message for e in doc.errors]}"
    servers = doc.blocks_of_type("server")
    assert len(servers) == 1
    assert servers[0].get("environment") == "staging"


def test_function_macro_splices_body():
    """Function macro splices its body items at the call site."""
    doc = wcl.parse("""
        macro make_server(p) {
            server {
                port = p
            }
        }

        make_server(8080)
    """)
    assert not doc.has_errors, f"errors: {[e.message for e in doc.errors]}"
    servers = doc.blocks_of_type("server")
    assert len(servers) == 1
    assert servers[0].get("port") == 8080


# ── For loops ─────────────────────────────────────────────────────────────────


def test_for_loop_generates_blocks():
    """For loop over a list generates multiple blocks."""
    doc = wcl.parse("""
        let environments = ["dev", "staging", "prod"]

        for env in environments {
            server "${env}" {
                name = env
            }
        }
    """)
    assert not doc.has_errors, f"errors: {[e.message for e in doc.errors]}"
    servers = doc.blocks_of_type("server")
    assert len(servers) == 3
    names = {s.get("name") for s in servers}
    assert names == {"dev", "staging", "prod"}


def test_for_loop_with_literal_list():
    """For loop with an inline literal list."""
    doc = wcl.parse("""
        for port in [80, 443, 8080] {
            server {
                listen_port = port
            }
        }
    """)
    assert not doc.has_errors, f"errors: {[e.message for e in doc.errors]}"
    servers = doc.blocks_of_type("server")
    assert len(servers) == 3
    ports = {s.get("listen_port") for s in servers}
    assert ports == {80, 443, 8080}


def test_for_loop_generates_attributes():
    """For loop can generate blocks with interpolated IDs."""
    doc = wcl.parse("""
        for x in [1, 2, 3] {
            entry {
                value = x
            }
        }
    """)
    assert not doc.has_errors, f"errors: {[e.message for e in doc.errors]}"
    entries = doc.blocks_of_type("entry")
    assert len(entries) == 3


# ── If/else control flow ─────────────────────────────────────────────────────


def test_if_true_branch():
    """If with a true condition includes the block."""
    doc = wcl.parse("""
        let debug = true

        if debug {
            server dev {
                log_level = "debug"
            }
        }
    """)
    assert not doc.has_errors, f"errors: {[e.message for e in doc.errors]}"
    servers = doc.blocks_of_type("server")
    assert len(servers) == 1
    assert servers[0].get("log_level") == "debug"


def test_if_false_branch():
    """If with a false condition excludes the block."""
    doc = wcl.parse("""
        let debug = false

        if debug {
            server dev {
                log_level = "debug"
            }
        }
    """)
    assert not doc.has_errors, f"errors: {[e.message for e in doc.errors]}"
    servers = doc.blocks_of_type("server")
    assert len(servers) == 0


def test_if_else():
    """If/else selects the correct branch."""
    doc = wcl.parse("""
        let production = true

        if production {
            server prod {
                log_level = "warn"
            }
        } else {
            server dev {
                log_level = "debug"
            }
        }
    """)
    assert not doc.has_errors, f"errors: {[e.message for e in doc.errors]}"
    servers = doc.blocks_of_type("server")
    assert len(servers) == 1
    assert servers[0].id == "prod"
    assert servers[0].get("log_level") == "warn"


def test_if_else_false():
    """If/else with false condition selects else branch."""
    doc = wcl.parse("""
        let production = false

        if production {
            server prod {
                log_level = "warn"
            }
        } else {
            server dev {
                log_level = "debug"
            }
        }
    """)
    assert not doc.has_errors, f"errors: {[e.message for e in doc.errors]}"
    servers = doc.blocks_of_type("server")
    assert len(servers) == 1
    assert servers[0].id == "dev"
    assert servers[0].get("log_level") == "debug"


# ── Inline args ───────────────────────────────────────────────────────────────


def test_inline_args_string():
    """Block with a string inline arg produces _args attribute."""
    doc = wcl.parse("""
        server "web" {
            port = 8080
        }
    """)
    assert not doc.has_errors, f"errors: {[e.message for e in doc.errors]}"
    servers = doc.blocks_of_type("server")
    assert len(servers) == 1
    assert servers[0].get("port") == 8080
    args = servers[0].get("_args")
    assert args is not None
    assert "web" in args


def test_inline_args_multiple():
    """Block with multiple inline args."""
    doc = wcl.parse("""
        server "web" "primary" {
            port = 8080
        }
    """)
    assert not doc.has_errors, f"errors: {[e.message for e in doc.errors]}"
    servers = doc.blocks_of_type("server")
    assert len(servers) == 1
    args = servers[0].get("_args")
    assert args is not None
    assert len(args) == 2
    assert args[0] == "web"
    assert args[1] == "primary"


def test_inline_args_with_id():
    """Block with both an ID and inline args."""
    doc = wcl.parse("""
        server main "web" {
            port = 8080
        }
    """)
    assert not doc.has_errors, f"errors: {[e.message for e in doc.errors]}"
    servers = doc.blocks_of_type("server")
    assert len(servers) == 1
    assert servers[0].id == "main"
    args = servers[0].get("_args")
    assert args is not None
    assert "web" in args


# ── Partial let ───────────────────────────────────────────────────────────────


def test_partial_let_merges_lists():
    """Multiple partial let declarations merge their list values."""
    doc = wcl.parse("""
        partial let tags = ["a", "b"]
        partial let tags = ["c"]

        result = tags
    """)
    assert not doc.has_errors, f"errors: {[e.message for e in doc.errors]}"
    assert doc.values["result"] == ["a", "b", "c"]


def test_partial_let_single():
    """Single partial let works as a normal let."""
    doc = wcl.parse("""
        partial let items = [1, 2]

        result = items
    """)
    assert not doc.has_errors, f"errors: {[e.message for e in doc.errors]}"
    assert doc.values["result"] == [1, 2]


def test_partial_let_three_way_merge():
    """Three partial let declarations merge correctly."""
    doc = wcl.parse("""
        partial let ports = [80]
        partial let ports = [443]
        partial let ports = [8080]

        result = ports
    """)
    assert not doc.has_errors, f"errors: {[e.message for e in doc.errors]}"
    assert doc.values["result"] == [80, 443, 8080]


def test_partial_let_non_list_error():
    """Partial let with a non-list value produces an error."""
    doc = wcl.parse("""
        partial let x = 42
    """)
    error_codes = [d.code for d in doc.errors]
    assert "E038" in error_codes, f"expected E038 in {error_codes}"


# ── Symbol sets and symbol literals ───────────────────────────────────────────


def test_symbol_literal_in_block():
    """Symbol literal is accessible as a string value in block attributes."""
    doc = wcl.parse("""
        symbol_set status {
            :active
            :inactive
            :pending
        }

        server web {
            status = :active
        }
    """)
    assert not doc.has_errors, f"errors: {[e.message for e in doc.errors]}"
    servers = doc.blocks_of_type("server")
    assert len(servers) == 1
    # Symbols are serialized as strings through the WASM/JSON boundary
    assert servers[0].get("status") == "active"


def test_symbol_set_validation_rejects_invalid():
    """Using a symbol not in the declared set produces an error."""
    doc = wcl.parse("""
        symbol_set status {
            :active
            :inactive
        }

        schema "item" {
            status: symbol @symbol_set("status")
        }

        item x {
            status = :unknown
        }
    """)
    error_codes = [d.code for d in doc.errors]
    assert "E100" in error_codes, f"expected E100 in {error_codes}"


def test_symbol_set_validation_accepts_valid():
    """Using a symbol from the declared set produces no errors."""
    doc = wcl.parse("""
        symbol_set status {
            :active
            :inactive
        }

        schema "item" {
            status: symbol @symbol_set("status")
        }

        item x {
            status = :active
        }
    """)
    symbol_errors = [d for d in doc.errors if d.code in ("E100", "E101")]
    assert len(symbol_errors) == 0, f"unexpected symbol errors: {symbol_errors}"


def test_symbol_literal_top_level():
    """Symbol literal as a top-level attribute value."""
    doc = wcl.parse("""
        symbol_set mode {
            :fast
            :safe
        }

        mode = :fast
    """)
    assert not doc.has_errors, f"errors: {[e.message for e in doc.errors]}"
    # Symbols are serialized as strings through the WASM/JSON boundary
    assert doc.values["mode"] == "fast"
