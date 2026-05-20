use crate::eval::value::{NativeStreamState, NativeStreamValue, ObjectValue, Value};
use base64::{engine::general_purpose::STANDARD as BASE64_STANDARD, Engine};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

/// A callable built-in function. Supports both plain `fn` pointers and closures.
pub type BuiltinFn = Arc<dyn Fn(&[Value]) -> Result<Value, String> + Send + Sync>;

/// Metadata for a function, used by the LSP for completions and signature help.
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    pub name: String,
    pub params: Vec<String>,
    pub return_type: String,
    pub doc: String,
}

/// A shareable registry of functions and their signatures.
#[derive(Clone, Default)]
pub struct FunctionRegistry {
    pub functions: HashMap<String, BuiltinFn>,
    pub signatures: Vec<FunctionSignature>,
}

impl std::fmt::Debug for FunctionRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FunctionRegistry")
            .field(
                "functions",
                &format!("<{} functions>", self.functions.len()),
            )
            .field("signatures", &self.signatures)
            .finish()
    }
}

impl FunctionRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a custom function with its signature metadata.
    pub fn register(
        &mut self,
        name: impl Into<String>,
        f: BuiltinFn,
        signature: FunctionSignature,
    ) {
        let name = name.into();
        self.functions.insert(name, f);
        self.signatures.push(signature);
    }
}

/// Return all builtin function signatures for LSP tooling.
pub fn builtin_signatures() -> Vec<FunctionSignature> {
    vec![
        FunctionSignature {
            name: "upper".into(),
            params: vec!["s: string".into()],
            return_type: "string".into(),
            doc: "Convert string to uppercase".into(),
        },
        FunctionSignature {
            name: "lower".into(),
            params: vec!["s: string".into()],
            return_type: "string".into(),
            doc: "Convert string to lowercase".into(),
        },
        FunctionSignature {
            name: "trim".into(),
            params: vec!["s: string".into()],
            return_type: "string".into(),
            doc: "Trim whitespace".into(),
        },
        FunctionSignature {
            name: "trim_prefix".into(),
            params: vec!["s: string".into(), "prefix: string".into()],
            return_type: "string".into(),
            doc: "Remove prefix".into(),
        },
        FunctionSignature {
            name: "trim_suffix".into(),
            params: vec!["s: string".into(), "suffix: string".into()],
            return_type: "string".into(),
            doc: "Remove suffix".into(),
        },
        FunctionSignature {
            name: "replace".into(),
            params: vec![
                "s: string".into(),
                "from: string".into(),
                "to: string".into(),
            ],
            return_type: "string".into(),
            doc: "Replace occurrences".into(),
        },
        FunctionSignature {
            name: "split".into(),
            params: vec!["s: string".into(), "sep: string".into()],
            return_type: "list(string)".into(),
            doc: "Split string by separator".into(),
        },
        FunctionSignature {
            name: "split_delimited".into(),
            params: vec!["s: string".into(), "sep: string".into()],
            return_type: "list(list(string))".into(),
            doc: "Split quoted delimited text into rows and fields".into(),
        },
        FunctionSignature {
            name: "join".into(),
            params: vec!["list: list".into(), "sep: string".into()],
            return_type: "string".into(),
            doc: "Join list elements".into(),
        },
        FunctionSignature {
            name: "starts_with".into(),
            params: vec!["s: string".into(), "prefix: string".into()],
            return_type: "bool".into(),
            doc: "Check prefix".into(),
        },
        FunctionSignature {
            name: "ends_with".into(),
            params: vec!["s: string".into(), "suffix: string".into()],
            return_type: "bool".into(),
            doc: "Check suffix".into(),
        },
        FunctionSignature {
            name: "contains".into(),
            params: vec!["s: string".into(), "sub: string".into()],
            return_type: "bool".into(),
            doc: "Check substring".into(),
        },
        FunctionSignature {
            name: "length".into(),
            params: vec!["s: string".into()],
            return_type: "i64".into(),
            doc: "String length".into(),
        },
        FunctionSignature {
            name: "substr".into(),
            params: vec!["s: string".into(), "start: i64".into(), "end: i64".into()],
            return_type: "string".into(),
            doc: "Substring".into(),
        },
        FunctionSignature {
            name: "char_codepoint".into(),
            params: vec!["s: string".into()],
            return_type: "i64".into(),
            doc: "Return the Unicode scalar value for a single-character string".into(),
        },
        FunctionSignature {
            name: "char_from_codepoint".into(),
            params: vec!["codepoint: i64".into()],
            return_type: "string".into(),
            doc: "Return the single-character string for a Unicode scalar value".into(),
        },
        FunctionSignature {
            name: "bytes".into(),
            params: vec!["data: list(int)".into()],
            return_type: "bytes".into(),
            doc: "Create a bytes value from a list of byte values".into(),
        },
        FunctionSignature {
            name: "bytes_data".into(),
            params: vec!["value: bytes".into()],
            return_type: "list(int)".into(),
            doc: "Return the byte values from a bytes value".into(),
        },
        FunctionSignature {
            name: "msgpack_ext".into(),
            params: vec!["type_id: i64".into(), "data: list(int)".into()],
            return_type: "msgpack_ext".into(),
            doc: "Create a MessagePack extension value".into(),
        },
        FunctionSignature {
            name: "msgpack_ext_type_id".into(),
            params: vec!["value: msgpack_ext".into()],
            return_type: "i64".into(),
            doc: "Return a MessagePack extension type id".into(),
        },
        FunctionSignature {
            name: "msgpack_ext_data".into(),
            params: vec!["value: msgpack_ext".into()],
            return_type: "list(int)".into(),
            doc: "Return MessagePack extension payload bytes".into(),
        },
        FunctionSignature {
            name: "msgpack_timestamp".into(),
            params: vec!["seconds: i64".into(), "nanoseconds: i64".into()],
            return_type: "msgpack_timestamp".into(),
            doc: "Create a MessagePack timestamp extension value".into(),
        },
        FunctionSignature {
            name: "msgpack_timestamp_seconds".into(),
            params: vec!["value: msgpack_timestamp".into()],
            return_type: "i64".into(),
            doc: "Return MessagePack timestamp seconds".into(),
        },
        FunctionSignature {
            name: "msgpack_timestamp_nanoseconds".into(),
            params: vec!["value: msgpack_timestamp".into()],
            return_type: "i64".into(),
            doc: "Return MessagePack timestamp nanoseconds".into(),
        },
        FunctionSignature {
            name: "bytes_to_uint_be".into(),
            params: vec!["data: list(int)".into()],
            return_type: "int".into(),
            doc: "Decode big-endian bytes as an unsigned integer".into(),
        },
        FunctionSignature {
            name: "bytes_to_int_be".into(),
            params: vec!["data: list(int)".into()],
            return_type: "int".into(),
            doc: "Decode big-endian bytes as a signed integer".into(),
        },
        FunctionSignature {
            name: "uint_to_bytes_be".into(),
            params: vec!["value: int".into(), "width: i64".into()],
            return_type: "list(int)".into(),
            doc: "Encode an unsigned integer as fixed-width big-endian bytes".into(),
        },
        FunctionSignature {
            name: "int_to_bytes_be".into(),
            params: vec!["value: int".into(), "width: i64".into()],
            return_type: "list(int)".into(),
            doc: "Encode a signed integer as fixed-width big-endian bytes".into(),
        },
        FunctionSignature {
            name: "bytes_to_f32_be".into(),
            params: vec!["data: list(int)".into()],
            return_type: "float".into(),
            doc: "Decode four big-endian bytes as a float32 value".into(),
        },
        FunctionSignature {
            name: "bytes_to_f64_be".into(),
            params: vec!["data: list(int)".into()],
            return_type: "float".into(),
            doc: "Decode eight big-endian bytes as a float64 value".into(),
        },
        FunctionSignature {
            name: "f32_to_bytes_be".into(),
            params: vec!["value: float".into()],
            return_type: "list(int)".into(),
            doc: "Encode a float32 value as big-endian bytes".into(),
        },
        FunctionSignature {
            name: "f64_to_bytes_be".into(),
            params: vec!["value: float".into()],
            return_type: "list(int)".into(),
            doc: "Encode a float64 value as big-endian bytes".into(),
        },
        FunctionSignature {
            name: "utf8_to_bytes".into(),
            params: vec!["text: string".into()],
            return_type: "list(int)".into(),
            doc: "Encode a string as UTF-8 bytes".into(),
        },
        FunctionSignature {
            name: "bytes_to_utf8".into(),
            params: vec!["data: list(int)".into()],
            return_type: "string".into(),
            doc: "Decode UTF-8 bytes as a string".into(),
        },
        FunctionSignature {
            name: "format".into(),
            params: vec!["fmt: string".into(), "...args".into()],
            return_type: "string".into(),
            doc: "Format string".into(),
        },
        FunctionSignature {
            name: "regex_match".into(),
            params: vec!["s: string".into(), "pattern: string".into()],
            return_type: "bool".into(),
            doc: "Regex match".into(),
        },
        FunctionSignature {
            name: "regex_capture".into(),
            params: vec!["s: string".into(), "pattern: string".into()],
            return_type: "list(string)".into(),
            doc: "Regex capture groups".into(),
        },
        FunctionSignature {
            name: "abs".into(),
            params: vec!["n: number".into()],
            return_type: "number".into(),
            doc: "Absolute value".into(),
        },
        FunctionSignature {
            name: "min".into(),
            params: vec!["a: number".into(), "b: number".into()],
            return_type: "number".into(),
            doc: "Minimum".into(),
        },
        FunctionSignature {
            name: "max".into(),
            params: vec!["a: number".into(), "b: number".into()],
            return_type: "number".into(),
            doc: "Maximum".into(),
        },
        FunctionSignature {
            name: "floor".into(),
            params: vec!["n: f64".into()],
            return_type: "i64".into(),
            doc: "Floor".into(),
        },
        FunctionSignature {
            name: "ceil".into(),
            params: vec!["n: f64".into()],
            return_type: "i64".into(),
            doc: "Ceiling".into(),
        },
        FunctionSignature {
            name: "round".into(),
            params: vec!["n: f64".into()],
            return_type: "i64".into(),
            doc: "Round".into(),
        },
        FunctionSignature {
            name: "sqrt".into(),
            params: vec!["n: f64".into()],
            return_type: "f64".into(),
            doc: "Square root".into(),
        },
        FunctionSignature {
            name: "pow".into(),
            params: vec!["base: f64".into(), "exp: f64".into()],
            return_type: "f64".into(),
            doc: "Power".into(),
        },
        FunctionSignature {
            name: "sin".into(),
            params: vec!["n: f64".into()],
            return_type: "f64".into(),
            doc: "Sine of an angle in radians".into(),
        },
        FunctionSignature {
            name: "cos".into(),
            params: vec!["n: f64".into()],
            return_type: "f64".into(),
            doc: "Cosine of an angle in radians".into(),
        },
        FunctionSignature {
            name: "tan".into(),
            params: vec!["n: f64".into()],
            return_type: "f64".into(),
            doc: "Tangent of an angle in radians".into(),
        },
        FunctionSignature {
            name: "asin".into(),
            params: vec!["n: f64".into()],
            return_type: "f64".into(),
            doc: "Arcsine in radians".into(),
        },
        FunctionSignature {
            name: "acos".into(),
            params: vec!["n: f64".into()],
            return_type: "f64".into(),
            doc: "Arccosine in radians".into(),
        },
        FunctionSignature {
            name: "atan".into(),
            params: vec!["n: f64".into()],
            return_type: "f64".into(),
            doc: "Arctangent in radians".into(),
        },
        FunctionSignature {
            name: "atan2".into(),
            params: vec!["y: f64".into(), "x: f64".into()],
            return_type: "f64".into(),
            doc: "Four-quadrant arctangent in radians".into(),
        },
        FunctionSignature {
            name: "degrees".into(),
            params: vec!["n: f64".into()],
            return_type: "f64".into(),
            doc: "Convert radians to degrees".into(),
        },
        FunctionSignature {
            name: "radians".into(),
            params: vec!["n: f64".into()],
            return_type: "f64".into(),
            doc: "Convert degrees to radians".into(),
        },
        FunctionSignature {
            name: "pi".into(),
            params: vec![],
            return_type: "f64".into(),
            doc: "The mathematical constant pi".into(),
        },
        FunctionSignature {
            name: "len".into(),
            params: vec!["collection".into()],
            return_type: "i64".into(),
            doc: "Collection length".into(),
        },
        FunctionSignature {
            name: "keys".into(),
            params: vec!["m: map".into()],
            return_type: "list(string)".into(),
            doc: "Map keys".into(),
        },
        FunctionSignature {
            name: "values".into(),
            params: vec!["m: map".into()],
            return_type: "list".into(),
            doc: "Map values".into(),
        },
        FunctionSignature {
            name: "flatten".into(),
            params: vec!["list: list".into()],
            return_type: "list".into(),
            doc: "Flatten nested lists".into(),
        },
        FunctionSignature {
            name: "concat".into(),
            params: vec!["a: list".into(), "b: list".into()],
            return_type: "list".into(),
            doc: "Concatenate lists".into(),
        },
        FunctionSignature {
            name: "distinct".into(),
            params: vec!["list: list".into()],
            return_type: "list".into(),
            doc: "Remove duplicates".into(),
        },
        FunctionSignature {
            name: "sort".into(),
            params: vec!["list: list".into()],
            return_type: "list".into(),
            doc: "Sort list".into(),
        },
        FunctionSignature {
            name: "reverse".into(),
            params: vec!["list: list".into()],
            return_type: "list".into(),
            doc: "Reverse list".into(),
        },
        FunctionSignature {
            name: "index_of".into(),
            params: vec!["list: list".into(), "elem".into()],
            return_type: "i64".into(),
            doc: "Find element index".into(),
        },
        FunctionSignature {
            name: "range".into(),
            params: vec!["start: i64".into(), "end: i64".into()],
            return_type: "list(i64)".into(),
            doc: "Integer range".into(),
        },
        FunctionSignature {
            name: "zip".into(),
            params: vec!["a: list".into(), "b: list".into()],
            return_type: "list".into(),
            doc: "Zip two lists".into(),
        },
        FunctionSignature {
            name: "force".into(),
            params: vec!["value".into()],
            return_type: "any".into(),
            doc: "Force a lazy value and return its result".into(),
        },
        FunctionSignature {
            name: "map_has".into(),
            params: vec!["map: map".into(), "key: string".into()],
            return_type: "bool".into(),
            doc: "Check map key existence".into(),
        },
        FunctionSignature {
            name: "block_kind".into(),
            params: vec!["block: block_ref".into()],
            return_type: "string".into(),
            doc: "Return a block reference kind name".into(),
        },
        FunctionSignature {
            name: "block_id".into(),
            params: vec!["block: block_ref".into()],
            return_type: "string|null".into(),
            doc: "Return a block reference id, or null if it has none".into(),
        },
        FunctionSignature {
            name: "block_attrs".into(),
            params: vec!["block: block_ref".into()],
            return_type: "map".into(),
            doc: "Return a block reference attribute map".into(),
        },
        FunctionSignature {
            name: "block_children".into(),
            params: vec!["block: block_ref".into()],
            return_type: "list".into(),
            doc: "Return a block reference child block list".into(),
        },
        FunctionSignature {
            name: "map_set".into(),
            params: vec!["map: map".into(), "key: string".into(), "value".into()],
            return_type: "map".into(),
            doc: "Return a map with a key set".into(),
        },
        FunctionSignature {
            name: "map".into(),
            params: vec!["list: list".into(), "fn: lambda".into()],
            return_type: "list".into(),
            doc: "Map over list".into(),
        },
        FunctionSignature {
            name: "filter".into(),
            params: vec!["list: list".into(), "fn: lambda".into()],
            return_type: "list".into(),
            doc: "Filter list".into(),
        },
        FunctionSignature {
            name: "every".into(),
            params: vec!["list: list".into(), "fn: lambda".into()],
            return_type: "bool".into(),
            doc: "All match predicate".into(),
        },
        FunctionSignature {
            name: "some".into(),
            params: vec!["list: list".into(), "fn: lambda".into()],
            return_type: "bool".into(),
            doc: "Any matches predicate".into(),
        },
        FunctionSignature {
            name: "reduce".into(),
            params: vec!["list: list".into(), "init".into(), "fn: lambda".into()],
            return_type: "any".into(),
            doc: "Reduce list".into(),
        },
        FunctionSignature {
            name: "sum".into(),
            params: vec!["list: list(number)".into()],
            return_type: "number".into(),
            doc: "Sum numbers".into(),
        },
        FunctionSignature {
            name: "avg".into(),
            params: vec!["list: list(number)".into()],
            return_type: "f64".into(),
            doc: "Average".into(),
        },
        FunctionSignature {
            name: "min_of".into(),
            params: vec!["list: list(number)".into()],
            return_type: "number".into(),
            doc: "Minimum of list".into(),
        },
        FunctionSignature {
            name: "max_of".into(),
            params: vec!["list: list(number)".into()],
            return_type: "number".into(),
            doc: "Maximum of list".into(),
        },
        FunctionSignature {
            name: "count".into(),
            params: vec!["list: list".into(), "fn: lambda".into()],
            return_type: "i64".into(),
            doc: "Count matching elements".into(),
        },
        FunctionSignature {
            name: "find".into(),
            params: vec![
                "list: list".into(),
                "key: string".into(),
                "value: any".into(),
            ],
            return_type: "map|null".into(),
            doc: "Find first row where key equals value".into(),
        },
        FunctionSignature {
            name: "insert_row".into(),
            params: vec!["list: list".into(), "row: map".into()],
            return_type: "list".into(),
            doc: "Append a row to a list".into(),
        },
        FunctionSignature {
            name: "remove_rows".into(),
            params: vec![
                "list: list".into(),
                "key: string".into(),
                "value: any".into(),
            ],
            return_type: "list".into(),
            doc: "Remove rows where key equals value".into(),
        },
        FunctionSignature {
            name: "update_rows".into(),
            params: vec![
                "list: list".into(),
                "key: string".into(),
                "value: any".into(),
                "updates: map".into(),
            ],
            return_type: "list".into(),
            doc: "Update rows where key equals value".into(),
        },
        FunctionSignature {
            name: "sha256".into(),
            params: vec!["s: string".into()],
            return_type: "string".into(),
            doc: "SHA-256 hash".into(),
        },
        FunctionSignature {
            name: "base64_encode".into(),
            params: vec!["s: string".into()],
            return_type: "string".into(),
            doc: "Base64 encode".into(),
        },
        FunctionSignature {
            name: "base64_decode".into(),
            params: vec!["s: string".into()],
            return_type: "string".into(),
            doc: "Base64 decode".into(),
        },
        FunctionSignature {
            name: "json_encode".into(),
            params: vec!["value".into()],
            return_type: "string".into(),
            doc: "Encode as JSON string".into(),
        },
        FunctionSignature {
            name: "to_string".into(),
            params: vec!["value".into()],
            return_type: "string".into(),
            doc: "Convert to string".into(),
        },
        FunctionSignature {
            name: "to_int".into(),
            params: vec!["value".into()],
            return_type: "i64".into(),
            doc: "Convert to int".into(),
        },
        FunctionSignature {
            name: "to_float".into(),
            params: vec!["value".into()],
            return_type: "f64".into(),
            doc: "Convert to float".into(),
        },
        FunctionSignature {
            name: "to_bool".into(),
            params: vec!["value".into()],
            return_type: "bool".into(),
            doc: "Convert to bool".into(),
        },
        FunctionSignature {
            name: "type_of".into(),
            params: vec!["value".into()],
            return_type: "string".into(),
            doc: "Get type name".into(),
        },
        FunctionSignature {
            name: "has".into(),
            params: vec!["value".into(), "key: string".into()],
            return_type: "bool".into(),
            doc: "Check if key exists".into(),
        },
        FunctionSignature {
            name: "children".into(),
            params: vec!["block".into(), "kind: string".into()],
            return_type: "list".into(),
            doc: "Return direct child blocks, optionally filtered by kind".into(),
        },
        FunctionSignature {
            name: "has_decorator".into(),
            params: vec!["block".into(), "name: string".into()],
            return_type: "bool".into(),
            doc: "Check decorator".into(),
        },
        FunctionSignature {
            name: "find_decorators".into(),
            params: vec!["name: string".into(), "target: string".into()],
            return_type: "list".into(),
            doc: "Find decorator usages by name and optional target".into(),
        },
        FunctionSignature {
            name: "is_imported".into(),
            params: vec!["path: string".into()],
            return_type: "bool".into(),
            doc: "Check if a file was imported".into(),
        },
        FunctionSignature {
            name: "import_codec".into(),
            params: vec![
                "path: string".into(),
                "codec: string".into(),
                "options: map".into(),
            ],
            return_type: "list".into(),
            doc: "Import a file through a standard transform codec".into(),
        },
        FunctionSignature {
            name: "has_schema".into(),
            params: vec!["name: string".into()],
            return_type: "bool".into(),
            doc: "Check if a schema is declared".into(),
        },
        FunctionSignature {
            name: "date".into(),
            params: vec!["s: string".into()],
            return_type: "date".into(),
            doc: "Parse ISO 8601 date (YYYY-MM-DD)".into(),
        },
        FunctionSignature {
            name: "offset_datetime".into(),
            params: vec!["s: string".into()],
            return_type: "offset_datetime".into(),
            doc: "Parse TOML/RFC 3339 offset date-time".into(),
        },
        FunctionSignature {
            name: "local_datetime".into(),
            params: vec!["s: string".into()],
            return_type: "local_datetime".into(),
            doc: "Parse TOML local date-time".into(),
        },
        FunctionSignature {
            name: "local_time".into(),
            params: vec!["s: string".into()],
            return_type: "local_time".into(),
            doc: "Parse TOML local time".into(),
        },
        FunctionSignature {
            name: "duration".into(),
            params: vec!["s: string".into()],
            return_type: "duration".into(),
            doc: "Parse ISO 8601 duration (PnYnMnDTnHnMnS)".into(),
        },
        FunctionSignature {
            name: "diagram_layout".into(),
            params: vec!["diagram: map".into()],
            return_type: "map".into(),
            doc: "Resolve diagram shape layout and return natural bounds".into(),
        },
        FunctionSignature {
            name: "diagram_intrinsic_size".into(),
            params: vec!["diagram: map".into()],
            return_type: "map".into(),
            doc: "Return diagram natural bounds after layout".into(),
        },
        FunctionSignature {
            name: "diagram_fit".into(),
            params: vec!["bounds: map".into(), "canvas: map".into()],
            return_type: "map".into(),
            doc: "Return a scale and translation that fits bounds into a canvas".into(),
        },
        FunctionSignature {
            name: "byte_stream".into(),
            params: vec!["value".into()],
            return_type: "stream".into(),
            doc: "Wrap bytes or a list of byte chunks as a stream".into(),
        },
    ]
}

