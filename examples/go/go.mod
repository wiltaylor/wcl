module example

go 1.25.0

require github.com/wiltaylor/wcl/bindings/go v0.0.0

require (
	github.com/tetratelabs/wazero v1.11.0 // indirect
	golang.org/x/sys v0.38.0 // indirect
)

replace github.com/wiltaylor/wcl/bindings/go => ../../bindings/go
