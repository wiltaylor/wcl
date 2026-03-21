// Package wasm provides the WASM runtime for the WCL Go bindings using wazero.
package wasm

import (
	"context"
	_ "embed"
	"encoding/binary"
	"fmt"
	"sync"

	"github.com/tetratelabs/wazero"
	"github.com/tetratelabs/wazero/api"
	"github.com/tetratelabs/wazero/imports/wasi_snapshot_preview1"
)

//go:embed wcl_wasm.wasm
var wasmBytes []byte

var (
	runtimeOnce sync.Once
	runtimeInst *Runtime
)

// GetRuntime returns the singleton WASM runtime instance.
func GetRuntime() *Runtime {
	runtimeOnce.Do(func() {
		runtimeInst = newRuntime()
	})
	return runtimeInst
}

// Runtime manages a wazero WASM instance for WCL operations.
type Runtime struct {
	mu  sync.Mutex
	rt  wazero.Runtime
	mod api.Module

	// Cached exported functions
	fnAlloc              api.Function
	fnDealloc            api.Function
	fnParse              api.Function
	fnParseWithFunctions api.Function
	fnDocumentFree       api.Function
	fnDocumentValues     api.Function
	fnDocumentHasErrors  api.Function
	fnDocumentDiagnostics api.Function
	fnDocumentQuery      api.Function
	fnDocumentBlocks     api.Function
	fnDocumentBlocksOfType api.Function
	fnStringFree         api.Function
}

func newRuntime() *Runtime {
	ctx := context.Background()

	rt := wazero.NewRuntime(ctx)

	// Instantiate WASI
	wasi_snapshot_preview1.MustInstantiate(ctx, rt)

	// Register host_call_function import
	_, err := rt.NewHostModuleBuilder("env").
		NewFunctionBuilder().
		WithFunc(hostCallFunction).
		WithParameterNames("name_ptr", "name_len", "args_ptr", "args_len", "result_ptr_out", "result_len_out").
		Export("host_call_function").
		Instantiate(ctx)
	if err != nil {
		panic(fmt.Sprintf("wcl: failed to register host functions: %v", err))
	}

	mod, err := rt.Instantiate(ctx, wasmBytes)
	if err != nil {
		panic(fmt.Sprintf("wcl: failed to instantiate WASM module: %v", err))
	}

	r := &Runtime{
		rt:  rt,
		mod: mod,
	}

	r.fnAlloc = mod.ExportedFunction("wcl_wasm_alloc")
	r.fnDealloc = mod.ExportedFunction("wcl_wasm_dealloc")
	r.fnParse = mod.ExportedFunction("wcl_wasm_parse")
	r.fnParseWithFunctions = mod.ExportedFunction("wcl_wasm_parse_with_functions")
	r.fnDocumentFree = mod.ExportedFunction("wcl_wasm_document_free")
	r.fnDocumentValues = mod.ExportedFunction("wcl_wasm_document_values")
	r.fnDocumentHasErrors = mod.ExportedFunction("wcl_wasm_document_has_errors")
	r.fnDocumentDiagnostics = mod.ExportedFunction("wcl_wasm_document_diagnostics")
	r.fnDocumentQuery = mod.ExportedFunction("wcl_wasm_document_query")
	r.fnDocumentBlocks = mod.ExportedFunction("wcl_wasm_document_blocks")
	r.fnDocumentBlocksOfType = mod.ExportedFunction("wcl_wasm_document_blocks_of_type")
	r.fnStringFree = mod.ExportedFunction("wcl_wasm_string_free")

	return r
}

// writeString allocates memory in WASM, copies the string with a null terminator,
// and returns the pointer. Returns 0 for empty strings.
func (r *Runtime) writeString(s string) uint32 {
	if s == "" {
		return 0
	}
	ctx := context.Background()
	b := []byte(s)
	size := uint64(len(b) + 1)

	results, err := r.fnAlloc.Call(ctx, size)
	if err != nil {
		panic(fmt.Sprintf("wcl: alloc failed: %v", err))
	}
	ptr := uint32(results[0])

	mem := r.mod.Memory()
	mem.Write(ptr, b)
	mem.WriteByte(ptr+uint32(len(b)), 0) // null terminator

	return ptr
}

// readCString reads a null-terminated C string from WASM memory.
func (r *Runtime) readCString(ptr uint32) string {
	if ptr == 0 {
		return ""
	}
	mem := r.mod.Memory()
	end := ptr
	for {
		b, ok := mem.ReadByte(end)
		if !ok || b == 0 {
			break
		}
		end++
	}
	buf, ok := mem.Read(ptr, end-ptr)
	if !ok {
		return ""
	}
	return string(buf)
}

// consumeString reads a null-terminated C string and then frees it.
func (r *Runtime) consumeString(ptr uint32) string {
	if ptr == 0 {
		return ""
	}
	s := r.readCString(ptr)
	r.fnStringFree.Call(context.Background(), uint64(ptr))
	return s
}

// deallocString frees a string that was written with writeString.
func (r *Runtime) deallocString(ptr uint32, s string) {
	if ptr == 0 {
		return
	}
	r.fnDealloc.Call(context.Background(), uint64(ptr), uint64(len(s)+1))
}

// Parse parses WCL source and returns a document handle.
func (r *Runtime) Parse(source, optionsJSON string) uint32 {
	r.mu.Lock()
	defer r.mu.Unlock()

	ctx := context.Background()
	srcPtr := r.writeString(source)
	optsPtr := r.writeString(optionsJSON)
	defer r.deallocString(srcPtr, source)
	defer r.deallocString(optsPtr, optionsJSON)

	results, err := r.fnParse.Call(ctx, uint64(srcPtr), uint64(optsPtr))
	if err != nil {
		return 0
	}
	return uint32(results[0])
}

