// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

/// Helper for working with data that may be in big-endian or little-endian byte order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PerfByteReader {
    source_big_endian: bool,
}

impl PerfByteReader {
    /// true if running on a big-endian system.
    pub const HOST_IS_BIG_ENDIAN: bool = cfg!(target_endian = "big");

    /// A reader that assumes the input data bytes do not need to be swapped when
    /// read, i.e. the input data is in the byte order expected by the host system.
    pub const KEEP_ENDIAN: Self = Self::new(Self::HOST_IS_BIG_ENDIAN);

    /// A reader that assumes the input data bytes must be swapped when read, i.e.
    /// the input data is NOT in the byte order expected by the host system.
    pub const SWAP_ENDIAN: Self = Self::new(!Self::HOST_IS_BIG_ENDIAN);

    /// Create a new reader that will interpret input data bytes as indicated by the
    /// source_big_endian parameter.
    pub const fn new(source_big_endian: bool) -> Self {
        return Self { source_big_endian };
    }

    /// Returns true if the input data bytes are being interpreted as big-endian.
    pub const fn source_big_endian(self) -> bool {
        return self.source_big_endian;
    }

    /// Returns true if the input data bytes are being byte-swapped.
    pub const fn byte_swap_needed(self) -> bool {
        return self.source_big_endian != Self::HOST_IS_BIG_ENDIAN;
    }

    /// Reads an i16 from the start of the given slice, swapping byte order if byte_swap_needed() is true.
    /// PRECONDITION: source.len() >= 2
    pub fn read_i16(self, source: &[u8]) -> i16 {
        let source_array = source[..2].try_into().unwrap();
        return if self.source_big_endian {
            i16::from_be_bytes(source_array)
        } else {
            i16::from_le_bytes(source_array)
        };
    }

    /// Reads a u16 from the start of the given slice, swapping byte order if byte_swap_needed() is true.
    /// PRECONDITION: bytes.len() >= 2
    pub fn read_u16(self, source: &[u8]) -> u16 {
        let source_array = source[..2].try_into().unwrap();
        return if self.source_big_endian {
            u16::from_be_bytes(source_array)
        } else {
            u16::from_le_bytes(source_array)
        };
    }

    /// Reads an i32 from the start of the given slice, swapping byte order if byte_swap_needed() is true.
    /// PRECONDITION: source.len() >= 4
    pub fn read_i32(self, source: &[u8]) -> i32 {
        let source_array = source[..4].try_into().unwrap();
        return if self.source_big_endian {
            i32::from_be_bytes(source_array)
        } else {
            i32::from_le_bytes(source_array)
        };
    }

    /// Reads a u32 from the start of the given slice, swapping byte order if byte_swap_needed() is true.
    /// PRECONDITION: bytes.len() >= 4
    pub fn read_u32(self, source: &[u8]) -> u32 {
        let source_array = source[..4].try_into().unwrap();
        return if self.source_big_endian {
            u32::from_be_bytes(source_array)
        } else {
            u32::from_le_bytes(source_array)
        };
    }

    /// Reads an i64 from the start of the given slice, swapping byte order if byte_swap_needed() is true.
    /// PRECONDITION: source.len() >= 8
    pub fn read_i64(self, source: &[u8]) -> i64 {
        let source_array = source[..8].try_into().unwrap();
        return if self.source_big_endian {
            i64::from_be_bytes(source_array)
        } else {
            i64::from_le_bytes(source_array)
        };
    }

    /// Reads a u64 from the start of the given slice, swapping byte order if byte_swap_needed() is true.
    /// PRECONDITION: bytes.len() >= 8
    pub fn read_u64(self, source: &[u8]) -> u64 {
        let source_array = source[..8].try_into().unwrap();
        return if self.source_big_endian {
            u64::from_be_bytes(source_array)
        } else {
            u64::from_le_bytes(source_array)
        };
    }

    /// Reads a f32 from the start of the given slice, swapping byte order if byte_swap_needed() is true.
    /// PRECONDITION: bytes.len() >= 4
    pub fn read_f32(self, source: &[u8]) -> f32 {
        let source_array = source[..4].try_into().unwrap();
        return if self.source_big_endian {
            f32::from_be_bytes(source_array)
        } else {
            f32::from_le_bytes(source_array)
        };
    }

    /// Reads a f64 from the start of the given slice, swapping byte order if byte_swap_needed() is true.
    /// PRECONDITION: source.len() >= 8
    pub fn read_f64(self, source: &[u8]) -> f64 {
        let source_array = source[..8].try_into().unwrap();
        return if self.source_big_endian {
            f64::from_be_bytes(source_array)
        } else {
            f64::from_le_bytes(source_array)
        };
    }

    /// If byte_swap_needed() is true, returns byte-swapped value, otherwise returns
    /// unmodified value.
    pub const fn fix_u16(self, value: u16) -> u16 {
        return if self.source_big_endian == Self::HOST_IS_BIG_ENDIAN {
            value
        } else {
            value.swap_bytes()
        };
    }

    /// If byte_swap_needed() is true, returns byte-swapped value, otherwise returns
    /// unmodified value.
    pub const fn fix_u32(self, value: u32) -> u32 {
        return if self.source_big_endian == Self::HOST_IS_BIG_ENDIAN {
            value
        } else {
            value.swap_bytes()
        };
    }

