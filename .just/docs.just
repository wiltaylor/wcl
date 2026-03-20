set working-directory := '..'

# Build the documentation book
book-build:
    mdbook build docs/book

# Serve the documentation book locally
book-serve:
    mdbook serve docs/book

# Build full website (Hugo + mdBook merged)
web-build: book-build
    cd site && hugo --minify
    mkdir -p site/public/docs
    cp -r docs/book/build/* site/public/docs/

# Serve website locally (Hugo dev server with mdBook at /docs/)
web-serve: book-build
    mkdir -p site/static/docs
    cp -r docs/book/build/* site/static/docs/
    cd site && hugo server --buildDrafts --navigateToChanged

# Clean website build output
web-clean:
    rm -rf site/public site/resources
