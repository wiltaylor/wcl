# JVM Binding (Java / Kotlin / Scala)

WASM-based via [Chicory](https://github.com/dylibso/chicory). Requires Java ≥ 17. Source: `bindings/jvm/src/main/java/io/github/wiltaylor/wcl/Wcl.java`, `WclDocument.java`, `ParseOptions.java`.

## Install

Maven:
```xml
<dependency>
  <groupId>io.github.wiltaylor</groupId>
  <artifactId>wcl</artifactId>
  <version>0.0.0-local</version>
</dependency>
```

Gradle:
```groovy
implementation 'io.github.wiltaylor:wcl:0.0.0-local'
```

Package: `io.github.wiltaylor.wcl`.

## Minimal Example

```java
import io.github.wiltaylor.wcl.Wcl;
import io.github.wiltaylor.wcl.WclDocument;

try (WclDocument doc = Wcl.parse("""
    server web {
        host = "0.0.0.0"
        port = 8080
    }
    """)) {

    if (doc.hasErrors()) {
        doc.getErrors().forEach(d ->
            System.err.println(d.code() + ": " + d.message()));
        return;
    }

    System.out.println(doc.getValues());
}
```

File:
```java
try (var doc = Wcl.parseFile("./config.wcl")) { ... }  // sets rootDir to parent
```

## Core API

From `Wcl.java`:

| Method | Purpose |
|--------|---------|
| `Wcl.parse(String source)` / `parse(String, ParseOptions)` | Parse + evaluate |
| `Wcl.parseFile(String path)` / `parseFile(String, ParseOptions)` | Read file, default rootDir |
| `Wcl.fromString(String source, Class<T> type)` / with options | Parse + deserialize, throws on errors |
| `Wcl.toString(T value)` | Serialize POCO to WCL text |
| `Wcl.toStringPretty(T value)` | Pretty-printed |

`WclDocument` (implements `AutoCloseable`):

| Method | |
|--------|---|
| `getValues()` | `Map<String, WclValue>` top-level evaluated values |
| `hasErrors()` | boolean |
| `getErrors()` | `List<Diagnostic>` (severity == error) |
| `getDiagnostics()` | All |
| `query(String)` | Run a WCL query |
| `getBlocks()` / `getBlocksOfType(String)` | Block access |
| `close()` | Release WASM resources |

## ParseOptions

Builder-style:

```java
var opts = new ParseOptions()
    .rootDir("/path")
    .allowImports(true)
    .maxImportDepth(32)
    .maxMacroDepth(64)
    .maxLoopDepth(8)
    .maxIterations(10000)
    .variables(Map.of("PORT", 8080));
```

## Serde

```java
record Server(String host, int port) {}

var src = """
    host = "0.0.0.0"
    port = 8080
    """;
var srv = Wcl.fromString(src, Server.class);
var back = Wcl.toStringPretty(srv);
```

## Value Type Mapping

| WCL | Java (`WclValue`) |
|-----|-------------------|
| string | `String` |
| int | `long` |
| float | `double` |
| bool | `boolean` |
| null | `null` |
| list | `List<WclValue>` |
| map | `Map<String, WclValue>` |
| block | `WclValue.Block` |
| date | ISO string (see `LocalDate` via serde) |
| duration | ISO string |
| symbol | string prefixed `:` |

## Custom Functions

```java
var opts = new ParseOptions()
    .functions(Map.of(
        "upper_rev", (WclValue[] args) ->
            WclValue.ofString(((String) args[0].asString()).toUpperCase())
    ));

try (var doc = Wcl.parse("x = upper_rev(\"hi\")", opts)) { ... }
```

## Error Handling

- `parse` / `parseFile` always return a `WclDocument`; check `hasErrors()`.
- `fromString` throws `WclException` with concatenated error messages when `hasErrors()`.
- `parseFile` throws `WclException` wrapping `IOException` for I/O failures.
- Use try-with-resources — `WclDocument` implements `AutoCloseable`.

## Gotchas

- Minimum Java version is 17.
- The artifact coordinates also publish to GitHub Packages (see `build.gradle`) — ensure Maven Central is your configured repository.
- `WclValue` is a tagged union; use the accessor methods (`asString`, `asLong`, etc.) to read.
- Integer precision: `long` values survive only up to JSON's safe range when values come back through the WASM boundary.
