export interface ParseOptions {
  /** Root directory for import jail checking (default: ".") */
  rootDir?: string;
  /** Whether imports are allowed (default: true) */
  allowImports?: boolean;
  /** Maximum import depth (default: 32) */
  maxImportDepth?: number;
  /** Maximum macro expansion depth (default: 64) */
  maxMacroDepth?: number;
  /** Maximum for-loop nesting depth (default: 32) */
  maxLoopDepth?: number;
  /** Maximum total iterations across all for loops (default: 10000) */
  maxIterations?: number;
  /**
   * Synchronous import resolver callback.
   * Receives a file path string, should return file contents as a string
   * or null if the file does not exist.
   */
  importResolver?: (path: string) => string | null;
  /**
   * In-memory files for import resolution.
   * Keys are file paths, values are file contents.
   * Mutually exclusive with importResolver (files takes precedence if both set).
   */
  files?: Record<string, string>;
  /**
   * Custom functions available in WCL expressions.
   * Keys are function names, values are synchronous functions.
   */
  functions?: Record<string, (...args: any[]) => any>;
}

export interface WclDiagnostic {
  severity: "error" | "warning";
  message: string;
  code?: string;
}

export interface WclDocument {
  /** Evaluated top-level values */
  values: Record<string, any>;
  /** Whether any errors occurred during parsing/evaluation */
  hasErrors: boolean;
  /** All diagnostics (errors and warnings) */
  diagnostics: WclDiagnostic[];
}

/**
 * Parse a WCL source string through the full pipeline.
 *
 * Returns a document object with values, error status, and diagnostics.
 */
export function parse(source: string, options?: ParseOptions): WclDocument;

/**
 * Parse a WCL source string and return just the evaluated values.
 *
 * Throws if there are parse errors.
 */
export function parseValues(
  source: string,
  options?: ParseOptions
): Record<string, any>;

/**
 * Parse a WCL source string and execute a query against it.
 *
 * Throws if there are parse errors or if the query is invalid.
 */
export function query(
  source: string,
  query: string,
  options?: ParseOptions
): any;
