Curated TOML compliance fixtures for the WCL-authored TOML codec.

Target spec: TOML v1.1.0, https://toml.io/en/v1.1.0
Reference suite: https://github.com/toml-lang/toml-test

The fixture shape follows toml-test's decoder idea: valid fixtures include a
tagged JSON file that preserves TOML scalar type identity, and invalid fixtures
must fail to decode.
