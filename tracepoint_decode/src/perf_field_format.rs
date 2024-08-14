// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

extern crate alloc;

use core::fmt;
use core::ops;

use alloc::string;

use crate::*;
use eventheader_types::*;

/// This macro is used in certain edge cases that I don't expect to happen in normal
/// `format` files. The code treats these as errors. The macro provides an easy way
/// to make an instrumented build that reports these cases.
///
/// At present, does nothing.
macro_rules! debug_eprintln {
    ($($arg:tt)*) => {};
}

/// The type of the array property of [`PerfFieldFormat`].
/// Array-ness of a field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PerfFieldArray {
    /// e.g. `char val; size:1;`.
    None,

    /// e.g. `char val[12]; size:12;`.
    Fixed,

    /// e.g. `char val[]; size:0;`.
    RestOfEvent,

    /// e.g. `__rel_loc char val[]; size:2;`.
    /// Value contains relativeOffset. dataLen is determined via strlen.
    RelLoc2,

    /// e.g. `__data_loc char val[]; size:2;`.
    /// Value contains offset. dataLen is determined via strlen.
    DataLoc2,

    /// e.g. `__rel_loc char val[]; size:4;`.
    /// Value contains `(dataLen << 16) | relativeOffset`.
    RelLoc4,

    /// e.g. `__data_loc char val[]; size:4;`.
    /// Value contains `(dataLen << 16) | offset`.
    DataLoc4,
}

impl fmt::Display for PerfFieldArray {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let str = match self {
            PerfFieldArray::None => "None",
            PerfFieldArray::Fixed => "Fixed",
            PerfFieldArray::RestOfEvent => "RestOfEvent",
            PerfFieldArray::RelLoc2 => "RelLoc2",
            PerfFieldArray::DataLoc2 => "DataLoc2",
            PerfFieldArray::RelLoc4 => "RelLoc4",
            PerfFieldArray::DataLoc4 => "DataLoc4",
        };
        return f.pad(str);
    }
}

/// Stores decoding information about a field, parsed from a tracefs "format" file.
#[derive(Debug)]
pub struct PerfFieldFormat {
    field: string::String,
    name_range: ops::Range<usize>,
    offset: u16,
    size: u16,
    signed: Option<bool>,
    specified_array_count: u16,
    deduced_array_count: u16,
    specified_encoding: FieldEncoding,
    deduced_encoding: FieldEncoding,
    specified_format: FieldFormat,
    deduced_format: FieldFormat,
    array: PerfFieldArray,
    element_size_shift: u8,
}

