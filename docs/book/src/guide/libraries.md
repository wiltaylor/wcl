# WCL Libraries

WCL supports well-known library files that can be shared across projects. Libraries are installed in standard XDG directories and imported using angle-bracket syntax.

## Importing a Library

```wcl
import <myapp.wcl>
```

This searches for `myapp.wcl` in the library search paths:

1. **User library**: `$XDG_DATA_HOME/wcl/lib/` (default: `~/.local/share/wcl/lib/`)
2. **System library**: dirs in `$XDG_DATA_DIRS` + `/wcl/lib/` (default: `/usr/local/share/wcl/lib/`, `/usr/share/wcl/lib/`)

The first match is used. Library imports skip the project root jail check. Relative imports inside library files also skip the jail check, so libraries can freely import helper files in their own directory.

### Custom Search Paths

You can prepend extra directories to the library search path using the CLI `--lib-path` flag (repeatable). These are searched **before** the default XDG/system paths:

```sh
wcl eval main.wcl --lib-path ./my-libs --lib-path /opt/wcl-libs
```

To disable the default paths entirely (only use `--lib-path` directories):

```sh
wcl eval main.wcl --lib-path ./my-libs --no-default-lib-paths
```

Programmatically, set `lib_paths` and `no_default_lib_paths` on `ParseOptions`.

## Library File Contents

A library file is a normal WCL file that can contain:

- **Schemas** -- type definitions for blocks
- **Function declarations** (`declare`) -- stubs for host-registered functions
- **Decorator schemas** -- custom decorator definitions
- **Exported variables** -- shared constants

Example library file:

```wcl
// ~/.local/share/wcl/lib/myapp.wcl

schema "server_config" {
    port: i64
    host: string @optional
    @validate(min = 1, max = 65535)
    port: i64
}

declare transform(input: string) -> string
declare validate_config(config: any) -> bool
```

## Function Declarations

The `declare` keyword creates a function stub:

```wcl
declare fn_name(param1: type1, param2: type2) -> return_type
```

Declarations tell the LSP about functions that will be provided by the host application at runtime. If a declared function is called but not registered, a helpful error is produced.

## Creating Library Files

Create `.wcl` files manually and place them in the user library directory (`~/.local/share/wcl/lib/` on Linux/macOS). For example:

```sh
mkdir -p ~/.local/share/wcl/lib
cat > ~/.local/share/wcl/lib/myapp.wcl << 'EOF'
schema "config" {
    port: i64
    host: string @optional
}

declare transform(input: string) -> string
EOF
```

To list installed libraries from Rust:

```rust
for lib in wcl::library::list_libraries().unwrap() {
    println!("{}", lib.display());
}
```

## LSP Support

The LSP automatically provides:

- **Completions** for functions declared in imported libraries
- **Signature help** with parameter names and types from `declare` statements
- **Go-to-definition** for library imports (jumps to the library file)
- **Diagnostics** if a declared function is not registered by the host
