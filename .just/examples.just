set working-directory := '..'

# Run all examples
all: cli rust python javascript go dotnet jvm ruby

# Patch examples to use locally-built packages (for CI)
patch-local:
    bash scripts/patch-examples-local.sh

# Update published package versions in examples
update-versions version:
    bash scripts/update-example-versions.sh {{version}}

# Run CLI examples
cli:
    bash examples/cli/eval.sh
    bash examples/cli/query.sh
    bash examples/cli/validate.sh
    bash examples/cli/fmt.sh
    bash examples/cli/convert.sh
    bash examples/cli/manipulate.sh

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
