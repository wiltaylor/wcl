#ifndef WCL_FFI_H
#define WCL_FFI_H

#include <stdarg.h>
#include <stdbool.h>
#include <stdint.h>
#include <stdlib.h>

/**
 * Opaque document handle. Use `wcl_ffi_document_free` to release.
 */
typedef void WclDocument;

/**
 * C callback function type for custom WCL functions.
 */
typedef char *(*WclCallbackFn)(void *ctx, const char *args_json);

/**
 * Parse a WCL source string and return an opaque Document pointer.
 *
 * `options_json` is an optional JSON string with parse options (may be null).
 * The caller must free the returned document with `wcl_ffi_document_free`.
 */
WclDocument *wcl_ffi_parse(const char *source, const char *options_json);

/**
 * Parse a WCL file and return an opaque Document pointer.
 *
 * Returns null on I/O failure; call `wcl_ffi_last_error` to get the message.
 * Sets root_dir to the file's parent directory if not specified in options.
 * The caller must free the returned document with `wcl_ffi_document_free`.
 */
WclDocument *wcl_ffi_parse_file(const char *path, const char *options_json);

/**
 * Get the last error message from a failed FFI call.
 *
 * Returns null if no error. Caller must free with `wcl_ffi_string_free`.
 */
char *wcl_ffi_last_error(void);

/**
 * Free a Document previously returned by `wcl_ffi_parse`.
 *
 * Safe to call with null. Must not be called twice on the same pointer.
 */
void wcl_ffi_document_free(WclDocument *doc);

/**
 * Get the evaluated values as a JSON string.
 *
 * Caller must free with `wcl_ffi_string_free`.
 */
char *wcl_ffi_document_values(const WclDocument *doc);

/**
 * Check if the document has any errors.
 */
bool wcl_ffi_document_has_errors(const WclDocument *doc);

/**
 * Get error diagnostics as a JSON array string.
 *
 * Caller must free with `wcl_ffi_string_free`.
 */
char *wcl_ffi_document_errors(const WclDocument *doc);

/**
 * Get all diagnostics as a JSON array string.
 *
 * Caller must free with `wcl_ffi_string_free`.
 */
char *wcl_ffi_document_diagnostics(const WclDocument *doc);

/**
 * Execute a query against the document.
 *
 * Returns JSON: `{"ok": <value>}` or `{"error": "message"}`.
 * Caller must free with `wcl_ffi_string_free`.
 */
char *wcl_ffi_document_query(const WclDocument *doc, const char *query);

/**
 * Get all blocks as a JSON array string.
 *
 * Caller must free with `wcl_ffi_string_free`.
 */
char *wcl_ffi_document_blocks(const WclDocument *doc);

/**
 * Get blocks of a specific type as a JSON array string.
 *
 * Caller must free with `wcl_ffi_string_free`.
 */
char *wcl_ffi_document_blocks_of_type(const WclDocument *doc, const char *kind);

/**
 * Parse a WCL source string with custom callback functions.
 *
 * - `func_names`: array of C strings (function names)
 * - `func_callbacks`: array of C callback function pointers
 * - `func_contexts`: array of opaque context pointers (one per callback)
 * - `func_count`: number of functions
 *
 * Returns an opaque Document pointer. Caller must free with `wcl_ffi_document_free`.
 */
WclDocument *wcl_ffi_parse_with_functions(const char *source,
                                          const char *options_json,
                                          const char *const *func_names,
                                          const WclCallbackFn *func_callbacks,
                                          const uintptr_t *func_contexts,
                                          uintptr_t func_count);

/**
 * List installed libraries. Returns JSON: `{"ok": ["path1", ...]}` or `{"error": "..."}`.
 *
 * Caller must free with `wcl_ffi_string_free`.
 */
char *wcl_ffi_list_libraries(void);

/**
 * Free a string previously returned by any `wcl_ffi_*` function.
 *
 * Safe to call with null.
 */
void wcl_ffi_string_free(char *s);

#endif  /* WCL_FFI_H */
