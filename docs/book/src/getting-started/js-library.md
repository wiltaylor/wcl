# Using WCL in JavaScript / TypeScript

WCL runs in the browser and Node.js via WebAssembly. The `wcl-wasm` package provides `parse`, `parseValues`, and `query` functions with full TypeScript types.

## Installation

```bash
npm install wcl-wasm
```

## Initialization

The WASM module must be initialized once before use:

**Browser / Deno:**

```typescript
import { init, parse, parseValues, query } from "wcl-wasm";

await init();
```

**Node.js (ESM):**

```typescript
import { init, parse, parseValues, query } from "wcl-wasm";
import { readFileSync } from "node:fs";

await init(readFileSync(new URL("../node_modules/wcl-wasm/pkg/wcl_wasm_bg.wasm", import.meta.url)));
```

**Node.js (CommonJS):**

```javascript
const { init, parse, parseValues, query } = require("wcl-wasm");
const fs = require("node:fs");
const path = require("node:path");

async function main() {
  await init(fs.readFileSync(path.join(__dirname, "../node_modules/wcl-wasm/pkg/wcl_wasm_bg.wasm")));
  const doc = parse('x = 42');
  console.log(doc.values);
}
main();
```

Calling any function before `init()` throws an error.

## Parsing a WCL String

`parse()` runs the full 11-phase pipeline and returns a document object:

```typescript
import { init, parse } from "wcl-wasm";

await init();

const doc = parse(`
  server web-prod {
    host = "0.0.0.0"
    port = 8080
    debug = false
  }
`);

if (doc.hasErrors) {
  for (const d of doc.diagnostics) {
    console.error(`${d.severity}: ${d.message}`);
  }
} else {
  console.log("Parsed successfully");
  console.log(doc.values);
}
```

The returned `WclDocument` has this shape:

```typescript
interface WclDocument {
  values: Record<string, any>;      // evaluated top-level values
  hasErrors: boolean;                // true if any errors occurred
  diagnostics: WclDiagnostic[];     // all diagnostics
}

interface WclDiagnostic {
  severity: "error" | "warning";
  message: string;
  code?: string;                    // e.g. "E071" for type mismatch
}
```

## Getting Just the Values

`parseValues()` returns only the evaluated values and throws on errors:

```typescript
try {
  const values = parseValues(`
    name = "my-app"
    port = 8080
    tags = ["web", "prod"]
  `);

  console.log(values.name);  // "my-app"
  console.log(values.port);  // 8080
  console.log(values.tags);  // ["web", "prod"]
} catch (e) {
  console.error("Parse error:", e);
}
```

This is the simplest way to use WCL when you just want the config values and don't need diagnostics.

## Working with Blocks

Block values appear as objects with `kind`, `id`, `labels`, `attributes`, and `children`:

```typescript
const doc = parse(`
  server web-prod {
    host = "0.0.0.0"
    port = 8080
  }

  server web-staging {
    host = "staging.internal"
    port = 8081
  }
`);

// Blocks appear as values keyed by their type
// Use queries for structured access (see below)
```

## Working with Tables

Tables evaluate to an array of row objects. Each row is an object mapping column names to cell values:

```typescript
const doc = parse(`
  table users {
    name : string
    age  : int
    | "alice" | 25 |
    | "bob"   | 30 |
  }
`);

console.log(doc.values.users);
// [{ name: "alice", age: 25 }, { name: "bob", age: 30 }]

console.log(doc.values.users[0].name); // "alice"
```

Tables inside blocks appear in the block's attributes:

```typescript
const doc = parse(`
  service main {
    table config {
      key   : string
      value : int
      | "port" | 8080 |
    }
  }
`);

// Access via the block's values
console.log(doc.values.main.attributes.config);
// [{ key: "port", value: 8080 }]
```

## Running Queries

`query()` parses a WCL string and executes a query in one call:

```typescript
const result = query(`
  server svc-api {
    port = 8080
    env = "prod"
  }

  server svc-admin {
    port = 9090
    env = "prod"
  }

  server svc-debug {
    port = 3000
    env = "dev"
  }
