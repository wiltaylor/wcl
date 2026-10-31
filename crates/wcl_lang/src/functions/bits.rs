//! Bit-field builtins: `bits`, which reads a run of bits out of an
//! integer.
//!
//! Unlike the [`math`](super::math) family nothing here widens to
//! `f64`. The value keeps its own integer type, because a field read
//! out of a `u8` register is still a `u8`, and the width of that type
//! is what bounds the indices. Registered in
//! [`Environment::new`](crate::Environment::new).

use crate::environment::Environment;
use crate::functions::from_fn;
use crate::numeric::for_each_integer_numeric_variant;
use crate::value::Value;

/// Read a bit index argument. Any integer type is accepted; anything
/// else, or a negative value, is an error naming the parameter. A
/// `u128` index is kept whole so one above `i128::MAX` still reports
/// as out of range rather than as a type error.
fn index(v: &Value, name: &str) -> Result<u128, String> {
    if !is_integer(v) {
        return Err(not_integer(name, v));
    }
    match (v, v.as_i128()) {
        (Value::U128(n), _) => Ok(*n),
        (_, Some(n)) if n < 0 => Err(format!("bits: {name} must not be negative, got {n}")),
        (_, Some(n)) => Ok(n as u128),
        (_, None) => Err(not_integer(name, v)),
    }
}

fn is_integer(v: &Value) -> bool {
    v.is_numeric() && !matches!(v, Value::F32(_) | Value::F64(_))
}

fn not_integer(name: &str, v: &Value) -> String {
    format!("bits: {name} must be an integer, got {}", v.type_name())
}

/// Bits `hi` down to `lo` of `v`, inclusive, shifted down so bit `lo`
/// lands on bit 0. The result has `v`'s type.
///
/// A signed `v` is read as its two's-complement pattern and the field
/// comes back zero-extended: a field is unsigned data, so
/// `bits(-1i8, 3, 0)` is `15i8`. A full-width slice of a signed value
/// therefore returns the value unchanged.
fn bits(v: &Value, hi: &Value, lo: &Value) -> Result<Value, String> {
    if !is_integer(v) {
        return Err(not_integer("v", v));
    }
    let hi = index(hi, "hi")?;
    let lo = index(lo, "lo")?;
    if hi < lo {
        return Err(format!("bits: hi ({hi}) must not be less than lo ({lo})"));
    }
    macro_rules! int_arm {
        ($t:ty, $variant:ident) => {
            if let Value::$variant(a) = v {
                let width = <$t>::BITS;
                if hi >= u128::from(width) {
                    return Err(format!(
                        "bits: hi ({hi}) is out of range for {} (bits are numbered 0 to {})",
                        v.type_name(),
                        width - 1
                    ));
                }
                // `as u128` sign-extends a signed value. Every bit read
                // here sits below `width`, so the extension never shows.
                let raw = *a as u128;
                let count = (hi - lo + 1) as u32;
                let mask = if count == u128::BITS {
                    u128::MAX
                } else {
                    (1u128 << count) - 1
                };
                // The field fits in `width` bits, so the cast back only
                // reinterprets: a full-width signed slice keeps its sign.
                return Ok(Value::$variant(((raw >> lo) & mask) as $t));
            }
        };
    }
    for_each_integer_numeric_variant!(int_arm);
    Err(not_integer("v", v))
}

