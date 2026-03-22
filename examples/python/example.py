#!/usr/bin/env -S uv run
# /// script
# requires-python = ">=3.9"
# dependencies = ["pywcl>=0.2.4a1"]
# ///
"""WCL Python binding example."""

import os
import wcl

# Read the shared config file
config_path = os.path.join(os.path.dirname(__file__), "..", "config", "app.wcl")
with open(config_path) as f:
    source = f.read()

# Parse the source
doc = wcl.parse(source)

# Check for errors
if doc.has_errors:
    print("Parse errors:")
    for err in doc.errors:
        print(f"  - {err.message}")
    raise SystemExit(1)

print("Parsed successfully!")

# Count server blocks
servers = doc.blocks_of_type("server")
print(f"Server blocks: {len(servers)}")

# Print server names and ports
print("\nServers:")
server_values = doc.values.get("server", {})
if isinstance(server_values, dict):
    for name, attrs in server_values.items():
        port = attrs.get("port", "?") if isinstance(attrs, dict) else "?"
        print(f"  {name}: port {port}")

# Query for servers with workers > 2
print("\nQuery: server | .workers > 2")
try:
    result = doc.query("server | .workers > 2")
    print(f"  Result: {result}")
except Exception as e:
    print(f"  Query error: {e}")

# Print the users table
print("\nUsers table:")
users = doc.values.get("users", [])
if isinstance(users, list):
    for row in users:
        if isinstance(row, dict):
            name = row.get("name", "")
            role = row.get("role", "")
            admin = row.get("admin", False)
            print(f"  {name} | {role} | admin={admin}")
