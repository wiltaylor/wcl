# wcl-wasm — JavaScript/WASM bindings for WCL

JavaScript and TypeScript bindings for [WCL (Wil's Configuration Language)](https://wcl.dev), running in the browser and Node.js via WebAssembly.

## Install

```bash
npm install wcl-wasm
```

## Usage

```javascript
import { init, parse, query } from 'wcl-wasm';

await init();

const doc = parse(`
    server web {
        port = 8080
        host = "localhost"
    }
`);

console.log(doc.values);
// { server: { web: { port: 8080, host: 'localhost' } } }

const result = query(`server web { port = 8080 }`, 'server | .port > 3000');
console.log(result);
```

## Links

- **Website**: [wcl.dev](https://wcl.dev)
- **Documentation**: [wcl.dev/docs](https://wcl.dev/docs/)
- **GitHub**: [github.com/wiltaylor/wcl](https://github.com/wiltaylor/wcl)

## License

MIT