    /// If byte_swap_needed() is true, returns byte-swapped value, otherwise returns
    /// unmodified value.
    pub const fn fix_u64(self, value: u64) -> u64 {
        return if self.source_big_endian == Self::HOST_IS_BIG_ENDIAN {
            value
        } else {
            value.swap_bytes()
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_I16: i16 = 0x1234;
    const TEST_U16: u16 = 0x1234;
    const TEST_I32: i32 = 0x12345678;
    const TEST_U32: u32 = 0x12345678;
    const TEST_I64: i64 = 0x1234567890abcdef;
    const TEST_U64: u64 = 0x1234567890abcdef;
    const TEST_F32: f32 = 1234.5678;
    const TEST_F64: f64 = 1234.5678;

    #[test]
    fn constants() {
        const TARGET_BIG_ENDIAN: bool = TEST_U32.to_be() == TEST_U32;
        assert_eq!(TARGET_BIG_ENDIAN, PerfByteReader::HOST_IS_BIG_ENDIAN);

        assert_eq!(
            TARGET_BIG_ENDIAN,
            PerfByteReader::KEEP_ENDIAN.source_big_endian()
        );
        assert_eq!(
            !TARGET_BIG_ENDIAN,
            PerfByteReader::SWAP_ENDIAN.source_big_endian()
        );
        assert_eq!(false, PerfByteReader::new(false).source_big_endian());
        assert_eq!(true, PerfByteReader::new(true).source_big_endian());

        assert_eq!(false, PerfByteReader::KEEP_ENDIAN.byte_swap_needed());
        assert_eq!(true, PerfByteReader::SWAP_ENDIAN.byte_swap_needed());
        assert_eq!(
            TARGET_BIG_ENDIAN,
            PerfByteReader::new(false).byte_swap_needed()
        );
        assert_eq!(
            !TARGET_BIG_ENDIAN,
            PerfByteReader::new(true).byte_swap_needed()
        );
    }

    #[test]
    fn read() {
        assert_eq!(
            TEST_I16,
            PerfByteReader::new(false).read_i16(&TEST_I16.to_le_bytes())
        );
        assert_eq!(
            TEST_I16,
            PerfByteReader::new(true).read_i16(&TEST_I16.to_be_bytes())
        );

        assert_eq!(
            TEST_U16,
            PerfByteReader::new(false).read_u16(&TEST_U16.to_le_bytes())
        );
        assert_eq!(
            TEST_U16,
            PerfByteReader::new(true).read_u16(&TEST_U16.to_be_bytes())
        );

        assert_eq!(
            TEST_I32,
            PerfByteReader::new(false).read_i32(&TEST_I32.to_le_bytes())
        );
        assert_eq!(
            TEST_I32,
            PerfByteReader::new(true).read_i32(&TEST_I32.to_be_bytes())
        );

        assert_eq!(
            TEST_U32,
            PerfByteReader::new(false).read_u32(&TEST_U32.to_le_bytes())
        );
        assert_eq!(
            TEST_U32,
            PerfByteReader::new(true).read_u32(&TEST_U32.to_be_bytes())
        );

        assert_eq!(
            TEST_I64,
            PerfByteReader::new(false).read_i64(&TEST_I64.to_le_bytes())
        );
        assert_eq!(
            TEST_I64,
            PerfByteReader::new(true).read_i64(&TEST_I64.to_be_bytes())
        );

        assert_eq!(
            TEST_U64,
            PerfByteReader::new(false).read_u64(&TEST_U64.to_le_bytes())
        );
        assert_eq!(
            TEST_U64,
            PerfByteReader::new(true).read_u64(&TEST_U64.to_be_bytes())
        );

        assert_eq!(
            TEST_F32,
            PerfByteReader::new(false).read_f32(&TEST_F32.to_le_bytes())
        );
        assert_eq!(
            TEST_F32,
            PerfByteReader::new(true).read_f32(&TEST_F32.to_be_bytes())
        );

        assert_eq!(
            TEST_F64,
            PerfByteReader::new(false).read_f64(&TEST_F64.to_le_bytes())
        );
        assert_eq!(
            TEST_F64,
            PerfByteReader::new(true).read_f64(&TEST_F64.to_be_bytes())
        );
    }

    #[test]
    fn fix() {
        assert_eq!(
            TEST_U16,
            PerfByteReader::new(false).fix_u16(TEST_U16.to_le())
        );
        assert_eq!(
            TEST_U16,
            PerfByteReader::new(true).fix_u16(TEST_U16.to_be())
        );

        assert_eq!(
            TEST_U32,
            PerfByteReader::new(false).fix_u32(TEST_U32.to_le())
        );
        assert_eq!(
            TEST_U32,
            PerfByteReader::new(true).fix_u32(TEST_U32.to_be())
        );

        assert_eq!(
            TEST_U64,
            PerfByteReader::new(false).fix_u64(TEST_U64.to_le())
        );
        assert_eq!(
            TEST_U64,
            PerfByteReader::new(true).fix_u64(TEST_U64.to_be())
        );
    }
}
