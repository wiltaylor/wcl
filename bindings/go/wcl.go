// Package wcl provides Go bindings for WCL (Wil's Configuration Language).
//
// It embeds a WASM module and uses wazero (a pure Go WebAssembly runtime) to
// execute the full WCL pipeline. Complex types cross the boundary as JSON strings.
package wcl

import (
	"encoding/json"
	"fmt"
	"os"
	"path/filepath"
	"runtime"

	"github.com/wiltaylor/wcl/bindings/go/wasm"
)

// Parse parses a WCL source string and returns a Document.
// The Document must be closed with Close() when no longer needed.
func Parse(source string, opts *ParseOptions) (*Document, error) {
	optsJSON := marshalOptions(opts)

	if opts != nil && len(opts.Functions) > 0 {
		return parseWithFunctions(source, optsJSON, opts.Functions)
	}

	rt := wasm.GetRuntime()
	handle := rt.Parse(source, optsJSON)
	if handle == 0 {
		return nil, fmt.Errorf("wcl: parse returned nil")
	}

	return newDocument(handle), nil
}

// ParseFile reads a file and parses it as WCL.
// Since the WASM module cannot access the host filesystem, the file is read
// in Go and the contents are passed to Parse. RootDir is set to the file's
// parent directory if not specified in options.
func ParseFile(path string, opts *ParseOptions) (*Document, error) {
	content, err := os.ReadFile(path)
	if err != nil {
		return nil, fmt.Errorf("wcl: %w", err)
	}

	if opts == nil {
		opts = &ParseOptions{}
	}
	if opts.RootDir == "" {
		opts.RootDir = filepath.Dir(path)
	}

	return Parse(string(content), opts)
}

func parseWithFunctions(source, optsJSON string, fns map[string]func([]any) (any, error)) (*Document, error) {
	names := make([]string, 0, len(fns))
	for name := range fns {
		names = append(names, name)
	}

	namesJSON, err := json.Marshal(names)
	if err != nil {
		return nil, fmt.Errorf("wcl: failed to marshal function names: %w", err)
	}

	bridge := wasm.CallbackBridge
	bridge.SetFunctions(fns)
	defer bridge.ClearFunctions()

	rt := wasm.GetRuntime()
	handle := rt.ParseWithFunctions(source, optsJSON, string(namesJSON))
	if handle == 0 {
		return nil, fmt.Errorf("wcl: parse returned nil")
	}

	return newDocument(handle), nil
}

func newDocument(handle uint32) *Document {
	doc := &Document{handle: handle}
	runtime.SetFinalizer(doc, (*Document).Close)
	return doc
}

// Values returns the evaluated top-level values as a map.
func (d *Document) Values() (map[string]any, error) {
	d.mu.RLock()
	defer d.mu.RUnlock()
	if d.closed {
		return nil, fmt.Errorf("wcl: document is closed")
	}

	jsonStr := wasm.GetRuntime().DocumentValues(d.handle)
	var result map[string]any
	if err := json.Unmarshal([]byte(jsonStr), &result); err != nil {
		return nil, fmt.Errorf("wcl: values: %w", err)
	}
	return result, nil
}

// HasErrors returns true if the document has any error diagnostics.
func (d *Document) HasErrors() bool {
	d.mu.RLock()
	defer d.mu.RUnlock()
	if d.closed {
		return false
	}
	return wasm.GetRuntime().DocumentHasErrors(d.handle)
}

// Errors returns only the error diagnostics.
func (d *Document) Errors() ([]Diagnostic, error) {
	d.mu.RLock()
	defer d.mu.RUnlock()
	if d.closed {
		return nil, fmt.Errorf("wcl: document is closed")
	}

	jsonStr := wasm.GetRuntime().DocumentDiagnostics(d.handle)
	var allDiags []Diagnostic
	if err := json.Unmarshal([]byte(jsonStr), &allDiags); err != nil {
		return nil, fmt.Errorf("wcl: errors: %w", err)
	}
	var errors []Diagnostic
	for _, d := range allDiags {
		if d.Severity == "error" {
			errors = append(errors, d)
		}
	}
	return errors, nil
}

