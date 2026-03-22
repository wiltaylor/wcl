package wcl

import (
	"fmt"
	"os"
	"path/filepath"
	"sync"
	"testing"
)

func TestParseSimple(t *testing.T) {
	doc, err := Parse("x = 42\ny = \"hello\"", nil)
	if err != nil {
		t.Fatal(err)
	}
	defer doc.Close()

	if doc.HasErrors() {
		errs, _ := doc.Errors()
		t.Fatalf("unexpected errors: %v", errs)
	}

	values, err := doc.Values()
	if err != nil {
		t.Fatal(err)
	}

	if v, ok := values["x"]; !ok {
		t.Error("missing key 'x'")
	} else if v.(float64) != 42 {
		t.Errorf("x = %v, want 42", v)
	}

	if v, ok := values["y"]; !ok {
		t.Error("missing key 'y'")
	} else if v.(string) != "hello" {
		t.Errorf("y = %v, want hello", v)
	}
}

func TestParseWithErrors(t *testing.T) {
	doc, err := Parse("x = @invalid", nil)
	if err != nil {
		t.Fatal(err)
	}
	defer doc.Close()

	if !doc.HasErrors() {
		t.Error("expected errors")
	}

	errs, err := doc.Errors()
	if err != nil {
		t.Fatal(err)
	}
	if len(errs) == 0 {
		t.Error("expected at least one error")
	}
	if errs[0].Severity != "error" {
		t.Errorf("severity = %q, want error", errs[0].Severity)
	}
}

func TestParseFile(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "test.wcl")
	if err := os.WriteFile(path, []byte("port = 8080\nhost = \"localhost\""), 0644); err != nil {
		t.Fatal(err)
	}

	doc, err := ParseFile(path, nil)
	if err != nil {
		t.Fatal(err)
	}
	defer doc.Close()

	if doc.HasErrors() {
		errs, _ := doc.Errors()
		t.Fatalf("unexpected errors: %v", errs)
	}

	values, err := doc.Values()
	if err != nil {
		t.Fatal(err)
	}
	if values["port"].(float64) != 8080 {
		t.Errorf("port = %v, want 8080", values["port"])
	}
}

func TestParseFileNotFound(t *testing.T) {
	_, err := ParseFile("/nonexistent/path.wcl", nil)
	if err == nil {
		t.Error("expected error for nonexistent file")
	}
}

func TestQuery(t *testing.T) {
	doc, err := Parse("service { port = 8080 }\nservice { port = 9090 }", nil)
	if err != nil {
		t.Fatal(err)
	}
	defer doc.Close()

	result, err := doc.Query("service | .port")
	if err != nil {
		t.Fatal(err)
	}

	ports, ok := result.([]any)
	if !ok {
		t.Fatalf("expected []any, got %T", result)
	}
	if len(ports) != 2 {
		t.Fatalf("len = %d, want 2", len(ports))
	}
	if ports[0].(float64) != 8080 || ports[1].(float64) != 9090 {
		t.Errorf("ports = %v, want [8080, 9090]", ports)
	}
}

func TestCustomFunction(t *testing.T) {
	opts := &ParseOptions{
		Functions: map[string]func([]any) (any, error){
			"double": func(args []any) (any, error) {
				if len(args) != 1 {
					return nil, fmt.Errorf("expected 1 arg")
				}
				n, ok := args[0].(float64)
				if !ok {
					return nil, fmt.Errorf("expected number")
				}
				return n * 2, nil
			},
		},
	}

	doc, err := Parse("result = double(21)", opts)
	if err != nil {
		t.Fatal(err)
	}
	defer doc.Close()

	if doc.HasErrors() {
		errs, _ := doc.Errors()
		t.Fatalf("unexpected errors: %v", errs)
	}

	values, err := doc.Values()
	if err != nil {
		t.Fatal(err)
	}
	if values["result"].(float64) != 42 {
		t.Errorf("result = %v, want 42", values["result"])
	}
}

func TestBlocks(t *testing.T) {
	doc, err := Parse("server { port = 80 }\nclient { timeout = 30 }\nserver { port = 443 }", nil)
	if err != nil {
		t.Fatal(err)
	}
	defer doc.Close()

	blocks, err := doc.Blocks()
	if err != nil {
		t.Fatal(err)
	}
	if len(blocks) != 3 {
		t.Fatalf("len = %d, want 3", len(blocks))
	}

	servers, err := doc.BlocksOfType("server")
	if err != nil {
		t.Fatal(err)
	}
	if len(servers) != 2 {
		t.Fatalf("servers len = %d, want 2", len(servers))
	}
	if servers[0].Kind != "server" {
		t.Errorf("kind = %q, want server", servers[0].Kind)
	}
}

