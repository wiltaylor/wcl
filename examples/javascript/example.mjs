/**
 * WCL JavaScript (WASM) binding example.
 */

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";
import { init, parse, query } from "wcl-wasm";

const __dirname = dirname(fileURLToPath(import.meta.url));

// Initialize the WASM module (required once before any other call)
const wasmPath = join(
  __dirname,
  "node_modules",
  "wcl-wasm",
  "pkg",
  "wcl_wasm_bg.wasm",
);
await init(readFileSync(wasmPath));

// Read the shared config file
const configPath = join(__dirname, "..", "config", "app.wcl");
const source = readFileSync(configPath, "utf-8");

// Parse the source
const doc = parse(source);

// Check for errors
if (doc.hasErrors) {
  console.error("Parse errors:");
  for (const d of doc.diagnostics) {
    if (d.severity === "error") console.error(`  - ${d.message}`);
  }
  process.exit(1);
}

console.log("Parsed successfully!");

// Count server blocks
const servers = doc.values.server || {};
const serverNames = Object.keys(servers);
console.log(`Server blocks: ${serverNames.length}`);

// Print server names and ports
console.log("\nServers:");
for (const [name, attrs] of Object.entries(servers)) {
  console.log(`  ${name}: port ${attrs.port ?? "?"}`);
}

// Query for servers with workers > 2
console.log("\nQuery: server | .workers > 2");
try {
  const result = query(source, "server | .workers > 2");
  console.log(`  Result: ${JSON.stringify(result)}`);
} catch (e) {
  console.error(`  Query error: ${e.message}`);
}

// Print the users table
console.log("\nUsers table:");
const users = doc.values.users || [];
for (const row of users) {
  console.log(`  ${row.name} | ${row.role} | admin=${row.admin}`);
}
