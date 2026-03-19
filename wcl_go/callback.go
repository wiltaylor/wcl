package wcl

/*
#include "wcl.h"
#include <stdlib.h>

// Forward declaration for the Go trampoline.
extern char* goCallbackTrampoline(void* ctx, char* args_json);
*/
import "C"
import (
	"encoding/json"
	"fmt"
	"sync"
	"sync/atomic"
	"unsafe"
)

// callbackRegistry stores Go functions keyed by an auto-incrementing ID.
// The ID is passed as the void* context to the C callback, avoiding
// the unsafe uintptr → unsafe.Pointer conversion that go vet flags.
var (
	callbackMu    sync.RWMutex
	callbackMap   = make(map[uintptr]func([]any) (any, error))
	callbackSeqID atomic.Uintptr
)

func registerCallback(fn func([]any) (any, error)) uintptr {
	id := callbackSeqID.Add(1)
	callbackMu.Lock()
	callbackMap[id] = fn
	callbackMu.Unlock()
	return id
}

func unregisterCallback(id uintptr) {
	callbackMu.Lock()
	delete(callbackMap, id)
	callbackMu.Unlock()
}

func lookupCallback(id uintptr) (func([]any) (any, error), bool) {
	callbackMu.RLock()
	fn, ok := callbackMap[id]
	callbackMu.RUnlock()
	return fn, ok
}

//export goCallbackTrampoline
func goCallbackTrampoline(ctx unsafe.Pointer, argsJSON *C.char) *C.char {
	id := uintptr(ctx)
	fn, ok := lookupCallback(id)
	if !ok {
		return C.CString("ERR:callback not found")
	}

	var args []any
	if err := json.Unmarshal([]byte(C.GoString(argsJSON)), &args); err != nil {
		return C.CString("ERR:" + err.Error())
	}

	result, err := fn(args)
	if err != nil {
		return C.CString("ERR:" + err.Error())
	}

	resultJSON, err := json.Marshal(result)
	if err != nil {
		return C.CString("ERR:" + fmt.Sprintf("marshal result: %v", err))
	}

	return C.CString(string(resultJSON))
}