impl PerfFieldFormat {
    /// Initializes a `PerfFieldFormat` from a `field` definition plus pre-parsed values for
    /// other properties. Normally you'll call `parse` to parse a field definition line instead
    /// of calling this method directly.
    ///
    /// Initializes `field`, `offset`, `size`, and `signed` properties exactly as specified.
    /// Parses `field` to deduce the other properties. The signed parameter should be `None`
    /// if the "signed:" property is not present in the format line.
    pub fn new(
        long_is_64_bits: bool,
        field: &str,
        offset: u16,
        size: u16,
        signed: Option<bool>,
    ) -> Self {
        const SIZEOF_U8: u16 = 1;
        const SIZEOF_U16: u16 = 2;
        const SIZEOF_U32: u16 = 4;
        const SIZEOF_U64: u16 = 8;

        let mut found_long_long = false;
        let mut found_long = false;
        let mut found_short = false;
        let mut found_unsigned = false;
        let mut found_signed = false;
        let mut found_struct = false;
        let mut found_data_loc = false;
        let mut found_rel_loc = false;
        let mut found_array = false;
        let mut found_pointer = false;
        let mut base_type_range = 0..0;
        let mut name_range = 0..0;

        let mut result = Self {
            field: string::String::from(field),
            name_range: 0..0,
            offset,
            size,
            signed,
            specified_array_count: 0,
            deduced_array_count: 0,
            specified_encoding: FieldEncoding::Invalid,
            deduced_encoding: FieldEncoding::Invalid,
            specified_format: FieldFormat::Default,
            deduced_format: FieldFormat::Default,
            array: PerfFieldArray::None,
            element_size_shift: 0,
        };

        // PARSE: Name, SpecifiedArrayCount

        let mut tokenizer = Tokenizer::new(field);
        loop {
            tokenizer.move_next();
            let token_value = &field[tokenizer.value_range.clone()];
            match tokenizer.kind {
                TokenKind::None => {
                    break; // TokensDone
                }

                TokenKind::Ident => match token_value {
                    "long" => {
                        if found_long {
                            found_long_long = true;
                        } else {
                            found_long = true;
                        }
                    }
                    "short" => {
                        found_short = true;
                    }
                    "unsigned" => {
                        found_unsigned = true;
                    }
                    "signed" => {
                        found_signed = true;
                    }
                    "struct" => {
                        found_struct = true;
                    }
                    "__data_loc" => {
                        found_data_loc = true;
                    }
                    "__rel_loc" => {
                        found_rel_loc = true;
                    }
                    _ => {
                        if token_value != "__attribute__"
                            && token_value != "const"
                            && token_value != "volatile"
                        {
                            base_type_range = name_range;
                            name_range = tokenizer.value_range.clone();
                        }
                    }
                },
                TokenKind::Brackets => {
                    // [] or [ElementCount]
                    found_array = true;
                    result.specified_array_count =
                        ascii_to_u32(&token_value.as_bytes()[1..]).unwrap_or(0) as u16;
                    tokenizer.move_next();
                    if tokenizer.kind == TokenKind::Ident {
                        base_type_range = name_range;
                        name_range = tokenizer.value_range.clone();
                    }

                    break; // TokensDone
                }
                TokenKind::Parentheses | TokenKind::String => {
                    // Ignored.
                }
                TokenKind::Punctuation => {
                    // Most punctuation ignored.
                    if token_value == "*" {
                        found_pointer = true;
                    }
                }
            }
        }

        // TokensDone

        let base_type = &field[base_type_range];
        result.name_range = name_range;

        // PARSE: SpecifiedEncoding, SpecifiedFormat

        if found_pointer {
            result.specified_format = FieldFormat::HexInt;
            result.specified_encoding = if long_is_64_bits {
                FieldEncoding::Value64
            } else {
                FieldEncoding::Value32
            };
        } else if found_struct {
            result.specified_format = FieldFormat::HexBytes; // SPECIAL
            result.specified_encoding = FieldEncoding::Struct; // SPECIAL
        } else if base_type.is_empty() || base_type == ("int") {
            result.specified_format = if found_unsigned {
                FieldFormat::UnsignedInt
            } else {
                FieldFormat::SignedInt
            };
            if found_long_long {
                result.specified_encoding = FieldEncoding::Value64;
            } else if found_long {
                result.specified_encoding = if long_is_64_bits {
                    FieldEncoding::Value64
                } else {
                    FieldEncoding::Value32
                };
                if found_unsigned {
                    result.specified_format = FieldFormat::HexInt; // Use hex for unsigned long.
                }
            } else if found_short {
                result.specified_encoding = FieldEncoding::Value16;
            } else {
                result.specified_encoding = FieldEncoding::Value32; // "unsigned" or "signed" means "int".
                if base_type.is_empty() && !found_unsigned && !found_signed {
                    // Unexpected.
                    debug_eprintln!("No base_type found for \"{}\"", result.field);
                }
            }
        } else if base_type == ("char") {
            result.specified_format = if found_unsigned {
                FieldFormat::UnsignedInt
            } else if found_signed {
                FieldFormat::SignedInt
            } else {
                FieldFormat::String8 // SPECIAL
            };
            result.specified_encoding = FieldEncoding::Value8;
        } else if base_type == ("u8") || base_type == ("__u8") || base_type == ("uint8_t") {
            result.specified_format = FieldFormat::UnsignedInt;
            result.specified_encoding = FieldEncoding::Value8;
        } else if base_type == ("s8") || base_type == ("__s8") || base_type == ("int8_t") {
            result.specified_format = FieldFormat::SignedInt;
            result.specified_encoding = FieldEncoding::Value8;
        } else if base_type == ("u16") || base_type == ("__u16") || base_type == ("uint16_t") {
            result.specified_format = FieldFormat::UnsignedInt;
            result.specified_encoding = FieldEncoding::Value16;
        } else if base_type == ("s16") || base_type == ("__s16") || base_type == ("int16_t") {
            result.specified_format = FieldFormat::SignedInt;
            result.specified_encoding = FieldEncoding::Value16;
        } else if base_type == ("u32") || base_type == ("__u32") || base_type == ("uint32_t") {
            result.specified_format = FieldFormat::UnsignedInt;
            result.specified_encoding = FieldEncoding::Value32;
        } else if base_type == ("s32") || base_type == ("__s32") || base_type == ("int32_t") {
            result.specified_format = FieldFormat::SignedInt;
            result.specified_encoding = FieldEncoding::Value32;
        } else if base_type == ("u64") || base_type == ("__u64") || base_type == ("uint64_t") {
            result.specified_format = FieldFormat::UnsignedInt;
            result.specified_encoding = FieldEncoding::Value64;
        } else if base_type == ("s64") || base_type == ("__s64") || base_type == ("int64_t") {
            result.specified_format = FieldFormat::SignedInt;
            result.specified_encoding = FieldEncoding::Value64;
        } else {
            result.specified_format = FieldFormat::HexInt;
            result.specified_encoding = FieldEncoding::Invalid; // SPECIAL
        }

        // PARSE: Array

        if result.size == 0 {
            result.array = PerfFieldArray::RestOfEvent;
        } else if result.size == 2 && found_rel_loc {
            result.array = PerfFieldArray::RelLoc2;
        } else if result.size == 2 && found_data_loc {
            result.array = PerfFieldArray::DataLoc2;
        } else if result.size == 4 && found_rel_loc {
            result.array = PerfFieldArray::RelLoc4;
        } else if result.size == 4 && found_data_loc {
            result.array = PerfFieldArray::DataLoc4;
        } else if found_array {
            result.array = PerfFieldArray::Fixed;
        } else {
            result.array = PerfFieldArray::None;
        }

        // DEDUCE: deduced_format.

        // Apply the "signed:" property if specified.
        if result.specified_format == FieldFormat::UnsignedInt
            || result.specified_format == FieldFormat::SignedInt
        {
            // If valid, signed overrides base_type.
            match signed {
                None => result.deduced_format = result.specified_format,
                Some(false) => result.deduced_format = FieldFormat::UnsignedInt,
                Some(true) => result.deduced_format = FieldFormat::SignedInt,
            }
        } else {
            result.deduced_format = result.specified_format;
        }

        // DEDUCE: deduced_encoding, deduced_array_count, element_size_shift.

        if result.specified_format == FieldFormat::String8 {
            debug_assert!(result.specified_encoding == FieldEncoding::Value8);
            result.deduced_encoding = if result.size == 1 {
                FieldEncoding::Value8
            } else {
                FieldEncoding::ZStringChar8
            };
            result.deduced_array_count = 1;
            result.element_size_shift = if result.size == 1 { 0u8 } else { u8::MAX };
        } else if result.specified_format == FieldFormat::HexBytes {
            debug_assert!(result.specified_encoding == FieldEncoding::Struct);
            result.deduced_encoding = if result.size == 1 {
                FieldEncoding::Value8
            } else {
                FieldEncoding::StringLength16Char8
            };
            result.deduced_array_count = 1;
            result.element_size_shift = u8::MAX;
        } else {
            #[allow(clippy::never_loop)]
            'DeductionDone: loop {
                match result.array {
                    PerfFieldArray::None => {
                        // Size overrides element size deduced from type name.
                        match result.size {
                            1 => {
                                result.deduced_encoding = FieldEncoding::Value8;
                                result.element_size_shift = 0;
                            }
                            2 => {
                                result.deduced_encoding = FieldEncoding::Value16;
                                result.element_size_shift = 1;
                            }
                            4 => {
                                result.deduced_encoding = FieldEncoding::Value32;
                                result.element_size_shift = 2;
                            }
                            8 => {
                                result.deduced_encoding = FieldEncoding::Value64;
                                result.element_size_shift = 3;
                            }
                            _ => {
                                result.set_hex_dump();
                                break 'DeductionDone;
                            }
                        }

                        result.deduced_array_count = 1;
                    }
                    PerfFieldArray::Fixed => {
                        if result.specified_array_count == 0 {
                            result.deduced_encoding = result
                                .specified_encoding
                                .with_flags(FieldEncoding::CArrayFlag);
                            match result.specified_encoding {
                                FieldEncoding::Value8 => {
                                    result.deduced_array_count = result.size;
                                    result.element_size_shift = 0;
                                }
                                FieldEncoding::Value16 => {
                                    if result.size % SIZEOF_U16 != 0 {
                                        result.set_hex_dump();
                                        break 'DeductionDone;
                                    }
                                    result.deduced_array_count = result.size / SIZEOF_U16;
                                    result.element_size_shift = 1;
                                }
                                FieldEncoding::Value32 => {
                                    if result.size % SIZEOF_U32 != 0 {
                                        result.set_hex_dump();
                                        break 'DeductionDone;
                                    }
                                    result.deduced_array_count = result.size / SIZEOF_U32;
                                    result.element_size_shift = 2;
                                }
                                FieldEncoding::Value64 => {
                                    if result.size % SIZEOF_U64 != 0 {
                                        result.set_hex_dump();
                                        break 'DeductionDone;
                                    }
                                    result.deduced_array_count = result.size / SIZEOF_U64;
                                    result.element_size_shift = 3;
                                }
                                _ => {
                                    debug_assert!(
                                        result.specified_encoding == FieldEncoding::Invalid
                                    );
                                    result.set_hex_dump();
                                    break 'DeductionDone;
                                }
                            }
                        } else {
                            if result.size % result.specified_array_count != 0 {
                                result.set_hex_dump();
                                break 'DeductionDone;
                            }

                            match result.size / result.specified_array_count {
                                1 => {
                                    result.deduced_encoding =
                                        FieldEncoding::Value8.with_flags(FieldEncoding::CArrayFlag);
                                    result.element_size_shift = 0;
                                }
                                2 => {
                                    result.deduced_encoding = FieldEncoding::Value16
                                        .with_flags(FieldEncoding::CArrayFlag);
                                    result.element_size_shift = 1;
                                }
                                4 => {
                                    result.deduced_encoding = FieldEncoding::Value32
                                        .with_flags(FieldEncoding::CArrayFlag);
                                    result.element_size_shift = 2;
                                }
                                8 => {
                                    result.deduced_encoding = FieldEncoding::Value64
                                        .with_flags(FieldEncoding::CArrayFlag);
                                    result.element_size_shift = 3;
                                }
                                _ => {
                                    result.set_hex_dump();
                                    break 'DeductionDone;
                                }
                            }

                            result.deduced_array_count = result.specified_array_count;
                        }
                    }
                    _ => {
                        // Variable-length data.

                        match result.specified_encoding {
                            FieldEncoding::Value8 => {
                                result.element_size_shift = 0;
                            }
                            FieldEncoding::Value16 => {
                                result.element_size_shift = 1;
                            }
                            FieldEncoding::Value32 => {
                                result.element_size_shift = 2;
                            }
                            FieldEncoding::Value64 => {
                                result.element_size_shift = 3;
                            }
                            _ => {
                                debug_assert!(result.specified_encoding == FieldEncoding::Invalid);
                                result.set_hex_dump();
                                break 'DeductionDone;
                            }
                        }

                        result.deduced_encoding = result
                            .specified_encoding
                            .with_flags(FieldEncoding::VArrayFlag);
                        result.deduced_array_count = 0;
                    }
                }

                break 'DeductionDone;
            }
        }

