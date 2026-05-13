Curated fixtures from yaml/yaml-test-suite.

Upstream: https://github.com/yaml/yaml-test-suite
Pinned commit: da267a5c4782e7361e82889e76c0dc7df0e1e870

Each vendored fixture is listed in `manifest.json` as `pass`, `error`, or
`skip` with a reason for skipped cases. The compliance harness compares `pass`
fixtures against the upstream `json` field using transform record semantics:
top-level JSON arrays become multiple records, and mappings/scalars become one
record.
