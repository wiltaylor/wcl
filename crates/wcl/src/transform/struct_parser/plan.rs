//! Parse plan — compiled instruction sequence for struct parsing.
//!
//! A ParsePlan is produced by compiling a ResolvedStruct + EncodingConfig.
//! It's an intermediate representation that can be executed efficiently.
//! For the initial implementation, the binary parser operates directly on
//! ResolvedStruct; this module provides types for future optimization.

use crate::eval::value::Value;
use crate::transform::struct_parser::Endianness;

/// Index into the instruction list.
pub type InstrIdx = usize;

/// Index into the field accumulator at runtime.
pub type FieldIdx = usize;

/// Numeric type kind for binary fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericKind {
    U8,
    U16,
    U32,
    U64,
    I8,
    I16,
    I32,
    I64,
    F32,
    F64,
}

/// How an array determines its element count.
#[derive(Debug, Clone)]
pub enum ArrayCount {
    Fixed(usize),
    Dynamic(FieldIdx),
    Remaining,
}

/// A single instruction in a parse plan.
#[derive(Debug, Clone)]
pub enum Instruction {
    RecordStart {
        name: String,
    },
    RecordEnd,
    ReadNumeric {
        name: String,
        kind: NumericKind,
        endian: Endianness,
    },
    ReadBytesFixed {
        name: String,
        count: usize,
    },
    ReadBytesDynamic {
        name: String,
        length_field: FieldIdx,
    },
    ReadNullTerminatedString {
        name: String,
    },
    ReadFixedString {
        name: String,
        length: usize,
    },
    PadFixed {
        count: usize,
    },
    AlignTo {
        alignment: usize,
    },
    AssertMagic {
        name: String,
        expected: Vec<u8>,
    },
    BranchEq {
        discriminant: FieldIdx,
        value: Value,
        else_target: InstrIdx,
    },
    Jump {
        target: InstrIdx,
    },
    BeginArray {
        name: String,
        count: ArrayCount,
    },
    ArrayNext,
    EndArray,
}

/// A compiled parse plan for a struct definition.
#[derive(Debug, Clone)]
pub struct ParsePlan {
    pub name: String,
    pub instructions: Vec<Instruction>,
}
