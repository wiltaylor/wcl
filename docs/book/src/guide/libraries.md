# WCL Libraries

WCL supports well-known library files that can be shared across projects. Libraries are installed in standard XDG directories and imported using angle-bracket syntax.

## Importing a Library

```wcl
import <myapp.wcl>
```

This searches for `myapp.wcl` in the library search paths:

1. **User library**: `$XDG_DATA_HOME/wcl/lib/` (default: `~/.local/share/wcl/lib/`)
2. **System library**: dirs in `$XDG_DATA_DIRS` + `/wcl/lib/` (default: `/usr/local/share/wcl/lib/`, `/usr/share/wcl/lib/`)

The first match is used. Library imports skip the project root jail check.

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
    port: int
    host: string @optional
    @validate(min = 1, max = 65535)
    port: int
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
    port: int
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
