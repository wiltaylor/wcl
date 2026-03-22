# Installation

## Prerequisites

WCL requires a Rust toolchain. If you do not have Rust installed, get it from [rustup.rs](https://rustup.rs):

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

After installation, ensure `cargo` and `rustc` are on your `PATH`:

```bash
rustc --version
cargo --version
```

## Install the CLI

From the root of the WCL repository, install the `wcl` binary directly to `~/.cargo/bin/`:

```bash
cargo install --path crates/wcl --features cli
```

Cargo will build the binary in release mode and place it at `~/.cargo/bin/wcl`. As long as `~/.cargo/bin` is on your `PATH` (the default after running `rustup`), the `wcl` command is immediately available.

## Building from Source

To build the entire workspace without installing:

```bash
cargo build --workspace
```

The debug binary will be at `target/debug/wcl`. For a release build:

```bash
cargo build --workspace --release
```

The release binary will be at `target/release/wcl`.

To run the full test suite:

```bash
cargo test --workspace
```

## Verify the Installation

```bash
wcl --version
```

You should see output like:

```
wcl 0.1.0
```

## Uninstalling

```bash
cargo uninstall wcl
```

This removes the `wcl` binary from `~/.cargo/bin/`.
