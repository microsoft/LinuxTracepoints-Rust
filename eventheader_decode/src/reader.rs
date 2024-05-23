// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PerfByteReader {
    data_big_endian: bool,
}

impl PerfByteReader {
    pub const HOST_ENDIAN: Self = Self::new(cfg!(target_endian = "big"));
    pub const SWAP_ENDIAN: Self = Self::new(!cfg!(target_endian = "big"));

    pub const fn new(data_big_endian: bool) -> Self {
        return Self { data_big_endian };
    }

    pub const fn data_big_endian(&self) -> bool {
        return self.data_big_endian;
    }

    pub const fn byte_swap_needed(&self) -> bool {
        return self.data_big_endian != cfg!(target_endian = "big");
    }

    pub const fn read_i16(&self, data: [u8; 2]) -> i16 {
        return if self.data_big_endian {
            i16::from_be_bytes(data)
        } else {
            i16::from_le_bytes(data)
        }
    }

    pub const fn read_u16(&self, data: [u8; 2]) -> u16 {
        return if self.data_big_endian {
            u16::from_be_bytes(data)
        } else {
            u16::from_le_bytes(data)
        }
    }

    pub const fn read_u16_at(&self, data: &[u8], offset: usize) -> u16 {
        return self.read_u16([data[offset], data[offset + 1]])
    }

    pub const fn read_i32(&self, data: [u8; 4]) -> i32 {
        return if self.data_big_endian {
            i32::from_be_bytes(data)
        } else {
            i32::from_le_bytes(data)
        }
    }

    pub const fn read_u32(&self, data: [u8; 4]) -> u32 {
        return if self.data_big_endian {
            u32::from_be_bytes(data)
        } else {
            u32::from_le_bytes(data)
        }
    }

    pub const fn read_i64(&self, data: [u8; 8]) -> i64 {
        return if self.data_big_endian {
            i64::from_be_bytes(data)
        } else {
            i64::from_le_bytes(data)
        }
    }

    pub const fn read_u64(&self, data: [u8; 8]) -> u64 {
        return if self.data_big_endian {
            u64::from_be_bytes(data)
        } else {
            u64::from_le_bytes(data)
        }
    }

    pub fn read_f32(&self, data: [u8; 4]) -> f32 {
        return if self.data_big_endian {
            f32::from_be_bytes(data)
        } else {
            f32::from_le_bytes(data)
        }
    }

    pub fn read_f64(&self, data: [u8; 8]) -> f64 {
        return if self.data_big_endian {
            f64::from_be_bytes(data)
        } else {
            f64::from_le_bytes(data)
        }
    }

    pub fn fix_u16(&self, data: u16) -> u16 {
        return if self.data_big_endian == cfg!(target_endian = "big") {
            data
        } else {
            data.swap_bytes()
        }
    }

    pub fn fix_u32(&self, data: u32) -> u32 {
        return if self.data_big_endian == cfg!(target_endian = "big") {
            data
        } else {
            data.swap_bytes()
        }
    }

    pub fn fix_u64(&self, data: u64) -> u64 {
        return if self.data_big_endian == cfg!(target_endian = "big") {
            data
        } else {
            data.swap_bytes()
        }
    }
}
