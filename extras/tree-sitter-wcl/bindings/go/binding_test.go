package tree_sitter_wcl_test

import (
	"testing"

	tree_sitter "github.com/tree-sitter/go-tree-sitter"
	tree_sitter_wcl "github.com/wiltaylor/wcl/extras/tree-sitter-wcl/bindings/go"
)

func TestCanLoadGrammar(t *testing.T) {
	language := tree_sitter.NewLanguage(tree_sitter_wcl.Language())
	if language == nil {
		t.Errorf("Error loading Wcl grammar")
	}
}