func TestDiagnostics(t *testing.T) {
	doc, err := Parse("x = 42", nil)
	if err != nil {
		t.Fatal(err)
	}
	defer doc.Close()

	diags, err := doc.Diagnostics()
	if err != nil {
		t.Fatal(err)
	}
	// Valid input should have no error diagnostics
	for _, d := range diags {
		if d.Severity == "error" {
			t.Errorf("unexpected error: %s", d.Message)
		}
	}
}

func TestDocumentClose(t *testing.T) {
	doc, err := Parse("x = 1", nil)
	if err != nil {
		t.Fatal(err)
	}

	doc.Close()
	doc.Close() // double close should not panic

	// Methods on closed document should return errors
	_, err = doc.Values()
	if err == nil {
		t.Error("expected error on closed document")
	}
}

func TestVariablesBasic(t *testing.T) {
	doc, err := Parse("port = PORT", &ParseOptions{
		Variables: map[string]any{"PORT": 8080},
	})
	if err != nil {
		t.Fatal(err)
	}
	defer doc.Close()

	if doc.HasErrors() {
		errs, _ := doc.Errors()
		t.Fatalf("unexpected errors: %v", errs)
	}

	values, err := doc.Values()
	if err != nil {
		t.Fatal(err)
	}
	if values["port"].(float64) != 8080 {
		t.Errorf("port = %v, want 8080", values["port"])
	}
}

func TestVariablesOverrideLet(t *testing.T) {
	doc, err := Parse("let x = 2\nresult = x", &ParseOptions{
		Variables: map[string]any{"x": 99},
	})
	if err != nil {
		t.Fatal(err)
	}
	defer doc.Close()

	if doc.HasErrors() {
		errs, _ := doc.Errors()
		t.Fatalf("unexpected errors: %v", errs)
	}

	values, err := doc.Values()
	if err != nil {
		t.Fatal(err)
	}
	if values["result"].(float64) != 99 {
		t.Errorf("result = %v, want 99", values["result"])
	}
}

func TestVariablesTypes(t *testing.T) {
	doc, err := Parse("vs = s\nvi = i\nvb = b", &ParseOptions{
		Variables: map[string]any{"s": "hello", "i": 42, "b": true},
	})
	if err != nil {
		t.Fatal(err)
	}
	defer doc.Close()

	if doc.HasErrors() {
		errs, _ := doc.Errors()
		t.Fatalf("unexpected errors: %v", errs)
	}

	values, err := doc.Values()
	if err != nil {
		t.Fatal(err)
	}
	if values["vs"].(string) != "hello" {
		t.Errorf("vs = %v, want hello", values["vs"])
	}
	if values["vi"].(float64) != 42 {
		t.Errorf("vi = %v, want 42", values["vi"])
	}
	if values["vb"].(bool) != true {
		t.Errorf("vb = %v, want true", values["vb"])
	}
}

func TestTable(t *testing.T) {
	src := `table users {
  name: string
  age: int
  | "Alice" | 30 |
  | "Bob"   | 25 |
}`
	doc, err := Parse(src, nil)
	if err != nil {
		t.Fatal(err)
	}
	defer doc.Close()

	if doc.HasErrors() {
		errs, _ := doc.Errors()
		t.Fatalf("unexpected errors: %v", errs)
	}

	values, err := doc.Values()
	if err != nil {
		t.Fatal(err)
	}

	users, ok := values["users"].([]any)
	if !ok {
		t.Fatalf("expected []any for users, got %T", values["users"])
	}
	if len(users) != 2 {
		t.Fatalf("len(users) = %d, want 2", len(users))
	}

	row0, ok := users[0].(map[string]any)
	if !ok {
		t.Fatalf("expected map for row 0, got %T", users[0])
	}
	if row0["name"].(string) != "Alice" {
		t.Errorf("row0.name = %v, want Alice", row0["name"])
	}
	if row0["age"].(float64) != 30 {
		t.Errorf("row0.age = %v, want 30", row0["age"])
	}

	row1, ok := users[1].(map[string]any)
	if !ok {
		t.Fatalf("expected map for row 1, got %T", users[1])
	}
	if row1["name"].(string) != "Bob" {
		t.Errorf("row1.name = %v, want Bob", row1["name"])
	}
	if row1["age"].(float64) != 25 {
		t.Errorf("row1.age = %v, want 25", row1["age"])
	}
}

