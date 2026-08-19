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
