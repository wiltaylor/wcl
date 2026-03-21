package wcl

/*
#cgo linux,amd64 LDFLAGS: ${SRCDIR}/lib/linux_amd64/libwcl_ffi.a -lm -ldl -lpthread
#cgo linux,arm64 LDFLAGS: ${SRCDIR}/lib/linux_arm64/libwcl_ffi.a -lm -ldl -lpthread
#cgo darwin,amd64 LDFLAGS: ${SRCDIR}/lib/darwin_amd64/libwcl_ffi.a -lm -ldl -lpthread -framework Security
#cgo darwin,arm64 LDFLAGS: ${SRCDIR}/lib/darwin_arm64/libwcl_ffi.a -lm -ldl -lpthread -framework Security
#cgo windows,amd64 LDFLAGS: ${SRCDIR}/lib/windows_amd64/wcl_ffi.lib -lws2_32 -lbcrypt -luserenv

#include "wcl.h"
#include <stdlib.h>
*/
import "C"
import "unsafe"

// cParse calls wcl_ffi_parse.
func cParse(source, optionsJSON string) unsafe.Pointer {
	cSource := C.CString(source)
	defer C.free(unsafe.Pointer(cSource))

	var cOpts *C.char
	if optionsJSON != "" {
		cOpts = C.CString(optionsJSON)
		defer C.free(unsafe.Pointer(cOpts))
	}

	return C.wcl_ffi_parse(cSource, cOpts)
}

// cParseFile calls wcl_ffi_parse_file, returns a doc pointer (nil on error).
func cParseFile(path, optionsJSON string) unsafe.Pointer {
	cPath := C.CString(path)
	defer C.free(unsafe.Pointer(cPath))

	var cOpts *C.char
	if optionsJSON != "" {
		cOpts = C.CString(optionsJSON)
		defer C.free(unsafe.Pointer(cOpts))
	}

	return C.wcl_ffi_parse_file(cPath, cOpts)
}

// cLastError calls wcl_ffi_last_error.
func cLastError() string {
	result := C.wcl_ffi_last_error()
	if result == nil {
		return ""
	}
	defer C.wcl_ffi_string_free(result)
	return C.GoString(result)
}

// cParseWithFunctions calls wcl_ffi_parse_with_functions.
func cParseWithFunctions(source, optionsJSON string, names []*C.char, callbacks []C.WclCallbackFn, contexts []C.uintptr_t) unsafe.Pointer {
	cSource := C.CString(source)
	defer C.free(unsafe.Pointer(cSource))

	var cOpts *C.char
	if optionsJSON != "" {
		cOpts = C.CString(optionsJSON)
		defer C.free(unsafe.Pointer(cOpts))
	}

	count := len(names)
	if count == 0 {
		return C.wcl_ffi_parse(cSource, cOpts)
	}

	return C.wcl_ffi_parse_with_functions(
		cSource,
		cOpts,
		&names[0],
		&callbacks[0],
		&contexts[0],
		C.uintptr_t(count),
	)
}

// cDocumentFree calls wcl_ffi_document_free.
func cDocumentFree(ptr unsafe.Pointer) {
	C.wcl_ffi_document_free(ptr)
}

// cDocumentValues calls wcl_ffi_document_values.
func cDocumentValues(ptr unsafe.Pointer) string {
	result := C.wcl_ffi_document_values(ptr)
	defer C.wcl_ffi_string_free(result)
	return C.GoString(result)
}

// cDocumentHasErrors calls wcl_ffi_document_has_errors.
func cDocumentHasErrors(ptr unsafe.Pointer) bool {
	return bool(C.wcl_ffi_document_has_errors(ptr))
}

// cDocumentErrors calls wcl_ffi_document_errors.
func cDocumentErrors(ptr unsafe.Pointer) string {
	result := C.wcl_ffi_document_errors(ptr)
	defer C.wcl_ffi_string_free(result)
	return C.GoString(result)
}

// cDocumentDiagnostics calls wcl_ffi_document_diagnostics.
func cDocumentDiagnostics(ptr unsafe.Pointer) string {
	result := C.wcl_ffi_document_diagnostics(ptr)
	defer C.wcl_ffi_string_free(result)
	return C.GoString(result)
}

// cDocumentQuery calls wcl_ffi_document_query.
func cDocumentQuery(ptr unsafe.Pointer, query string) string {
	cQuery := C.CString(query)
	defer C.free(unsafe.Pointer(cQuery))
	result := C.wcl_ffi_document_query(ptr, cQuery)
	defer C.wcl_ffi_string_free(result)
	return C.GoString(result)
}

// cDocumentBlocks calls wcl_ffi_document_blocks.
func cDocumentBlocks(ptr unsafe.Pointer) string {
	result := C.wcl_ffi_document_blocks(ptr)
	defer C.wcl_ffi_string_free(result)
	return C.GoString(result)
}

// cDocumentBlocksOfType calls wcl_ffi_document_blocks_of_type.
func cDocumentBlocksOfType(ptr unsafe.Pointer, kind string) string {
	cKind := C.CString(kind)
	defer C.free(unsafe.Pointer(cKind))
	result := C.wcl_ffi_document_blocks_of_type(ptr, cKind)
	defer C.wcl_ffi_string_free(result)
	return C.GoString(result)
}

// cListLibraries calls wcl_ffi_list_libraries.
func cListLibraries() string {
	result := C.wcl_ffi_list_libraries()
	defer C.wcl_ffi_string_free(result)
	return C.GoString(result)
}