fn wrap_builtin(f: fn(&[Value]) -> Result<Value, String>) -> BuiltinFn {
    Arc::new(f)
}

pub fn builtin_registry() -> HashMap<String, BuiltinFn> {
    let mut m: HashMap<String, BuiltinFn> = HashMap::new();

    // String functions (Section 14.1)
    m.insert("upper".into(), wrap_builtin(upper));
    m.insert("lower".into(), wrap_builtin(lower));
    m.insert("trim".into(), wrap_builtin(trim));
    m.insert("trim_prefix".into(), wrap_builtin(trim_prefix));
    m.insert("trim_suffix".into(), wrap_builtin(trim_suffix));
    m.insert("replace".into(), wrap_builtin(fn_replace));
    m.insert("split".into(), wrap_builtin(split));
    m.insert("split_delimited".into(), wrap_builtin(split_delimited));
    m.insert("join".into(), wrap_builtin(join));
    m.insert("starts_with".into(), wrap_builtin(starts_with));
    m.insert("ends_with".into(), wrap_builtin(ends_with));
    m.insert("contains".into(), wrap_builtin(fn_contains));
    m.insert("length".into(), wrap_builtin(length));
    m.insert("substr".into(), wrap_builtin(substr));
    m.insert("char_codepoint".into(), wrap_builtin(char_codepoint));
    m.insert(
        "char_from_codepoint".into(),
        wrap_builtin(char_from_codepoint),
    );
    m.insert("bytes".into(), wrap_builtin(bytes));
    m.insert("bytes_data".into(), wrap_builtin(bytes_data));
    m.insert("msgpack_ext".into(), wrap_builtin(msgpack_ext));
    m.insert(
        "msgpack_ext_type_id".into(),
        wrap_builtin(msgpack_ext_type_id),
    );
    m.insert("msgpack_ext_data".into(), wrap_builtin(msgpack_ext_data));
    m.insert("msgpack_timestamp".into(), wrap_builtin(msgpack_timestamp));
    m.insert(
        "msgpack_timestamp_seconds".into(),
        wrap_builtin(msgpack_timestamp_seconds),
    );
    m.insert(
        "msgpack_timestamp_nanoseconds".into(),
        wrap_builtin(msgpack_timestamp_nanoseconds),
    );
    m.insert("bytes_to_uint_be".into(), wrap_builtin(bytes_to_uint_be));
    m.insert("bytes_to_int_be".into(), wrap_builtin(bytes_to_int_be));
    m.insert("uint_to_bytes_be".into(), wrap_builtin(uint_to_bytes_be));
    m.insert("int_to_bytes_be".into(), wrap_builtin(int_to_bytes_be));
    m.insert("bytes_to_f32_be".into(), wrap_builtin(bytes_to_f32_be));
    m.insert("bytes_to_f64_be".into(), wrap_builtin(bytes_to_f64_be));
    m.insert("f32_to_bytes_be".into(), wrap_builtin(f32_to_bytes_be));
    m.insert("f64_to_bytes_be".into(), wrap_builtin(f64_to_bytes_be));
    m.insert("utf8_to_bytes".into(), wrap_builtin(utf8_to_bytes));
    m.insert("bytes_to_utf8".into(), wrap_builtin(bytes_to_utf8));
    m.insert("format".into(), wrap_builtin(fn_format));
    m.insert("regex_match".into(), wrap_builtin(regex_match));
    m.insert("regex_capture".into(), wrap_builtin(regex_capture));
    m.insert("regex_replace".into(), wrap_builtin(regex_replace));
    m.insert("regex_replace_all".into(), wrap_builtin(regex_replace_all));
    m.insert("regex_split".into(), wrap_builtin(regex_split));
    m.insert("regex_find".into(), wrap_builtin(regex_find));
    m.insert("regex_find_all".into(), wrap_builtin(regex_find_all));

    // Math functions (Section 14.2)
    m.insert("abs".into(), wrap_builtin(abs));
    m.insert("min".into(), wrap_builtin(fn_min));
    m.insert("max".into(), wrap_builtin(fn_max));
    m.insert("floor".into(), wrap_builtin(floor));
    m.insert("ceil".into(), wrap_builtin(ceil));
    m.insert("round".into(), wrap_builtin(fn_round));
    m.insert("sqrt".into(), wrap_builtin(sqrt));
    m.insert("pow".into(), wrap_builtin(pow));
    m.insert("sin".into(), wrap_builtin(sin));
    m.insert("cos".into(), wrap_builtin(cos));
    m.insert("tan".into(), wrap_builtin(tan));
    m.insert("asin".into(), wrap_builtin(asin));
    m.insert("acos".into(), wrap_builtin(acos));
    m.insert("atan".into(), wrap_builtin(atan));
    m.insert("atan2".into(), wrap_builtin(atan2));
    m.insert("degrees".into(), wrap_builtin(degrees));
    m.insert("radians".into(), wrap_builtin(radians));
    m.insert("pi".into(), wrap_builtin(pi));

    // Collection functions (Section 14.3)
    m.insert("len".into(), wrap_builtin(len));
    m.insert("keys".into(), wrap_builtin(keys));
    m.insert("values".into(), wrap_builtin(fn_values));
    m.insert("flatten".into(), wrap_builtin(flatten));
    m.insert("concat".into(), wrap_builtin(fn_concat));
    m.insert("distinct".into(), wrap_builtin(distinct));
    m.insert("sort".into(), wrap_builtin(fn_sort));
    m.insert("reverse".into(), wrap_builtin(fn_reverse));
    m.insert("index_of".into(), wrap_builtin(index_of));
    m.insert("range".into(), wrap_builtin(range));
    m.insert("zip".into(), wrap_builtin(zip));
    m.insert("map_has".into(), wrap_builtin(map_has));
    m.insert("block_kind".into(), wrap_builtin(block_kind));
    m.insert("block_id".into(), wrap_builtin(block_id));
    m.insert("block_attrs".into(), wrap_builtin(block_attrs));
    m.insert("block_children".into(), wrap_builtin(block_children));
    m.insert("map_set".into(), wrap_builtin(map_set));
    m.insert("object".into(), wrap_builtin(object));

    // Table manipulation functions (Section 14.3b)
    m.insert("find".into(), wrap_builtin(fn_find));
    m.insert("insert_row".into(), wrap_builtin(fn_insert_row));
    m.insert("remove_rows".into(), wrap_builtin(fn_remove_rows));
    m.insert("update_rows".into(), wrap_builtin(fn_update_rows));

    // Higher-order functions (Section 14.4) — require special evaluator support
    m.insert("map".into(), wrap_builtin(higher_order_placeholder));
    m.insert("filter".into(), wrap_builtin(higher_order_placeholder));
    m.insert("every".into(), wrap_builtin(higher_order_placeholder));
    m.insert("some".into(), wrap_builtin(higher_order_placeholder));
    m.insert("reduce".into(), wrap_builtin(higher_order_placeholder));

    // Aggregate functions (Section 14.5)
    m.insert("sum".into(), wrap_builtin(sum));
    m.insert("avg".into(), wrap_builtin(avg));
    m.insert("min_of".into(), wrap_builtin(min_of));
    m.insert("max_of".into(), wrap_builtin(max_of));
    m.insert("count".into(), wrap_builtin(higher_order_placeholder));

    // Hash/encoding (Section 14.6)
    m.insert("sha256".into(), wrap_builtin(fn_sha256));
    m.insert("base64_encode".into(), wrap_builtin(base64_encode));
    m.insert("base64_decode".into(), wrap_builtin(base64_decode));
    m.insert("json_encode".into(), wrap_builtin(json_encode));

    // Type coercion (Section 14.7)
    m.insert("to_string".into(), wrap_builtin(to_string));
    m.insert("to_int".into(), wrap_builtin(to_int));
    m.insert("to_float".into(), wrap_builtin(to_float));
    m.insert("to_bool".into(), wrap_builtin(to_bool));
    m.insert("type_of".into(), wrap_builtin(type_of));

    // Reference and Query Functions (Section 14.9)
    m.insert("has".into(), wrap_builtin(fn_has));
    m.insert("children".into(), wrap_builtin(fn_children));
    m.insert("has_decorator".into(), wrap_builtin(fn_has_decorator));

    // Date/Duration constructors (Section 14.8)
    m.insert("date".into(), wrap_builtin(fn_date));
    m.insert("offset_datetime".into(), wrap_builtin(fn_offset_datetime));
    m.insert("local_datetime".into(), wrap_builtin(fn_local_datetime));
    m.insert("local_time".into(), wrap_builtin(fn_local_time));
    m.insert("duration".into(), wrap_builtin(fn_duration));
    m.insert("diagram_layout".into(), wrap_builtin(diagram_layout));
    m.insert(
        "diagram_intrinsic_size".into(),
        wrap_builtin(diagram_intrinsic_size),
    );
    m.insert("diagram_fit".into(), wrap_builtin(diagram_fit));
    m.insert("byte_stream".into(), wrap_builtin(byte_stream));

    m
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn expect_args(args: &[Value], n: usize, name: &str) -> Result<(), String> {
    if args.len() != n {
        Err(format!(
            "{}: expected {} argument(s), got {}",
            name,
            n,
            args.len()
        ))
    } else {
        Ok(())
    }
}

fn diagram_layout(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "diagram_layout")?;
    crate::transform::codec::native::layout_diagram_value(
        &args[0],
        &crate::transform::codec::CodecOptions::new(),
    )
    .map_err(|e| e.to_string())
}

