// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

use crate::PerfByteReader;
use eventheader_types::*;

/// Flags used when formatting a value as a string.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PerfConvertOptions(u32);

#[allow(non_upper_case_globals)]
impl PerfConvertOptions {
    /// Returns a `PerfConvertOptions` with the specified numeric value.
    pub const fn from_int(value: u32) -> Self {
        return Self(value);
    }

    /// Returns the numeric value corresponding to this `PerfConvertOptions` value.
    pub const fn as_int(self) -> u32 {
        return self.0;
    }

    /// Returns true if `self & flag != 0`.
    pub const fn has(self, flag: PerfConvertOptions) -> bool {
        return self.0 & flag.0 != 0;
    }

    /// Returns `self & flag`.
    pub const fn and(self, flag: PerfConvertOptions) -> PerfConvertOptions {
        return Self(self.0 & flag.0);
    }

    /// Returns `self | flag`.
    pub const fn or(self, flag: PerfConvertOptions) -> PerfConvertOptions {
        return Self(self.0 | flag.0);
    }

    /// Add spaces to the output, e.g. "Name": [ 1, 2, 3 ] instead of "Name":[1,2,3].
    pub const Space: Self = Self(0x01);

    /// When formatting with AppendJsonItemToAndMoveNextSibling, include the
    /// "Name": prefix for the root item.
    pub const RootName: Self = Self(0x02);

    /// When formatting with AppendJsonItemToAndMoveNextSibling, for items with a
    /// non-zero tag, add a tag suffix to the item's "Name": prefix, e.g.
    /// "Name;tag=0xNNNN": "ItemValue".
    pub const FieldTag: Self = Self(0x04);

    /// If set, float will format with "g9" (single-precision) or "g17" (double-precision).
    /// If unset, float will format with "g".
    pub const FloatExtraPrecision: Self = Self(0x10);

    /// If set, non-finite float will format as a string like "NaN" or "-Infinity".
    /// If unset, non-finite float will format as a null.
    pub const FloatNonFiniteAsString: Self = Self(0x20);

    /// If set, hex integer will format in JSON as a string like "0xF123".
    /// If unset, a hex integer will format in JSON as a decimal like 61731.
    pub const IntHexAsString: Self = Self(0x40);

    /// If set, boolean outside 0..1 will format as a string like "BOOL(-123)".
    /// If unset, boolean outside 0..1 will format as a number like -123.
    pub const BoolOutOfRangeAsString: Self = Self(0x80);

    /// If set, UnixTime within year 0001..9999 will format as a string like "2024-04-08T23:59:59Z".
    /// If unset, UnixTime within year 0001..9999 will format as a number like 1712620799.
    pub const UnixTimeWithinRangeAsString: Self = Self(0x100);

    /// If set, UnixTime64 outside year 0001..9999 will format as a string like "TIME(-62135596801)".
    /// If unset, UnixTime64 outside year 0001..9999 will format as a number like -62135596801.
    pub const UnixTimeOutOfRangeAsString: Self = Self(0x200);

    /// If set, Errno within 0..133 will format as a string like "ERRNO(0)" or "ENOENT(2)".
    /// If unset, Errno within 0..133 will format as a number like 0 or 2.
    pub const ErrnoKnownAsString: Self = Self(0x400);

    /// If set, Errno outside 0..133 will format as a string like "ERRNO(-1)".
    /// If unset, Errno outside 0..133 will format as a number like -1.
    pub const ErrnoUnknownAsString: Self = Self(0x800);

    /// For non-JSON string conversions: replace control characters with space.
    /// Conflicts with StringControlCharsJsonEscape.
    pub const StringControlCharsReplaceWithSpace: Self = Self(0x10000);

    /// For non-JSON string conversions: escape control characters using JSON-compatible
    /// escapes sequences, e.g. "\n" for newline or "\u0000" for NUL.
    /// Conflicts with StringControlCharsReplaceWithSpace.
    pub const StringControlCharsJsonEscape: Self = Self(0x20000);

    /// Mask for string control character flags.
    pub const StringControlCharsMask: Self =
        Self(Self::StringControlCharsReplaceWithSpace.0 | Self::StringControlCharsJsonEscape.0);

