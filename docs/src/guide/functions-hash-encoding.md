# Hash & Encoding Functions

WCL provides functions for hashing strings and encoding/decoding data. These are pure, deterministic functions useful for generating stable identifiers, verifying content, and interoperating with systems that expect encoded data.

## Reference

| Function | Signature | Description |
|---|---|---|
| `sha256` | `sha256(s: string) -> string` | Hex-encoded SHA-256 digest of the UTF-8 bytes of `s` |
| `base64_encode` | `base64_encode(s: string) -> string` | Standard Base64 encoding of the UTF-8 bytes of `s` |
| `base64_decode` | `base64_decode(s: string) -> string` | Decode a Base64-encoded string back to UTF-8 |
| `json_encode` | `json_encode(value: any) -> string` | Serialize any WCL value to a JSON string |

## Examples

### sha256

```wcl
let digest = sha256("hello world")
// "b94d27b9934d3e08a52e52d7da7dabfac484efe04294e576b35568b3f5d8d4a5"
// (truncated for illustration)
```

Use `sha256` to generate stable, content-derived identifiers:

```wcl
let artifact_id = sha256(format("{}-{}-{}", name, version, arch))

artifact ${artifact_id} {
  name: name
  version: version
}
```

### base64_encode / base64_decode

```wcl
let encoded = base64_encode("user:password")   // "dXNlcjpwYXNzd29yZA=="
let decoded = base64_decode(encoded)           // "user:password"
```

Encode a config value for embedding in an environment variable:

```wcl
config app {
  db_url_b64: base64_encode("postgres://host:5432/mydb")
}
```

### json_encode

`json_encode` converts any WCL value to its JSON representation:

```wcl
let obj = {name: "web", port: 8080, tags: ["api", "public"]}
let json = json_encode(obj)
// "{\"name\":\"web\",\"port\":8080,\"tags\":[\"api\",\"public\"]}"
```

Scalar values:

```wcl
let s = json_encode("hello")    // "\"hello\""
let n = json_encode(42)         // "42"
let b = json_encode(true)       // "true"
let l = json_encode([1, 2, 3])  // "[1,2,3]"
```

`json_encode` is particularly useful when you need to embed structured data as a string in another system's configuration format:

```wcl
service api {
  env_vars: {
    APP_CONFIG: json_encode({
      timeout: 30
      retries: 3
      endpoints: ["primary", "fallback"]
    })
  }
}
```

## Notes

- `sha256` always produces lowercase hex output.
- `base64_encode` uses standard Base64 with padding (`=`). It does not produce URL-safe Base64.
- `base64_decode` returns a `string`. If the decoded bytes are not valid UTF-8, an error is raised.
- `json_encode` serializes WCL maps with keys in insertion order.
