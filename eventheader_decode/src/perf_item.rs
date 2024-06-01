// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

use crate::PerfByteReader;
use eventheader_types::*;

use core::array;
use core::fmt;
use core::net;

use crate::_internal as internal;

/// Flags used when formatting a value as a string.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PerfConvertOptions(u32);

/// Encoding of a string field of an event.
pub enum PerfTextEncoding {
    /// Corresponds to [`FieldFormat::String8`], i.e. "unspecified single-byte character set",
    /// generally decoded as Latin1 (ISO-8859-1) or Windows-1252.
    Latin1,

    /// UTF-8 string.
    Utf8,

    /// UTF-16 string, big-endian byte order.
    Utf16BE,

    /// UTF-16 string, little-endian byte order.
    Utf16LE,

    /// UTF-32 string, big-endian byte order.
    Utf32BE,

    /// UTF-32 string, little-endian byte order.
    Utf32LE,
}

impl PerfTextEncoding {
    /// Returns `(Option<PerfTextEncoding>, bom_size)` corresponding to the BOM at the start of
    /// the given bytes. If no BOM is present, returns `(None, 0)`.
    pub fn from_bom(bytes: &[u8]) -> (Option<Self>, u8) {
        let len = bytes.len();
        if len >= 4 && bytes[0] == 0x00 && bytes[1] == 0x00 && bytes[2] == 0xFE && bytes[3] == 0xFF
        {
            return (Some(Self::Utf32BE), 4);
        } else if len >= 4
            && bytes[0] == 0xFF
            && bytes[1] == 0xFE
            && bytes[2] == 0x00
            && bytes[3] == 0x00
        {
            return (Some(Self::Utf32LE), 4);
        } else if len >= 2 && bytes[0] == 0xFE && bytes[1] == 0xFF {
            return (Some(Self::Utf16BE), 2);
        } else if len >= 2 && bytes[0] == 0xFF && bytes[1] == 0xFE {
            return (Some(Self::Utf16LE), 2);
        } else if len >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
            return (Some(Self::Utf8), 3);
        } else {
            return (None, 0);
        }
    }
}

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
            encoding_and_array_flag.without_flags(),
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
        self.encoding_and_array_flag_and_is_scalar.without_flags()
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
pub struct PerfItemValue<'dat> {
    bytes: &'dat [u8],
    metadata: PerfItemMetadata,
}

impl<'dat> PerfItemValue<'dat> {
    /// Initializes a new instance of the `PerfItemValue` struct.
    /// These are not normally created directly. You'll normally get instances of this struct from
    /// [`EventHeaderEnumerator`]`.item_info()`.
    pub const fn new(bytes: &'dat [u8], metadata: PerfItemMetadata) -> Self {
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
    pub fn bytes(&self) -> &'dat [u8] {
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

    /// For [`FieldEncoding::Value8`]: gets a 1-byte array starting at offset `index * 1`.
    pub fn to_u8x1(&self, index: usize) -> &'dat [u8; 1] {
        array::from_ref(&self.bytes[index])
    }

