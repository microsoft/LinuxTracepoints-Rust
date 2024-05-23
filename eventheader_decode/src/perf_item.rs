// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

use crate::reader::PerfByteReader;
use eventheader_types::*;

#[derive(Clone, Copy)]
pub struct PerfItemType {
    element_count: u16,
    field_tag: u16,
    type_size: u8,
    encoding_and_array_flags: FieldEncoding,
    format: FieldFormat,
    byte_reader: PerfByteReader,
}

impl PerfItemType {
    pub const fn new(
        byte_reader: PerfByteReader,
        encoding_and_array_flags: FieldEncoding,
        format: FieldFormat,
        type_size: u8,
        element_count: u16,
        field_tag: u16,
    ) -> Self {
        // Chain flags must be masked-out by caller.
        debug_assert!(!encoding_and_array_flags.has_chain_flag());
        debug_assert!(!format.has_chain_flag());

        // If not an array, elementCount must be 1.
        if !encoding_and_array_flags.is_array()
        {
            debug_assert!(element_count == 1);
        }

        if encoding_and_array_flags.base_encoding().as_int() == FieldEncoding::Struct.as_int()
        {
            debug_assert!(type_size == 0);
            debug_assert!(format.as_int() != 0); // No zero-length structs.
        }

        return Self {
            element_count,
            field_tag,
            type_size,
            encoding_and_array_flags,
            format,
            byte_reader,
        }
    }

    pub const fn element_count(&self) -> u16 {
        self.element_count
    }

    pub const fn field_tag(&self) -> u16 {
        self.field_tag
    }

    pub const fn type_size(&self) -> u8 {
        self.type_size
    }

    pub const fn encoding(&self) -> FieldEncoding {
        self.encoding_and_array_flags.base_encoding()
    }

    pub const fn array_flags(&self) -> u8 {
        self.encoding_and_array_flags.array_flags()
    }

    pub const fn encoding_and_array_flags(&self) -> FieldEncoding {
        self.encoding_and_array_flags
    }

    pub const fn is_array_or_element(&self) -> bool {
        self.encoding_and_array_flags.is_array()
    }

    pub const fn format(&self) -> FieldFormat {
        self.format
    }

    pub const fn struct_field_count(&self) -> u8 {
        self.format.as_int()
    }

    pub const fn byte_reader(&self) -> PerfByteReader {
        self.byte_reader
    }

    #[allow(clippy::wrong_self_convention)]    
    pub const fn data_big_endian(&self) -> bool {
        self.byte_reader.data_big_endian()
    }
}

#[derive(Clone, Copy)]
pub struct PerfItemValue<'a> {
    bytes: &'a [u8],
    item_type: PerfItemType,
}

impl<'a> PerfItemValue<'a> {
    pub const fn new(bytes: &'a [u8], item_type: PerfItemType) -> Self {
        if item_type.type_size != 0 && !bytes.is_empty() {
            debug_assert!(bytes.len() == item_type.type_size as usize * item_type.element_count as usize);
        }

        if item_type.encoding().as_int() == FieldEncoding::Struct.as_int() {
            debug_assert!(bytes.is_empty());
        }

        return Self {
            bytes,
            item_type,
        }
    }

    pub fn bytes(&self) -> &'a [u8] {
        self.bytes
    }

    pub fn item_type(&self) -> PerfItemType {
        self.item_type
    }
}