        // DEBUG
        #[cfg(debug_assertions)]
        {
            debug_assert!(!result.field.is_empty());

            let encoding_value = result.deduced_encoding.without_flags();
            match encoding_value {
                FieldEncoding::Value8 => {
                    if result.deduced_array_count != 0 {
                        debug_assert!(result.size == result.deduced_array_count * SIZEOF_U8);
                    }
                    debug_assert!(result.element_size_shift == 0);
                }
                FieldEncoding::Value16 => {
                    if result.deduced_array_count != 0 {
                        debug_assert!(result.size == result.deduced_array_count * SIZEOF_U16);
                    }
                    debug_assert!(result.element_size_shift == 1);
                }
                FieldEncoding::Value32 => {
                    if result.deduced_array_count != 0 {
                        debug_assert!(result.size == result.deduced_array_count * SIZEOF_U32);
                    }
                    debug_assert!(result.element_size_shift == 2);
                }
                FieldEncoding::Value64 => {
                    if result.deduced_array_count != 0 {
                        debug_assert!(result.size == result.deduced_array_count * SIZEOF_U64);
                    }
                    debug_assert!(result.element_size_shift == 3);
                }
                FieldEncoding::StringLength16Char8 => {
                    debug_assert!(result.deduced_array_count == 1);
                    debug_assert!(
                        (result.deduced_encoding.as_int() & FieldEncoding::FlagMask) == 0
                    );
                    debug_assert!(result.deduced_format == FieldFormat::HexBytes);
                    debug_assert!(result.element_size_shift == u8::MAX);
                }
                FieldEncoding::ZStringChar8 => {
                    debug_assert!(result.deduced_array_count == 1);
                    debug_assert!(
                        (result.deduced_encoding.as_int() & FieldEncoding::FlagMask) == 0
                    );
                    debug_assert!(result.deduced_format == FieldFormat::String8);
                    debug_assert!(result.element_size_shift == u8::MAX);
                }
                _ => {
                    panic!("Unexpected deduced_encoding type");
                }
            }

            let encoding_flags = result.deduced_encoding.as_int() & FieldEncoding::FlagMask;
            match encoding_flags {
                0 => {
                    debug_assert!(result.deduced_array_count == 1);
                }
                FieldEncoding::VArrayFlag => {
                    debug_assert!(result.deduced_array_count == 0);
                }
                FieldEncoding::CArrayFlag => {
                    debug_assert!(result.deduced_array_count >= 1);
                }
                _ => {
                    panic!("Unexpected deduced_encoding flags");
                }
            }

            match result.deduced_format {
                FieldFormat::UnsignedInt | FieldFormat::SignedInt | FieldFormat::HexInt => {
                    debug_assert!(encoding_value >= FieldEncoding::Value8);
                    debug_assert!(encoding_value <= FieldEncoding::Value64);
                }
                FieldFormat::HexBytes => {
                    debug_assert!(encoding_value == FieldEncoding::StringLength16Char8);
                }
                FieldFormat::String8 => {
                    debug_assert!(
                        encoding_value == FieldEncoding::Value8
                            || encoding_value == FieldEncoding::ZStringChar8
                    );
                }
                _ => {
                    panic!("Unexpected deduced_format type");
                }
            }
        }

