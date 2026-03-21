require_relative "lib/wcl/version"

Gem::Specification.new do |s|
  s.name        = "wcl"
  s.version     = Wcl::VERSION
  s.summary     = "WCL (Wil's Configuration Language) Ruby bindings"
  s.description = "Ruby bindings for WCL, powered by a WASM module and the wasmtime runtime. " \
                  "Provides the full 11-phase parsing pipeline with native Ruby types."
  s.authors     = ["Wil Taylor"]
  s.license     = "MIT"
  s.homepage    = "https://github.com/wiltaylor/wcl"

  s.required_ruby_version = ">= 3.1"

  s.files = Dir["lib/**/*.rb", "lib/wcl/wcl_wasm.wasm", "LICENSE", "README.md"]

  s.add_dependency "wasmtime", "~> 42"
end
