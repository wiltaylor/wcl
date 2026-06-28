//! Numeric literal parsing and range checking.
//!
//! The lexer calls into this module once it has carved out the textual
//! parts of a literal (sign, base prefix, digit body, optional fractional
//! part, optional exponent, optional suffix). All range-checking and
//! base conversion happens here.
//!
//! This module also owns the canonical list of numeric `Value` variants via
//! the [`for_each_numeric_variant!`] macro. Other modules that need to walk
//! the numeric variants (host-binding traits, arithmetic, comparisons) use
//! that macro rather than re-enumerating the list.

use crate::lexer::NumberLit;

/// Invoke a callback macro once per numeric `Value` variant, passing the
/// Rust scalar type and the variant ident: `cb!(i8, I8); cb!(u32, U32); ...`.
macro_rules! for_each_numeric_variant {
    ($mac:ident) => {
        $mac!(i8, I8);
        $mac!(i16, I16);
        $mac!(i32, I32);
        $mac!(i64, I64);
        $mac!(i128, I128);
        $mac!(isize, Isize);
        $mac!(u8, U8);
        $mac!(u16, U16);
        $mac!(u32, U32);
        $mac!(u64, U64);
        $mac!(u128, U128);
        $mac!(usize, Usize);
        $mac!(f32, F32);
        $mac!(f64, F64);
    };
}

/// Like [`for_each_numeric_variant!`] but only signed integers and floats.
/// Used by unary negation, where unsigned types are rejected.
macro_rules! for_each_signed_numeric_variant {
    ($mac:ident) => {
        $mac!(i8, I8);
        $mac!(i16, I16);
        $mac!(i32, I32);
        $mac!(i64, I64);
        $mac!(i128, I128);
        $mac!(isize, Isize);
        $mac!(f32, F32);
        $mac!(f64, F64);
    };
}

/// Convert a numeric enum value (`&NumberLit` or `&Value`) to `u64`,
/// returning `None` for floats, negative signed values, and magnitudes that
/// don't fit. The two numeric enums share the same variant names, so this
/// single body serves both `NumberLit::as_u64` and `Value::as_u64`.
macro_rules! numeric_as_u64 {
    ($val:expr, $ty:ident) => {
        match $val {
            $ty::I8(v) if *v >= 0 => Some(*v as u64),
            $ty::I16(v) if *v >= 0 => Some(*v as u64),
            $ty::I32(v) if *v >= 0 => Some(*v as u64),
            $ty::I64(v) if *v >= 0 => Some(*v as u64),
            $ty::I128(v) if *v >= 0 => u64::try_from(*v).ok(),
            $ty::Isize(v) if *v >= 0 => Some(*v as u64),
            $ty::U8(v) => Some(*v as u64),
            $ty::U16(v) => Some(*v as u64),
            $ty::U32(v) => Some(*v as u64),
            $ty::U64(v) => Some(*v),
            $ty::U128(v) => u64::try_from(*v).ok(),
            $ty::Usize(v) => u64::try_from(*v).ok(),
            _ => None,
        }
    };
}

/// Stringify a numeric enum value (`&NumberLit` or `&Value`) to a bare,
/// suffix-free decimal segment for path addressing: an **integer** renders
/// its digits (`U32(1)` → `"1"`, `I32(-5)` → `"-5"`), a **float** is `None`
/// (fragile / meaningless as a label). The two numeric enums share variant
/// names, so this single body serves both the reify side
/// (`Value::as_path_segment`) and the parser (`NumberLit` after `.`), keeping
/// the segment they produce byte-identical. Deliberately suffix-free — unlike
/// the `Display` impl, which keeps `u32`/`i8`/… so dumps round-trip.
macro_rules! numeric_as_path_segment {
    ($val:expr, $ty:ident) => {
        match $val {
            $ty::I8(v) => Some(v.to_string()),
            $ty::I16(v) => Some(v.to_string()),
            $ty::I32(v) => Some(v.to_string()),
            $ty::I64(v) => Some(v.to_string()),
            $ty::I128(v) => Some(v.to_string()),
            $ty::Isize(v) => Some(v.to_string()),
            $ty::U8(v) => Some(v.to_string()),
            $ty::U16(v) => Some(v.to_string()),
            $ty::U32(v) => Some(v.to_string()),
            $ty::U64(v) => Some(v.to_string()),
            $ty::U128(v) => Some(v.to_string()),
            $ty::Usize(v) => Some(v.to_string()),
            _ => None,
        }
    };
}