    /// For [`FieldEncoding::Value16`]: gets a 2-byte array starting at offset `index * 2`.
    pub fn to_u8x2(&self, index: usize) -> &'dat [u8; 2] {
        const SIZE: usize = 2;
        self.bytes[index * SIZE..index * SIZE + SIZE]
            .try_into()
            .unwrap()
    }

    /// For [`FieldEncoding::Value32`]: gets a 4-byte array starting at offset `index * 4`.
    pub fn to_u8x4(&self, index: usize) -> &'dat [u8; 4] {
        const SIZE: usize = 4;
        self.bytes[index * SIZE..index * SIZE + SIZE]
            .try_into()
            .unwrap()
    }

    /// For [`FieldEncoding::Value64`]: gets a 8-byte array starting at offset `index * 8`.
    pub fn to_u8x8(&self, index: usize) -> &'dat [u8; 8] {
        const SIZE: usize = 8;
        self.bytes[index * SIZE..index * SIZE + SIZE]
            .try_into()
            .unwrap()
    }

    /// For [`FieldEncoding::Value128`]: gets a 16-byte array starting at offset `index * 16`.
    pub fn to_u8x16(&self, index: usize) -> &'dat [u8; 16] {
        const SIZE: usize = 16;
        self.bytes[index * SIZE..index * SIZE + SIZE]
            .try_into()
            .unwrap()
    }

    /// For [`FieldEncoding::Value8`]: gets a `u8` value starting at offset `index * 1`.
    pub fn to_u8(&self, index: usize) -> u8 {
        self.bytes[index]
    }

    /// For [`FieldEncoding::Value8`]: gets an `i8` value starting at offset `index * 1`.
    pub fn to_i8(&self, index: usize) -> i8 {
        self.bytes[index] as i8
    }

    /// For [`FieldEncoding::Value16`]: gets a `u16` value starting at offset `index * 2`.
    pub fn to_u16(&self, index: usize) -> u16 {
        self.metadata.byte_reader.read_u16(&self.bytes[index * 2..])
    }

    /// For [`FieldEncoding::Value16`]: gets an `i16` value starting at offset `index * 2`.
    pub fn to_i16(&self, index: usize) -> i16 {
        self.metadata.byte_reader.read_i16(&self.bytes[index * 2..])
    }

    /// For [`FieldEncoding::Value32`]: gets a `u32` value starting at offset `index * 4`.
    pub fn to_u32(&self, index: usize) -> u32 {
        self.metadata.byte_reader.read_u32(&self.bytes[index * 4..])
    }

    /// For [`FieldEncoding::Value32`]: gets an `i32` value starting at offset `index * 4`.
    pub fn to_i32(&self, index: usize) -> i32 {
        self.metadata.byte_reader.read_i32(&self.bytes[index * 4..])
    }

    /// For [`FieldEncoding::Value64`]: gets a `u64` value starting at offset `index * 8`.
    pub fn to_u64(&self, index: usize) -> u64 {
        self.metadata.byte_reader.read_u64(&self.bytes[index * 8..])
    }

    /// For [`FieldEncoding::Value64`]: gets an `i64` value starting at offset `index * 8`.
    pub fn to_i64(&self, index: usize) -> i64 {
        self.metadata.byte_reader.read_i64(&self.bytes[index * 8..])
    }

    /// For [`FieldEncoding::Value32`]: gets an `f32` value starting at offset `index * 4`.
    pub fn to_f32(&self, index: usize) -> f32 {
        self.metadata.byte_reader.read_f32(&self.bytes[index * 4..])
    }

    /// For [`FieldEncoding::Value64`]: gets an `f64` value starting at offset `index * 8`.
    pub fn to_f64(&self, index: usize) -> f64 {
        self.metadata.byte_reader.read_f64(&self.bytes[index * 8..])
    }

    /// For [`FieldEncoding::Value128`]: gets a big-endian [`Guid`] value starting at offset `index * 16`.
    pub fn to_guid(&self, index: usize) -> Guid {
        const SIZE: usize = 16;
        Guid::from_bytes_be(
            &self.bytes[index * SIZE..index * SIZE + SIZE]
                .try_into()
                .unwrap(),
        )
    }

    /// For [`FieldEncoding::Value16`]: gets a big-endian `u16` value starting at offset `index * 2`.
    pub fn to_port(&self, index: usize) -> u16 {
        const SIZE: usize = 2;
        u16::from_be_bytes(
            self.bytes[index * SIZE..index * SIZE + SIZE]
                .try_into()
                .unwrap(),
        )
    }

    /// For [`FieldEncoding::Value32`]: gets an [`net::Ipv4Addr`] value starting at offset `index * 4`.
    pub fn to_ipv4(&self, index: usize) -> net::Ipv4Addr {
        const SIZE: usize = 4;
        let bits: [u8; SIZE] = self.bytes[index * SIZE..index * SIZE + SIZE]
            .try_into()
            .unwrap();
        net::Ipv4Addr::new(bits[0], bits[1], bits[2], bits[3])
    }

    /// For [`FieldEncoding::Value128`]: gets an [`net::Ipv6Addr`] value starting at offset `index * 16`.
    pub fn to_ipv6(&self, index: usize) -> net::Ipv6Addr {
        const SIZE: usize = 16;
        let bits: &[u8; SIZE] = self.bytes[index * SIZE..index * SIZE + SIZE]
            .try_into()
            .unwrap();
        net::Ipv6Addr::new(
            u16::from_be_bytes(bits[0..2].try_into().unwrap()),
            u16::from_be_bytes(bits[2..4].try_into().unwrap()),
            u16::from_be_bytes(bits[4..6].try_into().unwrap()),
            u16::from_be_bytes(bits[6..8].try_into().unwrap()),
            u16::from_be_bytes(bits[8..10].try_into().unwrap()),
            u16::from_be_bytes(bits[10..12].try_into().unwrap()),
            u16::from_be_bytes(bits[12..14].try_into().unwrap()),
            u16::from_be_bytes(bits[14..16].try_into().unwrap()),
        )
    }

    /// For [`FieldEncoding::Value32`]: gets an `i32` value starting at offset `index * 4`.
    pub fn to_time32(&self, index: usize) -> i32 {
        self.metadata.byte_reader.read_i32(&self.bytes[index * 4..])
    }

    /// For [`FieldEncoding::Value64`]: gets an `i64` value starting at offset `index * 8`.
    pub fn to_time64(&self, index: usize) -> i64 {
        self.metadata.byte_reader.read_i64(&self.bytes[index * 8..])
    }

    /// Interprets the value as a string and returns the string's encoded bytes along
    /// with the encoding to use to convert the bytes to a string. The encoding is
    /// determined based on the field's `format`, `encoding`, and a BOM (if present) in
    /// the value bytes. If a BOM was detected, the returned encoded bytes will NOT
    /// include the BOM.
    pub fn to_string_bytes(&self) -> (&'dat [u8], PerfTextEncoding) {
        // First, check `format` for non-UTF and UTF-with-BOM cases.
        match self.metadata.format {
            FieldFormat::String8 => return (self.bytes, PerfTextEncoding::Latin1),
            FieldFormat::StringUtfBom | FieldFormat::StringXml | FieldFormat::StringJson => {
                let from_bom = PerfTextEncoding::from_bom(self.bytes);
                if let Some(enc) = from_bom.0 {
                    return (&self.bytes[from_bom.1 as usize..], enc);
                }
            }
            _ => {}
        }

        // No BOM but assumed to be UTF. Determine text encoding from element size.
        let enc = match self.metadata.encoding() {
            FieldEncoding::Value8
            | FieldEncoding::ZStringChar8
            | FieldEncoding::StringLength16Char8 => PerfTextEncoding::Utf8,

            FieldEncoding::Value16
            | FieldEncoding::ZStringChar16
            | FieldEncoding::StringLength16Char16 => {
                if self.metadata.source_big_endian() {
                    PerfTextEncoding::Utf16BE
                } else {
                    PerfTextEncoding::Utf16LE
                }
            }

            FieldEncoding::Value32
            | FieldEncoding::ZStringChar32
            | FieldEncoding::StringLength16Char32 => {
                if self.metadata.source_big_endian() {
                    PerfTextEncoding::Utf32BE
                } else {
                    PerfTextEncoding::Utf32LE
                }
            }

            // Invalid, Struct, Value64, Value128: probably garbage, but decode as Latin1.
            _ => PerfTextEncoding::Latin1,
        };

        return (self.bytes, enc);
    }

    /// Interprets the value as an encoded string. Decodes the string using the detected
    /// encoding and writes the decoded string to the provided writer. The encoding is
    /// determined based on the field's `format`, `encoding`, and a BOM (if present) in
    /// the value bytes. The BOM (if present) will NOT be written to the writer.
    ///
    /// - For UTF-8, invalid UTF-8 byte sequences will be treated as Latin-1 sequences.
    /// - For UTF-16 and UTF-32, invalid code units will be replaced with the Unicode
    ///   replacement character (U+FFFD).
    pub fn write_string_to<W: fmt::Write>(&self, writer: &mut W) -> fmt::Result {
        let (bytes, enc) = self.to_string_bytes();
        return match enc {
            PerfTextEncoding::Latin1 => internal::CharsFromLatin1::new(bytes).write_to(writer),
            PerfTextEncoding::Utf8 => {
                internal::CharsFromUtf8WithLatin1Fallback::new(bytes).write_to(writer)
            }
            PerfTextEncoding::Utf16BE => internal::CharsFromUtf16BE::new(bytes).write_to(writer),
            PerfTextEncoding::Utf16LE => internal::CharsFromUtf16LE::new(bytes).write_to(writer),
            PerfTextEncoding::Utf32BE => internal::CharsFromUtf32BE::new(bytes).write_to(writer),
            PerfTextEncoding::Utf32LE => internal::CharsFromUtf32LE::new(bytes).write_to(writer),
        };
    }

    /// Writes a string representation of this value to the writer.
    ///
    /// If this value is a scalar, this behaves like `write_scalar_to`.
    ///
    /// If thie value is an array, this behaves like `write_simple_array_to`.
    pub fn write_to<W: fmt::Write>(
        &self,
        writer: &mut W,
        convert_options: PerfConvertOptions,
    ) -> fmt::Result {
        if self.metadata.is_scalar() {
            return self.write_scalar_to(writer, convert_options);
        } else {
            return self.write_simple_array_to(writer, convert_options);
        }
    }

    /// Interprets this as a scalar and writes a string representation to the writer.
    ///
    /// For example:
    ///
    /// - If the value is a decimal integer or a finite float, writes a number like `123` or `-123.456`.
    /// - If the value is a boolean, writes `false` (for 0) or `true` (for 1). For values other
    ///   than 0 or 1, writes a string like `BOOL(-123)` if `convert_options` has
    ///   [`PerfConvertOptions::BoolOutOfRangeAsString`], or a string like `-123` otherwise.
    /// - If the value is a string, control characters (char values 0..31) are
    ///   filtered based on the flags in `convert_options` (kept, replaced with space,
    ///   or JSON-escaped).
    /// - If the value is a struct, writes `Struct[N]`, where `N` is the number of fields in the struct.
    pub fn write_scalar_to<W: fmt::Write>(
        &self,
        writer: &mut W,
        convert_options: PerfConvertOptions,
    ) -> fmt::Result {
        debug_assert!(self.metadata.type_size as usize <= self.bytes.len());

        match self.metadata.encoding() {
            FieldEncoding::Invalid => return writer.write_str("null"),
            FieldEncoding::Struct => {
                return write!(writer, "Struct[{}]", self.metadata.struct_field_count())
            }
            FieldEncoding::Value8 => return self.write_value8_to(writer, convert_options, 0),
            FieldEncoding::Value16 => return self.write_value16_to(writer, convert_options, 0),
            FieldEncoding::Value32 => return self.write_value32_to(writer, convert_options, 0),
            FieldEncoding::Value64 => return self.write_value64_to(writer, convert_options, 0),
            FieldEncoding::Value128 => return self.write_value128_to(writer, convert_options, 0),
            FieldEncoding::ZStringChar8 | FieldEncoding::StringLength16Char8 => {
                match self.metadata.format {
                    FieldFormat::HexBytes => return Self::write_hexbytes_to(writer, self.bytes),
                    FieldFormat::String8 => {
                        return Self::write_latin1_with_control_chars_to(
                            writer,
                            convert_options,
                            self.bytes,
                        )
                    }
                    FieldFormat::StringUtfBom
                    | FieldFormat::StringXml
                    | FieldFormat::StringJson => {
                        if let (Some(encoding), bom_len) = PerfTextEncoding::from_bom(self.bytes) {
                            return Self::write_string_with_control_chars_to(
                                writer,
                                convert_options,
                                &self.bytes[bom_len as usize..],
                                encoding,
                            );
                        } else {
                            return Self::write_string_with_control_chars_to(
                                writer,
                                convert_options,
                                self.bytes,
                                PerfTextEncoding::Utf8,
                            );
                        }
                    }
                    _ => {
                        return Self::write_string_with_control_chars_to(
                            writer,
                            convert_options,
                            self.bytes,
                            PerfTextEncoding::Utf8,
                        );
                    }
                }
            }
            _ => return write!(writer, "Encoding[{}]", self.metadata.encoding()),
        };
    }

    /// Interprets this as the beginning of an array of simple type.
    /// Converts the specified element of the array to a string and writes it to the writer.
    ///
    /// Requires `type_size != 0` (can only format fixed-length types).
    ///
    /// Requires `index <= bytes.len() / type_size`.
    ///
    /// The element is formatted as described for `write_scalar_to`.
    pub fn write_simple_element_to<W: fmt::Write>(
        &self,
        writer: &mut W,
        index: usize,
        convert_options: PerfConvertOptions,
    ) -> fmt::Result {
        debug_assert!(self.metadata.type_size != 0);
        debug_assert!(index <= 65535);
        debug_assert!((index + 1) * self.metadata.type_size as usize <= self.bytes.len());

        match self.metadata.encoding() {
            FieldEncoding::Value8 => return self.write_value8_to(writer, convert_options, index),
            FieldEncoding::Value16 => return self.write_value16_to(writer, convert_options, index),
            FieldEncoding::Value32 => return self.write_value32_to(writer, convert_options, index),
            FieldEncoding::Value64 => return self.write_value64_to(writer, convert_options, index),
            FieldEncoding::Value128 => {
                return self.write_value128_to(writer, convert_options, index)
            }
            _ => return write!(writer, "Encoding[{}]", self.metadata.encoding()),
        }
    }

    /// Interprets this as the beginning of an array of simple type.
    /// Converts this to a comma-separated list of items and writes it to the writer.
    ///
    /// Each array element is formatted as described for `write_scalar_to`.
    ///
    /// If this is an array-begin or array-end of complex type, this will simply write
    /// `Array[N]`, where `N` is the number of elements in the array.
    pub fn write_simple_array_to<W: fmt::Write>(
        &self,
        writer: &mut W,
        convert_options: PerfConvertOptions,
    ) -> fmt::Result {
        debug_assert!(self.metadata.type_size != 0);

        let separator = if convert_options.has(PerfConvertOptions::Space) {
            ", "
        } else {
            ","
        };

        match self.metadata.encoding() {
            FieldEncoding::Value8 => {
                let count = self.bytes.len();
                for i in 0..count {
                    if i > 0 {
                        writer.write_str(separator)?;
                    }
                    self.write_value8_to(writer, convert_options, i)?;
                }
            }
            FieldEncoding::Value16 => {
                let count = self.bytes.len() / 2;
                for i in 0..count {
                    if i > 0 {
                        writer.write_str(separator)?;
                    }
                    self.write_value16_to(writer, convert_options, i)?;
                }
            }
            FieldEncoding::Value32 => {
                let count = self.bytes.len() / 4;
                for i in 0..count {
                    if i > 0 {
                        writer.write_str(separator)?;
                    }
                    self.write_value32_to(writer, convert_options, i)?;
                }
            }
            FieldEncoding::Value64 => {
                let count = self.bytes.len() / 8;
                for i in 0..count {
                    if i > 0 {
                        writer.write_str(separator)?;
                    }
                    self.write_value64_to(writer, convert_options, i)?;
                }
            }
            FieldEncoding::Value128 => {
                let count = self.bytes.len() / 16;
                for i in 0..count {
                    if i > 0 {
                        writer.write_str(separator)?;
                    }
                    self.write_value128_to(writer, convert_options, i)?;
                }
            }
            _ => return write!(writer, "Encoding[{}]", self.metadata.encoding()),
        }

        return Ok(());
    }

    /// Writes a JSON representation of this value to the writer.
    ///
    /// If this value is a scalar, this behaves like `write_json_scalar_to`.
    ///
    /// If thie value is an array, this behaves like `write_json_simple_array_to`.
    pub fn write_json_to<W: fmt::Write>(
        &self,
        writer: &mut W,
        convert_options: PerfConvertOptions,
    ) -> fmt::Result {
        if self.metadata.is_scalar() {
            return self.write_json_scalar_to(writer, convert_options);
        } else {
            return self.write_json_simple_array_to(writer, convert_options);
        }
    }

    /// Interprets this as a scalar and writes a JSON representation to the writer.
    ///
    /// If this value is a struct, the value will be written as `{}`.
    /// Structs need to be processed by the enumerator.
    pub fn write_json_scalar_to<W: fmt::Write>(
        &self,
        writer: &mut W,
        convert_options: PerfConvertOptions,
    ) -> fmt::Result {
        debug_assert!(self.metadata.type_size as usize <= self.bytes.len());

        match self.metadata.encoding() {
            FieldEncoding::Invalid => return writer.write_str("null"),
            FieldEncoding::Struct => {
                return writer.write_str("{}");
            }
            FieldEncoding::Value8 => return self.write_json_value8_to(writer, convert_options, 0),
            FieldEncoding::Value16 => {
                return self.write_json_value16_to(writer, convert_options, 0)
            }
            FieldEncoding::Value32 => {
                return self.write_json_value32_to(writer, convert_options, 0)
            }
            FieldEncoding::Value64 => {
                return self.write_json_value64_to(writer, convert_options, 0)
            }
            FieldEncoding::Value128 => {
                return self.write_json_value128_to(writer, convert_options, 0)
            }
            FieldEncoding::ZStringChar8 | FieldEncoding::StringLength16Char8 => {
                match self.metadata.format {
                    FieldFormat::HexBytes => {
                        return Self::write_json_hexbytes_to(writer, self.bytes)
                    }
                    FieldFormat::String8 => return Self::write_json_latin1_to(writer, self.bytes),
                    FieldFormat::StringUtfBom
                    | FieldFormat::StringXml
                    | FieldFormat::StringJson => {
                        if let (Some(encoding), bom_len) = PerfTextEncoding::from_bom(self.bytes) {
                            return Self::write_json_string_to(
                                writer,
                                &self.bytes[bom_len as usize..],
                                encoding,
                            );
                        } else {
                            return Self::write_json_string_to(
                                writer,
                                self.bytes,
                                PerfTextEncoding::Utf8,
                            );
                        }
                    }
                    _ => {
                        return Self::write_json_string_to(
                            writer,
                            self.bytes,
                            PerfTextEncoding::Utf8,
                        );
                    }
                }
            }
            _ => return write!(writer, "\"Encoding[{}]\"", self.metadata.encoding()),
        };
    }

    /// Interprets this as the beginning of an array of simple type.
    /// Converts the specified element of the array to JSON and writes it to the writer.
    ///
    /// Requires `type_size != 0` (can only format fixed-length types).
    ///
    /// Requires `index <= bytes.len() / type_size`.
    ///
    /// The element is formatted as described for `write_json_scalar_to`.
    pub fn write_json_simple_element_to<W: fmt::Write>(
        &self,
        writer: &mut W,
        index: usize,
        convert_options: PerfConvertOptions,
    ) -> fmt::Result {
        debug_assert!(self.metadata.type_size != 0);
        debug_assert!(index <= 65535);
        debug_assert!((index + 1) * self.metadata.type_size as usize <= self.bytes.len());

        match self.metadata.encoding() {
            FieldEncoding::Value8 => {
                return self.write_json_value8_to(writer, convert_options, index)
            }
            FieldEncoding::Value16 => {
                return self.write_json_value16_to(writer, convert_options, index)
            }
            FieldEncoding::Value32 => {
                return self.write_json_value32_to(writer, convert_options, index)
            }
            FieldEncoding::Value64 => {
                return self.write_json_value64_to(writer, convert_options, index)
            }
            FieldEncoding::Value128 => {
                return self.write_json_value128_to(writer, convert_options, index)
            }
            _ => return write!(writer, "\"Encoding[{}]\"", self.metadata.encoding()),
        }
    }

    /// Interprets this as the beginning of an array of simple type.
    /// Converts this to a JSON array and writes it to the writer.
    ///
    /// Each array element is formatted as described for `write_json_scalar_to`.
    ///
    /// If this value is an array of complex type, the value will be written as `[]`.
    /// Complex arrays need to be processed by the enumerator.
    pub fn write_json_simple_array_to<W: fmt::Write>(
        &self,
        writer: &mut W,
        convert_options: PerfConvertOptions,
    ) -> fmt::Result {
        debug_assert!(self.metadata.type_size != 0);

        let space = convert_options.has(PerfConvertOptions::Space);

        writer.write_str("[")?;

        match self.metadata.encoding() {
            FieldEncoding::Value8 => {
                let count = self.bytes.len();
                for i in 0..count {
                    if i > 0 {
                        writer.write_str(",")?;
                    }
                    if space {
                        writer.write_str(" ")?;
                    }
                    self.write_json_value8_to(writer, convert_options, i)?;
                }
            }
            FieldEncoding::Value16 => {
                let count = self.bytes.len() / 2;
                for i in 0..count {
                    if i > 0 {
                        writer.write_str(",")?;
                    }
                    if space {
                        writer.write_str(" ")?;
                    }
                    self.write_json_value16_to(writer, convert_options, i)?;
                }
            }
            FieldEncoding::Value32 => {
                let count = self.bytes.len() / 4;
                for i in 0..count {
                    if i > 0 {
                        writer.write_str(",")?;
                    }
                    if space {
                        writer.write_str(" ")?;
                    }
                    self.write_json_value32_to(writer, convert_options, i)?;
                }
            }
            FieldEncoding::Value64 => {
                let count = self.bytes.len() / 8;
                for i in 0..count {
                    if i > 0 {
                        writer.write_str(",")?;
                    }
                    if space {
                        writer.write_str(" ")?;
                    }
                    self.write_json_value64_to(writer, convert_options, i)?;
                }
            }
            FieldEncoding::Value128 => {
                let count = self.bytes.len() / 16;
                for i in 0..count {
                    if i > 0 {
                        writer.write_str(",")?;
                    }
                    if space {
                        writer.write_str(" ")?;
                    }
                    self.write_json_value128_to(writer, convert_options, i)?;
                }
            }
            _ => return write!(writer, "\"Encoding[{}]\"", self.metadata.encoding()),
        }

        return if space {
            writer.write_str(" ]")
        } else {
            writer.write_str("]")
        };
    }

    fn write_value8_to<W: fmt::Write>(
        &self,
        writer: &mut W,
        convert_options: PerfConvertOptions,
        index: usize,
    ) -> fmt::Result {
        return write!(writer, "{}", self.to_u8(index));
    }

    fn write_json_value8_to<W: fmt::Write>(
        &self,
        writer: &mut W,
        convert_options: PerfConvertOptions,
        index: usize,
    ) -> fmt::Result {
        return write!(writer, "{}", self.to_u8(index));
    }

    fn write_value16_to<W: fmt::Write>(
        &self,
        writer: &mut W,
        convert_options: PerfConvertOptions,
        index: usize,
    ) -> fmt::Result {
        return write!(writer, "{}", self.to_u16(index));
    }

    fn write_json_value16_to<W: fmt::Write>(
        &self,
        writer: &mut W,
        convert_options: PerfConvertOptions,
        index: usize,
    ) -> fmt::Result {
        return write!(writer, "{}", self.to_u16(index));
    }

    fn write_value32_to<W: fmt::Write>(
        &self,
        writer: &mut W,
        convert_options: PerfConvertOptions,
        index: usize,
    ) -> fmt::Result {
        return write!(writer, "{}", self.to_u32(index));
    }

    fn write_json_value32_to<W: fmt::Write>(
        &self,
        writer: &mut W,
        convert_options: PerfConvertOptions,
        index: usize,
    ) -> fmt::Result {
        return write!(writer, "{}", self.to_u32(index));
    }

    fn write_value64_to<W: fmt::Write>(
        &self,
        writer: &mut W,
        convert_options: PerfConvertOptions,
        index: usize,
    ) -> fmt::Result {
        return write!(writer, "{}", self.to_u64(index));
    }

    fn write_json_value64_to<W: fmt::Write>(
        &self,
        writer: &mut W,
        convert_options: PerfConvertOptions,
        index: usize,
    ) -> fmt::Result {
        return write!(writer, "{}", self.to_u64(index));
    }

    fn write_value128_to<W: fmt::Write>(
        &self,
        writer: &mut W,
        convert_options: PerfConvertOptions,
        index: usize,
    ) -> fmt::Result {
        return write!(writer, "{:?}", self.to_u8x16(index));
    }

    fn write_json_value128_to<W: fmt::Write>(
        &self,
        writer: &mut W,
        convert_options: PerfConvertOptions,
        index: usize,
    ) -> fmt::Result {
        return write!(writer, "{:?}", self.to_u8x16(index));
    }

    fn write_latin1_with_control_chars_to<W: fmt::Write>(
        writer: &mut W,
        convert_options: PerfConvertOptions,
        bytes: &[u8],
    ) -> fmt::Result {
        return internal::CharsFromLatin1::new(bytes).write_to(writer);
    }

    fn write_json_latin1_to<W: fmt::Write>(writer: &mut W, bytes: &[u8]) -> fmt::Result {
        return internal::CharsFromLatin1::new(bytes).write_to(writer);
    }

    fn write_string_with_control_chars_to<W: fmt::Write>(
        writer: &mut W,
        convert_options: PerfConvertOptions,
        bytes: &[u8],
        encoding: PerfTextEncoding,
    ) -> fmt::Result {
        match encoding {
            PerfTextEncoding::Latin1 => {
                return internal::CharsFromLatin1::new(bytes).write_to(writer)
            }
            PerfTextEncoding::Utf8 => {
                return internal::CharsFromUtf8WithLatin1Fallback::new(bytes).write_to(writer)
            }
            PerfTextEncoding::Utf16BE => {
                return internal::CharsFromUtf16BE::new(bytes).write_to(writer)
            }
            PerfTextEncoding::Utf16LE => {
                return internal::CharsFromLatin1::new(bytes).write_to(writer)
            }
            PerfTextEncoding::Utf32BE => {
                return internal::CharsFromLatin1::new(bytes).write_to(writer)
            }
            PerfTextEncoding::Utf32LE => {
                return internal::CharsFromLatin1::new(bytes).write_to(writer)
            }
        }
    }

    fn write_json_string_to<W: fmt::Write>(
        writer: &mut W,
        bytes: &[u8],
        encoding: PerfTextEncoding,
    ) -> fmt::Result {
        match encoding {
            PerfTextEncoding::Latin1 => {
                return internal::CharsFromLatin1::new(bytes).write_to(writer)
            }
            PerfTextEncoding::Utf8 => {
                return internal::CharsFromUtf8WithLatin1Fallback::new(bytes).write_to(writer)
            }
            PerfTextEncoding::Utf16BE => {
                return internal::CharsFromUtf16BE::new(bytes).write_to(writer)
            }
            PerfTextEncoding::Utf16LE => {
                return internal::CharsFromLatin1::new(bytes).write_to(writer)
            }
            PerfTextEncoding::Utf32BE => {
                return internal::CharsFromLatin1::new(bytes).write_to(writer)
            }
            PerfTextEncoding::Utf32LE => {
                return internal::CharsFromLatin1::new(bytes).write_to(writer)
            }
        }
    }

    fn write_hexbytes_to<W: fmt::Write>(writer: &mut W, bytes: &[u8]) -> fmt::Result {
        if !bytes.is_empty() {
            write!(writer, "{:02X}", bytes[0])?;
            for b in bytes.iter().skip(1) {
                write!(writer, " {:02X}", b)?;
            }
        }
        return Ok(());
    }

    fn write_json_hexbytes_to<W: fmt::Write>(writer: &mut W, bytes: &[u8]) -> fmt::Result {
        if !bytes.is_empty() {
            write!(writer, "{:02X}", bytes[0])?;
            for b in bytes.iter().skip(1) {
                write!(writer, " {:02X}", b)?;
            }
        }
        return Ok(());
    }
}

impl fmt::Display for PerfItemValue<'_> {
    /// Writes a string representation of this value to the formatter.
    /// - Normal formatting is the same as `write_to` with [`PerfConvertOptions::Default`].
    /// - Alternate formatting is the same as `write_json_to` with [`PerfConvertOptions::Default`].
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        return if f.alternate() {
            self.write_json_to(f, PerfConvertOptions::Default)
        } else {
            self.write_to(f, PerfConvertOptions::Default)
        };
    }
}