fn diagram_intrinsic_size(args: &[Value]) -> Result<Value, String> {
    let layout = diagram_layout(args)?;
    let Value::Map(layout) = layout else {
        return Err("diagram_intrinsic_size: layout did not return a map".into());
    };
    let mut out = indexmap::IndexMap::new();
    for key in ["x", "y", "width", "height"] {
        if let Some(value) = layout.get(key) {
            out.insert(key.to_string(), value.clone());
        }
    }
    Ok(Value::Map(out))
}

fn diagram_fit(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "diagram_fit")?;
    let bounds = expect_map(&args[0], "diagram_fit bounds")?;
    let canvas = expect_map(&args[1], "diagram_fit canvas")?;
    let bx = map_number(bounds, "x").unwrap_or(0.0);
    let by = map_number(bounds, "y").unwrap_or(0.0);
    let bw = map_number(bounds, "width").ok_or("diagram_fit bounds missing width")?;
    let bh = map_number(bounds, "height").ok_or("diagram_fit bounds missing height")?;
    let cx = map_number(canvas, "x").unwrap_or(0.0);
    let cy = map_number(canvas, "y").unwrap_or(0.0);
    let cw = map_number(canvas, "width").ok_or("diagram_fit canvas missing width")?;
    let ch = map_number(canvas, "height").ok_or("diagram_fit canvas missing height")?;
    if bw <= 0.0 || bh <= 0.0 || cw <= 0.0 || ch <= 0.0 {
        return Err("diagram_fit: width and height must be positive".into());
    }
    let scale = (cw / bw).min(ch / bh).min(1.0);
    let fitted_width = bw * scale;
    let fitted_height = bh * scale;
    let target_x = cx + (cw - fitted_width).max(0.0) / 2.0;
    let target_y = cy + (ch - fitted_height).max(0.0) / 2.0;
    let mut out = indexmap::IndexMap::new();
    out.insert(
        "translate_x".to_string(),
        Value::Float(target_x - bx * scale),
    );
    out.insert(
        "translate_y".to_string(),
        Value::Float(target_y - by * scale),
    );
    out.insert("scale".to_string(), Value::Float(scale));
    Ok(Value::Map(out))
}

fn byte_stream(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "byte_stream")?;
    match &args[0] {
        Value::Stream(_) | Value::NativeStream(_) => Ok(args[0].clone()),
        Value::Bytes(_) => Ok(native_stream_from_chunks(vec![args[0].clone()])),
        Value::List(items) if is_byte_list(items) => {
            Ok(native_stream_from_chunks(vec![args[0].clone()]))
        }
        Value::List(items) => Ok(native_stream_from_chunks(items.clone())),
        other => Err(format!(
            "byte_stream: expected bytes, byte list, stream, or list of chunks, got {}",
            other.type_name()
        )),
    }
}

fn native_stream_from_chunks(chunks: Vec<Value>) -> Value {
    let mut iter = chunks.into_iter();
    Value::NativeStream(NativeStreamValue {
        inner: Arc::new(Mutex::new(NativeStreamState {
            next: Box::new(move || Ok(iter.next())),
            exhausted: false,
        })),
    })
}

fn is_byte_list(items: &[Value]) -> bool {
    items
        .iter()
        .all(|item| matches!(item, Value::Int(i) if (0..=255).contains(i)))
}

fn expect_map<'a>(
    value: &'a Value,
    name: &str,
) -> Result<&'a indexmap::IndexMap<String, Value>, String> {
    match value {
        Value::Map(map) => Ok(map),
        other => Err(format!("{name}: expected map, got {}", other.type_name())),
    }
}

fn map_number(map: &indexmap::IndexMap<String, Value>, key: &str) -> Option<f64> {
    match map.get(key)? {
        Value::Int(n) => Some(*n as f64),
        Value::BigInt(n) => Some(*n as f64),
        Value::Float(n) => Some(*n),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn expect_min_args(args: &[Value], n: usize, name: &str) -> Result<(), String> {
    if args.len() < n {
        Err(format!(
            "{}: expected at least {} argument(s), got {}",
            name,
            n,
            args.len()
        ))
    } else {
        Ok(())
    }
}

fn get_string<'a>(v: &'a Value, pos: usize, fn_name: &str) -> Result<&'a str, String> {
    match v {
        Value::String(s) => Ok(s.as_str()),
        other => Err(format!(
            "{}: argument {} must be string, got {}",
            fn_name,
            pos,
            other.type_name()
        )),
    }
}

fn get_int(v: &Value, pos: usize, fn_name: &str) -> Result<i64, String> {
    match v {
        Value::Int(i) => Ok(*i),
        Value::BigInt(i) => {
            if *i > i64::MAX as i128 || *i < i64::MIN as i128 {
                Err(format!(
                    "{}: argument {} bigint value {} overflows i64",
                    fn_name, pos, i
                ))
            } else {
                Ok(*i as i64)
            }
        }
        other => Err(format!(
            "{}: argument {} must be int, got {}",
            fn_name,
            pos,
            other.type_name()
        )),
    }
}

fn get_list<'a>(v: &'a Value, pos: usize, fn_name: &str) -> Result<&'a [Value], String> {
    match v {
        Value::List(l) => Ok(l.as_slice()),
        other => Err(format!(
            "{}: argument {} must be list, got {}",
            fn_name,
            pos,
            other.type_name()
        )),
    }
}

/// Coerce Int, BigInt, or Float to f64 for numeric operations.
fn coerce_to_float(v: &Value, pos: usize, fn_name: &str) -> Result<f64, String> {
    match v {
        Value::Int(i) => Ok(*i as f64),
        Value::BigInt(i) => Ok(*i as f64),
        Value::Float(f) => Ok(*f),
        other => Err(format!(
            "{}: argument {} must be int or float, got {}",
            fn_name,
            pos,
            other.type_name()
        )),
    }
}

// ---------------------------------------------------------------------------
// Section 14.1 — String Functions
// ---------------------------------------------------------------------------

fn upper(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "upper")?;
    let s = get_string(&args[0], 1, "upper")?;
    Ok(Value::String(s.to_uppercase()))
}

fn lower(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "lower")?;
    let s = get_string(&args[0], 1, "lower")?;
    Ok(Value::String(s.to_lowercase()))
}

fn trim(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "trim")?;
    let s = get_string(&args[0], 1, "trim")?;
    Ok(Value::String(s.trim().to_string()))
}

fn trim_prefix(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "trim_prefix")?;
    let s = get_string(&args[0], 1, "trim_prefix")?;
    let prefix = get_string(&args[1], 2, "trim_prefix")?;
    Ok(Value::String(
        s.strip_prefix(prefix).unwrap_or(s).to_string(),
    ))
}

fn trim_suffix(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "trim_suffix")?;
    let s = get_string(&args[0], 1, "trim_suffix")?;
    let suffix = get_string(&args[1], 2, "trim_suffix")?;
    Ok(Value::String(
        s.strip_suffix(suffix).unwrap_or(s).to_string(),
    ))
}

fn fn_replace(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 3, "replace")?;
    let s = get_string(&args[0], 1, "replace")?;
    let old = get_string(&args[1], 2, "replace")?;
    let new = get_string(&args[2], 3, "replace")?;
    Ok(Value::String(s.replace(old, new)))
}

fn split(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "split")?;
    let sep = get_string(&args[0], 1, "split")?;
    let s = get_string(&args[1], 2, "split")?;
    let parts: Vec<Value> = s
        .split(sep)
        .map(|p: &str| Value::String(p.to_string()))
        .collect();
    Ok(Value::List(parts))
}

fn split_delimited(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "split_delimited")?;
    let s = get_string(&args[0], 1, "split_delimited")?;
    let sep = get_string(&args[1], 2, "split_delimited")?;
    let mut sep_chars = sep.chars();
    let Some(separator) = sep_chars.next() else {
        return Err("split_delimited: separator must be a single character".into());
    };
    if sep_chars.next().is_some() {
        return Err("split_delimited: separator must be a single character".into());
    }

    let mut rows: Vec<Vec<String>> = Vec::new();
    let mut row: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut chars = s.chars().peekable();
    let mut in_quotes = false;
    let mut after_quote = false;
    let mut field_started = false;

    while let Some(ch) = chars.next() {
        if in_quotes {
            if ch == '"' {
                if chars.peek() == Some(&'"') {
                    chars.next();
                    field.push('"');
                } else {
                    in_quotes = false;
                    after_quote = true;
                }
            } else {
                field.push(ch);
            }
            field_started = true;
            continue;
        }

        if after_quote {
            match ch {
                c if c == separator => {
                    row.push(std::mem::take(&mut field));
                    after_quote = false;
                    field_started = false;
                }
                '\n' => {
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                    after_quote = false;
                    field_started = false;
                }
                '\r' => {
                    if chars.peek() == Some(&'\n') {
                        chars.next();
                    }
                    row.push(std::mem::take(&mut field));
                    rows.push(std::mem::take(&mut row));
                    after_quote = false;
                    field_started = false;
                }
                _ => return Err("split_delimited: unexpected character after quoted field".into()),
            }
            continue;
        }

        match ch {
            '"' if !field_started && field.is_empty() => {
                in_quotes = true;
                field_started = true;
            }
            '"' => return Err("split_delimited: unexpected quote in unquoted field".into()),
            c if c == separator => {
                row.push(std::mem::take(&mut field));
                field_started = false;
            }
            '\n' => {
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
                field_started = false;
            }
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                row.push(std::mem::take(&mut field));
                rows.push(std::mem::take(&mut row));
                field_started = false;
            }
            _ => {
                field.push(ch);
                field_started = true;
            }
        }
    }

    if in_quotes {
        return Err("split_delimited: unterminated quoted field".into());
    }
    if field_started || !field.is_empty() || !row.is_empty() {
        row.push(field);
        if !(row.len() == 1 && row[0].is_empty()) {
            rows.push(row);
        }
    }

    Ok(Value::List(
        rows.into_iter()
            .map(|row| Value::List(row.into_iter().map(Value::String).collect()))
            .collect(),
    ))
}

fn join(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "join")?;
    let sep = get_string(&args[0], 1, "join")?;
    let list = get_list(&args[1], 2, "join")?;
    let mut parts = Vec::with_capacity(list.len());
    for (i, v) in list.iter().enumerate() {
        match v {
            Value::String(s) => parts.push(s.as_str().to_string()),
            other => {
                return Err(format!(
                    "join: list element {} must be string, got {}",
                    i,
                    other.type_name()
                ))
            }
        }
    }
    Ok(Value::String(parts.join(sep)))
}

fn starts_with(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "starts_with")?;
    let s = get_string(&args[0], 1, "starts_with")?;
    let prefix = get_string(&args[1], 2, "starts_with")?;
    Ok(Value::Bool(s.starts_with(prefix)))
}

fn ends_with(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "ends_with")?;
    let s = get_string(&args[0], 1, "ends_with")?;
    let suffix = get_string(&args[1], 2, "ends_with")?;
    Ok(Value::Bool(s.ends_with(suffix)))
}

/// Overloaded: `contains(string, string)` or `contains(list, value)`.
fn fn_contains(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "contains")?;
    match &args[0] {
        Value::String(s) => {
            let substr = get_string(&args[1], 2, "contains")?;
            Ok(Value::Bool(s.contains(substr)))
        }
        Value::List(list) => Ok(Value::Bool(list.contains(&args[1]))),
        other => Err(format!(
            "contains: argument 1 must be string or list, got {}",
            other.type_name()
        )),
    }
}

fn length(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "length")?;
    let s = get_string(&args[0], 1, "length")?;
    Ok(Value::Int(s.chars().count() as i64))
}

fn substr(args: &[Value]) -> Result<Value, String> {
    if args.len() < 2 || args.len() > 3 {
        return Err(format!(
            "substr: expected 2 or 3 arguments, got {}",
            args.len()
        ));
    }
    let s = get_string(&args[0], 1, "substr")?;
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len() as i64;

    let start = get_int(&args[1], 2, "substr")?;
    let end = if args.len() == 3 {
        get_int(&args[2], 3, "substr")?
    } else {
        len
    };

    // Clamp to valid range
    let start = start.max(0).min(len) as usize;
    let end = end.max(0).min(len) as usize;
    let end = end.max(start);

    Ok(Value::String(chars[start..end].iter().collect()))
}

fn char_codepoint(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "char_codepoint")?;
    let s = get_string(&args[0], 1, "char_codepoint")?;
    let mut chars = s.chars();
    let Some(ch) = chars.next() else {
        return Err("char_codepoint: expected a single-character string".into());
    };
    if chars.next().is_some() {
        return Err("char_codepoint: expected a single-character string".into());
    }
    Ok(Value::Int(ch as u32 as i64))
}

fn char_from_codepoint(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "char_from_codepoint")?;
    let code = get_int(&args[0], 1, "char_from_codepoint")?;
    let code = u32::try_from(code)
        .map_err(|_| "char_from_codepoint: codepoint must be non-negative".to_string())?;
    let ch = char::from_u32(code)
        .ok_or_else(|| "char_from_codepoint: invalid Unicode scalar value".to_string())?;
    Ok(Value::String(ch.to_string()))
}

fn list_to_bytes(value: &Value, fn_name: &str, pos: usize) -> Result<Vec<u8>, String> {
    let items = get_list(value, pos, fn_name)?;
    let mut bytes = Vec::with_capacity(items.len());
    for item in items {
        let Value::Int(i) = item else {
            return Err(format!("{fn_name}: byte list elements must be int"));
        };
        if !(0..=255).contains(i) {
            return Err(format!("{fn_name}: byte value out of range: {i}"));
        }
        bytes.push(*i as u8);
    }
    Ok(bytes)
}

fn bytes_to_value(bytes: &[u8]) -> Value {
    Value::List(bytes.iter().map(|b| Value::Int(i64::from(*b))).collect())
}

fn numeric_to_i128(value: &Value, fn_name: &str, pos: usize) -> Result<i128, String> {
    match value {
        Value::Int(i) => Ok(*i as i128),
        Value::BigInt(i) => Ok(*i),
        other => Err(format!(
            "{fn_name}: argument {pos} must be int or bigint, got {}",
            other.type_name()
        )),
    }
}

fn numeric_to_u128(value: &Value, fn_name: &str, pos: usize) -> Result<u128, String> {
    let n = numeric_to_i128(value, fn_name, pos)?;
    u128::try_from(n).map_err(|_| format!("{fn_name}: argument {pos} must be non-negative"))
}

fn int_value(n: i128) -> Value {
    if (i64::MIN as i128..=i64::MAX as i128).contains(&n) {
        Value::Int(n as i64)
    } else {
        Value::BigInt(n)
    }
}

fn bytes(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "bytes")?;
    Ok(Value::Bytes(list_to_bytes(&args[0], "bytes", 1)?))
}

fn bytes_data(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "bytes_data")?;
    match &args[0] {
        Value::Bytes(bytes) => Ok(bytes_to_value(bytes)),
        other => Err(format!(
            "bytes_data: argument 1 must be bytes, got {}",
            other.type_name()
        )),
    }
}

fn msgpack_ext(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "msgpack_ext")?;
    let type_id = get_int(&args[0], 1, "msgpack_ext")?;
    let type_id = i8::try_from(type_id)
        .map_err(|_| "msgpack_ext: type_id must fit in signed 8-bit range".to_string())?;
    let data = list_to_bytes(&args[1], "msgpack_ext", 2)?;
    Ok(Value::MsgPackExt { type_id, data })
}

