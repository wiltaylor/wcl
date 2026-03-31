# String Functions

WCL's string functions operate on `string` values and return strings, booleans, lists, or integers depending on the operation. All are pure and side-effect free.

## Reference

| Function | Signature | Description |
|---|---|---|
| `upper` | `upper(s: string) -> string` | Convert to uppercase |
| `lower` | `lower(s: string) -> string` | Convert to lowercase |
| `trim` | `trim(s: string) -> string` | Remove leading and trailing whitespace |
| `trim_prefix` | `trim_prefix(s: string, prefix: string) -> string` | Remove prefix if present |
| `trim_suffix` | `trim_suffix(s: string, suffix: string) -> string` | Remove suffix if present |
| `replace` | `replace(s: string, from: string, to: string) -> string` | Replace all occurrences of `from` with `to` |
| `split` | `split(s: string, sep: string) -> list` | Split on separator, returning a list of strings |
| `join` | `join(list: list, sep: string) -> string` | Join a list of strings with a separator |
| `starts_with` | `starts_with(s: string, prefix: string) -> bool` | True if `s` starts with `prefix` |
| `ends_with` | `ends_with(s: string, suffix: string) -> bool` | True if `s` ends with `suffix` |
| `contains` | `contains(s: string, sub: string) -> bool` | True if `s` contains `sub` |
| `length` | `length(s: string) -> i64` | Number of characters (Unicode code points) |
| `substr` | `substr(s: string, start: i64, end: i64) -> string` | Substring from `start` (inclusive) to `end` (exclusive) |
| `format` | `format(template: string, ...args) -> string` | Format string with `{}` placeholders |
| `regex_match` | `regex_match(s: string, pattern: string) -> bool` | True if `s` matches the regex pattern |
| `regex_capture` | `regex_capture(s: string, pattern: string) -> list` | List of capture groups from the first match |

## Examples

### upper / lower

```wcl
let name = "Hello World"
let up = upper(name)     // "HELLO WORLD"
let lo = lower(name)     // "hello world"
```

### trim / trim_prefix / trim_suffix

```wcl
let padded = "  hello  "
let clean = trim(padded)                         // "hello"
let path = trim_prefix("/api/v1/users", "/api")  // "/v1/users"
let file = trim_suffix("report.csv", ".csv")     // "report"
```

### replace

```wcl
let msg = replace("foo bar foo", "foo", "baz")  // "baz bar baz"
```

### split / join

```wcl
let parts = split("a,b,c", ",")      // ["a", "b", "c"]
let rejoined = join(parts, " | ")    // "a | b | c"
```

### starts_with / ends_with / contains

```wcl
let url = "https://example.com/api"
let secure = starts_with(url, "https")   // true
let is_api = ends_with(url, "/api")      // true
let has_ex = contains(url, "example")   // true
```

### length / substr

```wcl
let s = "abcdef"
let n = length(s)          // 6
let sub = substr(s, 1, 4)  // "bcd"
```

### format

```wcl
let msg = format("Hello, {}! You have {} messages.", "Alice", 3)
// "Hello, Alice! You have 3 messages."
```

Placeholders are filled left to right. Each `{}` consumes one argument.

### regex_match

```wcl
let valid = regex_match("user@example.com", "^[\\w.]+@[\\w.]+\\.[a-z]{2,}$")
// true
```

### regex_capture

```wcl
let groups = regex_capture("2024-03-15", "(\\d{4})-(\\d{2})-(\\d{2})")
// ["2024", "03", "15"]
```

Returns an empty list if there is no match. The list contains only the capture groups, not the full match.

## String Interpolation

In addition to these functions, WCL supports `${}` interpolation inside string literals and block IDs:

```wcl
let env = "prod"
let tag = "deploy-${env}"   // "deploy-prod"
```

Interpolation converts any value to its string representation automatically.
