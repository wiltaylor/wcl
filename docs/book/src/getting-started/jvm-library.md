# Using WCL as a JVM Library

WCL can be embedded into Java, Kotlin, Scala, and other JVM programs via the `wcl` Maven package. It uses [Chicory](https://github.com/nicovank/chicory) (a pure-Java WASM runtime) under the hood, so you get the full 11-phase WCL pipeline with no native dependencies.

## Adding the Dependency

### Gradle (Kotlin DSL)

```kotlin
dependencies {
    implementation("io.github.wiltaylor:wcl:0.1.0")
}
```

### Gradle (Groovy)

```groovy
dependencies {
    implementation 'io.github.wiltaylor:wcl:0.1.0'
}
```

### Maven

```xml
<dependency>
    <groupId>io.github.wiltaylor</groupId>
    <artifactId>wcl</artifactId>
    <version>0.1.0</version>
</dependency>
```

The library requires Java 17+.

## Parsing a WCL String

Use `Wcl.parse()` to run the full pipeline and get a `WclDocument`:

```java
import io.github.wiltaylor.wcl.Wcl;

try (var doc = Wcl.parse("""
    server web-prod {
        host = "0.0.0.0"
        port = 8080
        debug = false
    }
""")) {
    if (doc.hasErrors()) {
        for (var diag : doc.getErrors()) {
            System.err.println("error: " + diag.message());
        }
    } else {
        System.out.println("Document parsed successfully");
    }
}
```

`WclDocument` implements `AutoCloseable` and should be closed when no longer needed (via try-with-resources or `close()`).

## Parsing a WCL File

`parseFile` reads and parses a file. It automatically sets `rootDir` to the file's parent directory so imports resolve correctly:

```java
try (var doc = Wcl.parseFile("config/main.wcl")) {
    // ...
}
```

To override parse options:

```java
var options = new ParseOptions().rootDir("./config");
try (var doc = Wcl.parseFile("config/main.wcl", options)) {
    // ...
}
```

## Accessing Evaluated Values

After parsing, `doc.getValues()` returns a `LinkedHashMap<String, WclValue>` containing all evaluated top-level attributes and blocks:

```java
import io.github.wiltaylor.wcl.Wcl;
import io.github.wiltaylor.wcl.eval.WclValue;

try (var doc = Wcl.parse("""
    name = "my-app"
    port = 8080
    tags = ["web", "prod"]
""")) {
    var values = doc.getValues();

    System.out.println("name: " + values.get("name").asString());
    System.out.println("port: " + values.get("port").asInt());

    for (var tag : values.get("tags").asList()) {
        System.out.println("tag: " + tag.asString());
    }
}
```

`WclValue` is a tagged union with these value kinds:

| Kind | Factory | Accessor |
|------|---------|----------|
| `STRING` | `WclValue.ofString("...")` | `.asString()` |
| `INT` | `WclValue.ofInt(42)` | `.asInt()` |
| `FLOAT` | `WclValue.ofFloat(3.14)` | `.asFloat()` |
| `BOOL` | `WclValue.ofBool(true)` | `.asBool()` |
| `NULL` | `WclValue.NULL` | `.isNull()` |
| `LIST` | `WclValue.ofList(...)` | `.asList()` |
| `MAP` | `WclValue.ofMap(...)` | `.asMap()` |
| `SET` | `WclValue.ofSet(...)` | `.asSet()` |
| `BLOCK_REF` | `WclValue.ofBlockRef(...)` | `.asBlockRef()` |

Safe accessors like `.tryAsString()` return `Optional` instead of throwing on type mismatch.

## Working with Blocks

Use `getBlocks()` and `getBlocksOfType()` to access blocks with their resolved attribute values:

```java
try (var doc = Wcl.parse("""
    server web-prod {
        host = "0.0.0.0"
        port = 8080
    }
    server web-staging {
        host = "staging.internal"
        port = 8081
    }
    database main-db {
        host = "db.internal"
        port = 5432
    }
""")) {
    var blocks = doc.getBlocks();
    System.out.println("Total blocks: " + blocks.size()); // 3

    var servers = doc.getBlocksOfType("server");
    for (var s : servers) {
        System.out.printf("%s: %s:%s%n", s.getId(), s.get("host"), s.get("port"));
    }
}
```

Each `BlockRef` provides:

- `getKind()` - block type name
- `getId()` - optional block identifier
- `getAttributes()` - resolved attribute map (includes `_args` if inline args are present)
- `getChildren()` - nested child blocks
- `getDecorators()` - attached decorators
- `get(key)` - safe attribute access (returns `null` if missing)
- `hasDecorator(name)` / `getDecorator(name)` - decorator access

## Running Queries

`query()` accepts the same query syntax as the `wcl query` CLI command:

```java
try (var doc = Wcl.parse("""
    server svc-api { port = 8080, env = "prod" }
    server svc-admin { port = 9090, env = "prod" }
    server svc-debug { port = 3000, env = "dev" }
""")) {
    var all = doc.query("server");
    var prod = doc.query("server | .env == \"prod\"");
    var ports = doc.query("server | .port");
    var api = doc.query("server#svc-api");
}
```

## Custom Functions

Register Java functions that are callable from WCL expressions:

```java
import io.github.wiltaylor.wcl.Wcl;
import io.github.wiltaylor.wcl.ParseOptions;
import io.github.wiltaylor.wcl.eval.WclValue;

var functions = Map.<String, java.util.function.Function<WclValue[], WclValue>>of(
    "double", args -> WclValue.ofInt(args[0].asInt() * 2),
    "greet", args -> WclValue.ofString("Hello, " + args[0].asString() + "!")
);

var opts = new ParseOptions().functions(functions);

try (var doc = Wcl.parse("""
    result = double(21)
    message = greet("World")
""", opts)) {
    System.out.println(doc.getValues().get("result").asInt());     // 42
    System.out.println(doc.getValues().get("message").asString()); // Hello, World!
}
```

Arguments and return values are serialized as JSON across the WASM boundary. Functions receive `WclValue[]` arguments and must return a `WclValue`.

## Deserializing into Java Types

### With `fromString`

Deserialize a WCL string directly into a Java type:

```java
import io.github.wiltaylor.wcl.Wcl;
import java.util.Map;

var config = Wcl.fromString("""
    name = "my-app"
    port = 8080
    debug = false
""", Map.class);
```

For POJOs, fields are matched by snake_case conversion:

```java
public class AppConfig {
    public String name;
    public long port;
    public boolean debug;
}

var config = Wcl.fromString("name = \"my-app\"\nport = 8080\ndebug = false", AppConfig.class);
```

### Serializing to WCL

Convert a Java object back to WCL text:

```java
var wcl = Wcl.toString(config);
// name = "my-app"
// port = 8080
// debug = false

var pretty = Wcl.toStringPretty(config);
```

## Parse Options

`ParseOptions` uses a fluent builder pattern. All fields are optional:

```java
var options = new ParseOptions()
    .rootDir("./config")
    .allowImports(true)
    .maxImportDepth(32)
    .maxMacroDepth(64)
    .maxLoopDepth(32)
    .maxIterations(10000)
    .functions(Map.of("double", args -> WclValue.ofInt(args[0].asInt() * 2)));
```

When processing untrusted input, disable imports to prevent file system access:

```java
var options = new ParseOptions().allowImports(false);
try (var doc = Wcl.parse(untrustedInput, options)) { ... }
```

## Library Files

Create `.wcl` library files manually and place them in `~/.local/share/wcl/lib/`. Use `LibraryManager.list()` to list installed libraries. See the [Libraries guide](../guide/libraries.md) for details.

## Error Handling

The `WclDocument` collects all diagnostics from every pipeline phase. Each `Diagnostic` includes a severity, message, and optional error code:

```java
try (var doc = Wcl.parse("""
    server web {
        port = "not_a_number"
    }
    schema "server" {
        port: i64
    }
""")) {
    for (var diag : doc.getDiagnostics()) {
        var severity = diag.isError() ? "ERROR" : "WARN";
        var code = diag.code() != null ? diag.code() : "----";
        System.err.printf("[%s] %s: %s%n", severity, code, diag.message());
    }
}
```

The `Diagnostic` record:

```java
public record Diagnostic(String severity, String message, String code) {
    public boolean isError();
}
```

## Thread Safety

Documents are safe to use from multiple threads. All methods acquire a lock internally, and values are cached after first access:

```java
try (var doc = Wcl.parse("x = 42")) {
    var threads = new Thread[10];
    for (int i = 0; i < 10; i++) {
        threads[i] = new Thread(() -> {
            var values = doc.getValues();
            System.out.println(values.get("x").asInt()); // 42
        });
        threads[i].start();
    }
    for (var t : threads) t.join();
}
```

## Complete Example

```java
import io.github.wiltaylor.wcl.Wcl;
import io.github.wiltaylor.wcl.ParseOptions;
import io.github.wiltaylor.wcl.eval.WclValue;

import java.util.Map;

public class Example {
    public static void main(String[] args) {
        // 1. Parse with schema validation
        try (var doc = Wcl.parse("""
            schema "server" {
                port: i64
                host: string @optional
            }
            server svc-api {
                port = 8080
                host = "api.internal"
            }
            server svc-admin {
                port = 9090
                host = "admin.internal"
            }
        """)) {
            if (doc.hasErrors()) {
                doc.getErrors().forEach(e -> System.err.println(e.message()));
                return;
            }

            // 2. Query for all server ports
            var ports = doc.query("server | .port");
            System.out.println("All ports: " + ports);

            // 3. Iterate resolved blocks
            for (var server : doc.getBlocksOfType("server")) {
                System.out.printf("%s: %s:%s%n",
                    server.getId(), server.get("host"), server.get("port"));
            }
        }

        // 4. Custom functions
        var opts = new ParseOptions().functions(Map.of(
            "double", a -> WclValue.ofInt(a[0].asInt() * 2)
        ));
        try (var doc2 = Wcl.parse("result = double(21)", opts)) {
            System.out.println("result = " + doc2.getValues().get("result").asInt()); // 42
        }
    }
}
```

## Building from Source

```bash
# Build the WASM module and Java project
just build jvm

# Run JVM tests
just test jvm
```

This requires Java 17+ and a Rust toolchain (for building the WASM module).
