// Package wcl provides Go bindings for WCL (Wil's Configuration Language).
//
// It uses a prebuilt static library (libwcl_ffi) via CGo. Documents are parsed
// and evaluated through the full WCL pipeline. Complex types cross the FFI
// boundary as JSON strings.
package wcl

/*
#include "wcl.h"
#include <stdlib.h>

extern char* goCallbackTrampoline(void* ctx, char* args_json);
*/
import "C"
import (
	"encoding/json"
	"fmt"
	"runtime"
	"unsafe"
)

// Parse parses a WCL source string and returns a Document.
// The Document must be closed with Close() when no longer needed.
func Parse(source string, opts *ParseOptions) (*Document, error) {
	optsJSON := marshalOptions(opts)

	if opts != nil && len(opts.Functions) > 0 {
		return parseWithFunctions(source, optsJSON, opts.Functions)
	}

	ptr := cParse(source, optsJSON)
	if ptr == nil {
		return nil, fmt.Errorf("wcl: parse returned nil")
	}

	return newDocument(ptr, nil), nil
}

// ParseFile reads a file and parses it as WCL.
func ParseFile(path string, opts *ParseOptions) (*Document, error) {
	optsJSON := marshalOptions(opts)
	ptr := cParseFile(path, optsJSON)
	if ptr == nil {
		errMsg := cLastError()
		if errMsg != "" {
			return nil, fmt.Errorf("wcl: %s", errMsg)
		}
		return nil, fmt.Errorf("wcl: failed to parse file %s", path)
	}

	return newDocument(ptr, nil), nil
}

func parseWithFunctions(source, optsJSON string, fns map[string]func([]any) (any, error)) (*Document, error) {
	count := len(fns)
	names := make([]*C.char, 0, count)
	callbacks := make([]C.WclCallbackFn, 0, count)
	contexts := make([]C.uintptr_t, 0, count)
	cbIDs := make([]uintptr, 0, count)

	for name, fn := range fns {
		cName := C.CString(name)
		defer C.free(unsafe.Pointer(cName))

		id := registerCallback(fn)
		cbIDs = append(cbIDs, id)

		names = append(names, cName)
		callbacks = append(callbacks, C.WclCallbackFn(C.goCallbackTrampoline))
		contexts = append(contexts, C.uintptr_t(id))
	}

	ptr := cParseWithFunctions(source, optsJSON, names, callbacks, contexts)
	if ptr == nil {
		for _, id := range cbIDs {
			unregisterCallback(id)
		}
		return nil, fmt.Errorf("wcl: parse returned nil")
	}

	return newDocument(ptr, cbIDs), nil
}

func newDocument(ptr unsafe.Pointer, cbIDs []uintptr) *Document {
	doc := &Document{ptr: ptr, callbackIDs: cbIDs}
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

	jsonStr := cDocumentValues(d.ptr)
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
	return cDocumentHasErrors(d.ptr)
}

// Errors returns only the error diagnostics.
func (d *Document) Errors() ([]Diagnostic, error) {
	d.mu.RLock()
	defer d.mu.RUnlock()
	if d.closed {
		return nil, fmt.Errorf("wcl: document is closed")
	}

	jsonStr := cDocumentErrors(d.ptr)
	var diags []Diagnostic
	if err := json.Unmarshal([]byte(jsonStr), &diags); err != nil {
		return nil, fmt.Errorf("wcl: errors: %w", err)
	}
	return diags, nil
}

// Diagnostics returns all diagnostics (errors, warnings, etc.).
func (d *Document) Diagnostics() ([]Diagnostic, error) {
	d.mu.RLock()
	defer d.mu.RUnlock()
	if d.closed {
		return nil, fmt.Errorf("wcl: document is closed")
	}

	jsonStr := cDocumentDiagnostics(d.ptr)
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

	jsonStr := cDocumentQuery(d.ptr, query)
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

	jsonStr := cDocumentBlocks(d.ptr)
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

	jsonStr := cDocumentBlocksOfType(d.ptr, kind)
	var blocks []BlockRef
	if err := json.Unmarshal([]byte(jsonStr), &blocks); err != nil {
		return nil, fmt.Errorf("wcl: blocks_of_type: %w", err)
	}
	return blocks, nil
}

// Close releases the underlying Rust resources.
// Safe to call multiple times.
func (d *Document) Close() {
	d.mu.Lock()
	defer d.mu.Unlock()
	if d.closed {
		return
	}
	d.closed = true
	runtime.SetFinalizer(d, nil)

	if d.ptr != nil {
		cDocumentFree(d.ptr)
		d.ptr = nil
	}

	for _, id := range d.callbackIDs {
		unregisterCallback(id)
	}
	d.callbackIDs = nil
}

// ListLibraries returns the paths of installed WCL libraries.
func ListLibraries() ([]string, error) {
	jsonStr := cListLibraries()
	var result struct {
		Ok    []string `json:"ok"`
		Error *string  `json:"error"`
	}
	if err := json.Unmarshal([]byte(jsonStr), &result); err != nil {
		return nil, fmt.Errorf("wcl: list_libraries: %w", err)
	}
	if result.Error != nil {
		return nil, fmt.Errorf("wcl: %s", *result.Error)
	}
	return result.Ok, nil
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
	if len(m) == 0 {
		return ""
	}
	b, _ := json.Marshal(m)
	return string(b)
}
