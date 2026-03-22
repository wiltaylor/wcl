# WCL — JVM bindings

JVM bindings for [WCL (Wil's Configuration Language)](https://wcl.dev), powered by the Chicory WASM runtime. Works with Java, Kotlin, Scala, and other JVM languages.

## Install

**Gradle:**
```groovy
implementation 'io.github.wiltaylor:wcl:0.2.4-alpha'
```

**Maven:**
```xml
<dependency>
    <groupId>io.github.wiltaylor</groupId>
    <artifactId>wcl</artifactId>
    <version>0.2.4-alpha</version>
</dependency>
```

## Usage

```java
import io.github.wiltaylor.wcl.Wcl;
import io.github.wiltaylor.wcl.WclDocument;

try (var doc = Wcl.parse("""
    server web {
        port = 8080
        host = "localhost"
    }
""")) {
    System.out.println(doc.getValues());

    var servers = doc.blocksOfType("server");
    System.out.println("Found " + servers.size() + " server(s)");
}
```

## Links

- **Website**: [wcl.dev](https://wcl.dev)
- **Documentation**: [wcl.dev/docs](https://wcl.dev/docs/)
- **GitHub**: [github.com/wiltaylor/wcl](https://github.com/wiltaylor/wcl)

## License

MIT