    /// Default flags.
    pub const Default: Self = Self(
        Self::Space.0
            | Self::RootName.0
            | Self::FieldTag.0
            | Self::FloatExtraPrecision.0
            | Self::FloatNonFiniteAsString.0
            | Self::IntHexAsString.0
            | Self::BoolOutOfRangeAsString.0
            | Self::UnixTimeWithinRangeAsString.0
            | Self::UnixTimeOutOfRangeAsString.0
            | Self::ErrnoKnownAsString.0
            | Self::ErrnoUnknownAsString.0
            | Self::StringControlCharsReplaceWithSpace.0,
    );

    /// All flags set.
    pub const All: Self = Self(!0u32);
}

/// Provides access to the metadata
/// of a perf event item. An item is a field of the event or an element of an
/// array field of the event.
///
/// The item may represent one of the following, determined by the
/// `Metadata.IsScalar` and `Metadata.TypeSize`
/// properties:
///
/// - **Simple scalar:** `IsScalar && TypeSize != 0`
///
///   Non-array field, or one element of an array field.
///   Value type is simple (fixed-size value).
///
///   `ElementCount` is always 1.
///
///   `Format` is significant and `StructFieldCount` should be ignored
///   (simple type is never `Struct`).
///
/// - **Complex scalar:** `IsScalar && TypeSize == 0`
///
///   Non-array field, or one element of an array field.
///   Value type is complex (variable-size or struct value).
///
///   `ElementCount` is always 1.
///
///   If `Encoding == Struct`, this is the beginning or end of a structure,
///   `Format` should be ignored, and `StructFieldCount` is significant.
///   Otherwise, this is a variable-length value, `Format` is significant,
///   and `StructFieldCount` should be ignored.
///
/// - **Simple array:** `!IsScalar && TypeSize != 0`
///
///   Array field (array-begin or array-end item).
///   Array element type is simple (fixed-size element).
///
///   `ElementCount` is the number of elements in the array.
///
///   `Format` is significant and `StructFieldCount` should be ignored
///   (simple type is never `Struct`).
///
/// - **Complex array:** `!IsScalar && TypeSize == 0`
///
///   Array field (array-begin or array-end item).
///   Array element type is complex (variable-size or struct element).
///
///   `ElementCount` is the number of elements in the array.
///
///   If `Encoding == Struct`, this is the beginning or end of an array of structures,
///   `Format` should be ignored, and `StructFieldCount` is significant.
///   Otherwise, this is an array of variable-length values, `Format` is significant,
///   and `StructFieldCount` should be ignored.
#[derive(Clone, Copy, Debug)]
pub struct PerfItemMetadata {
    element_count: u16,
    field_tag: u16,
    type_size: u8,
    encoding_and_array_flag_and_is_scalar: FieldEncoding,
    format: FieldFormat,
    byte_reader: PerfByteReader,
}

impl PerfItemMetadata {
    /// Initializes a new instance of the PerfItemMetadata struct.
    ///
    /// These are not normally created directly. You'll normally get instances of this struct from
    /// [EventHeaderEnumerator]`.item_metadata()` or indirectly from [EventHeaderEnumerator]`.item_info()`.
    ///
    /// - **byte_reader:**
    ///   Reader that is configured for the event data's byte order.
    ///
    /// - **encoding_and_array_flag:**
    ///   The field encoding, including the appropriate array flag if the field is an array element,
    ///   array-begin, or array-end. The chain flag must be unset.
    ///
    /// - **format:**
    ///   The field format. The chain flag must be unset.
    ///
    /// - **is_scalar:**
    ///   True if this represents a non-array value or a single element of an array.
    ///   False if this represents an array-begin or an array-end.
    ///
    /// - **type_size:**
    ///   For simple encodings (e.g. Value8, Value16, Value32, Value64, Value128),
    ///   this is the size of one element in bytes (1, 2, 4, 8, 16). For complex types
    ///   (e.g. Struct or string), this is 0.
    ///
    /// - **element_count:**
    ///   For array-begin or array-end, this is number of elements in the array.
    ///   For non-array or for array element, this is 1.
    ///   This may be 0 in the case of a variable-length array of length 0.
    ///
    /// - **field_tag:**
    ///   Field tag, or 0 if none.
    pub const fn new(
        byte_reader: PerfByteReader,
        encoding_and_array_flag: FieldEncoding,
        format: FieldFormat,
        is_scalar: bool,
        type_size: u8,
        element_count: u16,
        field_tag: u16,
    ) -> Self {
        // Chain flags must be masked-out by caller.
        debug_assert!(!encoding_and_array_flag.has_chain_flag());
        debug_assert!(!format.has_chain_flag());

        debug_assert!(encoding_and_array_flag.array_flags() != FieldEncoding::ArrayFlagMask);

        #[cfg(debug_assertions)]
        if is_scalar {
            // If scalar, elementCount must be 1.
            debug_assert!(element_count == 1);
        } else {
            // If not scalar, must be an array.
            debug_assert!(encoding_and_array_flag.is_array());
        }

        #[cfg(debug_assertions)]
        if matches!(
            encoding_and_array_flag.base_encoding(),
            FieldEncoding::Struct
        ) {
            debug_assert!(type_size == 0); // Structs are not simple types.
            debug_assert!(format.as_int() != 0); // No zero-length structs.
        }

        let is_scalar_flag = if is_scalar {
            FieldEncoding::ChainFlag
        } else {
            0
        };
        return Self {
            element_count,
            field_tag,
            type_size,
            encoding_and_array_flag_and_is_scalar: FieldEncoding::from_int(
                encoding_and_array_flag.as_int() | is_scalar_flag,
            ),
            format,
            byte_reader,
        };
    }

