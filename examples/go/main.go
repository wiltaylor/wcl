package main

import (
	"fmt"
	"os"
	"path/filepath"
	"runtime"

	wcl "github.com/wiltaylor/wcl/bindings/go"
)

func main() {
	// Read the shared config file
	_, thisFile, _, _ := runtime.Caller(0)
	configPath := filepath.Join(filepath.Dir(thisFile), "..", "config", "app.wcl")
	source, err := os.ReadFile(configPath)
	if err != nil {
		fmt.Fprintf(os.Stderr, "failed to read config: %v\n", err)
		os.Exit(1)
	}

	// Parse the source
	doc, err := wcl.Parse(string(source), nil)
	if err != nil {
		fmt.Fprintf(os.Stderr, "parse error: %v\n", err)
		os.Exit(1)
	}
	defer doc.Close()

	// Check for errors
	if doc.HasErrors() {
		errors, _ := doc.Errors()
		fmt.Println("Parse errors:")
		for _, e := range errors {
			fmt.Printf("  - %s\n", e.Message)
		}
		os.Exit(1)
	}

	fmt.Println("Parsed successfully!")

	// Count server blocks
	servers, err := doc.BlocksOfType("server")
	if err != nil {
		fmt.Fprintf(os.Stderr, "blocks error: %v\n", err)
		os.Exit(1)
	}
	fmt.Printf("Server blocks: %d\n", len(servers))

	// Print server names and ports
	fmt.Println("\nServers:")
	values, err := doc.Values()
	if err != nil {
		fmt.Fprintf(os.Stderr, "values error: %v\n", err)
		os.Exit(1)
	}
	if serverMap, ok := values["server"].(map[string]any); ok {
		for name, v := range serverMap {
			if attrs, ok := v.(map[string]any); ok {
				fmt.Printf("  %s: port %v\n", name, attrs["port"])
			}
		}
	}

	// Query for servers with workers > 2
	fmt.Println("\nQuery: server | .workers > 2")
	result, err := doc.Query("server | .workers > 2")
	if err != nil {
		fmt.Printf("  Query error: %v\n", err)
	} else {
		fmt.Printf("  Result: %v\n", result)
	}

	// Print the users table
	fmt.Println("\nUsers table:")
	if users, ok := values["users"].([]any); ok {
		for _, row := range users {
			if r, ok := row.(map[string]any); ok {
				fmt.Printf("  %v | %v | admin=%v\n", r["name"], r["role"], r["admin"])
			}
		}
	}
}