// Diagnostics returns all diagnostics (errors, warnings, etc.).
func (d *Document) Diagnostics() ([]Diagnostic, error) {
	d.mu.RLock()
	defer d.mu.RUnlock()
	if d.closed {
		return nil, fmt.Errorf("wcl: document is closed")
	}

	jsonStr := wasm.GetRuntime().DocumentDiagnostics(d.handle)
	var diags []Diagnostic
	if err := json.Unmarshal([]byte(jsonStr), &diags); err != nil {
		return nil, fmt.Errorf("wcl: diagnostics: %w", err)
	}
	return diags, nil
}

// Query executes a WCL query against the document.
func (d *Document) Query(query string) (any, error) {
	d.mu.RLock()
	defer d.mu.RUnlock()
	if d.closed {
		return nil, fmt.Errorf("wcl: document is closed")
	}

	jsonStr := wasm.GetRuntime().DocumentQuery(d.handle, query)
	var result struct {
		Ok    any     `json:"ok"`
		Error *string `json:"error"`
	}
	if err := json.Unmarshal([]byte(jsonStr), &result); err != nil {
		return nil, fmt.Errorf("wcl: query: %w", err)
	}
	if result.Error != nil {
		return nil, fmt.Errorf("wcl: query: %s", *result.Error)
	}
	return result.Ok, nil
}

// Blocks returns all top-level blocks.
func (d *Document) Blocks() ([]BlockRef, error) {
	d.mu.RLock()
	defer d.mu.RUnlock()
	if d.closed {
		return nil, fmt.Errorf("wcl: document is closed")
	}

	jsonStr := wasm.GetRuntime().DocumentBlocks(d.handle)
	var blocks []BlockRef
	if err := json.Unmarshal([]byte(jsonStr), &blocks); err != nil {
		return nil, fmt.Errorf("wcl: blocks: %w", err)
	}
	return blocks, nil
}

// BlocksOfType returns blocks of the specified type.
func (d *Document) BlocksOfType(kind string) ([]BlockRef, error) {
	d.mu.RLock()
	defer d.mu.RUnlock()
	if d.closed {
		return nil, fmt.Errorf("wcl: document is closed")
	}

	jsonStr := wasm.GetRuntime().DocumentBlocksOfType(d.handle, kind)
	var blocks []BlockRef
	if err := json.Unmarshal([]byte(jsonStr), &blocks); err != nil {
		return nil, fmt.Errorf("wcl: blocks_of_type: %w", err)
	}
	return blocks, nil
}

// Close releases the underlying WASM resources.
// Safe to call multiple times.
func (d *Document) Close() {
	d.mu.Lock()
	defer d.mu.Unlock()
	if d.closed {
		return
	}
	d.closed = true
	runtime.SetFinalizer(d, nil)

	if d.handle != 0 {
		wasm.GetRuntime().DocumentFree(d.handle)
		d.handle = 0
	}
}

// ── Helpers ──────────────────────────────────────────────────────────────

func marshalOptions(opts *ParseOptions) string {
	if opts == nil {
		return ""
	}
	m := make(map[string]any)
	if opts.RootDir != "" {
		m["rootDir"] = opts.RootDir
	}
	if opts.AllowImports != nil {
		m["allowImports"] = *opts.AllowImports
	}
	if opts.MaxImportDepth != 0 {
		m["maxImportDepth"] = opts.MaxImportDepth
	}
	if opts.MaxMacroDepth != 0 {
		m["maxMacroDepth"] = opts.MaxMacroDepth
	}
	if opts.MaxLoopDepth != 0 {
		m["maxLoopDepth"] = opts.MaxLoopDepth
	}
	if opts.MaxIterations != 0 {
		m["maxIterations"] = opts.MaxIterations
	}
	if len(opts.Variables) > 0 {
		m["variables"] = opts.Variables
	}
	if len(m) == 0 {
		return ""
	}
	b, _ := json.Marshal(m)
	return string(b)
}
