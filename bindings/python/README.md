# pywcl — Python bindings for WCL

Python bindings for [WCL (Wil's Configuration Language)](https://wcl.dev), powered by a WASM runtime.

## Install

```bash
pip install pywcl
```

## Usage

```python
import wcl

doc = wcl.parse("""
    server web {
        port = 8080
        host = "localhost"
    }
""")

print(doc.values)  # {'server': {'web': {'port': 8080, 'host': 'localhost'}}}

servers = doc.blocks_of_type("server")
print(f"Found {len(servers)} server(s)")
```

## Links

- **Website**: [wcl.dev](https://wcl.dev)
- **Documentation**: [wcl.dev/docs](https://wcl.dev/docs/)
- **GitHub**: [github.com/wiltaylor/wcl](https://github.com/wiltaylor/wcl)

## License

MIT