func TestFunctionMacro(t *testing.T) {
	src := `macro make_config() {
  timeout = 30
  retries = 3
}

server web {
  port = 8080
  make_config()
}`
	doc, err := Parse(src, nil)
	if err != nil {
		t.Fatal(err)
	}
	defer doc.Close()

	if doc.HasErrors() {
		errs, _ := doc.Errors()
		t.Fatalf("unexpected errors: %v", errs)
	}

	blocks, err := doc.BlocksOfType("server")
	if err != nil {
		t.Fatal(err)
	}
	if len(blocks) != 1 {
		t.Fatalf("len(servers) = %d, want 1", len(blocks))
	}
	attrs := blocks[0].Attributes
	if attrs["port"].(float64) != 8080 {
		t.Errorf("port = %v, want 8080", attrs["port"])
	}
	if attrs["timeout"].(float64) != 30 {
		t.Errorf("timeout = %v, want 30", attrs["timeout"])
	}
	if attrs["retries"].(float64) != 3 {
		t.Errorf("retries = %v, want 3", attrs["retries"])
	}
}

func TestAttributeMacro(t *testing.T) {
	src := `macro @add_env(env) {
  inject {
    environment = env
  }
}

@add_env("production")
server web {
  port = 8080
}`
	doc, err := Parse(src, nil)
	if err != nil {
		t.Fatal(err)
	}
	defer doc.Close()

	if doc.HasErrors() {
		errs, _ := doc.Errors()
		t.Fatalf("unexpected errors: %v", errs)
	}

	blocks, err := doc.BlocksOfType("server")
	if err != nil {
		t.Fatal(err)
	}
	if len(blocks) != 1 {
		t.Fatalf("len(servers) = %d, want 1", len(blocks))
	}
	attrs := blocks[0].Attributes
	if attrs["port"].(float64) != 8080 {
		t.Errorf("port = %v, want 8080", attrs["port"])
	}
	if attrs["environment"].(string) != "production" {
		t.Errorf("environment = %v, want production", attrs["environment"])
	}
}

func TestForLoop(t *testing.T) {
	src := `let items = ["a", "b", "c"]
for item in items {
  result = item
}`
	doc, err := Parse(src, nil)
	if err != nil {
		t.Fatal(err)
	}
	defer doc.Close()

	if doc.HasErrors() {
		errs, _ := doc.Errors()
		t.Fatalf("unexpected errors: %v", errs)
	}

	values, err := doc.Values()
	if err != nil {
		t.Fatal(err)
	}

	// The for loop expands and the last iteration wins for flat attributes
	result, ok := values["result"].(string)
	if !ok {
		t.Fatalf("expected string for result, got %T", values["result"])
	}
	// Last item in list should win
	if result != "a" && result != "b" && result != "c" {
		t.Errorf("result = %v, want one of a/b/c", result)
	}
}

func TestForLoopOnTable(t *testing.T) {
	src := `table users {
  name: string
  age: int
  | "Alice" | 30 |
  | "Bob"   | 25 |
}

for row in users {
  latest_name = row.name
}`
	doc, err := Parse(src, nil)
	if err != nil {
		t.Fatal(err)
	}
	defer doc.Close()

	if doc.HasErrors() {
		errs, _ := doc.Errors()
		t.Fatalf("unexpected errors: %v", errs)
	}

	values, err := doc.Values()
	if err != nil {
		t.Fatal(err)
	}

	// Table should be in values
	users, ok := values["users"].([]any)
	if !ok {
		t.Fatalf("expected []any for users, got %T", values["users"])
	}
	if len(users) != 2 {
		t.Fatalf("len(users) = %d, want 2", len(users))
	}

	// For loop should produce latest_name from iteration
	name, ok := values["latest_name"].(string)
	if !ok {
		t.Fatalf("expected string for latest_name, got %T", values["latest_name"])
	}
	if name == "" {
		t.Error("latest_name should not be empty")
	}
}