        return result;
    }

    /// Parses a line of the "format:" section of an event's "format" file. The
    /// `format_line` string will generally look like
    /// `"[whitespace?]field:[declaration]; offset:[number]; size:[number]; ..."`.
    ///
    /// If "field:" is non-empty, "offset:" is a valid unsigned integer, and
    /// "size:" is a valid unsigned integer, returns
    /// PerfFieldFormat::new(long_is_64_bits, field, offset, size, signed).
    ///
    /// Otherwise, returns `None`.
    ///
    /// Note that You'll usually use PerfEventFormat::parse to parse the entire format
    /// file rather than calling this method directly.
    pub fn parse(long_is_64_bits: bool, format_line: &str) -> Option<Self> {
        let mut field = "";
        let mut offset = None;
        let mut size = None;
        let mut signed = None;

        let format_bytes = format_line.as_bytes();
        let mut format_pos = 0;

        // FIND: field, offset, size

        // Search for " NAME: VALUE;"
        'TopLevel: while format_pos < format_bytes.len() {
            // Skip spaces and semicolons.
            while is_space_or_tab_or_semicolon(format_bytes[format_pos]) {
                format_pos += 1;
                if format_pos >= format_bytes.len() {
                    break 'TopLevel;
                }
            }

            // "NAME:"
            let name_pos = format_pos;
            while format_bytes[format_pos] != b':' {
                format_pos += 1;
                if format_pos >= format_bytes.len() {
                    debug_eprintln!("EOL before ':' in format");
                    break 'TopLevel; // Unexpected.
                }
            }

            let name = &format_line[name_pos..format_pos];
            format_pos += 1; // Skip ':'

            // Skip spaces.
            while format_pos < format_bytes.len() && is_space_or_tab(format_bytes[format_pos]) {
                debug_eprintln!("Space before propval in format");
                format_pos += 1; // Unexpected.
            }

            // "VALUE;"
            let value_pos = format_pos;
            while format_pos < format_bytes.len() && format_bytes[format_pos] != b';' {
                format_pos += 1;
            }

            let value = &format_line[value_pos..format_pos];
            if name == "field" || name == "field special" {
                field = value;
            } else if name == "offset" && format_pos < format_bytes.len() {
                offset = ascii_to_u32(value.as_bytes()).map(|n32| n32 as u16);
            } else if name == "size" && format_pos < format_bytes.len() {
                size = ascii_to_u32(value.as_bytes()).map(|n32| n32 as u16);
            } else if name == "signed" && format_pos < format_bytes.len() {
                signed = ascii_to_u32(value.as_bytes()).map(|value_int| value_int != 0);
            }
        }

        match (offset, size) {
            (Some(offset), Some(size)) if !field.is_empty() => {
                return Some(PerfFieldFormat::new(
                    long_is_64_bits,
                    field,
                    offset,
                    size,
                    signed,
                ))
            }
            _ => return None,
        }
    }

    /// Name of the field, or `"noname"` if unable to determine the name.
    /// (Parsed from `field`, e.g. if `field == "char my_field[8]"` then `name == "my_field"`.)
    pub fn name(&self) -> &str {
        if self.name_range.is_empty() {
            "noname"
        } else {
            &self.field[self.name_range.clone()]
        }
    }

    /// Field declaration in pseudo-C syntax, e.g. `"char my_field[8]"`.
    /// (Value of the format line's `"field:"` property.)
    pub fn field(&self) -> &str {
        &self.field
    }

    /// The byte offset of the start of the field data from the start of
    /// the event raw data. (Value of the format line's `"offset:"` property.)
    pub fn offset(&self) -> u16 {
        self.offset
    }

    /// The byte size of the field data. May be 0 to indicate "rest of event".
    /// (Value of the format line's `"size:"` property.)
    pub fn size(&self) -> u16 {
        self.size
    }

    /// Whether the field is signed; null if unspecified.
    /// (Value of the format's `"signed:"` property.)
    pub fn signed(&self) -> Option<bool> {
        self.signed
    }

    /// The number of elements in this field, as specified in the field property,
    /// or 0 if no array count was found.
    /// (Parsed from field, e.g. if `field == "char my_field[8]"` then `specified_array_count == 8`.)
    pub fn specified_array_count(&self) -> u16 {
        self.specified_array_count
    }

    /// The number of elements in this field, as deduced from field and size.
    /// If the field is not being treated as an array (i.e. a single item, or
    /// an array that is being treated as a string or a blob), this will be 1.
    /// If the field is a variable-length array, this will be 0.
    pub fn deduced_array_count(&self) -> u16 {
        self.deduced_array_count
    }

    /// The encoding of the field's base type, as specified in the field property.
    /// This may be Value8, Value16, Value32, Value64, Struct, or Invalid if no
    /// recognized encoding was found.
    /// (Parsed from field, e.g. if `field = "char my_field[8]"` then base type is
    /// "char" so `specified_encoding == "Value8"`.)
    pub fn specified_encoding(&self) -> FieldEncoding {
        self.specified_encoding
    }

    /// The encoding of the field's base type, as deduced from field and size.
    /// This will be Value8, Value16, Value32, Value64, ZStringChar8 for a
    /// nul-terminated string, or StringLength16Char8 for a binary blob.
    /// The VArrayFlag flag or the CArrayFlag flag may be set for Value8,
    /// Value16, Value32, and Value64.
    pub fn deduced_encoding(&self) -> FieldEncoding {
        self.deduced_encoding
    }

    /// The format of the field's base type, as specified by the field and signed properties.
    /// This will be UnsignedInt, SignedInt, HexInt, String8, or HexBytes.
    /// (Parsed from field, e.g. if `field == "char my_field[8]"` then base type is
    /// "char" so `specified_format == "String8"`.)
    pub fn specified_format(&self) -> FieldFormat {
        self.specified_format
    }

    /// The format of the field's base type, as deduced from field, size, and signed.
    pub fn deduced_format(&self) -> FieldFormat {
        self.deduced_format
    }

    /// The kind of array this field is, as specified in the field property.
    /// (Parsed from field and size, e.g. if `field == "char my_field[8]"` then `array == Fixed`.)
    pub fn array(&self) -> PerfFieldArray {
        self.array
    }

    /// For string or blob, this is byte.MaxValue.
    /// For other types, `element_size_shift` is the log2 of the size of each element
    /// in the field. If the field is a single N-bit integer or an array of N-bit
    /// integers, `element_size_shift` is: 0 for 8-bit integers, 1 for 16-bit integers,
    /// 2 for 32-bit integers, and 3 for 64-bit integers.
    pub fn element_size_shift(&self) -> u8 {
        self.element_size_shift
    }

    /// For string or blob, this is 0.
    /// For other types, `element_size` is the size of each element in the field, in bytes.
    /// For example, if the field is a single 32-bit integer or an array of 32-bit integers,
    /// `element_size` is 4.
    pub fn element_size(&self) -> u8 {
        (1u32 << (self.element_size_shift & 0x1F)) as u8
    }

    /// Given the event's raw data (e.g. PerfSampleEventInfo::raw_data) and a `byte_reader`
    /// that indicates whether the event is big-endian or little-endian, return this
    /// field's data bytes. Returns `None` for out-of-bounds, i.e. if `event_raw_data` is
    /// too short for the expected position+length of the field's data bytes.
    ///
    /// Does not do any byte-swapping. This method uses `byte_reader` to resolve
    /// `data_loc` and `rel_loc` references, not to fix up the field data.
    ///
    /// In some cases, the length of the slice returned by `get_field_bytes` may be
    /// different from the value returned by the `size()` property:
    ///
    /// - If `size() == 0`, returns all data from offset to the end of the event,
    ///   i.e. it returns `event_raw_data - offset()` bytes.
    ///
    /// - If `array()` is `Dynamic` or `RelDyn`, the returned size depends on the
    ///   event contents.
    pub fn get_field_bytes<'dat>(
        &self,
        event_raw_data: &'dat [u8],
        byte_reader: PerfByteReader,
    ) -> Option<&'dat [u8]> {
        let raw_begin = self.offset as usize;
        let raw_end = raw_begin + self.size as usize;
        if raw_begin <= raw_end && raw_end <= event_raw_data.len() {
            match self.array {
                PerfFieldArray::None | PerfFieldArray::Fixed => {
                    return Some(&event_raw_data[raw_begin..raw_end]);
                }
                PerfFieldArray::RestOfEvent => {
                    return Some(&event_raw_data[raw_begin..]);
                }
                PerfFieldArray::DataLoc2 | PerfFieldArray::RelLoc2 => {
                    // 2-byte value is an offset leading to the real data, size is strlen.
                    let dyn_offset = byte_reader.read_u16(&event_raw_data[raw_begin..]) as usize
                        + if self.array == PerfFieldArray::RelLoc2 {
                            // offset is relative to end of field.
                            raw_end
                        } else {
                            0
                        };

                    if dyn_offset <= event_raw_data.len() {
                        return Some(until_first_nul(&event_raw_data[dyn_offset..]));
                    }
                }
                PerfFieldArray::DataLoc4 | PerfFieldArray::RelLoc4 => {
                    // 4-byte value is an offset/length pair leading to the real data.
                    let dyn32 = byte_reader.read_u32(&event_raw_data[raw_begin..]);
                    let dyn_size = (dyn32 >> 16) as usize;
                    let dyn_offset = (dyn32 & 0xFFFF) as usize
                        + if self.array == PerfFieldArray::RelLoc2 {
                            // offset is relative to end of field.
                            raw_end
                        } else {
                            0
                        };

                    if dyn_offset + dyn_size <= event_raw_data.len() {
                        return Some(&event_raw_data[dyn_offset..dyn_offset + dyn_size]);
                    }
                }
            }
        }

        return None;
    }

    /// Given the event's raw data (e.g. PerfSampleEventInfo::raw_data) and a `byte_reader`
    /// that indicates whether the event is big-endian or little-endian, return a
    /// [`PerfItemValue`] representing the field's type and data bytes. Returns an empty
    /// value (result.encoding() == Invalid) for out-of-bounds, i.e. if `event_raw_data` is
    /// too short for the expected position+length of the field's data bytes.
    ///
    /// Does not do any byte-swapping. This method uses `byte_reader` to resolve
    /// `data_loc` and `rel_loc` references, not to fix up the field data.
    pub fn get_field_value<'dat>(
        &self,
        event_raw_data: &'dat [u8],
        byte_reader: PerfByteReader,
    ) -> PerfItemValue<'dat> {
        let mut bytes;
        let mut check_str_len = self.deduced_encoding == FieldEncoding::ZStringChar8;

        let raw_begin = self.offset as usize;
        let raw_end = raw_begin + self.size as usize;

        // Loop used to simulate goto.
        #[allow(clippy::never_loop)]
        'VariableSize: loop {
            if raw_begin <= raw_end && raw_end <= event_raw_data.len() {
                match self.array {
                    PerfFieldArray::None | PerfFieldArray::Fixed => {
                        bytes = &event_raw_data[raw_begin..raw_end];
                        if check_str_len {
                            bytes = until_first_nul(bytes);
                        }

                        let array_count = self.deduced_array_count;
                        return PerfItemValue::new(
                            bytes,
                            PerfItemMetadata::new(
                                byte_reader,
                                self.deduced_encoding,
                                self.deduced_format,
                                !self.deduced_encoding.is_array(),
                                self.element_size(),
                                array_count,
                                0,
                            ),
                        );
                    }
                    PerfFieldArray::RestOfEvent => {
                        bytes = &event_raw_data[raw_begin..];
                        break 'VariableSize;
                    }
                    PerfFieldArray::DataLoc2 | PerfFieldArray::RelLoc2 => {
                        // 2-byte value is an offset leading to the real data, size is strlen.
                        let dyn_offset = byte_reader.read_u16(&event_raw_data[raw_begin..])
                            as usize
                            + if self.array == PerfFieldArray::RelLoc2 {
                                // offset is relative to end of field.
                                raw_end
                            } else {
                                0
                            };

                        if dyn_offset <= event_raw_data.len() {
                            bytes = until_first_nul(&event_raw_data[dyn_offset..]);
                            check_str_len = true;
                            break 'VariableSize;
                        }
                    }
                    PerfFieldArray::DataLoc4 | PerfFieldArray::RelLoc4 => {
                        // 4-byte value is an offset/length pair leading to the real data.
                        let dyn32 = byte_reader.read_u32(&event_raw_data[raw_begin..]);
                        let dyn_size = (dyn32 >> 16) as usize;
                        let dyn_offset = (dyn32 & 0xFFFF) as usize
                            + if self.array == PerfFieldArray::RelLoc2 {
                                // offset is relative to end of field.
                                raw_end
                            } else {
                                0
                            };

                        if dyn_offset + dyn_size <= event_raw_data.len() {
                            bytes = &event_raw_data[dyn_offset..dyn_offset + dyn_size];
                            break 'VariableSize;
                        }
                    }
                }
            }

            return PerfItemValue::new(&[], PerfItemMetadata::null());
        }

        // VariableSize

        if check_str_len {
            bytes = until_first_nul(bytes);
        }

        let element_size = self.element_size();
        let mask = element_size as usize - 1;
        if 0 != (bytes.len() & mask) {
            bytes = &bytes[..bytes.len() & !mask];
        }

        let array_count = if self.deduced_array_count != 0 {
            self.deduced_array_count
        } else {
            (bytes.len() >> self.element_size_shift) as u16
        };

        return PerfItemValue::new(
            bytes,
            PerfItemMetadata::new(
                byte_reader,
                self.deduced_encoding,
                self.deduced_format,
                !self.deduced_encoding.is_array(),
                element_size,
                array_count,
                0,
            ),
        );
    }

    fn set_hex_dump(&mut self) {
        self.deduced_encoding = FieldEncoding::StringLength16Char8;
        self.deduced_format = FieldFormat::HexBytes;
        self.deduced_array_count = 1;
        self.element_size_shift = u8::MAX;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TokenKind {
    None,
    Ident,       // e.g. MyFile
    Brackets,    // e.g. [...]
    Parentheses, // e.g. (...)
    String,      // e.g. "asdf"
    Punctuation, // e.g. *
}

struct Tokenizer<'a> {
    input: &'a str,
    input_pos: usize,
    kind: TokenKind,
    value_range: ops::Range<usize>,
}

