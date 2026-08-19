//! String builtins: splitting and joining, case and whitespace,
//! prefix/suffix tests, padding, and the `format` template.
//!
//! [`format_value`] also backs string interpolation in the evaluator,
//! so the two render a value identically.

use super::builtin::{BuiltinFn, Caller, from_fn};
use crate::environment::Environment;
use crate::value::Value;

/// Register every string builtin into `env`.
pub(super) fn register(env: &mut Environment) {
    env.add_builtin(
        "concat",
        from_fn(|a: String, b: String| -> String { format!("{a}{b}") })
            .doc("Concatenate two strings into one.")
            .param("a", "utf8", "The left-hand string.")
            .param("b", "utf8", "The string appended after `a`.")
            .returns("utf8", "The two strings joined together."),
    );
    env.add_builtin(
        "format",
        BuiltinFn::hof(0, format_hof)
            // Variadic: keep an explicit signature since `...args` isn't a
            // structurally-expressible parameter.
            .with_signature("fn (utf8, ...args) -> utf8")
            .doc("Substitute trailing arguments into a template's `{}` placeholders (`{{`/`}}` are literal braces).")
            .param("template", "utf8", "Template string with `{}` placeholders.")
            .returns("utf8", "The template with placeholders substituted."),
    );
    env.add_builtin(
        "split",
        from_fn(|s: String, sep: String| -> Value {
            Value::list(s.split(&sep).map(|p| Value::Utf8(p.to_string())).collect())
        })
        .doc("Split a string on every occurrence of a separator into a list of pieces.")
        .param("s", "utf8", "The string to split.")
        .param("sep", "utf8", "The separator to split on.")
        .returns("[utf8]", "The pieces between separators."),
    );
    env.add_builtin(
        "join",
        from_fn(|parts: Vec<String>, sep: String| -> String { parts.join(&sep) })
            .doc("Join a list of strings into one, inserting a separator between each.")
            .param("parts", "[utf8]", "The strings to join.")
            .param("sep", "utf8", "The separator inserted between parts.")
            .returns("utf8", "The joined string."),
    );
    env.add_builtin(
        "replace",
        from_fn(|s: String, old: String, new: String| -> String { s.replace(&old, &new) })
            .doc("Replace every occurrence of a substring with another.")
            .param("s", "utf8", "The string to search.")
            .param("old", "utf8", "The substring to find.")
            .param("new", "utf8", "The replacement substring.")
            .returns("utf8", "The string with every match replaced."),
    );
    env.add_builtin(
        "contains",
        from_fn(|s: String, needle: String| -> bool { s.contains(&needle) })
            .doc("Whether a string contains a substring.")
            .param("s", "utf8", "The string to search.")
            .param("needle", "utf8", "The substring to look for.")
            .returns("bool", "`true` if the substring is present."),
    );
    env.add_builtin(
        "starts_with",
        from_fn(|s: String, prefix: String| -> bool { s.starts_with(&prefix) })
            .doc("Whether a string begins with a prefix.")
            .param("s", "utf8", "The string to test.")
            .param("prefix", "utf8", "The prefix to look for.")
            .returns("bool", "`true` if the string starts with the prefix."),
    );
    env.add_builtin(
        "ends_with",
        from_fn(|s: String, suffix: String| -> bool { s.ends_with(&suffix) })
            .doc("Whether a string ends with a suffix.")
            .param("s", "utf8", "The string to test.")
            .param("suffix", "utf8", "The suffix to look for.")
            .returns("bool", "`true` if the string ends with the suffix."),
    );
    env.add_builtin(
        "to_upper",
        from_fn(|s: String| -> String { s.to_uppercase() })
            .doc("Uppercase every character of a string.")
            .param("s", "utf8", "The string to uppercase.")
            .returns("utf8", "The uppercased string."),
    );
    env.add_builtin(
        "to_lower",
        from_fn(|s: String| -> String { s.to_lowercase() })
            .doc("Lowercase every character of a string.")
            .param("s", "utf8", "The string to lowercase.")
            .returns("utf8", "The lowercased string."),
    );
    env.add_builtin(
        "trim",
        from_fn(|s: String| -> String { s.trim().to_string() })
            .doc("Remove leading and trailing whitespace from a string.")
            .param("s", "utf8", "The string to trim.")
            .returns("utf8", "The string without leading/trailing whitespace."),
    );
    env.add_builtin(
        "chars",
        from_fn(|s: String| -> Value {
            Value::list(s.chars().map(|c| Value::Utf8(c.to_string())).collect())
        })
        .doc("The characters of a string as a list of one-character strings.")
        .param("s", "utf8", "The string to split into characters.")
        .returns("[utf8]", "One string per character."),
    );
    env.add_builtin(
        "repeat",
        from_fn(|s: String, n: i64| -> String { s.repeat(n.max(0) as usize) })
            .doc("A string repeated `n` times (empty for `n <= 0`).")
            .param("s", "utf8", "The string to repeat.")
            .param("n", "i64", "How many copies to concatenate.")
            .returns("utf8", "`n` copies of `s`."),
    );
    env.add_builtin(
        "pad_start",
        from_fn(
            |s: String, width: i64, pad: String| -> Result<Value, String> {
                Ok(Value::Utf8(pad_string(s, width, &pad, true)?))
            },
        )
        .doc("Left-pad a string with a fill pattern until it is `width` characters long.")
        .param("s", "utf8", "The string to pad.")
        .param("width", "i64", "The target character count.")
        .param(
            "pad",
            "utf8",
            "The fill pattern (repeated / truncated as needed).",
        )
        .returns(
            "utf8",
            "The padded string (unchanged if already wide enough).",
        ),
    );
    env.add_builtin(
        "pad_end",
        from_fn(
            |s: String, width: i64, pad: String| -> Result<Value, String> {
                Ok(Value::Utf8(pad_string(s, width, &pad, false)?))
            },
        )
        .doc("Right-pad a string with a fill pattern until it is `width` characters long.")
        .param("s", "utf8", "The string to pad.")
        .param("width", "i64", "The target character count.")
        .param(
            "pad",
            "utf8",
            "The fill pattern (repeated / truncated as needed).",
        )
        .returns(
            "utf8",
            "The padded string (unchanged if already wide enough).",
        ),
    );
}