    /// For array-begin or array-end item, this is number of elements in the array.
    /// For non-array or for element of an array, this is 1.
    /// This may be 0 in the case of a variable-length array of length 0.
    pub const fn element_count(&self) -> u16 {
        self.element_count
    }

    /// Field tag, or 0 if none.
    pub const fn field_tag(&self) -> u16 {
        self.field_tag
    }

    /// For simple encodings (e.g. Value8, Value16, Value32, Value64, Value128),
    /// this is the size of one element in bytes (1, 2, 4, 8, 16). For complex types
    /// (e.g. Struct or string), this is 0.
    pub const fn type_size(&self) -> u8 {
        self.type_size
    }

    /// Item's underlying encoding. The encoding indicates how to determine the item's
    /// size. The encoding also implies a default formatting that should be used if
    /// the specified format is `Default` (0), unrecognized, or unsupported. The value
    /// returned by this property does not include any flags.
    pub const fn encoding(&self) -> FieldEncoding {
        self.encoding_and_array_flag_and_is_scalar.base_encoding()
    }

    /// Returns the field's `CArrayFlag` or `VArrayFlag` if the item represents an array-begin
    /// field, an array-end field, or an element within an array field.
    /// Returns 0 for a non-array item.
    pub const fn array_flag(&self) -> u8 {
        self.encoding_and_array_flag_and_is_scalar.array_flags()
    }

    /// Returns true if this item is a scalar (a non-array field or a single element of an array field).
    /// Returns false if this item is an array (an array-begin or an array-end item).
    pub const fn is_scalar(&self) -> bool {
        self.encoding_and_array_flag_and_is_scalar.has_chain_flag()
    }

    /// Returns true if this item represents an element within an array.
    /// Returns false if this item is a non-array field, an array-begin, or an array-end.
    pub const fn is_element(&self) -> bool {
        let enc = self.encoding_and_array_flag_and_is_scalar.as_int();
        // Return item_is_scalar && field_is_array
        return 0 != (enc & FieldEncoding::ChainFlag) && 0 != (enc & FieldEncoding::ArrayFlagMask);
    }

    /// Field's semantic type. May be `Default`.
    /// Meaningful only when `encoding() != Struct` (`format` is aliased with `struct_field_count`).
    pub const fn format(&self) -> FieldFormat {
        self.format
    }

    /// Number of fields in the struct. Should never be 0.
    /// Meaningful only when `encoding() == Struct` (`struct_field_count` is aliased with `format`).
    pub const fn struct_field_count(&self) -> u8 {
        self.format.as_int()
    }

    /// A [`PerfByteReader`] that can be used to fix the byte order of this item's data.
    /// This is the same as `PerfByteReader::new(self.source_big_endian())`.
    pub const fn byte_reader(&self) -> PerfByteReader {
        self.byte_reader
    }

    /// True if this item's data uses big-endian byte order.
    /// This is the same as `self.byte_reader().source_big_endian()`.
    pub const fn source_big_endian(&self) -> bool {
        self.byte_reader.source_big_endian()
    }
}