pub(crate) use for_each_numeric_variant;
pub(crate) use for_each_signed_numeric_variant;
pub(crate) use numeric_as_path_segment;
pub(crate) use numeric_as_u64;

#[derive(Debug, Clone, PartialEq)]
pub struct NumericParseError {
    pub message: String,
}

impl NumericParseError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

/// Output of pre-suffix tokenisation: what the lexer collected before we
/// decide a concrete type.
#[derive(Debug)]
pub struct ParsedNumber<'a> {
    pub neg: bool,
    pub base: u32,
    /// Digits with `_` separators stripped, plus the optional fractional
    /// part for floats (e.g. "12.5").
    pub body: &'a str,
    /// Optional `eN` / `e+N` / `e-N` exponent, digits only with sign.
    pub exponent: Option<&'a str>,
    pub is_float: bool,
    /// Suffix text after the body (e.g. `"u8"`, `"f32"`, or empty).
    pub suffix: &'a str,
}

/// Outcome of [`finalize`]: the typed magnitude plus an optional unit.
///
/// A recognised numeric type suffix (`u8`, `i64`, `f32`, …) yields
/// `unit == None`. Any other non-empty suffix is a **literal unit**
/// (`5MiB`, `3km`): the magnitude defaults to `i64`/`f64` and the suffix
/// rides along as `unit` for type-directed resolution later. Units are
/// user- and stdlib-defined, so the lexer carries the string verbatim
/// with no validation here.
#[derive(Debug, Clone, PartialEq)]
pub struct FinalizedNumber {
    pub lit: NumberLit,
    pub unit: Option<String>,
}

/// Resolve the typed [`NumberLit`] for a tokenised number, applying
/// suffix-driven range checks. An unrecognised suffix is treated as a
/// literal-unit name rather than an error (see [`FinalizedNumber`]).
pub fn finalize(parsed: ParsedNumber<'_>) -> Result<FinalizedNumber, NumericParseError> {
    if parsed.is_float {
        finalize_float(&parsed)
    } else {
        finalize_int(&parsed)
    }
}

fn finalize_float(p: &ParsedNumber) -> Result<FinalizedNumber, NumericParseError> {
    if p.base != 10 {
        return Err(NumericParseError::new(
            "float literals must be decimal (no 0x/0b/0o prefix)",
        ));
    }
    let mut text = String::with_capacity(p.body.len() + p.exponent.map_or(0, str::len) + 2);
    if p.neg {
        text.push('-');
    }
    text.push_str(p.body);
    if let Some(exp) = p.exponent {
        text.push('e');
        text.push_str(exp);
    }
    let f = text
        .parse::<f64>()
        .map_err(|e| NumericParseError::new(format!("invalid float literal: {e}")))?;
    let lit = match p.suffix {
        "" | "f64" => NumberLit::F64(f),
        "f32" => {
            let narrowed = f as f32;
            if !narrowed.is_finite() && f.is_finite() {
                return Err(NumericParseError::new("literal overflows f32"));
            }
            NumberLit::F32(narrowed)
        }
        other if is_int_suffix(other) => {
            return Err(NumericParseError::new(format!(
                "integer suffix '{other}' cannot be applied to a float literal"
            )));
        }
        // Any other suffix is a literal unit; the magnitude defaults to f64.
        other => {
            return Ok(FinalizedNumber {
                lit: NumberLit::F64(f),
                unit: Some(other.to_string()),
            });
        }
    };
    Ok(FinalizedNumber { lit, unit: None })
}

