/**
 * WCL JavaScript/TypeScript bindings.
 *
 * Usage (browser):
 *   import { init, parse, parseValues, query } from "wcl-wasm";
 *   await init();
 *   const doc = parse('x = 42');
 *
 * Usage (Node.js):
 *   import { init, parse, parseValues, query } from "wcl-wasm";
 *   import { readFileSync } from "node:fs";
 *   await init(readFileSync(new URL("../pkg/wcl_wasm_bg.wasm", import.meta.url)));
 *   const doc = parse('x = 42');
 */

import __wbg_init, {
  parse as __wasm_parse,
  parseValues as __wasm_parseValues,
  query as __wasm_query,
} from "../pkg/wcl_wasm.js";

let _initialized = false;

/**
 * Initialise the WASM module. Must be called (and awaited) once before
 * calling any other function.
 *
 * @param wasmInput - Optional: a URL, `Request`, `Response`, `ArrayBuffer`,
 *   or `WebAssembly.Module` to load the WASM binary from. If omitted the
 *   default fetch-based loader is used (works in browsers and Deno).
 *   For Node.js, pass the bytes via `fs.readFileSync(...)`.
 */
export async function init(wasmInput) {
  if (_initialized) return;
  await __wbg_init(wasmInput);
  _initialized = true;
}

function assertReady() {
  if (!_initialized) {
    throw new Error(
      "WCL WASM module not initialised. Call and await init() first."
    );
  }
}

/** Parse a WCL source string. Returns `{ values, hasErrors, diagnostics }`. */
export function parse(source, options) {
  assertReady();
  return __wasm_parse(source, options);
}

/** Parse a WCL source string and return just the evaluated values. Throws on errors. */
export function parseValues(source, options) {
  assertReady();
  return __wasm_parseValues(source, options);
}

/** Parse a WCL source string and execute a query against it. Throws on errors. */
export function query(source, queryStr, options) {
  assertReady();
  return __wasm_query(source, queryStr, options);
}
