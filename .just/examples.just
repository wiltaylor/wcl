set working-directory := '..'

# Run all examples
all: cli rust python javascript go dotnet jvm ruby

# Build and run ALL examples against the locally-built wcl
run-local: build-local patch-local run-local-cli run-local-transform unpatch-local

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

# Install the locally-built CLI into PATH (only needed for non-local recipes)
install-local:
    cargo install --path crates/wcl --quiet

# Patch binding examples to use locally-built packages (for CI)
patch-local:
    bash scripts/patch-examples-local.sh

# Restore binding examples to use published package versions
unpatch-local:
    rm -rf examples/rust/.cargo
    git checkout -- examples/python/example.py examples/ruby/Gemfile examples/dotnet/Example.csproj
    git checkout -- examples/ruby/Gemfile.lock 2>/dev/null || true

# Update published package versions in examples
update-versions version:
    bash scripts/update-example-versions.sh {{version}}

# Build wdoc example (requires wcl in PATH)
wdoc:
    wcl wdoc build examples/wdoc/site.wcl examples/wdoc/pages.wcl --output /tmp/wdoc-example
    @echo "wdoc: output at /tmp/wdoc-example/index.html"

# Build wdoc example using locally-built binary
wdoc-local: build-local
    ./target/debug/wcl wdoc build examples/wdoc/site.wcl examples/wdoc/pages.wcl --output /tmp/wdoc-example
    @echo "wdoc: output at /tmp/wdoc-example/index.html"

# Serve wdoc example with live reload (requires wcl in PATH)
wdoc-serve:
    wcl wdoc serve examples/wdoc/site.wcl examples/wdoc/pages.wcl --port 3000 --open

# Serve wdoc example using locally-built binary (restores patched files on exit)
wdoc-serve-local: build-local patch-local && unpatch-local
    ./target/debug/wcl wdoc serve examples/wdoc/site.wcl examples/wdoc/pages.wcl --port 3000 --open

# Run CLI examples (requires wcl in PATH)
cli:
    bash examples/cli/eval.sh
    bash examples/cli/validate.sh
    bash examples/cli/fmt.sh
    bash examples/cli/manipulate.sh

# Run transform examples (requires wcl in PATH)
transform:
    bash examples/cli/transform.sh

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
