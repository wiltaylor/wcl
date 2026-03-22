use std::path::Path;
use wcl::{ParseOptions, Value};

fn main() {
    // Read the shared config file
    let config_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../config/app.wcl");
    let source = std::fs::read_to_string(&config_path)
        .unwrap_or_else(|e| panic!("failed to read {}: {e}", config_path.display()));

    // Parse with default options
    let doc = wcl::parse(&source, ParseOptions::default());

    // Check for errors
    if doc.has_errors() {
        eprintln!("Parse errors:");
        for err in doc.errors() {
            eprintln!("  - {}", err.message);
        }
        std::process::exit(1);
    }

    println!("Parsed successfully!");

    // Count server blocks (from AST)
    let servers = doc.blocks_of_type("server");
    println!("Server blocks: {}", servers.len());

    // Print server names and ports (from evaluated values)
    // Values are flat: each block ID maps to a BlockRef value
    println!("\nServers:");
    for (_key, val) in &doc.values {
        if let Value::BlockRef(br) = val {
            if br.kind == "server" {
                let id = br.id.as_deref().unwrap_or("(anonymous)");
                let port = br
                    .attributes
                    .get("port")
                    .map(|v| format!("{v}"))
                    .unwrap_or_else(|| "?".into());
                println!("  {id}: port {port}");
            }
        }
    }

    // Run a query for servers with workers > 2
    println!("\nQuery: server | .workers > 2");
    match doc.query("server | .workers > 2") {
        Ok(result) => println!("  Result: {result}"),
        Err(e) => eprintln!("  Query error: {e}"),
    }

    // Print the users table
    println!("\nUsers table:");
    if let Some(Value::List(rows)) = doc.values.get("users") {
        for row in rows {
            if let Value::Map(cols) = row {
                let name = cols.get("name").map(|v| format!("{v}")).unwrap_or_default();
                let role = cols.get("role").map(|v| format!("{v}")).unwrap_or_default();
                let admin = cols.get("admin").map(|v| format!("{v}")).unwrap_or_default();
                println!("  {name} | {role} | admin={admin}");
            }
        }
    }
}