fn msgpack_ext_type_id(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "msgpack_ext_type_id")?;
    match &args[0] {
        Value::MsgPackExt { type_id, .. } => Ok(Value::Int(i64::from(*type_id))),
        other => Err(format!(
            "msgpack_ext_type_id: argument 1 must be msgpack_ext, got {}",
            other.type_name()
        )),
    }
}

fn msgpack_ext_data(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "msgpack_ext_data")?;
    match &args[0] {
        Value::MsgPackExt { data, .. } => Ok(bytes_to_value(data)),
        other => Err(format!(
            "msgpack_ext_data: argument 1 must be msgpack_ext, got {}",
            other.type_name()
        )),
    }
}

fn msgpack_timestamp(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "msgpack_timestamp")?;
    let seconds = get_int(&args[0], 1, "msgpack_timestamp")?;
    let nanoseconds = get_int(&args[1], 2, "msgpack_timestamp")?;
    let nanoseconds = u32::try_from(nanoseconds)
        .map_err(|_| "msgpack_timestamp: nanoseconds must be non-negative".to_string())?;
    if nanoseconds >= 1_000_000_000 {
        return Err("msgpack_timestamp: nanoseconds must be less than 1000000000".into());
    }
    Ok(Value::MsgPackTimestamp {
        seconds,
        nanoseconds,
    })
}

fn msgpack_timestamp_seconds(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "msgpack_timestamp_seconds")?;
    match &args[0] {
        Value::MsgPackTimestamp { seconds, .. } => Ok(Value::Int(*seconds)),
        other => Err(format!(
            "msgpack_timestamp_seconds: argument 1 must be msgpack_timestamp, got {}",
            other.type_name()
        )),
    }
}

fn msgpack_timestamp_nanoseconds(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "msgpack_timestamp_nanoseconds")?;
    match &args[0] {
        Value::MsgPackTimestamp { nanoseconds, .. } => Ok(Value::Int(i64::from(*nanoseconds))),
        other => Err(format!(
            "msgpack_timestamp_nanoseconds: argument 1 must be msgpack_timestamp, got {}",
            other.type_name()
        )),
    }
}

fn bytes_to_uint_be(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "bytes_to_uint_be")?;
    let bytes = list_to_bytes(&args[0], "bytes_to_uint_be", 1)?;
    if bytes.len() > 8 {
        return Err("bytes_to_uint_be: at most 8 bytes are supported".into());
    }
    let mut n: u128 = 0;
    for byte in bytes {
        n = (n << 8) | u128::from(byte);
    }
    if n <= i64::MAX as u128 {
        Ok(Value::Int(n as i64))
    } else {
        Ok(Value::BigInt(n as i128))
    }
}

fn bytes_to_int_be(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "bytes_to_int_be")?;
    let bytes = list_to_bytes(&args[0], "bytes_to_int_be", 1)?;
    if bytes.is_empty() || bytes.len() > 8 {
        return Err("bytes_to_int_be: byte list length must be 1..=8".into());
    }
    let bits = bytes.len() * 8;
    let mut unsigned: u128 = 0;
    for byte in bytes {
        unsigned = (unsigned << 8) | u128::from(byte);
    }
    let sign_bit = 1u128 << (bits - 1);
    let value = if unsigned & sign_bit == 0 {
        unsigned as i128
    } else {
        (unsigned as i128) - (1i128 << bits)
    };
    Ok(int_value(value))
}

fn uint_to_bytes_be(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "uint_to_bytes_be")?;
    let mut n = numeric_to_u128(&args[0], "uint_to_bytes_be", 1)?;
    let len = get_int(&args[1], 2, "uint_to_bytes_be")?;
    if !(0..=8).contains(&len) {
        return Err("uint_to_bytes_be: length must be 0..=8".into());
    }
    let mut bytes = vec![0u8; len as usize];
    for byte in bytes.iter_mut().rev() {
        *byte = (n & 0xff) as u8;
        n >>= 8;
    }
    if n != 0 {
        return Err("uint_to_bytes_be: value does not fit in requested length".into());
    }
    Ok(bytes_to_value(&bytes))
}

fn int_to_bytes_be(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "int_to_bytes_be")?;
    let n = numeric_to_i128(&args[0], "int_to_bytes_be", 1)?;
    let len = get_int(&args[1], 2, "int_to_bytes_be")?;
    if !(1..=8).contains(&len) {
        return Err("int_to_bytes_be: length must be 1..=8".into());
    }
    let bits = len * 8;
    let min = -(1i128 << (bits - 1));
    let max = (1i128 << (bits - 1)) - 1;
    if n < min || n > max {
        return Err("int_to_bytes_be: value does not fit in requested length".into());
    }
    let unsigned = if n < 0 { (1i128 << bits) + n } else { n } as u128;
    uint_to_bytes_be(&[int_value(unsigned as i128), Value::Int(len)])
}

fn bytes_to_f32_be(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "bytes_to_f32_be")?;
    let bytes = list_to_bytes(&args[0], "bytes_to_f32_be", 1)?;
    let arr: [u8; 4] = bytes
        .try_into()
        .map_err(|_| "bytes_to_f32_be: expected exactly 4 bytes".to_string())?;
    Ok(Value::Float(f32::from_be_bytes(arr) as f64))
}

fn bytes_to_f64_be(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "bytes_to_f64_be")?;
    let bytes = list_to_bytes(&args[0], "bytes_to_f64_be", 1)?;
    let arr: [u8; 8] = bytes
        .try_into()
        .map_err(|_| "bytes_to_f64_be: expected exactly 8 bytes".to_string())?;
    Ok(Value::Float(f64::from_be_bytes(arr)))
}

fn f32_to_bytes_be(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "f32_to_bytes_be")?;
    let f = coerce_to_float(&args[0], 1, "f32_to_bytes_be")? as f32;
    Ok(bytes_to_value(&f.to_be_bytes()))
}

fn f64_to_bytes_be(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "f64_to_bytes_be")?;
    let f = coerce_to_float(&args[0], 1, "f64_to_bytes_be")?;
    Ok(bytes_to_value(&f.to_be_bytes()))
}

fn utf8_to_bytes(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "utf8_to_bytes")?;
    let s = get_string(&args[0], 1, "utf8_to_bytes")?;
    Ok(bytes_to_value(s.as_bytes()))
}

fn bytes_to_utf8(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "bytes_to_utf8")?;
    let bytes = list_to_bytes(&args[0], "bytes_to_utf8", 1)?;
    String::from_utf8(bytes)
        .map(Value::String)
        .map_err(|e| format!("bytes_to_utf8: invalid UTF-8: {}", e))
}

/// `format(fmt, args...)` — replace `{}` placeholders positionally.
fn fn_format(args: &[Value]) -> Result<Value, String> {
    expect_min_args(args, 1, "format")?;
    let fmt = get_string(&args[0], 1, "format")?;
    let fmt_args = &args[1..];

    let mut result = String::new();
    let mut arg_idx = 0;
    let mut chars = fmt.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '{' {
            if chars.peek() == Some(&'}') {
                chars.next(); // consume '}'
                if arg_idx >= fmt_args.len() {
                    return Err(format!(
                        "format: not enough arguments (placeholder {} but only {} args)",
                        arg_idx,
                        fmt_args.len()
                    ));
                }
                result.push_str(&fmt_args[arg_idx].to_string());
                arg_idx += 1;
            } else {
                result.push(c);
            }
        } else {
            result.push(c);
        }
    }

    Ok(Value::String(result))
}

/// Extract a regex pattern string from either a Value::String or Value::Pattern.
fn get_pattern<'a>(v: &'a Value, pos: usize, fn_name: &str) -> Result<&'a str, String> {
    match v {
        Value::Pattern(s) => Ok(s.as_str()),
        Value::String(s) => Ok(s.as_str()),
        _ => Err(format!(
            "{}: argument {} must be a pattern or string, got {}",
            fn_name,
            pos,
            v.type_name()
        )),
    }
}

fn regex_match(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "regex_match")?;
    let s = get_string(&args[0], 1, "regex_match")?;
    let pattern = get_pattern(&args[1], 2, "regex_match")?;
    let re =
        regex::Regex::new(pattern).map_err(|e| format!("regex_match: invalid pattern: {}", e))?;
    Ok(Value::Bool(re.is_match(s)))
}

fn regex_capture(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "regex_capture")?;
    let s = get_string(&args[0], 1, "regex_capture")?;
    let pattern = get_pattern(&args[1], 2, "regex_capture")?;
    let re =
        regex::Regex::new(pattern).map_err(|e| format!("regex_capture: invalid pattern: {}", e))?;

    let captures: Vec<Value> = match re.captures(s) {
        None => vec![],
        Some(caps) => caps
            .iter()
            .skip(1) // skip full match, return capture groups only
            .map(|m| match m {
                Some(m) => Value::String(m.as_str().to_string()),
                None => Value::Null,
            })
            .collect(),
    };
    Ok(Value::List(captures))
}

fn regex_replace(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 3, "regex_replace")?;
    let s = get_string(&args[0], 1, "regex_replace")?;
    let pattern = get_pattern(&args[1], 2, "regex_replace")?;
    let replacement = get_string(&args[2], 3, "regex_replace")?;
    let re =
        regex::Regex::new(pattern).map_err(|e| format!("regex_replace: invalid pattern: {}", e))?;
    Ok(Value::String(re.replace(s, replacement).into_owned()))
}

fn regex_replace_all(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 3, "regex_replace_all")?;
    let s = get_string(&args[0], 1, "regex_replace_all")?;
    let pattern = get_pattern(&args[1], 2, "regex_replace_all")?;
    let replacement = get_string(&args[2], 3, "regex_replace_all")?;
    let re = regex::Regex::new(pattern)
        .map_err(|e| format!("regex_replace_all: invalid pattern: {}", e))?;
    Ok(Value::String(re.replace_all(s, replacement).into_owned()))
}

fn regex_split(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "regex_split")?;
    let s = get_string(&args[0], 1, "regex_split")?;
    let pattern = get_pattern(&args[1], 2, "regex_split")?;
    let re =
        regex::Regex::new(pattern).map_err(|e| format!("regex_split: invalid pattern: {}", e))?;
    let parts: Vec<Value> = re.split(s).map(|p| Value::String(p.to_string())).collect();
    Ok(Value::List(parts))
}

fn regex_find(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "regex_find")?;
    let s = get_string(&args[0], 1, "regex_find")?;
    let pattern = get_pattern(&args[1], 2, "regex_find")?;
    let re =
        regex::Regex::new(pattern).map_err(|e| format!("regex_find: invalid pattern: {}", e))?;
    match re.find(s) {
        Some(m) => Ok(Value::String(m.as_str().to_string())),
        None => Ok(Value::Null),
    }
}

fn regex_find_all(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "regex_find_all")?;
    let s = get_string(&args[0], 1, "regex_find_all")?;
    let pattern = get_pattern(&args[1], 2, "regex_find_all")?;
    let re = regex::Regex::new(pattern)
        .map_err(|e| format!("regex_find_all: invalid pattern: {}", e))?;
    let matches: Vec<Value> = re
        .find_iter(s)
        .map(|m| Value::String(m.as_str().to_string()))
        .collect();
    Ok(Value::List(matches))
}

// ---------------------------------------------------------------------------
// Section 14.2 — Math Functions
// ---------------------------------------------------------------------------

fn abs(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "abs")?;
    match &args[0] {
        Value::Int(i) => Ok(Value::Int(i.abs())),
        Value::BigInt(i) => Ok(Value::BigInt(i.abs())),
        Value::Float(f) => Ok(Value::Float(f.abs())),
        other => Err(format!(
            "abs: argument must be int or float, got {}",
            other.type_name()
        )),
    }
}

fn fn_min(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "min")?;
    match (&args[0], &args[1]) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(*a.min(b))),
        (Value::BigInt(a), Value::BigInt(b)) => Ok(Value::BigInt(*a.min(b))),
        (Value::Int(a), Value::BigInt(b)) => Ok(Value::BigInt((*a as i128).min(*b))),
        (Value::BigInt(a), Value::Int(b)) => Ok(Value::BigInt((*a).min(*b as i128))),
        _ => {
            let a = coerce_to_float(&args[0], 1, "min")?;
            let b = coerce_to_float(&args[1], 2, "min")?;
            Ok(Value::Float(a.min(b)))
        }
    }
}

fn fn_max(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "max")?;
    match (&args[0], &args[1]) {
        (Value::Int(a), Value::Int(b)) => Ok(Value::Int(*a.max(b))),
        (Value::BigInt(a), Value::BigInt(b)) => Ok(Value::BigInt(*a.max(b))),
        (Value::Int(a), Value::BigInt(b)) => Ok(Value::BigInt((*a as i128).max(*b))),
        (Value::BigInt(a), Value::Int(b)) => Ok(Value::BigInt((*a).max(*b as i128))),
        _ => {
            let a = coerce_to_float(&args[0], 1, "max")?;
            let b = coerce_to_float(&args[1], 2, "max")?;
            Ok(Value::Float(a.max(b)))
        }
    }
}

fn floor(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "floor")?;
    let f = coerce_to_float(&args[0], 1, "floor")?;
    Ok(Value::Int(f.floor() as i64))
}

fn ceil(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "ceil")?;
    let f = coerce_to_float(&args[0], 1, "ceil")?;
    Ok(Value::Int(f.ceil() as i64))
}

fn fn_round(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "round")?;
    let f = coerce_to_float(&args[0], 1, "round")?;
    Ok(Value::Int(f.round() as i64))
}

fn sqrt(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "sqrt")?;
    let f = coerce_to_float(&args[0], 1, "sqrt")?;
    Ok(Value::Float(f.sqrt()))
}

fn pow(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "pow")?;
    let base = coerce_to_float(&args[0], 1, "pow")?;
    let exp = coerce_to_float(&args[1], 2, "pow")?;
    Ok(Value::Float(base.powf(exp)))
}

fn sin(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "sin")?;
    let f = coerce_to_float(&args[0], 1, "sin")?;
    Ok(Value::Float(f.sin()))
}

fn cos(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "cos")?;
    let f = coerce_to_float(&args[0], 1, "cos")?;
    Ok(Value::Float(f.cos()))
}

fn tan(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "tan")?;
    let f = coerce_to_float(&args[0], 1, "tan")?;
    Ok(Value::Float(f.tan()))
}

fn asin(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "asin")?;
    let f = coerce_to_float(&args[0], 1, "asin")?;
    Ok(Value::Float(f.asin()))
}

fn acos(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "acos")?;
    let f = coerce_to_float(&args[0], 1, "acos")?;
    Ok(Value::Float(f.acos()))
}

fn atan(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "atan")?;
    let f = coerce_to_float(&args[0], 1, "atan")?;
    Ok(Value::Float(f.atan()))
}

fn atan2(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "atan2")?;
    let y = coerce_to_float(&args[0], 1, "atan2")?;
    let x = coerce_to_float(&args[1], 2, "atan2")?;
    Ok(Value::Float(y.atan2(x)))
}

fn degrees(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "degrees")?;
    let f = coerce_to_float(&args[0], 1, "degrees")?;
    Ok(Value::Float(f.to_degrees()))
}

fn radians(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "radians")?;
    let f = coerce_to_float(&args[0], 1, "radians")?;
    Ok(Value::Float(f.to_radians()))
}

fn pi(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 0, "pi")?;
    Ok(Value::Float(std::f64::consts::PI))
}

// ---------------------------------------------------------------------------
// Section 14.3 — Collection Functions
// ---------------------------------------------------------------------------

