# wcl — Ruby bindings for WCL

Ruby bindings for [WCL (Wil's Configuration Language)](https://wcl.dev), powered by a WASM runtime. Ruby 3.1+.

## Install

```bash
gem install wcl
```

## Usage

```ruby
require "wcl"

doc = Wcl.parse(<<~WCL)
    server web {
        port = 8080
        host = "localhost"
    }
WCL

puts doc.values
# {"server"=>{"web"=>{"port"=>8080, "host"=>"localhost"}}}

servers = doc.blocks_of_type("server")
puts "Found #{servers.length} server(s)"
```

## Links

- **Website**: [wcl.dev](https://wcl.dev)
- **Documentation**: [wcl.dev/docs](https://wcl.dev/docs/)
- **GitHub**: [github.com/wiltaylor/wcl](https://github.com/wiltaylor/wcl)

## License

MIT