impl<'a> Tokenizer<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            input,
            input_pos: 0,
            kind: TokenKind::None,
            value_range: 0..0,
        }
    }

    fn move_next(&mut self) {
        let input_bytes = self.input.as_bytes();
        let mut pos = self.input_pos;

        while pos < input_bytes.len() && input_bytes[pos] <= b' ' {
            pos += 1;
        }

        let start_pos = pos;

        let new_kind;
        if pos == input_bytes.len() {
            new_kind = TokenKind::None;
        } else if is_ident_start(input_bytes[pos]) {
            // Return identifier.
            pos += 1;
            while pos < input_bytes.len() && is_ident_continue(input_bytes[pos]) {
                pos += 1;
            }

            new_kind = TokenKind::Ident;
        } else {
            match input_bytes[pos] {
                b'\'' | b'\"' => {
                    // Return up to the closing quote.
                    pos = consume_string(pos + 1, input_bytes, input_bytes[pos]);
                    new_kind = TokenKind::String;
                }
                b'(' => {
                    // Return up to closing paren (allow nesting).
                    pos = consume_braced(pos + 1, input_bytes, b'(', b')');
                    new_kind = TokenKind::Parentheses;
                }
                b'[' => {
                    // Return up to closing brace (allow nesting).
                    pos = consume_braced(pos + 1, input_bytes, b'[', b']');
                    new_kind = TokenKind::Brackets;
                }
                _ => {
                    // Return single character token.
                    pos += 1;
                    new_kind = TokenKind::Punctuation;
                }
            }
        }

        self.input_pos = pos;
        self.value_range = start_pos..pos;
        self.kind = new_kind;
    }
}