`, "server | .port");

console.log(result);  // [8080, 9090, 3000]
```

More query examples:

```typescript
// Select all server blocks
query(source, "server");

// Filter by attribute
query(source, 'server | .env == "prod"');

// Filter and project
query(source, 'server | .env == "prod" | .port');
// → [8080, 9090]

// Select by ID
query(source, "server#svc-api");
```

Throws if there are parse errors or if the query is invalid.

## Custom Functions

Register custom JavaScript functions that are callable from WCL expressions:

```typescript
const values = parseValues(`
  result = double(21)
  message = greet("World")
`, {
  functions: {
    double: (n: number) => n * 2,
    greet: (name: string) => `Hello, ${name}!`,
  },
});

console.log(values.result);   // 42
console.log(values.message);  // "Hello, World!"
```

Functions receive native JavaScript values and should return native values. Errors can be thrown normally:

```typescript
const values = parseValues("result = safe_div(10, 0)", {
  functions: {
    safe_div: (a: number, b: number) => {
      if (b === 0) throw new Error("division by zero");
      return a / b;
    },
  },
});
```

## In-Memory Files for Imports

Provide files as a map for import resolution without filesystem access (useful in browsers):

```typescript
const doc = parse('import "utils.wcl"\nresult = base_port + 1', {
  files: {
    "utils.wcl": "base_port = 8080",
  },
});

console.log(doc.values.result);  // 8081
```

## Custom Import Resolver

For more control over import resolution, provide a synchronous callback:

```typescript
const doc = parse('import "config.wcl"', {
  importResolver: (path: string) => {
    if (path.endsWith("config.wcl")) {
      return 'port = 8080';
    }
    return null;  // file not found
  },
});
```

If both `files` and `importResolver` are provided, `files` takes precedence.

## Parse Options

All options are optional:

```typescript
interface ParseOptions {
  rootDir?: string;            // root directory for import resolution (default: ".")
  allowImports?: boolean;      // enable/disable imports (default: true)
  maxImportDepth?: number;     // max nested import depth (default: 32)
  maxMacroDepth?: number;      // max macro expansion depth (default: 64)
  maxLoopDepth?: number;       // max for-loop nesting (default: 32)
  maxIterations?: number;      // max total loop iterations (default: 10000)
  importResolver?: (path: string) => string | null;
  files?: Record<string, string>;
  functions?: Record<string, (...args: any[]) => any>;
}
```

When processing untrusted input, disable imports:

```typescript
const doc = parse(untrustedInput, { allowImports: false });
```

## Error Handling

Check `hasErrors` and inspect `diagnostics`:

```typescript
const doc = parse(`
  server web {
    port = "not_a_number"
  }

  schema "server" {
    port: int
  }
`);

if (doc.hasErrors) {
  for (const d of doc.diagnostics) {
    const code = d.code ? `[${d.code}] ` : "";
    console.error(`${d.severity}: ${code}${d.message}`);
  }
}
```

`parseValues()` and `query()` throw on errors instead of returning them.

## Complete Example

```typescript
import { init, parse, parseValues, query } from "wcl-wasm";

await init();

// Simple value extraction
const config = parseValues(`
  app_name = "my-service"
  port = 8080
  debug = false
`);
console.log(`Starting ${config.app_name} on port ${config.port}`);

// Full document with validation
const doc = parse(`
  schema "server" {
    port: int
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
`);

if (doc.hasErrors) {
  doc.diagnostics.forEach(d => console.error(d.message));
  process.exit(1);
}

// Query for all ports
const ports = query(doc_source, "server | .port");
console.log("All ports:", ports);  // [8080, 9090]

// Custom functions
const result = parseValues("doubled = double(21)", {
  functions: {
    double: (n) => n * 2,
  },
});
console.log(result.doubled);  // 42
```

## Building from Source

To rebuild the WASM package from the Rust source:

```bash
# Build the WASM package
just build-wasm

# Run WASM tests
just test-wasm
```

This requires `wasm-pack` and the Rust toolchain.