// ParseWithFunctions parses WCL source with custom function callbacks.
func (r *Runtime) ParseWithFunctions(source, optionsJSON, funcNamesJSON string) uint32 {
	r.mu.Lock()
	defer r.mu.Unlock()

	ctx := context.Background()
	srcPtr := r.writeString(source)
	optsPtr := r.writeString(optionsJSON)
	namesPtr := r.writeString(funcNamesJSON)
	defer r.deallocString(srcPtr, source)
	defer r.deallocString(optsPtr, optionsJSON)
	defer r.deallocString(namesPtr, funcNamesJSON)

	results, err := r.fnParseWithFunctions.Call(ctx, uint64(srcPtr), uint64(optsPtr), uint64(namesPtr))
	if err != nil {
		return 0
	}
	return uint32(results[0])
}

// DocumentFree releases a document handle.
func (r *Runtime) DocumentFree(handle uint32) {
	r.mu.Lock()
	defer r.mu.Unlock()
	r.fnDocumentFree.Call(context.Background(), uint64(handle))
}

// DocumentValues returns the evaluated values as a JSON string.
func (r *Runtime) DocumentValues(handle uint32) string {
	r.mu.Lock()
	defer r.mu.Unlock()
	results, err := r.fnDocumentValues.Call(context.Background(), uint64(handle))
	if err != nil {
		return "{}"
	}
	return r.consumeString(uint32(results[0]))
}

// DocumentHasErrors returns true if the document has errors.
func (r *Runtime) DocumentHasErrors(handle uint32) bool {
	r.mu.Lock()
	defer r.mu.Unlock()
	results, err := r.fnDocumentHasErrors.Call(context.Background(), uint64(handle))
	if err != nil {
		return false
	}
	return results[0] != 0
}

// DocumentDiagnostics returns all diagnostics as a JSON array string.
func (r *Runtime) DocumentDiagnostics(handle uint32) string {
	r.mu.Lock()
	defer r.mu.Unlock()
	results, err := r.fnDocumentDiagnostics.Call(context.Background(), uint64(handle))
	if err != nil {
		return "[]"
	}
	return r.consumeString(uint32(results[0]))
}

// DocumentQuery executes a query and returns the result as a JSON string.
func (r *Runtime) DocumentQuery(handle uint32, query string) string {
	r.mu.Lock()
	defer r.mu.Unlock()

	ctx := context.Background()
	qPtr := r.writeString(query)
	defer r.deallocString(qPtr, query)

	results, err := r.fnDocumentQuery.Call(ctx, uint64(handle), uint64(qPtr))
	if err != nil {
		return `{"error":"wasm call failed"}`
	}
	return r.consumeString(uint32(results[0]))
}

// DocumentBlocks returns all blocks as a JSON array string.
func (r *Runtime) DocumentBlocks(handle uint32) string {
	r.mu.Lock()
	defer r.mu.Unlock()
	results, err := r.fnDocumentBlocks.Call(context.Background(), uint64(handle))
	if err != nil {
		return "[]"
	}
	return r.consumeString(uint32(results[0]))
}

// DocumentBlocksOfType returns blocks of the given type as a JSON array string.
func (r *Runtime) DocumentBlocksOfType(handle uint32, kind string) string {
	r.mu.Lock()
	defer r.mu.Unlock()

	ctx := context.Background()
	kPtr := r.writeString(kind)
	defer r.deallocString(kPtr, kind)

	results, err := r.fnDocumentBlocksOfType.Call(ctx, uint64(handle), uint64(kPtr))
	if err != nil {
		return "[]"
	}
	return r.consumeString(uint32(results[0]))
}

// Close releases all wazero resources.
func (r *Runtime) Close() {
	r.mu.Lock()
	defer r.mu.Unlock()
	ctx := context.Background()
	if r.mod != nil {
		r.mod.Close(ctx)
	}
	if r.rt != nil {
		r.rt.Close(ctx)
	}
}

// hostCallFunction is the host import called by the WASM module for custom function callbacks.
// Signature: (name_ptr, name_len, args_ptr, args_len, result_ptr_out, result_len_out) -> i32
func hostCallFunction(ctx context.Context, mod api.Module, namePtr, nameLen, argsPtr, argsLen, resultPtrOut, resultLenOut uint32) int32 {
	mem := mod.Memory()

	nameBytes, ok := mem.Read(namePtr, nameLen)
	if !ok {
		return -1
	}
	name := string(nameBytes)

	argsBytes, ok := mem.Read(argsPtr, argsLen)
	if !ok {
		return -1
	}
	argsJSON := string(argsBytes)

	success, resultJSON := CallbackBridge.Invoke(name, argsJSON)

	if resultJSON != "" {
		resultBytes := []byte(resultJSON)
		allocFn := mod.ExportedFunction("wcl_wasm_alloc")
		results, err := allocFn.Call(ctx, uint64(len(resultBytes)))
		if err != nil {
			return -1
		}
		ptr := uint32(results[0])
		mem.Write(ptr, resultBytes)

		// Write pointer and length to the output locations
		ptrBuf := make([]byte, 4)
		lenBuf := make([]byte, 4)
		binary.LittleEndian.PutUint32(ptrBuf, ptr)
		binary.LittleEndian.PutUint32(lenBuf, uint32(len(resultBytes)))
		mem.Write(resultPtrOut, ptrBuf)
		mem.Write(resultLenOut, lenBuf)
	}

	if success {
		return 0
	}
	return -1
}
