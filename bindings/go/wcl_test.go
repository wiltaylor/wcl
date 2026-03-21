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
