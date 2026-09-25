//! The canonical list of numeric variants, handed out as macros.
//!
//! WCL has fourteen numeric types, and several places have to do the
//! same thing once per variant: the host-binding conversions, the
//! arithmetic and comparison operators, `as_u64`, path-segment
//! rendering. Enumerating that ladder by hand in each of them is how a
//! variant goes missing in one place and not the others, so it is
//! written down once here and expanded by
//! [`for_each_numeric_variant!`] and the subsets an operation that only
//! covers part of the ladder needs
//! ([`for_each_integer_numeric_variant!`],
//! [`for_each_signed_integer_numeric_variant!`],
//! [`for_each_float_numeric_variant!`]).
//!
//! The macros are deliberately type-agnostic: each takes the enum to
//! match on, so the same list serves both [`Value`](crate::Value) and
//! [`NumberLit`](crate::NumberLit) — which is what keeps a number
//! reading identically whether it came from the parser or from an
//! evaluated field.
//!
//! Parsing a literal into one of these variants is the lexer's job, in
//! [`lexer::finalize`](crate::lexer).
//!
//! Beside the macros live the two numeric judgements every consumer has
//! to make the same way: [`NumberKey`], which compares numbers of any
//! two types exactly, and [`fit_to_builtin`], which decides whether a
//! number can take a declared numeric type.

use std::cmp::Ordering;

use crate::ast::BuiltinType;
use crate::value::Value;

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

/// Like [`for_each_numeric_variant!`] but only the integer variants.
/// Integers and floats part company wherever an operation can fail —
/// integers have `checked_*` and a zero divisor is fatal, floats have
/// neither — so arithmetic walks the two lists separately.
macro_rules! for_each_integer_numeric_variant {
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
    };
}

/// The signed half of [`for_each_integer_numeric_variant!`]. Used by unary
/// negation, where unsigned types are rejected outright.
macro_rules! for_each_signed_integer_numeric_variant {
    ($mac:ident) => {
        $mac!(i8, I8);
        $mac!(i16, I16);
        $mac!(i32, I32);
        $mac!(i64, I64);
        $mac!(i128, I128);
        $mac!(isize, Isize);
    };
}