fn finalize_int(p: &ParsedNumber) -> Result<FinalizedNumber, NumericParseError> {
    // Parse magnitude as u128. The body has `_` already stripped.
    let mag = u128_from_str_radix(p.body, p.base)
        .map_err(|e| NumericParseError::new(format!("invalid integer literal: {e}")))?;
    if matches!(p.suffix, "f32" | "f64") {
        return Err(NumericParseError::new(format!(
            "float suffix '{}' cannot be applied to an integer literal",
            p.suffix
        )));
    }
    if p.neg && is_unsigned_suffix(p.suffix) {
        return Err(NumericParseError::new(format!(
            "negative value cannot have an unsigned suffix '{}'",
            p.suffix
        )));
    }
    let lit = match p.suffix {
        "" | "i64" => signed::<i64>(p.neg, mag, "i64", i64::MIN as i128, i64::MAX as i128)
            .map(|n| NumberLit::I64(n as i64))?,
        "i8" => signed::<i8>(p.neg, mag, "i8", i8::MIN as i128, i8::MAX as i128)
            .map(|n| NumberLit::I8(n as i8))?,
        "i16" => signed::<i16>(p.neg, mag, "i16", i16::MIN as i128, i16::MAX as i128)
            .map(|n| NumberLit::I16(n as i16))?,
        "i32" => signed::<i32>(p.neg, mag, "i32", i32::MIN as i128, i32::MAX as i128)
            .map(|n| NumberLit::I32(n as i32))?,
        "i128" => signed_128(p.neg, mag).map(NumberLit::I128)?,
        "isize" => signed::<isize>(p.neg, mag, "isize", isize::MIN as i128, isize::MAX as i128)
            .map(|n| NumberLit::Isize(n as isize))?,
        "u8" => unsigned(mag, "u8", u8::MAX as u128).map(|n| NumberLit::U8(n as u8))?,
        "u16" => unsigned(mag, "u16", u16::MAX as u128).map(|n| NumberLit::U16(n as u16))?,
        "u32" => unsigned(mag, "u32", u32::MAX as u128).map(|n| NumberLit::U32(n as u32))?,
        "u64" => unsigned(mag, "u64", u64::MAX as u128).map(|n| NumberLit::U64(n as u64))?,
        "u128" => NumberLit::U128(mag),
        "usize" => {
            unsigned(mag, "usize", usize::MAX as u128).map(|n| NumberLit::Usize(n as usize))?
        }
        // Any other suffix is a literal unit; the magnitude defaults to i64.
        other => {
            let n = signed::<i64>(p.neg, mag, "i64", i64::MIN as i128, i64::MAX as i128)?;
            return Ok(FinalizedNumber {
                lit: NumberLit::I64(n as i64),
                unit: Some(other.to_string()),
            });
        }
    };
    Ok(FinalizedNumber { lit, unit: None })
}

fn signed<T>(
    neg: bool,
    mag: u128,
    ty: &str,
    min: i128,
    max: i128,
) -> Result<i128, NumericParseError> {
    // `T` is here just to make the call site self-documenting; we work in i128.
    let _ = std::marker::PhantomData::<T>;
    if neg {
        // |min| = -min, but min is INT_MIN where -min overflows i128 only when
        // T is i128 (handled separately). For i64 and smaller, this is safe.
        let neg_max = (-min) as u128;
        if mag > neg_max {
            return Err(NumericParseError::new(format!(
                "literal out of range for {ty}"
            )));
        }
        if mag == neg_max {
            return Ok(min);
        }
        Ok(-(mag as i128))
    } else {
        if mag > max as u128 {
            return Err(NumericParseError::new(format!(
                "literal out of range for {ty}"
            )));
        }
        Ok(mag as i128)
    }
}

fn signed_128(neg: bool, mag: u128) -> Result<i128, NumericParseError> {
    if neg {
        // |i128::MIN| == 2^127; that's representable in u128.
        let neg_max = (i128::MAX as u128) + 1;
        if mag > neg_max {
            return Err(NumericParseError::new(
                "literal out of range for i128".to_string(),
            ));
        }
        if mag == neg_max {
            return Ok(i128::MIN);
        }
        Ok(-(mag as i128))
    } else {
        if mag > i128::MAX as u128 {
            return Err(NumericParseError::new(
                "literal out of range for i128".to_string(),
            ));
        }
        Ok(mag as i128)
    }
}

