package wcl

// To rebuild the native static library from the Rust source, run:
//
//   go generate ./...
//
// This requires a Rust toolchain and the WCL monorepo at ../../

//go:generate sh -c "cd ../.. && just build-go 2>/dev/null || (cargo build -p wcl_ffi --release && mkdir -p bindings/go/lib/$(go env GOOS)_$(go env GOARCH) && cp target/release/libwcl_ffi.a bindings/go/lib/$(go env GOOS)_$(go env GOARCH)/ && cp crates/wcl_ffi/wcl.h bindings/go/)"