/// The float half of [`for_each_numeric_variant!`].
macro_rules! for_each_float_numeric_variant {
    ($mac:ident) => {
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

/// An exact, width-independent reading of a numeric [`Value`], for
/// comparing numbers of different types without a lossy promotion.
///
/// Every integer width fits as a sign plus a `u128` magnitude, which
/// covers `u128::MAX` and `i128::MIN` alike; a float stays a float. The
/// comparison between the two halves is exact too: `9007199254740993`
/// is *not* equal to `9007199254740992.0`, although both round to the
/// same `f64`. `==`, the ordering operators and `sort` all order numbers
/// through this one type so they cannot disagree.
#[derive(Debug, Clone, Copy)]
pub(crate) enum NumberKey {
    /// Sign and absolute magnitude cover every signed and unsigned integer width.
    Integer {
        /// Whether the original integer is below zero.
        negative: bool,
        /// Absolute value, including magnitudes above i128::MAX.
        magnitude: u128,
    },
    /// An f64, or an exactly widened f32. May be NaN; see
    /// [`NumberKey::partial_cmp`].
    Float(f64),
}

impl NumberKey {
    /// Read a numeric value exactly, or `None` when `v` is not a number.
    pub(crate) fn new(v: &Value) -> Option<Self> {
        match v {
            Value::F32(n) => Some(Self::Float(f64::from(*n))),
            Value::F64(n) => Some(Self::Float(*n)),
            Value::U128(n) => Some(Self::Integer {
                negative: false,
                magnitude: *n,
            }),
            _ => v.as_i128().map(|n| Self::Integer {
                negative: n < 0,
                magnitude: n.unsigned_abs(),
            }),
        }
    }

    /// Read a numeric value as a sort key, rejecting NaN so every pair of
    /// accepted keys has a consistent order under [`NumberKey::cmp`].
    /// Callers exclude non-numeric values.
    pub(crate) fn sortable(v: &Value) -> Result<Self, String> {
        match Self::new(v) {
            Some(Self::Float(n)) if n.is_nan() => Err("numeric keys must not be NaN".into()),
            Some(key) => Ok(key),
            None => Err(format!("expected a number, got {}", v.type_name())),
        }
    }

    /// Compare mixed numbers by magnitude, treating signed zeros as
    /// equal. `None` when either side is NaN, which is unordered against
    /// everything, itself included (IEEE 754).
    pub(crate) fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        match (self, other) {
            (
                Self::Integer {
                    negative: an,
                    magnitude: a,
                },
                Self::Integer {
                    negative: bn,
                    magnitude: b,
                },
            ) => Some(match (an, bn) {
                // `-0` has no integer spelling, so a negative key is never zero.
                (true, false) => Ordering::Less,
                (false, true) => Ordering::Greater,
                (true, true) => b.cmp(a),
                (false, false) => a.cmp(b),
            }),
            (Self::Float(a), Self::Float(b)) => a.partial_cmp(b),
            (
                Self::Integer {
                    negative,
                    magnitude,
                },
                Self::Float(n),
            ) => {
                if n.is_nan() {
                    return None;
                }
                // `-0.0 < 0.0` is false, so a signed zero reads as zero.
                let float_negative = *n < 0.0;
                if *negative != float_negative {
                    return Some(if *negative {
                        Ordering::Less
                    } else {
                        Ordering::Greater
                    });
                }
                let n = n.abs();
                // 2^128 is exactly representable, but is outside u128. Below
                // it, truncating the float preserves every integer bit, and
                // the integral part decides before the fractional part.
                let order = if n >= 2.0_f64.powi(128) {
                    Ordering::Less
                } else {
                    magnitude.cmp(&(n as u128)).then_with(|| {
                        if n.fract() > 0.0 {
                            Ordering::Less
                        } else {
                            Ordering::Equal
                        }
                    })
                };
                Some(if *negative { order.reverse() } else { order })
            }
            (Self::Float(_), Self::Integer { .. }) => {
                other.partial_cmp(self).map(Ordering::reverse)
            }
        }
    }

    /// Total order over keys built by [`NumberKey::sortable`], which
    /// never holds NaN.
    pub(crate) fn cmp(&self, other: &Self) -> Ordering {
        self.partial_cmp(other).expect("non-NaN sort keys")
    }
}

/// Why a number cannot take a declared numeric type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NumericMisfit {
    /// The value lies outside the target type's range.
    OutOfRange,
    /// A float with a fractional part (or NaN / infinity) against an
    /// integer type.
    NotWhole,
}

/// Convert a numeric value to the numeric builtin `target`, if it fits.
///
/// - An integer fits an integer type when it lies in that type's range.
/// - A float fits an integer type when it is whole-valued and in range:
///   `1.0` becomes `1`, `2.5` is [`NumericMisfit::NotWhole`].
/// - Any integer fits a float type, rounding to the nearest
///   representable float when it has more significant bits than the
///   mantissa holds (`16777217` into `f32` is `16777216.0`).
/// - A float fits a float type unless a finite `f64` overflows `f32`.
///
/// `None` when `v` is not a number or `target` is not a numeric type.
pub(crate) fn fit_to_builtin(
    v: &Value,
    target: BuiltinType,
) -> Option<Result<Value, NumericMisfit>> {
    use BuiltinType as B;
    if !target.is_numeric() {
        return None;
    }
    let key = NumberKey::new(v)?;
    let as_f64 = v.as_f64()?;
    if target == B::F64 {
        return Some(Ok(Value::F64(as_f64)));
    }
    if target == B::F32 {
        let narrowed = match v {
            // An exact f32 stays exactly itself.
            Value::F32(n) => *n,
            // Integers round to nearest; `as` from a wide integer does too.
            _ if matches!(key, NumberKey::Integer { .. }) => match v {
                Value::U128(n) => *n as f32,
                _ => v.as_i128()? as f32,
            },
            _ => as_f64 as f32,
        };
        if narrowed.is_infinite() && as_f64.is_finite() {
            return Some(Err(NumericMisfit::OutOfRange));
        }
        return Some(Ok(Value::F32(narrowed)));
    }
    // Integer target: reduce the value to a sign and magnitude first.
    let (negative, magnitude) = match key {
        NumberKey::Integer {
            negative,
            magnitude,
        } => (negative, magnitude),
        NumberKey::Float(n) => {
            if !n.is_finite() || n.fract() != 0.0 {
                return Some(Err(NumericMisfit::NotWhole));
            }
            if n.abs() >= 2.0_f64.powi(128) {
                return Some(Err(NumericMisfit::OutOfRange));
            }
            // `-0.0` is zero, not negative.
            (n < 0.0, n.abs() as u128)
        }
    };
    let signed: Option<i128> = if negative {
        0i128.checked_sub_unsigned(magnitude)
    } else {
        i128::try_from(magnitude).ok()
    };
    macro_rules! narrow {
        ($t:ty, $variant:ident) => {
            if target == B::$variant {
                let fitted = match signed {
                    Some(n) => <$t>::try_from(n).ok(),
                    None if !negative => <$t>::try_from(magnitude).ok(),
                    None => None,
                };
                return Some(fitted.map(Value::$variant).ok_or(NumericMisfit::OutOfRange));
            }
        };
    }
    for_each_integer_numeric_variant!(narrow);
    unreachable!("numeric builtin {target:?} is an integer or a float")
}

