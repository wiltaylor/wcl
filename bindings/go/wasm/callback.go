package wasm

import (
	"encoding/json"
	"fmt"
	"sync"
)

// callbackBridge manages custom function callbacks for WASM host calls.
type callbackBridge struct {
	mu        sync.Mutex
	functions map[string]func([]any) (any, error)
}

// CallbackBridge is the global callback bridge instance.
var CallbackBridge = &callbackBridge{}

// SetFunctions registers callbacks for use during a parse operation.
func (b *callbackBridge) SetFunctions(fns map[string]func([]any) (any, error)) {
	b.mu.Lock()
	defer b.mu.Unlock()
	b.functions = fns
}

// ClearFunctions removes all registered callbacks.
func (b *callbackBridge) ClearFunctions() {
	b.mu.Lock()
	defer b.mu.Unlock()
	b.functions = nil
}

// Invoke calls a registered callback by name with JSON-encoded arguments.
// Returns (success, resultJSON).
func (b *callbackBridge) Invoke(name, argsJSON string) (bool, string) {
	b.mu.Lock()
	fn, ok := b.functions[name]
	b.mu.Unlock()

	if !ok {
		return false, "callback not found: " + name
	}

	var args []any
	if err := json.Unmarshal([]byte(argsJSON), &args); err != nil {
		return false, fmt.Sprintf("failed to unmarshal args: %v", err)
	}

	result, err := fn(args)
	if err != nil {
		return false, err.Error()
	}

	resultJSON, err := json.Marshal(result)
	if err != nil {
		return false, fmt.Sprintf("failed to marshal result: %v", err)
	}

	return true, string(resultJSON)
}