/// Skips leading spaces and tabs. Parses as hex if leading "0x", decimal otherwise.
/// If no digits, returns None. Ignores overflow.
pub(crate) fn ascii_to_u32(chars: &[u8]) -> Option<u32> {
    let mut pos = 0;
    while pos < chars.len() && is_space_or_tab(chars[pos]) {
        pos += 1;
    }

    let mut any_digits = false;
    let mut value = 0;
    if chars.len() - pos > 2
        && chars[pos] == b'0'
        && (chars[pos + 1] == b'x' || chars[pos + 1] == b'X')
    {
        pos += 2; // Skip "0x".
        while pos < chars.len() {
            let ch = chars[pos] as char;
            match ch.to_digit(16) {
                Some(digit) => {
                    value = value * 16 + digit;
                }
                None => {
                    break;
                }
            }

            pos += 1;
            any_digits = true;
        }
    } else {
        while pos < chars.len() {
            let ch = chars[pos] as char;
            match ch.to_digit(10) {
                Some(digit) => {
                    value = value * 10 + digit;
                }
                None => {
                    break;
                }
            }

            pos += 1;
            any_digits = true;
        }
    }

    return if any_digits { Some(value) } else { None };
}
pub(crate) fn is_space_or_tab(c: u8) -> bool {
    c == b' ' || c == b'\t'
}

