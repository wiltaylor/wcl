package wcl

import "sync"

// Document represents a parsed and evaluated WCL document.
// Call Close() when done to release the underlying WASM resources.
type Document struct {
	handle uint32
	closed bool
	mu     sync.RWMutex
}

// ParseOptions configures the WCL parser.
type ParseOptions struct {
	RootDir        string
	AllowImports   *bool
	MaxImportDepth uint32
	MaxMacroDepth  uint32
	MaxLoopDepth   uint32
	MaxIterations  uint32
	Functions      map[string]func(args []any) (any, error)
	Variables      map[string]any
}

// Diagnostic represents a parser/evaluator diagnostic.
type Diagnostic struct {
	Severity string  `json:"severity"`
	Message  string  `json:"message"`
	Code     *string `json:"code,omitempty"`
}

// BlockRef represents a reference to a WCL block with resolved attributes.
type BlockRef struct {
	Kind       string         `json:"kind"`
	ID         *string        `json:"id,omitempty"`
	Attributes map[string]any `json:"attributes,omitempty"`
	Children   []BlockRef     `json:"children,omitempty"`
	Decorators []Decorator    `json:"decorators,omitempty"`
}

// Decorator represents a WCL decorator with its arguments.
type Decorator struct {
	Name string         `json:"name"`
	Args map[string]any `json:"args"`
}