/// Shared implementation of `pad_start` and `pad_end`: repeat `pad`
/// until `s` reaches `width`, on whichever side `at_start` selects.
fn pad_string(s: String, width: i64, pad: &str, at_start: bool) -> Result<String, String> {
    if pad.is_empty() {
        return Err("pad_start/pad_end: pad pattern must not be empty".to_string());
    }
    let want = width.max(0) as usize;
    let have = s.chars().count();
    if have >= want {
        return Ok(s);
    }
    let fill: String = pad.chars().cycle().take(want - have).collect();
    Ok(if at_start {
        format!("{fill}{s}")
    } else {
        format!("{s}{fill}")
    })
}

/// `format(template, ...args)` — `{}` positional substitution.
fn format_hof(_caller: &mut dyn Caller, args: &[Value]) -> Result<Value, String> {
    let Some((template_v, rest)) = args.split_first() else {
        return Err("format: missing template argument".into());
    };
    let template = match template_v {
        Value::Utf8(s) | Value::Ascii(s) => s.clone(),
        other => {
            return Err(format!(
                "format: template must be a string, got {}",
                other.type_name()
            ));
        }
    };
    let mut out = String::with_capacity(template.len());
    let mut idx = 0usize;
    let mut chars = template.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '{' {
            if chars.peek() == Some(&'{') {
                chars.next();
                out.push('{');
                continue;
            }
            // Expect `}`.
            if chars.next() != Some('}') {
                return Err("format: expected '{}' placeholder".into());
            }
            let Some(arg) = rest.get(idx) else {
                return Err(format!(
                    "format: not enough arguments for placeholder #{idx}"
                ));
            };
            idx += 1;
            out.push_str(&format_value(arg));
        } else if c == '}' {
            if chars.peek() == Some(&'}') {
                chars.next();
                out.push('}');
                continue;
            }
            return Err("format: unmatched '}' in template".into());
        } else {
            out.push(c);
        }
    }
    if idx < rest.len() {
        return Err(format!(
            "format: {} extra arguments after template",
            rest.len() - idx
        ));
    }
    Ok(Value::Utf8(out))
}

/// Render a `Value` for inclusion in a `format` substitution. Stays
/// compact and predictable — host CLIs render richer forms.
pub(crate) fn format_value(v: &Value) -> String {
    match v {
        Value::Utf8(s) | Value::Ascii(s) | Value::Identifier(s) => s.clone(),
        Value::Symbol(s) => format!(":{s}"),
        Value::Bool(b) => b.to_string(),
        Value::None => "none".to_string(),
        Value::I8(n) => n.to_string(),
        Value::I16(n) => n.to_string(),
        Value::I32(n) => n.to_string(),
        Value::I64(n) => n.to_string(),
        Value::I128(n) => n.to_string(),
        Value::Isize(n) => n.to_string(),
        Value::U8(n) => n.to_string(),
        Value::U16(n) => n.to_string(),
        Value::U32(n) => n.to_string(),
        Value::U64(n) => n.to_string(),
        Value::U128(n) => n.to_string(),
        Value::Usize(n) => n.to_string(),
        Value::F32(n) => n.to_string(),
        Value::F64(n) => n.to_string(),
        Value::Utf16(units) => String::from_utf16_lossy(units),
        Value::Utf32(chars) => chars.iter().collect(),
        Value::PendingUnit { magnitude, unit } => format!("{}{unit}", format_value(magnitude)),
        Value::List(items) => {
            let parts: Vec<String> = items.iter().map(format_value).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Tensor { shape, data } => {
            let dims: Vec<String> = shape.iter().map(u64::to_string).collect();
            let elems: Vec<String> = data.iter().map(format_value).collect();
            format!("tensor[{}]({})", dims.join("x"), elems.join(", "))
        }
        Value::Variant {
            union,
            variant,
            payload,
        } => {
            use crate::value::VariantPayload;
            let path = format!("{}::{}", union.join("."), variant);
            match payload {
                VariantPayload::Unit => path,
                VariantPayload::Positional(v) => format!("{path}({})", format_value(v)),
                VariantPayload::Record(map) => {
                    let parts: Vec<String> = map
                        .iter()
                        .map(|(k, v)| format!("{k}: {}", format_value(v)))
                        .collect();
                    format!("{path} {{ {} }}", parts.join(", "))
                }
            }
        }
        Value::Function(_) => "<fn>".to_string(),
        Value::Record { ty, fields } => {
            let parts: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{k}: {}", format_value(v)))
                .collect();
            format!("{} {{ {} }}", ty.join("."), parts.join(", "))
        }
        Value::DataPath { kind, segments } => {
            format!("&{}<{kind}>", segments.join("."))
        }
    }
}