/// Given start_pos pointing after the opening quote, returns pos after the closing quote.
pub(crate) fn consume_string(start_pos: usize, bytes: &[u8], quote: u8) -> usize {
    let mut pos = start_pos;
    while pos < bytes.len() {
        let consumed = bytes[pos];
        pos += 1;

        if consumed == quote {
            break;
        } else if consumed == b'\\' {
            if pos >= bytes.len() {
                debug_eprintln!("EOF within '\\' escape");
                break; // Unexpected.
            }

            // Ignore whatever comes after the backslash, which
            // is significant if it is quote or '\\'.
            pos += 1;
        }
    }

    return pos;
}

// Given start_pos after the opening brace, returns position after the closing brace.
fn consume_braced(start_pos: usize, bytes: &[u8], open: u8, close: u8) -> usize {
    let mut pos = start_pos;
    let mut depth = 1;

    while pos < bytes.len() {
        let consumed = bytes[pos];
        pos += 1;

        if consumed == close {
            depth -= 1;
            if depth == 0 {
                break;
            }
        } else if consumed == open {
            depth += 1;
        }
    }

    return pos;
}

fn is_space_or_tab_or_semicolon(c: u8) -> bool {
    c == b' ' || c == b'\t' || c == b';'
}

fn is_ident_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_ident_continue(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

fn until_first_nul(bytes: &[u8]) -> &[u8] {
    let mut pos = 0;
    while pos < bytes.len() && bytes[pos] != 0 {
        pos += 1;
    }

    return &bytes[..pos];
}
