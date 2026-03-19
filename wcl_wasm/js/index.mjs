/**
 * WCL Node.js wrapper
 *
 * Re-exports WASM functions with a default Node.js import resolver.
 * Browser users should pass their own `importResolver` or use `files`.
 */

export { parse, parseValues, query } from '../pkg/wcl_wasm.js';