fn len(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "len")?;
    match &args[0] {
        Value::List(l) => Ok(Value::Int(l.len() as i64)),
        Value::Map(m) => Ok(Value::Int(m.len() as i64)),
        Value::Object(o) => Ok(Value::Int(o.fields.len() as i64)),
        Value::Set(s) => Ok(Value::Int(s.len() as i64)),
        other => Err(format!(
            "len: argument must be list, map, object, or set, got {}",
            other.type_name()
        )),
    }
}

fn keys(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "keys")?;
    match &args[0] {
        Value::Map(m) => {
            let ks: Vec<Value> = m.keys().map(|k| Value::String(k.clone())).collect();
            Ok(Value::List(ks))
        }
        Value::Object(o) => {
            let ks: Vec<Value> = o.fields.keys().map(|k| Value::String(k.clone())).collect();
            Ok(Value::List(ks))
        }
        other => Err(format!(
            "keys: argument must be map or object, got {}",
            other.type_name()
        )),
    }
}

fn fn_values(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "values")?;
    match &args[0] {
        Value::Map(m) => {
            let vs: Vec<Value> = m.values().cloned().collect();
            Ok(Value::List(vs))
        }
        Value::Object(o) => {
            let vs: Vec<Value> = o.fields.values().cloned().collect();
            Ok(Value::List(vs))
        }
        other => Err(format!(
            "values: argument must be map or object, got {}",
            other.type_name()
        )),
    }
}

fn flatten(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "flatten")?;
    let list = get_list(&args[0], 1, "flatten")?;
    let mut result = Vec::new();
    for item in list {
        match item {
            Value::List(inner) => result.extend(inner.iter().cloned()),
            other => result.push(other.clone()),
        }
    }
    Ok(Value::List(result))
}

fn fn_concat(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "concat")?;
    let l1 = get_list(&args[0], 1, "concat")?;
    let l2 = get_list(&args[1], 2, "concat")?;
    let mut result = l1.to_vec();
    result.extend_from_slice(l2);
    Ok(Value::List(result))
}

fn distinct(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "distinct")?;
    let list = get_list(&args[0], 1, "distinct")?;
    let mut seen: Vec<Value> = Vec::new();
    for item in list {
        if !seen.contains(item) {
            seen.push(item.clone());
        }
    }
    Ok(Value::List(seen))
}

/// Compare two Values for sorting purposes. Returns None if not comparable.
fn value_cmp(a: &Value, b: &Value) -> Option<std::cmp::Ordering> {
    use std::cmp::Ordering;
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => x.partial_cmp(y),
        (Value::BigInt(x), Value::BigInt(y)) => x.partial_cmp(y),
        (Value::Int(x), Value::BigInt(y)) => (*x as i128).partial_cmp(y),
        (Value::BigInt(x), Value::Int(y)) => x.partial_cmp(&(*y as i128)),
        (Value::Float(x), Value::Float(y)) => x.partial_cmp(y),
        (Value::Int(x), Value::Float(y)) => (*x as f64).partial_cmp(y),
        (Value::Float(x), Value::Int(y)) => x.partial_cmp(&(*y as f64)),
        (Value::BigInt(x), Value::Float(y)) => (*x as f64).partial_cmp(y),
        (Value::Float(x), Value::BigInt(y)) => x.partial_cmp(&(*y as f64)),
        (Value::String(x), Value::String(y)) => Some(x.cmp(y)),
        (Value::Bool(x), Value::Bool(y)) => Some(x.cmp(y)),
        _ => Some(Ordering::Equal), // fallback: treat as equal for mixed types
    }
}

fn fn_sort(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "sort")?;
    let list = get_list(&args[0], 1, "sort")?;
    let mut result = list.to_vec();
    result.sort_by(|a, b| value_cmp(a, b).unwrap_or(std::cmp::Ordering::Equal));
    Ok(Value::List(result))
}

fn fn_reverse(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "reverse")?;
    let list = get_list(&args[0], 1, "reverse")?;
    let mut result = list.to_vec();
    result.reverse();
    Ok(Value::List(result))
}

fn index_of(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "index_of")?;
    let list = get_list(&args[0], 1, "index_of")?;
    let needle = &args[1];
    for (i, v) in list.iter().enumerate() {
        if v == needle {
            return Ok(Value::Int(i as i64));
        }
    }
    Ok(Value::Int(-1))
}

fn range(args: &[Value]) -> Result<Value, String> {
    if args.len() < 2 || args.len() > 3 {
        return Err(format!(
            "range: expected 2 or 3 arguments, got {}",
            args.len()
        ));
    }
    let start = get_int(&args[0], 1, "range")?;
    let end = get_int(&args[1], 2, "range")?;
    let step = if args.len() == 3 {
        let s = get_int(&args[2], 3, "range")?;
        if s == 0 {
            return Err("range: step must not be zero".to_string());
        }
        s
    } else {
        1
    };

    let mut result = Vec::new();
    if step > 0 {
        let mut i = start;
        while i < end {
            result.push(Value::Int(i));
            i += step;
        }
    } else {
        let mut i = start;
        while i > end {
            result.push(Value::Int(i));
            i += step;
        }
    }
    Ok(Value::List(result))
}

fn zip(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "zip")?;
    let l1 = get_list(&args[0], 1, "zip")?;
    let l2 = get_list(&args[1], 2, "zip")?;
    let result: Vec<Value> = l1
        .iter()
        .zip(l2.iter())
        .map(|(a, b): (&Value, &Value)| Value::List(vec![a.clone(), b.clone()]))
        .collect();
    Ok(Value::List(result))
}

fn map_has(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "map_has")?;
    let map = match &args[0] {
        Value::Map(m) => m,
        Value::Object(o) => &o.fields,
        other => {
            return Err(format!(
                "map_has: argument 1 must be map or object, got {}",
                other.type_name()
            ))
        }
    };
    let key = get_string(&args[1], 2, "map_has")?;
    Ok(Value::Bool(map.contains_key(key)))
}

fn block_kind(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "block_kind")?;
    match &args[0] {
        Value::BlockRef(block) => Ok(Value::String(block.kind.clone())),
        other => Err(format!(
            "block_kind: argument 1 must be block_ref, got {}",
            other.type_name()
        )),
    }
}

fn block_id(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "block_id")?;
    match &args[0] {
        Value::BlockRef(block) => Ok(block
            .id
            .as_ref()
            .map(|id| Value::String(id.clone()))
            .unwrap_or(Value::Null)),
        other => Err(format!(
            "block_id: argument 1 must be block_ref, got {}",
            other.type_name()
        )),
    }
}

fn block_attrs(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "block_attrs")?;
    match &args[0] {
        Value::BlockRef(block) => {
            let mut attrs = block.attributes.clone();
            if let Some(id) = &block.id {
                attrs
                    .entry("id".to_string())
                    .or_insert_with(|| Value::String(id.clone()));
            }
            Ok(Value::Map(attrs))
        }
        other => Err(format!(
            "block_attrs: argument 1 must be block_ref, got {}",
            other.type_name()
        )),
    }
}

fn block_children(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "block_children")?;
    match &args[0] {
        Value::BlockRef(block) => Ok(Value::List(
            block
                .children
                .iter()
                .cloned()
                .map(Value::BlockRef)
                .collect(),
        )),
        other => Err(format!(
            "block_children: argument 1 must be block_ref, got {}",
            other.type_name()
        )),
    }
}

fn map_set(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 3, "map_set")?;
    let map = match &args[0] {
        Value::Map(m) => m,
        other => {
            return Err(format!(
                "map_set: argument 1 must be map, got {}",
                other.type_name()
            ))
        }
    };
    let key = get_string(&args[1], 2, "map_set")?;
    let mut result = map.clone();
    result.insert(key.to_string(), args[2].clone());
    Ok(Value::Map(result))
}

fn object(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "object")?;
    let type_name = get_string(&args[0], 1, "object")?;
    let fields = match &args[1] {
        Value::Map(m) => m.clone(),
        other => {
            return Err(format!(
                "object: argument 2 must be map, got {}",
                other.type_name()
            ))
        }
    };
    Ok(Value::Object(ObjectValue {
        type_name: type_name.to_string(),
        fields,
    }))
}

// ---------------------------------------------------------------------------
// Section 14.3b — Table Manipulation Functions
// ---------------------------------------------------------------------------

fn fn_find(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 3, "find")?;
    let list = get_list(&args[0], 1, "find")?;
    let key = get_string(&args[1], 2, "find")?;
    let needle = &args[2];
    for item in list {
        if let Value::Map(map) = item {
            if map.get(key) == Some(needle) {
                return Ok(item.clone());
            }
        }
    }
    Ok(Value::Null)
}

fn fn_insert_row(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "insert_row")?;
    let list = get_list(&args[0], 1, "insert_row")?;
    match &args[1] {
        Value::Map(_) => {}
        other => {
            return Err(format!(
                "insert_row: argument 2 must be map, got {}",
                other.type_name()
            ))
        }
    }
    let mut result = list.to_vec();
    result.push(args[1].clone());
    Ok(Value::List(result))
}

fn fn_remove_rows(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 3, "remove_rows")?;
    let list = get_list(&args[0], 1, "remove_rows")?;
    let key = get_string(&args[1], 2, "remove_rows")?;
    let needle = &args[2];
    let result: Vec<Value> = list
        .iter()
        .filter(|item| {
            if let Value::Map(map) = item {
                map.get(key) != Some(needle)
            } else {
                true
            }
        })
        .cloned()
        .collect();
    Ok(Value::List(result))
}

fn fn_update_rows(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 4, "update_rows")?;
    let list = get_list(&args[0], 1, "update_rows")?;
    let key = get_string(&args[1], 2, "update_rows")?;
    let needle = &args[2];
    let updates = match &args[3] {
        Value::Map(m) => m,
        other => {
            return Err(format!(
                "update_rows: argument 4 must be map, got {}",
                other.type_name()
            ))
        }
    };
    let result: Vec<Value> = list
        .iter()
        .map(|item| {
            if let Value::Map(map) = item {
                if map.get(key) == Some(needle) {
                    let mut merged = map.clone();
                    for (k, v) in updates {
                        merged.insert(k.clone(), v.clone());
                    }
                    Value::Map(merged)
                } else {
                    item.clone()
                }
            } else {
                item.clone()
            }
        })
        .collect();
    Ok(Value::List(result))
}

// ---------------------------------------------------------------------------
// Section 14.4 — Higher-Order Functions (placeholder)
// ---------------------------------------------------------------------------

fn higher_order_placeholder(_args: &[Value]) -> Result<Value, String> {
    Err("higher-order functions require special evaluation".to_string())
}

// ---------------------------------------------------------------------------
// Section 14.5 — Aggregate Functions
// ---------------------------------------------------------------------------

fn sum(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "sum")?;
    let list = get_list(&args[0], 1, "sum")?;
    if list.is_empty() {
        return Ok(Value::Int(0));
    }
    let mut has_float = false;
    let mut has_bigint = false;
    for v in list {
        match v {
            Value::Float(_) => {
                has_float = true;
                break;
            }
            Value::BigInt(_) => {
                has_bigint = true;
            }
            _ => {}
        }
    }
    if has_float {
        let mut acc = 0.0f64;
        for (i, v) in list.iter().enumerate() {
            acc += coerce_to_float(v, i + 1, "sum")?;
        }
        Ok(Value::Float(acc))
    } else if has_bigint {
        let mut acc = 0i128;
        for (i, v) in list.iter().enumerate() {
            match v {
                Value::Int(n) => acc += *n as i128,
                Value::BigInt(n) => acc += n,
                other => {
                    return Err(format!(
                        "sum: element {} must be int or float, got {}",
                        i + 1,
                        other.type_name()
                    ))
                }
            }
        }
        Ok(Value::BigInt(acc))
    } else {
        let mut acc = 0i64;
        for (i, v) in list.iter().enumerate() {
            acc += get_int(v, i + 1, "sum")?;
        }
        Ok(Value::Int(acc))
    }
}

fn avg(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "avg")?;
    let list = get_list(&args[0], 1, "avg")?;
    if list.is_empty() {
        return Err("avg: cannot average empty list".to_string());
    }
    let mut acc = 0.0f64;
    for (i, v) in list.iter().enumerate() {
        acc += coerce_to_float(v, i + 1, "avg")?;
    }
    Ok(Value::Float(acc / list.len() as f64))
}

fn min_of(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "min_of")?;
    let list = get_list(&args[0], 1, "min_of")?;
    if list.is_empty() {
        return Err("min_of: cannot find minimum of empty list".to_string());
    }
    let mut result = list[0].clone();
    for v in &list[1..] {
        if value_cmp(v, &result) == Some(std::cmp::Ordering::Less) {
            result = v.clone();
        }
    }
    Ok(result)
}

fn max_of(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "max_of")?;
    let list = get_list(&args[0], 1, "max_of")?;
    if list.is_empty() {
        return Err("max_of: cannot find maximum of empty list".to_string());
    }
    let mut result = list[0].clone();
    for v in &list[1..] {
        if value_cmp(v, &result) == Some(std::cmp::Ordering::Greater) {
            result = v.clone();
        }
    }
    Ok(result)
}

// ---------------------------------------------------------------------------
// Section 14.6 — Hash and Encoding Functions
// ---------------------------------------------------------------------------

fn fn_sha256(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "sha256")?;
    let s = get_string(&args[0], 1, "sha256")?;
    let mut hasher = Sha256::new();
    hasher.update(s.as_bytes());
    let result = hasher.finalize();
    let hex = result
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>();
    Ok(Value::String(hex))
}

fn base64_encode(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "base64_encode")?;
    let s = get_string(&args[0], 1, "base64_encode")?;
    Ok(Value::String(BASE64_STANDARD.encode(s.as_bytes())))
}

fn base64_decode(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "base64_decode")?;
    let s = get_string(&args[0], 1, "base64_decode")?;
    let bytes = BASE64_STANDARD
        .decode(s)
        .map_err(|e| format!("base64_decode: invalid base64: {}", e))?;
    let decoded = String::from_utf8(bytes)
        .map_err(|e| format!("base64_decode: decoded bytes are not valid UTF-8: {}", e))?;
    Ok(Value::String(decoded))
}

fn value_to_json(v: &Value) -> serde_json::Value {
    match v {
        Value::String(s) => serde_json::Value::String(s.clone()),
        Value::Int(i) => serde_json::json!(i),
        Value::Float(f) => serde_json::json!(f),
        Value::Bool(b) => serde_json::Value::Bool(*b),
        Value::Null => serde_json::Value::Null,
        Value::Identifier(s) => serde_json::Value::String(s.clone()),
        Value::List(items) => serde_json::Value::Array(items.iter().map(value_to_json).collect()),
        Value::Map(m) => {
            let obj: serde_json::Map<String, serde_json::Value> = m
                .iter()
                .map(|(k, v)| (k.clone(), value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        _ => serde_json::Value::String(format!("{}", v)),
    }
}

fn json_encode(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "json_encode")?;
    let j = value_to_json(&args[0]);
    let s = serde_json::to_string(&j)
        .map_err(|e| format!("json_encode: serialization failed: {}", e))?;
    Ok(Value::String(s))
}

// ---------------------------------------------------------------------------
// Section 14.7 — Type Coercion Functions
// ---------------------------------------------------------------------------

fn to_string(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "to_string")?;
    Ok(Value::String(args[0].to_string()))
}

fn to_int(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "to_int")?;
    match &args[0] {
        Value::Int(i) => Ok(Value::Int(*i)),
        Value::BigInt(i) => {
            if *i > i64::MAX as i128 || *i < i64::MIN as i128 {
                Err(format!("to_int: bigint value {} overflows i64", i))
            } else {
                Ok(Value::Int(*i as i64))
            }
        }
        Value::Float(f) => Ok(Value::Int(*f as i64)),
        Value::Bool(b) => Ok(Value::Int(if *b { 1 } else { 0 })),
        Value::String(s) => s
            .trim()
            .parse::<i64>()
            .map(Value::Int)
            .map_err(|_| format!("to_int: cannot convert string {:?} to int", s)),
        other => Err(format!(
            "to_int: cannot convert {} to int",
            other.type_name()
        )),
    }
}

