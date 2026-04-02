# Namespaces

Namespaces let you group schemas, blocks, and other items under a named prefix to avoid name collisions. This is especially useful in libraries and large projects with many schemas.

## Braced Namespace

Wrap items in a `namespace` block to scope them:

```wcl
namespace networking {
  schema "service" {
    port: int
    host: string
  }

  service "web-api" {
    port = 8080
    host = "0.0.0.0"
  }
}
```

Items inside the namespace are accessible to each other without qualification. From outside, use the `::` separator:

```wcl
networking::service "internal" {
  port = 9090
  host = "127.0.0.1"
}
```

## File-Level Namespace

Place `namespace` at the top of a file (without braces) to scope **all** items in the file:

```wcl
namespace networking

schema "service" {
  port: int
  host: string
}

service "web-api" {
  port = 8080
  host = "0.0.0.0"
}
```

This is the recommended pattern for library files where every item should be namespaced.

## Importing Namespaced Files

When you import a file that declares a namespace, its items come in already qualified:

```wcl
import "./networking.wcl"

// Access items with the namespace prefix
networking::service "my-svc" {
  port = 3000
  host = "localhost"
}
```

The import itself does not add a namespace — the namespace is declared by the file author.

## Use Declarations

Bring namespaced items into the current scope with `use`:

```wcl
import "./networking.wcl"

// Single import
use networking::service

service "my-svc" {
  port = 3000
}
```

### Aliasing

Rename items with `->`:

```wcl
use networking::service -> svc

svc "my-svc" {
  port = 3000
}
```

### Grouped Imports

Import multiple items from the same namespace:

```wcl
use networking::{service, endpoint}
```

With aliases:

```wcl
use networking::{service -> svc, endpoint -> ep}
```

## Nested Namespaces

Namespaces can be nested to create deeper hierarchies:

```wcl
namespace cloud {
  namespace aws {
    schema "instance" {
      type: string
      region: string
    }
  }

  namespace gcp {
    schema "instance" {
      machine_type: string
      zone: string
    }
  }
}
```

Access nested items with `::`:

```wcl
cloud::aws::instance "web-1" {
  type = "t3.micro"
  region = "us-east-1"
}
```

### Shorthand Path Syntax

Declare deep namespaces in a single line with `::`:

```wcl
namespace cloud::aws {
  schema "instance" {
    type: string
  }
}
```

This is equivalent to `namespace cloud { namespace aws { ... } }`.

The file-level form also supports paths:

```wcl
namespace cloud::aws

schema "instance" {
  type: string
}
```

### Use with Nested Namespaces

`use` declarations work with multi-segment paths:

```wcl
use cloud::aws::instance
use cloud::gcp::{instance -> gcp_instance}
```

## Rules

- **File-level namespace** must appear before any other items in the file.
- Inside a namespace, peer items are accessible without the `::` prefix.
- Outside a namespace, always use `namespace::item` or a `use` declaration.
- Namespace names follow standard identifier rules (`[a-zA-Z_][a-zA-Z0-9_]*`).
- Namespaces can be nested to arbitrary depth.
