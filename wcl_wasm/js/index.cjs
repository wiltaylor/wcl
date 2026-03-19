/**
 * WCL CommonJS wrapper.
 *
 * Usage:
 *   const { init, parse, parseValues, query } = require("wcl-wasm");
 *   const fs = require("node:fs");
 *   const path = require("node:path");
 *
 *   async function main() {
 *     await init(fs.readFileSync(path.join(__dirname, "../pkg/wcl_wasm_bg.wasm")));
 *     const doc = parse('x = 42');
 *     console.log(doc.values);
 *   }
 *   main();
 */

"use strict";

let _wasm;

async function loadESM() {
  if (!_wasm) {
    _wasm = await import("../pkg/wcl_wasm.js");
  }
  return _wasm;
}

let _initialized = false;

async function init(wasmInput) {
  if (_initialized) return;
  const mod = await loadESM();
  if (wasmInput !== undefined) {
    await mod.default({ module_or_path: wasmInput });
  } else {
    await mod.default();
  }
  _initialized = true;
}

function assertReady() {
  if (!_initialized) {
    throw new Error(
      "WCL WASM module not initialised. Call and await init() first."
    );
  }
}

function parse(source, options) {
  assertReady();
  return _wasm.parse(source, options);
}

function parseValues(source, options) {
  assertReady();
  return _wasm.parseValues(source, options);
}

function query(source, queryStr, options) {
  assertReady();
  return _wasm.query(source, queryStr, options);
}

module.exports = { init, parse, parseValues, query };