pub(crate) use for_each_float_numeric_variant;
pub(crate) use for_each_integer_numeric_variant;
pub(crate) use for_each_numeric_variant;
pub(crate) use for_each_signed_integer_numeric_variant;
pub(crate) use numeric_as_path_segment;
pub(crate) use numeric_as_u64;

#[cfg(test)]
mod tests {
    use crate::lexer::NumberLit;

    #[test]
    fn fit_to_builtin_handles_the_edges_of_each_range() {
        use super::{NumericMisfit, fit_to_builtin};
        use crate::ast::BuiltinType as B;
        use crate::value::Value;
        let fit = |v: Value, b: B| fit_to_builtin(&v, b).expect("numeric pair");
        assert_eq!(
            fit(Value::I128(i128::MIN), B::I128),
            Ok(Value::I128(i128::MIN))
        );
        assert_eq!(
            fit(Value::U128(u128::MAX), B::U128),
            Ok(Value::U128(u128::MAX))
        );
        assert_eq!(
            fit(Value::U128(u128::MAX), B::I128),
            Err(NumericMisfit::OutOfRange)
        );
        assert_eq!(fit(Value::I64(-1), B::U64), Err(NumericMisfit::OutOfRange));
        assert_eq!(fit(Value::F64(-0.0), B::U8), Ok(Value::U8(0)));
        assert_eq!(fit(Value::F64(255.0), B::U8), Ok(Value::U8(255)));
        assert_eq!(fit(Value::F64(0.5), B::U8), Err(NumericMisfit::NotWhole));
        assert_eq!(
            fit(Value::F64(f64::NAN), B::I32),
            Err(NumericMisfit::NotWhole)
        );
        assert_eq!(
            fit(Value::F64(2.0_f64.powi(128)), B::U128),
            Err(NumericMisfit::OutOfRange)
        );
        assert_eq!(
            fit(Value::F64(1e300), B::F32),
            Err(NumericMisfit::OutOfRange)
        );
        assert_eq!(fit(Value::F64(1.5), B::F32), Ok(Value::F32(1.5)));
        assert!(fit_to_builtin(&Value::Bool(true), B::U8).is_none());
        assert!(fit_to_builtin(&Value::I64(1), B::Utf8).is_none());
    }

    #[test]
    fn number_key_orders_across_widths_exactly() {
        use super::NumberKey;
        use crate::value::Value;
        use std::cmp::Ordering;
        let key = |v: Value| NumberKey::new(&v).expect("numeric");
        let cmp = |a: Value, b: Value| key(a).partial_cmp(&key(b));
        assert_eq!(
            cmp(Value::U128(u128::MAX), Value::I64(-1)),
            Some(Ordering::Greater)
        );
        assert_eq!(
            cmp(Value::I128(i128::MIN), Value::F64(-1e300)),
            Some(Ordering::Greater)
        );
        assert_eq!(
            cmp(
                Value::I64(9_007_199_254_740_993),
                Value::F64(9_007_199_254_740_992.0)
            ),
            Some(Ordering::Greater)
        );
        assert_eq!(cmp(Value::I64(0), Value::F64(-0.0)), Some(Ordering::Equal));
        assert_eq!(cmp(Value::I64(-3), Value::F64(-2.5)), Some(Ordering::Less));
        assert_eq!(cmp(Value::F64(f64::NAN), Value::I64(1)), None);
        assert_eq!(cmp(Value::I64(1), Value::F64(f64::NAN)), None);
        assert!(NumberKey::sortable(&Value::F32(f32::NAN)).is_err());
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
}