fn unsigned(mag: u128, ty: &str, max: u128) -> Result<u128, NumericParseError> {
    if mag > max {
        return Err(NumericParseError::new(format!(
            "literal out of range for {ty}"
        )));
    }
    Ok(mag)
}

fn u128_from_str_radix(s: &str, radix: u32) -> Result<u128, String> {
    if s.is_empty() {
        return Err("expected at least one digit".into());
    }
    let mut out: u128 = 0;
    for c in s.chars() {
        let digit = c
            .to_digit(radix)
            .ok_or_else(|| format!("invalid digit '{c}' for base {radix}"))?;
        out = out
            .checked_mul(radix as u128)
            .ok_or_else(|| "literal exceeds 128-bit magnitude".to_string())?;
        out = out
            .checked_add(digit as u128)
            .ok_or_else(|| "literal exceeds 128-bit magnitude".to_string())?;
    }
    Ok(out)
}

fn is_unsigned_suffix(s: &str) -> bool {
    matches!(s, "u8" | "u16" | "u32" | "u64" | "u128" | "usize")
}

fn is_int_suffix(s: &str) -> bool {
    matches!(
        s,
        "" | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(
        body: &str,
        suffix: &str,
        base: u32,
        neg: bool,
    ) -> Result<NumberLit, NumericParseError> {
        finalize(ParsedNumber {
            neg,
            base,
            body,
            exponent: None,
            is_float: false,
            suffix,
        })
        .map(|f| f.lit)
    }

    fn parse_float(
        body: &str,
        suffix: &str,
        exponent: Option<&str>,
        neg: bool,
    ) -> Result<NumberLit, NumericParseError> {
        finalize(ParsedNumber {
            neg,
            base: 10,
            body,
            exponent,
            is_float: true,
            suffix,
        })
        .map(|f| f.lit)
    }

    #[test]
    fn default_int_is_i64() {
        assert_eq!(parse("42", "", 10, false).unwrap(), NumberLit::I64(42));
    }

    #[test]
    fn path_segment_agrees_across_numberlit_and_value() {
        use crate::value::Value;
        // The reify side (Value) and the parser side (NumberLit) must produce
        // byte-identical segments for the same integer, across widths/signs,
        // and both must reject floats — else a `from = x.1` path can't match
        // a numeric `@inline(0)` label.
        macro_rules! both {
            ($n:literal, $V:ident, $N:ident) => {{
                let v = numeric_as_path_segment!(&Value::$V($n), Value);
                let n = numeric_as_path_segment!(&NumberLit::$N($n), NumberLit);
                assert_eq!(v, n);
                v
            }};
        }
        assert_eq!(both!(1, U32, U32).as_deref(), Some("1"));
        assert_eq!(both!(255, U8, U8).as_deref(), Some("255"));
        assert_eq!(both!(-5, I32, I32).as_deref(), Some("-5"));
        assert_eq!(both!(42, I64, I64).as_deref(), Some("42"));
        // Floats are unaddressable on both sides.
        assert_eq!(
            numeric_as_path_segment!(&Value::F64(1.5), Value),
            None::<String>
        );
        assert_eq!(
            numeric_as_path_segment!(&NumberLit::F64(1.5), NumberLit),
            None::<String>
        );
        // String-likes address as their bare text (no quotes / no `:`).
        assert_eq!(
            Value::Symbol("foo".into()).as_path_segment().as_deref(),
            Some("foo")
        );
        assert_eq!(
            Value::Utf8("a".into()).as_path_segment().as_deref(),
            Some("a")
        );
        assert_eq!(Value::Bool(true).as_path_segment(), None);
    }

    #[test]
    fn default_float_is_f64() {
        assert_eq!(
            parse_float("1.25", "", None, false).unwrap(),
            NumberLit::F64(1.25)
        );
    }

    #[test]
    fn signed_min_max_boundaries() {
        assert_eq!(parse("127", "i8", 10, false).unwrap(), NumberLit::I8(127));
        assert_eq!(parse("128", "i8", 10, true).unwrap(), NumberLit::I8(-128));
        assert!(parse("128", "i8", 10, false).is_err());
        assert!(parse("129", "i8", 10, true).is_err());
    }

    #[test]
    fn unsigned_max_and_overflow() {
        assert_eq!(parse("255", "u8", 10, false).unwrap(), NumberLit::U8(255));
        assert!(parse("256", "u8", 10, false).is_err());
    }

    #[test]
    fn negative_unsigned_errors() {
        let err = parse("1", "u32", 10, true).unwrap_err();
        assert!(err.message.contains("unsigned"));
    }

    #[test]
    fn float_suffix_on_int_errors() {
        let err = parse("42", "f32", 10, false).unwrap_err();
        assert!(err.message.contains("float suffix"));
    }

    #[test]
    fn int_suffix_on_float_errors() {
        let err = parse_float("1.5", "u8", None, false).unwrap_err();
        assert!(err.message.contains("integer suffix"));
    }

    #[test]
    fn unknown_suffix_becomes_unit() {
        // An unrecognised suffix is now a literal unit, not an error: the
        // magnitude defaults to i64 and the suffix rides along as `unit`.
        let fin = finalize(ParsedNumber {
            neg: false,
            base: 10,
            body: "5",
            exponent: None,
            is_float: false,
            suffix: "MiB",
        })
        .unwrap();
        assert_eq!(fin.lit, NumberLit::I64(5));
        assert_eq!(fin.unit.as_deref(), Some("MiB"));
    }

    #[test]
    fn float_unit_defaults_to_f64() {
        let fin = finalize(ParsedNumber {
            neg: false,
            base: 10,
            body: "1.5",
            exponent: None,
            is_float: true,
            suffix: "km",
        })
        .unwrap();
        assert_eq!(fin.lit, NumberLit::F64(1.5));
        assert_eq!(fin.unit.as_deref(), Some("km"));
    }

    #[test]
    fn i128_min_max() {
        assert_eq!(
            parse(&i128::MAX.to_string(), "i128", 10, false).unwrap(),
            NumberLit::I128(i128::MAX)
        );
        // |i128::MIN| is 2^127; written as the absolute value plus sign.
        let abs_min = "170141183460469231731687303715884105728";
        assert_eq!(
            parse(abs_min, "i128", 10, true).unwrap(),
            NumberLit::I128(i128::MIN)
        );
        assert!(parse(abs_min, "i128", 10, false).is_err());
    }

    #[test]
    fn u128_max() {
        let max = "340282366920938463463374607431768211455";
        assert_eq!(
            parse(max, "u128", 10, false).unwrap(),
            NumberLit::U128(u128::MAX)
        );
        assert!(parse("340282366920938463463374607431768211456", "u128", 10, false).is_err());
    }

    #[test]
    fn hex_bin_oct() {
        assert_eq!(parse("ff", "u8", 16, false).unwrap(), NumberLit::U8(255));
        assert_eq!(
            parse("10101100", "u8", 2, false).unwrap(),
            NumberLit::U8(0b1010_1100)
        );
        assert_eq!(
            parse("755", "u16", 8, false).unwrap(),
            NumberLit::U16(0o755)
        );
    }

    #[test]
    fn invalid_digit_for_base() {
        let err = parse("2", "u8", 2, false).unwrap_err();
        assert!(err.message.contains("invalid digit"));
    }

    #[test]
    fn float_with_exponent() {
        assert_eq!(
            parse_float("1.5", "", Some("3"), false).unwrap(),
            NumberLit::F64(1500.0)
        );
        assert_eq!(
            parse_float("2", "", Some("-3"), false).unwrap(),
            NumberLit::F64(0.002)
        );
    }

    #[test]
    fn float_overflowing_f32_errors() {
        let err = parse_float("1", "f32", Some("40"), false).unwrap_err();
        assert!(err.message.contains("overflows f32"));
    }

    #[test]
    fn float_non_decimal_base_errors() {
        let err = finalize(ParsedNumber {
            neg: false,
            base: 16,
            body: "ff",
            exponent: None,
            is_float: true,
            suffix: "",
        })
        .unwrap_err();
        assert!(err.message.contains("decimal"));
    }
}