fn to_float(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "to_float")?;
    match &args[0] {
        Value::Float(f) => Ok(Value::Float(*f)),
        Value::Int(i) => Ok(Value::Float(*i as f64)),
        Value::BigInt(i) => Ok(Value::Float(*i as f64)),
        Value::String(s) => s
            .trim()
            .parse::<f64>()
            .map(Value::Float)
            .map_err(|_| format!("to_float: cannot convert string {:?} to float", s)),
        other => Err(format!(
            "to_float: cannot convert {} to float",
            other.type_name()
        )),
    }
}

fn to_bool(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "to_bool")?;
    match &args[0] {
        Value::Bool(b) => Ok(Value::Bool(*b)),
        Value::Int(0) => Ok(Value::Bool(false)),
        Value::Int(1) => Ok(Value::Bool(true)),
        Value::Int(i) => Err(format!(
            "to_bool: int {} cannot be converted to bool (only 0 or 1)",
            i
        )),
        Value::String(s) => match s.trim() {
            "true" => Ok(Value::Bool(true)),
            "false" => Ok(Value::Bool(false)),
            other => Err(format!(
                "to_bool: string {:?} cannot be converted to bool (expected \"true\" or \"false\")",
                other
            )),
        },
        other => Err(format!(
            "to_bool: cannot convert {} to bool",
            other.type_name()
        )),
    }
}

fn type_of(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "type_of")?;
    Ok(Value::String(args[0].type_name().to_string()))
}

// ---------------------------------------------------------------------------
// Section 14.9 — Reference and Query Functions
// ---------------------------------------------------------------------------

fn fn_has(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "has")?;
    let block_ref = match &args[0] {
        Value::BlockRef(br) => br,
        other => {
            return Err(format!(
                "has: argument 1 must be block_ref, got {}",
                other.type_name()
            ))
        }
    };
    let attr_name = get_string(&args[1], 2, "has")?;
    // Check attributes AND child blocks by kind
    let has_attr = block_ref.attributes.contains_key(attr_name);
    let has_child = block_ref.children.iter().any(|c| c.kind == attr_name);
    Ok(Value::Bool(has_attr || has_child))
}

fn fn_children(args: &[Value]) -> Result<Value, String> {
    if args.is_empty() || args.len() > 2 {
        return Err("children expects 1 or 2 arguments".into());
    }

    let block_ref = match &args[0] {
        Value::BlockRef(br) => br,
        other => {
            return Err(format!(
                "children: argument 1 must be block_ref, got {}",
                other.type_name()
            ))
        }
    };

    let kind = match args.get(1) {
        Some(value) => Some(get_string(value, 2, "children")?),
        None => None,
    };

    let children = block_ref
        .children
        .iter()
        .filter(|child| kind.map(|kind| child.kind == kind).unwrap_or(true))
        .cloned()
        .map(Value::BlockRef)
        .collect();
    Ok(Value::List(children))
}

fn fn_has_decorator(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 2, "has_decorator")?;
    let block_ref = match &args[0] {
        Value::BlockRef(br) => br,
        other => {
            return Err(format!(
                "has_decorator: argument 1 must be block_ref, got {}",
                other.type_name()
            ))
        }
    };
    let deco_name = get_string(&args[1], 2, "has_decorator")?;
    let found = block_ref.decorators.iter().any(|d| d.name == deco_name);
    Ok(Value::Bool(found))
}

// ---------------------------------------------------------------------------
// Section 14.8 — Date/Duration Constructors
// ---------------------------------------------------------------------------

/// Validate and construct an ISO 8601 date (YYYY-MM-DD).
fn fn_date(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "date")?;
    let s = get_string(&args[0], 1, "date")?;
    validate_iso_date(s)?;
    Ok(Value::Date(s.to_string()))
}

fn fn_offset_datetime(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "offset_datetime")?;
    let s = get_string(&args[0], 1, "offset_datetime")?;
    validate_toml_offset_datetime(s)?;
    Ok(Value::OffsetDateTime(s.to_string()))
}

fn fn_local_datetime(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "local_datetime")?;
    let s = get_string(&args[0], 1, "local_datetime")?;
    validate_toml_local_datetime(s)?;
    Ok(Value::LocalDateTime(s.to_string()))
}

fn fn_local_time(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "local_time")?;
    let s = get_string(&args[0], 1, "local_time")?;
    validate_toml_local_time(s)?;
    Ok(Value::LocalTime(s.to_string()))
}

/// Validate and construct an ISO 8601 duration (PnYnMnDTnHnMnS).
fn fn_duration(args: &[Value]) -> Result<Value, String> {
    expect_args(args, 1, "duration")?;
    let s = get_string(&args[0], 1, "duration")?;
    validate_iso_duration(s)?;
    Ok(Value::Duration(s.to_string()))
}

fn validate_iso_date(s: &str) -> Result<(), String> {
    let bytes = s.as_bytes();
    // Expect exactly YYYY-MM-DD (10 chars)
    if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
        return Err(format!("date: invalid format {:?}, expected YYYY-MM-DD", s));
    }
    let year: u32 = s[0..4]
        .parse()
        .map_err(|_| format!("date: invalid year in {:?}", s))?;
    let month: u32 = s[5..7]
        .parse()
        .map_err(|_| format!("date: invalid month in {:?}", s))?;
    let day: u32 = s[8..10]
        .parse()
        .map_err(|_| format!("date: invalid day in {:?}", s))?;

    if !(1..=9999).contains(&year) {
        return Err(format!("date: year {} out of range 0001-9999", year));
    }
    if !(1..=12).contains(&month) {
        return Err(format!("date: month {} out of range 01-12", month));
    }
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            if (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400) {
                29
            } else {
                28
            }
        }
        _ => unreachable!(),
    };
    if !(1..=max_day).contains(&day) {
        return Err(format!(
            "date: day {} out of range for month {} (max {})",
            day, month, max_day
        ));
    }
    Ok(())
}

fn validate_toml_local_time(s: &str) -> Result<(), String> {
    let re = regex::Regex::new(r"^[0-9]{2}:[0-9]{2}(:[0-9]{2}(\.[0-9]+)?)?$").unwrap();
    if !re.is_match(s) {
        return Err(format!(
            "local_time: invalid format {:?}, expected HH:MM[:SS[.fraction]]",
            s
        ));
    }
    let hour: u32 = s[0..2].parse().unwrap_or(99);
    let minute: u32 = s[3..5].parse().unwrap_or(99);
    let second: u32 = if s.len() >= 8 {
        s[6..8].parse().unwrap_or(99)
    } else {
        0
    };
    if hour > 23 || minute > 59 || second > 59 {
        return Err(format!("local_time: invalid time {:?}", s));
    }
    Ok(())
}

fn validate_toml_local_datetime(s: &str) -> Result<(), String> {
    let Some((date, time)) = s.split_once('T').or_else(|| s.split_once(' ')) else {
        return Err(format!(
            "local_datetime: invalid format {:?}, expected YYYY-MM-DD[T ]HH:MM[:SS]",
            s
        ));
    };
    validate_iso_date(date).map_err(|e| e.replace("date:", "local_datetime date:"))?;
    validate_toml_local_time(time).map_err(|e| e.replace("local_time:", "local_datetime time:"))?;
    Ok(())
}

fn validate_toml_offset_datetime(s: &str) -> Result<(), String> {
    let re = regex::Regex::new(r"^(.+[T ].+)(Z|[+-][0-9]{2}:[0-9]{2})$").unwrap();
    let Some(caps) = re.captures(s) else {
        return Err(format!(
            "offset_datetime: invalid format {:?}, expected RFC 3339 date-time with offset",
            s
        ));
    };
    validate_toml_local_datetime(&caps[1])
        .map_err(|e| e.replace("local_datetime:", "offset_datetime:"))?;
    if &caps[2] != "Z" {
        let offset = &caps[2];
        let hour: u32 = offset[1..3].parse().unwrap_or(99);
        let minute: u32 = offset[4..6].parse().unwrap_or(99);
        if hour > 23 || minute > 59 {
            return Err(format!("offset_datetime: invalid offset {:?}", offset));
        }
    }
    Ok(())
}