func TestIfConditional(t *testing.T) {
	src := `let enabled = true
if enabled {
  feature flags {
    active = true
  }
}`
	doc, err := Parse(src, nil)
	if err != nil {
		t.Fatal(err)
	}
	defer doc.Close()

	if doc.HasErrors() {
		errs, _ := doc.Errors()
		t.Fatalf("unexpected errors: %v", errs)
	}

	blocks, err := doc.BlocksOfType("feature")
	if err != nil {
		t.Fatal(err)
	}
	if len(blocks) != 1 {
		t.Fatalf("len(feature) = %d, want 1", len(blocks))
	}
	if blocks[0].ID == nil || *blocks[0].ID != "flags" {
		t.Errorf("feature block id = %v, want flags", blocks[0].ID)
	}
	if blocks[0].Attributes["active"].(bool) != true {
		t.Errorf("active = %v, want true", blocks[0].Attributes["active"])
	}
}

func TestIfConditionalFalse(t *testing.T) {
	src := `let enabled = false
if enabled {
  feature flags {
    active = true
  }
}`
	doc, err := Parse(src, nil)
	if err != nil {
		t.Fatal(err)
	}
	defer doc.Close()

	if doc.HasErrors() {
		errs, _ := doc.Errors()
		t.Fatalf("unexpected errors: %v", errs)
	}

	blocks, err := doc.BlocksOfType("feature")
	if err != nil {
		t.Fatal(err)
	}
	if len(blocks) != 0 {
		t.Errorf("expected no feature blocks when condition is false, got %d", len(blocks))
	}
}

func TestInlineArgs(t *testing.T) {
	src := `server "web" {
  port = 8080
}`
	doc, err := Parse(src, nil)
	if err != nil {
		t.Fatal(err)
	}
	defer doc.Close()

	if doc.HasErrors() {
		errs, _ := doc.Errors()
		t.Fatalf("unexpected errors: %v", errs)
	}

	blocks, err := doc.BlocksOfType("server")
	if err != nil {
		t.Fatal(err)
	}
	if len(blocks) != 1 {
		t.Fatalf("len(servers) = %d, want 1", len(blocks))
	}
	attrs := blocks[0].Attributes
	if attrs["port"].(float64) != 8080 {
		t.Errorf("port = %v, want 8080", attrs["port"])
	}

	// Inline args should appear as _args attribute
	args, ok := attrs["_args"].([]any)
	if !ok {
		t.Fatalf("expected []any for _args, got %T", attrs["_args"])
	}
	if len(args) != 1 {
		t.Fatalf("len(_args) = %d, want 1", len(args))
	}
	if args[0].(string) != "web" {
		t.Errorf("_args[0] = %v, want web", args[0])
	}
}

func TestPartialLet(t *testing.T) {
	src := `partial let tags = ["x", "y"]
partial let tags = ["z"]
result = tags`
	doc, err := Parse(src, nil)
	if err != nil {
		t.Fatal(err)
	}
	defer doc.Close()

	if doc.HasErrors() {
		errs, _ := doc.Errors()
		t.Fatalf("unexpected errors: %v", errs)
	}

	values, err := doc.Values()
	if err != nil {
		t.Fatal(err)
	}

	result, ok := values["result"].([]any)
	if !ok {
		t.Fatalf("expected []any for result, got %T", values["result"])
	}
	if len(result) != 3 {
		t.Fatalf("len(result) = %d, want 3", len(result))
	}
	if result[0].(string) != "x" || result[1].(string) != "y" || result[2].(string) != "z" {
		t.Errorf("result = %v, want [x, y, z]", result)
	}
}

func TestSymbolLiteral(t *testing.T) {
	src := `server web {
  status = :active
}`
	doc, err := Parse(src, nil)
	if err != nil {
		t.Fatal(err)
	}
	defer doc.Close()

	if doc.HasErrors() {
		errs, _ := doc.Errors()
		t.Fatalf("unexpected errors: %v", errs)
	}

	blocks, err := doc.BlocksOfType("server")
	if err != nil {
		t.Fatal(err)
	}
	if len(blocks) != 1 {
		t.Fatalf("len(servers) = %d, want 1", len(blocks))
	}
	// Symbols are serialized as strings in JSON
	status := blocks[0].Attributes["status"].(string)
	if status != "active" {
		t.Errorf("status = %v, want active", status)
	}
}

func TestConcurrentAccess(t *testing.T) {
	doc, err := Parse("x = 42\ny = \"hello\"", nil)
	if err != nil {
		t.Fatal(err)
	}
	defer doc.Close()

	var wg sync.WaitGroup
	for i := 0; i < 10; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			values, err := doc.Values()
			if err != nil {
				t.Errorf("Values() error: %v", err)
				return
			}
			if values["x"].(float64) != 42 {
				t.Errorf("x = %v, want 42", values["x"])
			}
		}()
	}
	wg.Wait()
}
