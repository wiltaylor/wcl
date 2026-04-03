set working-directory := '..'

# Build the documentation site
wdoc-build:
    wcl wdoc build docs/*.wcl --output docs/build

# Serve the documentation site locally with live reload
wdoc-serve:
    wcl wdoc serve docs/*.wcl --port 3000

# Build the documentation site using locally-built binary
wdoc-build-local:
    cargo build --quiet -p wcl
    ./target/debug/wcl wdoc build docs/*.wcl --output docs/build

# Serve the documentation site using locally-built binary
wdoc-serve-local:
    cargo build --quiet -p wcl
    ./target/debug/wcl wdoc serve docs/*.wcl --port 3000

# Build full website (Hugo + wdoc merged)
web-build: wdoc-build
    cp extras/highlightjs/wcl.js site/static/js/wcl.js
    cd site && hugo --minify
    mkdir -p site/public/docs
    cp -r docs/build/* site/public/docs/

# Serve website locally (Hugo dev server with wdoc at /docs/)
web-serve: wdoc-build
    cp extras/highlightjs/wcl.js site/static/js/wcl.js
    mkdir -p site/static/docs
    cp -r docs/build/* site/static/docs/
    cd site && hugo server --buildDrafts --navigateToChanged

# Clean website build output
web-clean:
    rm -rf site/public site/resources docs/build
