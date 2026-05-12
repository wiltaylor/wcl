set working-directory := '..'

# Run all examples against local source builds/packages
all: cli rust python javascript go dotnet jvm ruby

# Build and run CLI examples against the locally-built wcl
run-local: cli transform

# Build the CLI binary (no install)
build-local:
    cargo build --quiet -p wcl

# Run CLI examples using the locally-built binary
run-local-cli:
    PATH="$(pwd)/target/debug:$PATH" bash examples/cli/eval.sh
    PATH="$(pwd)/target/debug:$PATH" bash examples/cli/validate.sh
    PATH="$(pwd)/target/debug:$PATH" bash examples/cli/fmt.sh
    PATH="$(pwd)/target/debug:$PATH" bash examples/cli/manipulate.sh

# Run transform examples using the locally-built binary
run-local-transform:
    PATH="$(pwd)/target/debug:$PATH" bash examples/cli/transform.sh

# Install the locally-built CLI into PATH (only needed for manual shell testing)
install-local:
    cargo install --path crates/wcl --quiet

# Deprecated: examples are local-first by default.
patch-local:
    bash scripts/patch-examples-local.sh

# Deprecated: examples are local-first by default.
unpatch-local:
    @echo "examples are local-first; nothing to unpatch"

# Update published package versions in examples
update-versions version:
    bash scripts/update-example-versions.sh {{version}}

# Build wdoc example using locally-built binary
wdoc: build-local
    ./target/debug/wcl wdoc build examples/wdoc/site.wcl examples/wdoc/pages.wcl --output /tmp/wdoc-example
    @echo "wdoc: output at /tmp/wdoc-example/index.html"

# Backward-compatible alias for local-first default
wdoc-local: wdoc

# Serve wdoc example using locally-built binary
wdoc-serve: build-local
    ./target/debug/wcl wdoc serve examples/wdoc/site.wcl examples/wdoc/pages.wcl --port 3000 --open

# Backward-compatible alias for local-first default
wdoc-serve-local: wdoc-serve

# Run CLI examples using locally-built binary
cli: run-local-cli

# Run transform examples using locally-built binary
transform: run-local-transform

# Run Rust example
rust:
    cd examples/rust && cargo run --quiet

# Run Python example
python:
    cd examples/python && uv run example.py

# Run JavaScript example
javascript:
    cd examples/javascript && npm install --silent && node example.mjs

# Run Go example
go:
    cd examples/go && go run .

# Run .NET example
dotnet:
    cd examples/dotnet && dotnet run --verbosity quiet

# Run JVM example
jvm:
    just build jvm
    cd examples/jvm && ./gradlew run --quiet

# Run Ruby example
ruby:
    cd examples/ruby && bundle install --quiet && bundle exec ruby example.rb