fn validate_iso_duration(s: &str) -> Result<(), String> {
    if s.is_empty() || !s.starts_with('P') {
        return Err(format!(
            "duration: invalid format {:?}, must start with 'P'",
            s
        ));
    }
    let rest = &s[1..];
    if rest.is_empty() {
        return Err(format!(
            "duration: empty duration {:?}, need at least one component",
            s
        ));
    }

    // Split on 'T' to separate date and time parts
    let (date_part, time_part) = if let Some(t_pos) = rest.find('T') {
        (&rest[..t_pos], Some(&rest[t_pos + 1..]))
    } else {
        (rest, None)
    };

    let mut found_any = false;

    // Parse date components: nY, nM, nD
    if !date_part.is_empty() {
        let mut remaining = date_part;
        for expected in ['Y', 'M', 'D'] {
            if let Some(pos) = remaining.find(expected) {
                let num_str = &remaining[..pos];
                if num_str.is_empty() || num_str.parse::<f64>().is_err() {
                    return Err(format!(
                        "duration: invalid number before '{}' in {:?}",
                        expected, s
                    ));
                }
                found_any = true;
                remaining = &remaining[pos + 1..];
            }
        }
        if !remaining.is_empty() {
            return Err(format!(
                "duration: unexpected characters {:?} in date part of {:?}",
                remaining, s
            ));
        }
    }

    // Parse time components: nH, nM, nS
    if let Some(tp) = time_part {
        if tp.is_empty() {
            return Err(format!(
                "duration: 'T' present but no time components in {:?}",
                s
            ));
        }
        let mut remaining = tp;
        for expected in ['H', 'M', 'S'] {
            if let Some(pos) = remaining.find(expected) {
                let num_str = &remaining[..pos];
                if num_str.is_empty() || num_str.parse::<f64>().is_err() {
                    return Err(format!(
                        "duration: invalid number before '{}' in {:?}",
                        expected, s
                    ));
                }
                found_any = true;
                remaining = &remaining[pos + 1..];
            }
        }
        if !remaining.is_empty() {
            return Err(format!(
                "duration: unexpected characters {:?} in time part of {:?}",
                remaining, s
            ));
        }
    }

    if !found_any {
        return Err(format!("duration: no valid components found in {:?}", s));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;

    fn s(v: &str) -> Value {
        Value::String(v.to_string())
    }

    fn i(v: i64) -> Value {
        Value::Int(v)
    }

    fn f(v: f64) -> Value {
        Value::Float(v)
    }

    fn list(items: Vec<Value>) -> Value {
        Value::List(items)
    }

    // --- String ---

    #[test]
    fn test_upper() {
        assert_eq!(upper(&[s("hello")]).unwrap(), s("HELLO"));
        assert_eq!(upper(&[s("Hello World")]).unwrap(), s("HELLO WORLD"));
        assert!(upper(&[]).is_err());
        assert!(upper(&[i(1)]).is_err());
    }

    #[test]
    fn test_lower() {
        assert_eq!(lower(&[s("HELLO")]).unwrap(), s("hello"));
        assert_eq!(lower(&[s("MiXeD")]).unwrap(), s("mixed"));
    }

    #[test]
    fn test_trim() {
        assert_eq!(trim(&[s("  hello  ")]).unwrap(), s("hello"));
        assert_eq!(trim(&[s("\t\n foo \n")]).unwrap(), s("foo"));
    }

    #[test]
    fn test_split_delimited() {
        assert_eq!(
            split_delimited(&[s("name,note\nalice,\"hello, \"\"world\"\"\""), s(",")]).unwrap(),
            list(vec![
                list(vec![s("name"), s("note")]),
                list(vec![s("alice"), s("hello, \"world\"")]),
            ])
        );
        assert!(split_delimited(&[s("name,note\nalice,\"unterminated"), s(",")]).is_err());
    }

    #[test]
    fn test_trim_prefix() {
        assert_eq!(
            trim_prefix(&[s("hello world"), s("hello ")]).unwrap(),
            s("world")
        );
        // no match → unchanged
        assert_eq!(
            trim_prefix(&[s("hello world"), s("xyz")]).unwrap(),
            s("hello world")
        );
    }

    #[test]
    fn test_trim_suffix() {
        assert_eq!(
            trim_suffix(&[s("hello world"), s(" world")]).unwrap(),
            s("hello")
        );
    }

    #[test]
    fn test_replace() {
        assert_eq!(
            fn_replace(&[s("aabbcc"), s("bb"), s("XX")]).unwrap(),
            s("aaXXcc")
        );
    }

    #[test]
    fn test_split() {
        let result = split(&[s(","), s("a,b,c")]).unwrap();
        assert_eq!(result, list(vec![s("a"), s("b"), s("c")]));
    }

    #[test]
    fn test_join() {
        let result = join(&[s(", "), list(vec![s("a"), s("b"), s("c")])]).unwrap();
        assert_eq!(result, s("a, b, c"));
    }

    #[test]
    fn test_starts_with() {
        assert_eq!(
            starts_with(&[s("hello"), s("he")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            starts_with(&[s("hello"), s("lo")]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_ends_with() {
        assert_eq!(
            ends_with(&[s("hello"), s("lo")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            ends_with(&[s("hello"), s("he")]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_contains_string() {
        assert_eq!(
            fn_contains(&[s("foobar"), s("oba")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            fn_contains(&[s("foobar"), s("xyz")]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_contains_list() {
        assert_eq!(
            fn_contains(&[list(vec![i(1), i(2), i(3)]), i(2)]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            fn_contains(&[list(vec![i(1), i(2), i(3)]), i(5)]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_length() {
        assert_eq!(length(&[s("hello")]).unwrap(), i(5));
        assert_eq!(length(&[s("caf\u{00e9}")]).unwrap(), i(4)); // "café" is 4 chars
    }

    #[test]
    fn test_substr() {
        assert_eq!(substr(&[s("hello"), i(1), i(4)]).unwrap(), s("ell"));
        assert_eq!(substr(&[s("hello"), i(0), i(5)]).unwrap(), s("hello"));
        assert_eq!(substr(&[s("hello"), i(2)]).unwrap(), s("llo"));
    }

    #[test]
    fn test_char_codepoint() {
        assert_eq!(char_codepoint(&[s("A")]).unwrap(), i(65));
        assert_eq!(char_codepoint(&[s("é")]).unwrap(), i(233));
        assert_eq!(char_codepoint(&[s("\u{10ffff}")]).unwrap(), i(0x10ffff));
        assert!(char_codepoint(&[s("")]).is_err());
        assert!(char_codepoint(&[s("ab")]).is_err());
    }

    #[test]
    fn test_char_from_codepoint() {
        assert_eq!(char_from_codepoint(&[i(65)]).unwrap(), s("A"));
        assert_eq!(char_from_codepoint(&[i(233)]).unwrap(), s("é"));
        assert_eq!(
            char_from_codepoint(&[i(0x10ffff)]).unwrap(),
            s("\u{10ffff}")
        );
        assert!(char_from_codepoint(&[i(-1)]).is_err());
        assert!(char_from_codepoint(&[i(0xd800)]).is_err());
        assert!(char_from_codepoint(&[i(0x110000)]).is_err());
    }

    #[test]
    fn test_format() {
        assert_eq!(
            fn_format(&[s("Hello, {}!"), s("world")]).unwrap(),
            s("Hello, world!")
        );
        assert_eq!(
            fn_format(&[s("{} + {} = {}"), i(1), i(2), i(3)]).unwrap(),
            s("1 + 2 = 3")
        );
    }

    #[test]
    fn test_regex_match() {
        assert_eq!(
            regex_match(&[s("hello123"), s(r"\d+")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            regex_match(&[s("hello"), s(r"^\d+$")]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_regex_capture() {
        let result = regex_capture(&[s("2024-03-15"), s(r"(\d{4})-(\d{2})-(\d{2})")]).unwrap();
        assert_eq!(result, list(vec![s("2024"), s("03"), s("15")]));
    }

    #[test]
    fn test_regex_match_with_pattern_value() {
        assert_eq!(
            regex_match(&[s("hello123"), Value::Pattern(r"\d+".into())]).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_regex_replace() {
        assert_eq!(
            regex_replace(&[s("hello world"), s(r"\s+"), s("-")]).unwrap(),
            s("hello-world")
        );
    }

    #[test]
    fn test_regex_replace_all() {
        assert_eq!(
            regex_replace_all(&[s("a b c"), s(r"\s+"), s("-")]).unwrap(),
            s("a-b-c")
        );
    }

    #[test]
    fn test_regex_split() {
        assert_eq!(
            regex_split(&[s("one:two::three"), s(r":+")]).unwrap(),
            list(vec![s("one"), s("two"), s("three")])
        );
    }

    #[test]
    fn test_regex_find() {
        assert_eq!(
            regex_find(&[s("price: $42.50"), s(r"\d+\.\d+")]).unwrap(),
            s("42.50")
        );
        assert_eq!(regex_find(&[s("none"), s(r"\d+")]).unwrap(), Value::Null);
    }

    #[test]
    fn test_regex_find_all() {
        assert_eq!(
            regex_find_all(&[s("a1 b2 c3"), s(r"[a-z]\d")]).unwrap(),
            list(vec![s("a1"), s("b2"), s("c3")])
        );
    }

    // --- Math ---

    #[test]
    fn test_abs() {
        assert_eq!(abs(&[i(-5)]).unwrap(), i(5));
        assert_eq!(abs(&[i(5)]).unwrap(), i(5));
        assert_eq!(abs(&[f(-3.14)]).unwrap(), f(3.14));
    }

    #[test]
    fn test_min() {
        assert_eq!(fn_min(&[i(3), i(7)]).unwrap(), i(3));
        assert_eq!(fn_min(&[f(3.5), f(2.1)]).unwrap(), f(2.1));
        // int + float → float promotion
        assert_eq!(fn_min(&[i(5), f(3.0)]).unwrap(), f(3.0));
    }

    #[test]
    fn test_max() {
        assert_eq!(fn_max(&[i(3), i(7)]).unwrap(), i(7));
        assert_eq!(fn_max(&[f(3.5), f(2.1)]).unwrap(), f(3.5));
        assert_eq!(fn_max(&[i(5), f(6.0)]).unwrap(), f(6.0));
    }

    #[test]
    fn test_floor_ceil_round() {
        assert_eq!(floor(&[f(3.7)]).unwrap(), i(3));
        assert_eq!(ceil(&[f(3.2)]).unwrap(), i(4));
        assert_eq!(fn_round(&[f(3.5)]).unwrap(), i(4));
        assert_eq!(fn_round(&[f(3.4)]).unwrap(), i(3));
    }

    #[test]
    fn test_sqrt() {
        assert_eq!(sqrt(&[i(4)]).unwrap(), f(2.0));
        assert_eq!(sqrt(&[f(9.0)]).unwrap(), f(3.0));
    }

    #[test]
    fn test_pow() {
        assert_eq!(pow(&[i(2), i(10)]).unwrap(), f(1024.0));
        assert_eq!(pow(&[f(2.0), f(0.5)]).unwrap(), f(2.0f64.powf(0.5)));
    }

    #[test]
    fn test_trig() {
        assert_eq!(sin(&[i(0)]).unwrap(), f(0.0));
        assert!(
            (match sin(&[f(std::f64::consts::FRAC_PI_2)]).unwrap() {
                Value::Float(v) => v,
                other => panic!("expected float, got {other:?}"),
            } - 1.0)
                .abs()
                < 1e-12
        );
        assert!(
            (match cos(&[f(std::f64::consts::PI)]).unwrap() {
                Value::Float(v) => v,
                other => panic!("expected float, got {other:?}"),
            } + 1.0)
                .abs()
                < 1e-12
        );
        assert!(
            (match tan(&[f(std::f64::consts::FRAC_PI_4)]).unwrap() {
                Value::Float(v) => v,
                other => panic!("expected float, got {other:?}"),
            } - 1.0)
                .abs()
                < 1e-12
        );
        assert_eq!(asin(&[f(1.0)]).unwrap(), f(std::f64::consts::FRAC_PI_2));
        assert_eq!(acos(&[f(0.0)]).unwrap(), f(std::f64::consts::FRAC_PI_2));
        assert_eq!(atan(&[f(1.0)]).unwrap(), f(std::f64::consts::FRAC_PI_4));
        assert_eq!(
            atan2(&[f(1.0), f(1.0)]).unwrap(),
            f(std::f64::consts::FRAC_PI_4)
        );
        assert_eq!(degrees(&[f(std::f64::consts::PI)]).unwrap(), f(180.0));
        assert_eq!(radians(&[f(180.0)]).unwrap(), f(std::f64::consts::PI));
        assert_eq!(pi(&[]).unwrap(), f(std::f64::consts::PI));
    }

    // --- Collections ---

    #[test]
    fn test_len() {
        assert_eq!(len(&[list(vec![i(1), i(2), i(3)])]).unwrap(), i(3));
        assert_eq!(len(&[list(vec![])]).unwrap(), i(0));

        let mut m = IndexMap::new();
        m.insert("a".to_string(), i(1));
        m.insert("b".to_string(), i(2));
        assert_eq!(len(&[Value::Map(m)]).unwrap(), i(2));

        assert_eq!(len(&[Value::Set(vec![i(1), i(2)])]).unwrap(), i(2));
    }

    #[test]
    fn test_keys() {
        let mut m = IndexMap::new();
        m.insert("x".to_string(), i(1));
        m.insert("y".to_string(), i(2));
        let result = keys(&[Value::Map(m)]).unwrap();
        assert_eq!(result, list(vec![s("x"), s("y")]));
    }

    #[test]
    fn test_values_fn() {
        let mut m = IndexMap::new();
        m.insert("x".to_string(), i(10));
        m.insert("y".to_string(), i(20));
        let result = fn_values(&[Value::Map(m)]).unwrap();
        assert_eq!(result, list(vec![i(10), i(20)]));
    }

    #[test]
    fn test_flatten() {
        let nested = list(vec![
            list(vec![i(1), i(2)]),
            list(vec![i(3)]),
            list(vec![i(4), i(5)]),
        ]);
        assert_eq!(
            flatten(&[nested]).unwrap(),
            list(vec![i(1), i(2), i(3), i(4), i(5)])
        );
    }

    #[test]
    fn test_concat() {
        let result = fn_concat(&[list(vec![i(1), i(2)]), list(vec![i(3), i(4)])]).unwrap();
        assert_eq!(result, list(vec![i(1), i(2), i(3), i(4)]));
    }

    #[test]
    fn test_distinct() {
        let result = distinct(&[list(vec![i(1), i(2), i(1), i(3), i(2)])]).unwrap();
        assert_eq!(result, list(vec![i(1), i(2), i(3)]));
    }

    #[test]
    fn test_sort() {
        let result = fn_sort(&[list(vec![i(3), i(1), i(2)])]).unwrap();
        assert_eq!(result, list(vec![i(1), i(2), i(3)]));

        let result = fn_sort(&[list(vec![s("banana"), s("apple"), s("cherry")])]).unwrap();
        assert_eq!(result, list(vec![s("apple"), s("banana"), s("cherry")]));
    }

    #[test]
    fn test_reverse() {
        let result = fn_reverse(&[list(vec![i(1), i(2), i(3)])]).unwrap();
        assert_eq!(result, list(vec![i(3), i(2), i(1)]));
    }

    #[test]
    fn test_index_of() {
        assert_eq!(
            index_of(&[list(vec![i(10), i(20), i(30)]), i(20)]).unwrap(),
            i(1)
        );
        assert_eq!(
            index_of(&[list(vec![i(10), i(20), i(30)]), i(99)]).unwrap(),
            i(-1)
        );
    }

    #[test]
    fn test_range() {
        assert_eq!(
            range(&[i(0), i(5)]).unwrap(),
            list(vec![i(0), i(1), i(2), i(3), i(4)])
        );
        assert_eq!(
            range(&[i(0), i(10), i(2)]).unwrap(),
            list(vec![i(0), i(2), i(4), i(6), i(8)])
        );
        assert_eq!(
            range(&[i(5), i(0), i(-1)]).unwrap(),
            list(vec![i(5), i(4), i(3), i(2), i(1)])
        );
        assert!(range(&[i(0), i(5), i(0)]).is_err());
    }

    #[test]
    fn test_zip() {
        let result = zip(&[list(vec![i(1), i(2)]), list(vec![s("a"), s("b")])]).unwrap();
        assert_eq!(
            result,
            list(vec![list(vec![i(1), s("a")]), list(vec![i(2), s("b")])])
        );
    }

    // --- Aggregate ---

    #[test]
    fn test_sum() {
        assert_eq!(sum(&[list(vec![i(1), i(2), i(3)])]).unwrap(), i(6));
        assert_eq!(sum(&[list(vec![f(1.5), f(2.5)])]).unwrap(), f(4.0));
        assert_eq!(sum(&[list(vec![])]).unwrap(), i(0));
    }

    #[test]
    fn test_avg() {
        assert_eq!(avg(&[list(vec![i(1), i(2), i(3)])]).unwrap(), f(2.0));
        assert!(avg(&[list(vec![])]).is_err());
    }

    #[test]
    fn test_min_of_max_of() {
        assert_eq!(
            min_of(&[list(vec![i(3), i(1), i(4), i(1), i(5)])]).unwrap(),
            i(1)
        );
        assert_eq!(
            max_of(&[list(vec![i(3), i(1), i(4), i(1), i(5)])]).unwrap(),
            i(5)
        );
        assert!(min_of(&[list(vec![])]).is_err());
        assert!(max_of(&[list(vec![])]).is_err());
    }

    // --- Hash / encoding ---

    #[test]
    fn test_sha256() {
        let result = fn_sha256(&[s("hello")]).unwrap();
        // known SHA-256 of "hello"
        assert_eq!(
            result,
            s("2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824")
        );
    }

    #[test]
    fn test_base64_encode() {
        let result = base64_encode(&[s("hello")]).unwrap();
        assert_eq!(result, s("aGVsbG8="));
    }

    #[test]
    fn test_base64_roundtrip() {
        let encoded = base64_encode(&[s("Hello, World!")]).unwrap();
        let decoded = base64_decode(&[encoded]).unwrap();
        assert_eq!(decoded, s("Hello, World!"));
    }

    #[test]
    fn test_base64_decode_invalid() {
        assert!(base64_decode(&[s("not valid base64!!!")]).is_err());
    }

    #[test]
    fn test_json_encode() {
        assert_eq!(json_encode(&[i(42)]).unwrap(), s("42"));
        assert_eq!(json_encode(&[s("hello")]).unwrap(), s("\"hello\""));
        assert_eq!(json_encode(&[Value::Bool(true)]).unwrap(), s("true"));
        assert_eq!(json_encode(&[Value::Null]).unwrap(), s("null"));

        let result = json_encode(&[list(vec![i(1), i(2)])]).unwrap();
        assert_eq!(result, s("[1,2]"));

        let mut m = IndexMap::new();
        m.insert("k".to_string(), s("v"));
        let result = json_encode(&[Value::Map(m)]).unwrap();
        assert_eq!(result, s("{\"k\":\"v\"}"));
    }

    // --- Type coercion ---

    #[test]
    fn test_to_string() {
        assert_eq!(to_string(&[i(42)]).unwrap(), s("42"));
        assert_eq!(to_string(&[f(3.14)]).unwrap(), s("3.14"));
        assert_eq!(to_string(&[Value::Bool(true)]).unwrap(), s("true"));
        assert_eq!(to_string(&[Value::Null]).unwrap(), s("null"));
        assert_eq!(to_string(&[s("already")]).unwrap(), s("already"));
    }

    #[test]
    fn test_to_int() {
        assert_eq!(to_int(&[s("42")]).unwrap(), i(42));
        assert_eq!(to_int(&[f(3.9)]).unwrap(), i(3));
        assert_eq!(to_int(&[Value::Bool(true)]).unwrap(), i(1));
        assert_eq!(to_int(&[Value::Bool(false)]).unwrap(), i(0));
        assert!(to_int(&[s("not a number")]).is_err());
    }

    #[test]
    fn test_to_float() {
        assert_eq!(to_float(&[s("3.14")]).unwrap(), f(3.14));
        assert_eq!(to_float(&[i(7)]).unwrap(), f(7.0));
        assert!(to_float(&[s("abc")]).is_err());
    }

    #[test]
    fn test_to_bool() {
        assert_eq!(to_bool(&[s("true")]).unwrap(), Value::Bool(true));
        assert_eq!(to_bool(&[s("false")]).unwrap(), Value::Bool(false));
        assert_eq!(to_bool(&[i(1)]).unwrap(), Value::Bool(true));
        assert_eq!(to_bool(&[i(0)]).unwrap(), Value::Bool(false));
        assert!(to_bool(&[i(2)]).is_err());
        assert!(to_bool(&[s("yes")]).is_err());
    }

    #[test]
    fn test_type_of() {
        assert_eq!(type_of(&[s("hello")]).unwrap(), s("string"));
        assert_eq!(type_of(&[i(1)]).unwrap(), s("int"));
        assert_eq!(type_of(&[f(1.0)]).unwrap(), s("float"));
        assert_eq!(type_of(&[Value::Bool(true)]).unwrap(), s("bool"));
        assert_eq!(type_of(&[Value::Null]).unwrap(), s("null"));
        assert_eq!(type_of(&[list(vec![])]).unwrap(), s("list"));
        assert_eq!(type_of(&[Value::Map(IndexMap::new())]).unwrap(), s("map"));
    }

    // --- Builtin registry ---

    #[test]
    fn test_registry_completeness() {
        let registry = builtin_registry();
        let expected = [
            "upper",
            "lower",
            "trim",
            "trim_prefix",
            "trim_suffix",
            "replace",
            "split",
            "join",
            "starts_with",
            "ends_with",
            "contains",
            "length",
            "substr",
            "char_codepoint",
            "char_from_codepoint",
            "bytes",
            "bytes_data",
            "msgpack_ext",
            "msgpack_ext_type_id",
            "msgpack_ext_data",
            "msgpack_timestamp",
            "msgpack_timestamp_seconds",
            "msgpack_timestamp_nanoseconds",
            "bytes_to_uint_be",
            "bytes_to_int_be",
            "uint_to_bytes_be",
            "int_to_bytes_be",
            "bytes_to_f32_be",
            "bytes_to_f64_be",
            "f32_to_bytes_be",
            "f64_to_bytes_be",
            "utf8_to_bytes",
            "bytes_to_utf8",
            "format",
            "regex_match",
            "regex_capture",
            "abs",
            "min",
            "max",
            "floor",
            "ceil",
            "round",
            "sqrt",
            "pow",
            "sin",
            "cos",
            "tan",
            "asin",
            "acos",
            "atan",
            "atan2",
            "degrees",
            "radians",
            "pi",
            "len",
            "keys",
            "values",
            "flatten",
            "concat",
            "distinct",
            "sort",
            "reverse",
            "index_of",
            "range",
            "zip",
            "map_has",
            "map_set",
            "find",
            "insert_row",
            "remove_rows",
            "update_rows",
            "map",
            "filter",
            "every",
            "some",
            "reduce",
            "count",
            "sum",
            "avg",
            "min_of",
            "max_of",
            "sha256",
            "base64_encode",
            "base64_decode",
            "json_encode",
            "to_string",
            "to_int",
            "to_float",
            "to_bool",
            "type_of",
            "has",
            "children",
            "has_decorator",
            "date",
            "offset_datetime",
            "local_datetime",
            "local_time",
            "duration",
        ];
        for name in &expected {
            assert!(registry.contains_key(*name), "missing builtin: {}", name);
        }
    }

    #[test]
    fn test_higher_order_placeholder() {
        let err = higher_order_placeholder(&[]).unwrap_err();
        assert!(err.contains("higher-order functions require special evaluation"));
    }

    // --- Reference and Query Functions ---

    // --- Table manipulation ---

    fn make_row(pairs: &[(&str, Value)]) -> Value {
        let mut m = IndexMap::new();
        for (k, v) in pairs {
            m.insert(k.to_string(), v.clone());
        }
        Value::Map(m)
    }

    fn sample_table() -> Value {
        list(vec![
            make_row(&[("name", s("alice")), ("role", s("admin"))]),
            make_row(&[("name", s("bob")), ("role", s("user"))]),
            make_row(&[("name", s("charlie")), ("role", s("user"))]),
        ])
    }

    #[test]
    fn test_find_returns_matching_row() {
        let table = sample_table();
        let result = fn_find(&[table, s("name"), s("alice")]).unwrap();
        assert_eq!(
            result,
            make_row(&[("name", s("alice")), ("role", s("admin"))])
        );
    }

    #[test]
    fn test_find_returns_null_when_not_found() {
        let table = sample_table();
        let result = fn_find(&[table, s("name"), s("nobody")]).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn test_find_on_empty_list() {
        let result = fn_find(&[list(vec![]), s("name"), s("alice")]).unwrap();
        assert_eq!(result, Value::Null);
    }

    #[test]
    fn test_insert_row_appends() {
        let table = sample_table();
        let new_row = make_row(&[("name", s("dave")), ("role", s("admin"))]);
        let result = fn_insert_row(&[table, new_row.clone()]).unwrap();
        if let Value::List(rows) = result {
            assert_eq!(rows.len(), 4);
            assert_eq!(rows[3], new_row);
        } else {
            panic!("expected list");
        }
    }

    #[test]
    fn test_remove_rows_filters_matching() {
        let table = sample_table();
        let result = fn_remove_rows(&[table, s("role"), s("user")]).unwrap();
        if let Value::List(rows) = result {
            assert_eq!(rows.len(), 1);
            assert_eq!(
                rows[0],
                make_row(&[("name", s("alice")), ("role", s("admin"))])
            );
        } else {
            panic!("expected list");
        }
    }

    #[test]
    fn test_remove_rows_no_match_returns_same() {
        let table = sample_table();
        let result = fn_remove_rows(&[table.clone(), s("role"), s("superadmin")]).unwrap();
        assert_eq!(result, table);
    }

    #[test]
    fn test_update_rows_merges_updates() {
        let table = sample_table();
        let updates = make_row(&[("role", s("superadmin"))]);
        let result = fn_update_rows(&[table, s("name"), s("alice"), updates]).unwrap();
        if let Value::List(rows) = result {
            assert_eq!(
                rows[0],
                make_row(&[("name", s("alice")), ("role", s("superadmin"))])
            );
            // other rows unchanged
            assert_eq!(
                rows[1],
                make_row(&[("name", s("bob")), ("role", s("user"))])
            );
        } else {
            panic!("expected list");
        }
    }

    #[test]
    fn test_update_rows_no_match_returns_same() {
        let table = sample_table();
        let updates = make_row(&[("role", s("superadmin"))]);
        let result = fn_update_rows(&[table.clone(), s("name"), s("nobody"), updates]).unwrap();
        assert_eq!(result, table);
    }

    #[test]
    fn test_update_rows_preserves_unmatched() {
        let table = sample_table();
        let updates = make_row(&[("role", s("moderator"))]);
        let result = fn_update_rows(&[table, s("name"), s("bob"), updates]).unwrap();
        if let Value::List(rows) = result {
            assert_eq!(rows.len(), 3);
            // alice unchanged
            assert_eq!(
                rows[0],
                make_row(&[("name", s("alice")), ("role", s("admin"))])
            );
            // bob updated
            assert_eq!(
                rows[1],
                make_row(&[("name", s("bob")), ("role", s("moderator"))])
            );
            // charlie unchanged
            assert_eq!(
                rows[2],
                make_row(&[("name", s("charlie")), ("role", s("user"))])
            );
        } else {
            panic!("expected list");
        }
    }

    // --- Reference and Query Functions ---

    fn block_ref(
        kind: &str,
        id: Option<&str>,
        children: Vec<crate::eval::value::BlockRef>,
    ) -> crate::eval::value::BlockRef {
        crate::eval::value::BlockRef {
            kind: kind.to_string(),
            id: id.map(str::to_string),
            qualified_id: id.map(str::to_string),
            attributes: IndexMap::new(),
            attribute_decorators: IndexMap::new(),
            children,
            decorators: vec![],
            span: crate::lang::Span::dummy(),
        }
    }

    fn block_ids(value: Value) -> Vec<String> {
        match value {
            Value::List(items) => items
                .into_iter()
                .map(|item| match item {
                    Value::BlockRef(block) => block.id.unwrap_or_default(),
                    other => panic!("expected block ref, got {other:?}"),
                })
                .collect(),
            other => panic!("expected list, got {other:?}"),
        }
    }

    fn block_kinds(value: Value) -> Vec<String> {
        match value {
            Value::List(items) => items
                .into_iter()
                .map(|item| match item {
                    Value::BlockRef(block) => block.kind,
                    other => panic!("expected block ref, got {other:?}"),
                })
                .collect(),
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn test_block_introspection_functions() {
        let child = block_ref("item", Some("first"), vec![]);
        let mut block = block_ref("html::section", Some("intro"), vec![child.clone()]);
        block.attributes.insert("class".to_string(), s("hero"));
        let value = Value::BlockRef(block);

        assert_eq!(block_kind(&[value.clone()]).unwrap(), s("html::section"));
        assert_eq!(block_id(&[value.clone()]).unwrap(), s("intro"));
        let children = block_children(&[value.clone()]).unwrap();
        let Value::List(children) = children else {
            panic!("expected child list");
        };
        assert_eq!(children.len(), 1);
        assert!(
            matches!(&children[0], Value::BlockRef(block) if block.kind == child.kind && block.id == child.id)
        );
        let attrs = block_attrs(&[value]).unwrap();
        let Value::Map(attrs) = attrs else {
            panic!("expected attrs map");
        };
        assert_eq!(attrs.get("class"), Some(&s("hero")));
        assert_eq!(attrs.get("id"), Some(&s("intro")));
    }

    #[test]
    fn test_has_attribute_present() {
        let mut attrs = IndexMap::new();
        attrs.insert("port".to_string(), i(8080));
        attrs.insert("tls".to_string(), Value::Bool(true));
        let br = Value::BlockRef(crate::eval::value::BlockRef {
            kind: "service".to_string(),
            id: Some("svc-api".to_string()),
            qualified_id: None,
            attributes: attrs,
            attribute_decorators: IndexMap::new(),
            children: vec![],
            decorators: vec![],
            span: crate::lang::Span::dummy(),
        });
        assert_eq!(fn_has(&[br.clone(), s("port")]).unwrap(), Value::Bool(true));
        assert_eq!(fn_has(&[br.clone(), s("tls")]).unwrap(), Value::Bool(true));
        assert_eq!(fn_has(&[br, s("missing")]).unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_has_child_block() {
        let child = crate::eval::value::BlockRef {
            kind: "monitoring".to_string(),
            id: None,
            qualified_id: None,
            attributes: IndexMap::new(),
            attribute_decorators: IndexMap::new(),
            children: vec![],
            decorators: vec![],
            span: crate::lang::Span::dummy(),
        };
        let br = Value::BlockRef(crate::eval::value::BlockRef {
            kind: "service".to_string(),
            id: Some("svc-api".to_string()),
            qualified_id: None,
            attributes: IndexMap::new(),
            attribute_decorators: IndexMap::new(),
            children: vec![child],
            decorators: vec![],
            span: crate::lang::Span::dummy(),
        });
        assert_eq!(
            fn_has(&[br.clone(), s("monitoring")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(fn_has(&[br, s("logging")]).unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_has_wrong_arg_type() {
        assert!(fn_has(&[s("not a block"), s("attr")]).is_err());
        assert!(fn_has(&[i(42), s("attr")]).is_err());
    }

    #[test]
    fn test_children_returns_direct_child_blocks_in_order() {
        let first = block_ref("UiMenuItem", Some("first"), vec![]);
        let second = block_ref("UiDivider", Some("divider"), vec![]);
        let br = Value::BlockRef(block_ref(
            "UiMenu",
            Some("menu"),
            vec![first.clone(), second.clone()],
        ));

        let result = fn_children(&[br]).unwrap();
        assert_eq!(block_ids(result.clone()), vec!["first", "divider"]);
        assert_eq!(block_kinds(result), vec!["UiMenuItem", "UiDivider"]);
    }

    #[test]
    fn test_children_filters_by_exact_kind() {
        let first = block_ref("UiMenuItem", Some("first"), vec![]);
        let second = block_ref("UiDivider", Some("divider"), vec![]);
        let third = block_ref("UiMenuItem", Some("third"), vec![]);
        let br = Value::BlockRef(block_ref(
            "UiMenu",
            Some("menu"),
            vec![first.clone(), second, third.clone()],
        ));

        let result = fn_children(&[br, s("UiMenuItem")]).unwrap();
        assert_eq!(block_ids(result), vec!["first", "third"]);
    }

    #[test]
    fn test_children_returns_only_direct_children() {
        let grandchild = block_ref("UiMenuItem", Some("grandchild"), vec![]);
        let child = block_ref("UiMenuItem", Some("child"), vec![grandchild]);
        let br = Value::BlockRef(block_ref("UiMenu", Some("menu"), vec![child.clone()]));

        let result = fn_children(&[br]).unwrap();
        assert_eq!(block_ids(result), vec!["child"]);
    }

    #[test]
    fn test_children_rejects_wrong_arg_type_and_arity() {
        assert!(fn_children(&[]).is_err());
        assert!(fn_children(&[s("not a block")]).is_err());
        assert!(fn_children(&[Value::BlockRef(block_ref("UiMenu", None, vec![])), i(1)]).is_err());
        assert!(fn_children(&[
            Value::BlockRef(block_ref("UiMenu", None, vec![])),
            s("UiMenuItem"),
            s("extra"),
        ])
        .is_err());
    }

    #[test]
    fn test_has_decorator_present() {
        let br = Value::BlockRef(crate::eval::value::BlockRef {
            kind: "service".to_string(),
            id: Some("svc-api".to_string()),
            qualified_id: None,
            attributes: IndexMap::new(),
            attribute_decorators: IndexMap::new(),
            children: vec![],
            decorators: vec![
                crate::eval::value::DecoratorValue {
                    name: "deprecated".to_string(),
                    args: IndexMap::new(),
                },
                crate::eval::value::DecoratorValue {
                    name: "sensitive".to_string(),
                    args: IndexMap::new(),
                },
            ],
            span: crate::lang::Span::dummy(),
        });
        assert_eq!(
            fn_has_decorator(&[br.clone(), s("deprecated")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            fn_has_decorator(&[br.clone(), s("sensitive")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            fn_has_decorator(&[br, s("optional")]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_has_decorator_wrong_arg_type() {
        assert!(fn_has_decorator(&[s("not a block"), s("deco")]).is_err());
    }

    // --- BigInt support ---

    fn bi(v: i128) -> Value {
        Value::BigInt(v)
    }

    #[test]
    fn test_abs_bigint() {
        assert_eq!(abs(&[bi(-100)]).unwrap(), bi(100));
        assert_eq!(abs(&[bi(42)]).unwrap(), bi(42));
    }

    #[test]
    fn test_min_max_bigint() {
        assert_eq!(fn_min(&[bi(10), bi(20)]).unwrap(), bi(10));
        assert_eq!(fn_max(&[bi(10), bi(20)]).unwrap(), bi(20));
        // Mixed Int/BigInt promotion
        assert_eq!(fn_min(&[i(5), bi(3)]).unwrap(), bi(3));
        assert_eq!(fn_max(&[i(5), bi(3)]).unwrap(), bi(5));
        assert_eq!(fn_min(&[bi(5), i(3)]).unwrap(), bi(3));
        assert_eq!(fn_max(&[bi(5), i(3)]).unwrap(), bi(5));
    }

    #[test]
    fn test_sum_bigint() {
        assert_eq!(
            sum(&[list(vec![bi(100), bi(200), bi(300)])]).unwrap(),
            bi(600)
        );
        // Mixed Int and BigInt
        assert_eq!(sum(&[list(vec![i(10), bi(20)])]).unwrap(), bi(30));
    }

    #[test]
    fn test_avg_bigint() {
        // BigInt values get coerced to f64 for avg
        assert_eq!(avg(&[list(vec![bi(10), bi(20)])]).unwrap(), f(15.0));
    }

    #[test]
    fn test_to_int_bigint() {
        assert_eq!(to_int(&[bi(42)]).unwrap(), i(42));
        // Overflow
        assert!(to_int(&[bi(i64::MAX as i128 + 1)]).is_err());
    }

    #[test]
    fn test_to_float_bigint() {
        assert_eq!(to_float(&[bi(42)]).unwrap(), f(42.0));
    }

    #[test]
    fn test_coerce_to_float_bigint() {
        // Tested indirectly through sqrt/pow
        assert_eq!(sqrt(&[bi(9)]).unwrap(), f(3.0));
    }

    // --- Date/Duration constructors ---

    #[test]
    fn test_date_valid() {
        assert_eq!(
            fn_date(&[s("2024-03-15")]).unwrap(),
            Value::Date("2024-03-15".to_string())
        );
        assert_eq!(
            fn_date(&[s("2000-02-29")]).unwrap(),
            Value::Date("2000-02-29".to_string())
        );
    }

    #[test]
    fn test_date_invalid() {
        assert!(fn_date(&[s("not-a-date")]).is_err());
        assert!(fn_date(&[s("2024-13-01")]).is_err()); // invalid month
        assert!(fn_date(&[s("2024-02-30")]).is_err()); // invalid day
        assert!(fn_date(&[s("2023-02-29")]).is_err()); // not a leap year
        assert!(fn_date(&[s("0000-01-01")]).is_err()); // year 0
        assert!(fn_date(&[i(42)]).is_err()); // wrong type
    }

    #[test]
    fn test_duration_valid() {
        assert_eq!(
            fn_duration(&[s("P1Y")]).unwrap(),
            Value::Duration("P1Y".to_string())
        );
        assert_eq!(
            fn_duration(&[s("P1Y2M3D")]).unwrap(),
            Value::Duration("P1Y2M3D".to_string())
        );
        assert_eq!(
            fn_duration(&[s("PT1H30M")]).unwrap(),
            Value::Duration("PT1H30M".to_string())
        );
        assert_eq!(
            fn_duration(&[s("P1Y2M3DT4H5M6S")]).unwrap(),
            Value::Duration("P1Y2M3DT4H5M6S".to_string())
        );
        assert_eq!(
            fn_duration(&[s("PT0.5S")]).unwrap(),
            Value::Duration("PT0.5S".to_string())
        );
    }

    #[test]
    fn test_duration_invalid() {
        assert!(fn_duration(&[s("not-a-duration")]).is_err());
        assert!(fn_duration(&[s("P")]).is_err()); // empty
        assert!(fn_duration(&[s("PT")]).is_err()); // T but no components
        assert!(fn_duration(&[s("1Y")]).is_err()); // missing P
        assert!(fn_duration(&[i(42)]).is_err()); // wrong type
    }

    #[test]
    fn test_type_of_new_variants() {
        assert_eq!(type_of(&[bi(42)]).unwrap(), s("bigint"));
        assert_eq!(
            type_of(&[Value::Date("2024-01-01".into())]).unwrap(),
            s("date")
        );
        assert_eq!(
            type_of(&[Value::Duration("P1D".into())]).unwrap(),
            s("duration")
        );
    }
}