/// Register every bit-field builtin into `env`.
pub(crate) fn register(env: &mut Environment) {
    env.add_builtin(
        "bits",
        from_fn(|v: Value, hi: Value, lo: Value| bits(&v, &hi, &lo))
            .doc(
                "Bits `hi` down to `lo` of an integer, inclusive, shifted down so bit `lo` \
                 becomes bit 0.",
            )
            .param("v", "integer", "The integer to read from, of any width.")
            .param(
                "hi",
                "integer",
                "The highest bit of the field, counted from 0 at the least significant bit.",
            )
            .param(
                "lo",
                "integer",
                "The lowest bit of the field. No more than `hi`.",
            )
            .returns(
                "integer",
                "The field, zero-extended, in the same integer type as `v`.",
            ),
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(v: Value, hi: Value, lo: Value) -> Value {
        bits(&v, &hi, &lo).expect("bits succeeds")
    }

    fn err(v: Value, hi: Value, lo: Value) -> String {
        bits(&v, &hi, &lo).expect_err("bits fails")
    }

    #[test]
    fn slices_a_middle_field() {
        assert_eq!(
            ok(Value::U8(0b1011_0110), Value::I64(5), Value::I64(2)),
            Value::U8(0b1101)
        );
        assert_eq!(
            ok(Value::U32(0xDEAD_BEEF), Value::I64(15), Value::I64(8)),
            Value::U32(0xBE)
        );
    }

    #[test]
    fn single_bit_when_hi_equals_lo() {
        assert_eq!(
            ok(Value::U8(0b1000_0000), Value::I64(7), Value::I64(7)),
            Value::U8(1)
        );
        assert_eq!(
            ok(Value::U8(0b1000_0000), Value::I64(6), Value::I64(6)),
            Value::U8(0)
        );
    }

    #[test]
    fn full_width_returns_the_value() {
        assert_eq!(
            ok(Value::U64(u64::MAX), Value::I64(63), Value::I64(0)),
            Value::U64(u64::MAX)
        );
        assert_eq!(
            ok(Value::U128(u128::MAX), Value::I64(127), Value::I64(0)),
            Value::U128(u128::MAX)
        );
        // A full-width signed slice keeps the original pattern.
        assert_eq!(
            ok(Value::I8(-5), Value::I64(7), Value::I64(0)),
            Value::I8(-5)
        );
        assert_eq!(
            ok(Value::I128(i128::MIN), Value::I64(127), Value::I64(0)),
            Value::I128(i128::MIN)
        );
    }

    #[test]
    fn signed_input_is_zero_extended() {
        assert_eq!(
            ok(Value::I8(-1), Value::I64(3), Value::I64(0)),
            Value::I8(15)
        );
        // -2 is 0b1111_1110: the top seven bits, shifted down, are 127.
        assert_eq!(
            ok(Value::I8(-2), Value::I64(7), Value::I64(1)),
            Value::I8(127)
        );
        assert_eq!(
            ok(Value::I64(-1), Value::I64(63), Value::I64(63)),
            Value::I64(1)
        );
        assert_eq!(
            ok(Value::Isize(-1), Value::I64(7), Value::I64(4)),
            Value::Isize(15)
        );
    }

    #[test]
    fn u128_high_bits() {
        let v = Value::U128(0xABCD_u128 << 112);
        assert_eq!(
            ok(v.clone(), Value::I64(127), Value::I64(112)),
            Value::U128(0xABCD)
        );
        assert_eq!(ok(v, Value::I64(127), Value::I64(124)), Value::U128(0xA));
    }

    #[test]
    fn indices_take_any_integer_type() {
        assert_eq!(
            ok(Value::Usize(0xF0), Value::U8(7), Value::U128(4)),
            Value::Usize(0xF)
        );
        assert_eq!(
            ok(Value::U16(0x0F00), Value::I8(11), Value::Usize(8)),
            Value::U16(0xF)
        );
    }

    #[test]
    fn non_integer_arguments_error() {
        assert_eq!(
            err(Value::F64(1.0), Value::I64(0), Value::I64(0)),
            "bits: v must be an integer, got f64"
        );
        assert_eq!(
            err(Value::Bool(true), Value::I64(0), Value::I64(0)),
            "bits: v must be an integer, got bool"
        );
        assert_eq!(
            err(Value::U8(1), Value::F32(1.0), Value::I64(0)),
            "bits: hi must be an integer, got f32"
        );
        assert_eq!(
            err(Value::U8(1), Value::I64(1), Value::Utf8("0".into())),
            "bits: lo must be an integer, got utf8"
        );
    }

    #[test]
    fn negative_index_errors() {
        assert_eq!(
            err(Value::U8(1), Value::I64(-1), Value::I64(0)),
            "bits: hi must not be negative, got -1"
        );
        assert_eq!(
            err(Value::U8(1), Value::I64(3), Value::I8(-2)),
            "bits: lo must not be negative, got -2"
        );
    }

    #[test]
    fn hi_below_lo_errors() {
        assert_eq!(
            err(Value::U8(1), Value::I64(2), Value::I64(5)),
            "bits: hi (2) must not be less than lo (5)"
        );
    }

    #[test]
    fn hi_past_the_width_errors() {
        assert_eq!(
            err(Value::U8(1), Value::I64(8), Value::I64(0)),
            "bits: hi (8) is out of range for u8 (bits are numbered 0 to 7)"
        );
        assert_eq!(
            err(Value::I128(1), Value::I64(128), Value::I64(0)),
            "bits: hi (128) is out of range for i128 (bits are numbered 0 to 127)"
        );
        assert_eq!(
            err(Value::U128(1), Value::U128(u128::MAX), Value::I64(0)),
            format!(
                "bits: hi ({}) is out of range for u128 (bits are numbered 0 to 127)",
                u128::MAX
            )
        );
    }
}