/// Provides access to the metadata
/// and content
/// of a perf event item. An item is a field of the event or an element of an
/// array field of the event.
///
/// The item may represent one of the following, determined by the
/// `Metadata.IsScalar` and `Metadata.TypeSize`
/// properties:
///
/// - **Simple scalar:** `IsScalar && TypeSize != 0`
///
///   Non-array field, or one element of an array field.
///   Value type is simple (fixed-size value).
///
///   `ElementCount` is always 1.
///
///   `Format` is significant and `StructFieldCount` should be ignored
///   (simple type is never `Struct`).
///
///   `Bytes` contains the field's value and
///   `Bytes.Length == TypeSize`,
///   e.g. for a `Value32`, `TypeSize == 4` and `Bytes.Length == 4`.
///
/// - **Complex scalar:** `IsScalar && TypeSize == 0`
///
///   Non-array field, or one element of an array field.
///   Value type is complex (variable-size or struct value).
///
///   `ElementCount` is always 1.
///
///   If `Encoding == Struct`, this is the beginning or end of a structure,
///   `Format` should be ignored, and `StructFieldCount` is significant.
///   Otherwise, this is a variable-length value, `Format` is significant,
///   and `StructFieldCount` should be ignored.
///
///   If `Encoding == Struct` then `Bytes` will be empty and you should use
///   [`EventHeaderEnumerator`]`.MoveNext()` to visit the struct's member fields.
///   Otherwise, `Bytes` will contain the field's variable-length value without any length
///   prefix or nul-termination suffix.
///
/// - **Simple array:** `!IsScalar && TypeSize != 0`
///
///   Array field (array-begin or array-end item).
///   Array element type is simple (fixed-size element).
///
///   `ElementCount` is the number of elements in the array.
///
///   `Format` is significant and `StructFieldCount` should be ignored
///   (simple type is never `Struct`).
///
///   For array-end, `Bytes` will be empty.
///
///   For array-begin, `Bytes` contains the field's values and
///   `Bytes.Length == TypeSize * ElementCount`,
///   e.g. for a `Value32`, `TypeSize == 4` and `Bytes.Length == 4 * ElementCount`.
///   You may use [`EventHeaderEnumerator`]`.MoveNext()` to visit the array elements,
///   or you may process the array values directly and then use
///   [`EventHeaderEnumerator`]`.MoveNextSibling()` to skip the array elements.
///
/// - **Complex array:** `!IsScalar && TypeSize == 0`
///
///   Array field (array-begin or array-end item).
///   Array element type is complex (variable-size or struct element).
///
///   `ElementCount` is the number of elements in the array.
///
///   If `Encoding == Struct`, this is the beginning or end of an array of structures,
///   `Format` should be ignored, and `StructFieldCount` is significant.
///   Otherwise, this is an array of variable-length values, `Format` is significant,
///   and `StructFieldCount` should be ignored.
///
///   `Bytes` will be empty. Use [`EventHeaderEnumerator`]`.MoveNext()`
///   to visit the array elements.
#[derive(Clone, Copy, Debug)]
pub struct PerfItemValue<'a> {
    bytes: &'a [u8],
    metadata: PerfItemMetadata,
}

impl<'a> PerfItemValue<'a> {
    /// Initializes a new instance of the `PerfItemValue` struct.
    /// These are not normally created directly. You'll normally get instances of this struct from
    /// [`EventHeaderEnumerator`]`.item_info()`.
    pub const fn new(bytes: &'a [u8], metadata: PerfItemMetadata) -> Self {
        #[cfg(debug_assertions)]
        if metadata.type_size != 0 && !bytes.is_empty() {
            debug_assert!(
                bytes.len() == metadata.type_size as usize * metadata.element_count as usize
            );
        }

        #[cfg(debug_assertions)]
        if metadata.encoding().as_int() == FieldEncoding::Struct.as_int() {
            debug_assert!(bytes.is_empty());
        }

        return Self { bytes, metadata };
    }

    /// The content of this item, in event byte order.
    /// This may be empty for a complex item such as a struct, or an array
    /// of variable-size elements, in which case you must access the individual
    /// sub-items using the event's enumerator.
    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    /// The metadata (type, endian, tag) of this item.
    pub fn metadata(&self) -> PerfItemMetadata {
        self.metadata
    }

    /// A [`PerfByteReader`] that can be used to fix the byte order of this item's data.
    /// This is the same as `self.metadata().byte_reader()`.
    pub fn byte_reader(&self) -> PerfByteReader {
        self.metadata.byte_reader()
    }

    /// True if this item's data uses big-endian byte order.
    /// This is the same as `self.byte_reader().source_big_endian()`.
    pub fn source_big_endian(&self) -> bool {
        self.metadata.source_big_endian()
    }
}
