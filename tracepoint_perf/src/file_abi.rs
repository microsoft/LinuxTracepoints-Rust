// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

#[derive(Debug)]
#[repr(C)]
pub struct ClockData {
    pub version: u32,
    pub clockid: u32,
    pub wall_clock_ns: u64,
    pub clockid_time_ns: u64,
}

impl ClockData {
    pub fn byte_swap(&mut self) {
        self.version = self.version.swap_bytes();
        self.clockid = self.clockid.swap_bytes();
        self.wall_clock_ns = self.wall_clock_ns.swap_bytes();
        self.clockid_time_ns = self.clockid_time_ns.swap_bytes();
    }
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct PerfFileSection {
    pub offset: u64, // offset from start of file
    pub size: u64,   // size of the section
}

impl PerfFileSection {
    pub fn byte_swap(&mut self) {
        self.offset = self.offset.swap_bytes();
        self.size = self.size.swap_bytes();
    }
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct PerfFileHeaderPipe {
    pub magic: u64, // If correctly byte-swapped, this will be equal to PERFILE2_MAGIC_HOST_ENDIAN.
    pub size: u64,  // Size of the header, 16 for pipe-mode or 104 for seek-mode.
}

impl PerfFileHeaderPipe {
    pub fn byte_swap(&mut self) {
        self.magic = self.magic.swap_bytes();
        self.size = self.size.swap_bytes();
    }
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct PerfFileHeaderRest {
    pub attr_size: u64, // size of (perf_event_attrs + perf_file_section) in attrs.
    pub attrs: PerfFileSection,
    pub data: PerfFileSection,
    pub event_types: PerfFileSection, // Not used
    pub flags: [u64; 4],              // 256-bit bitmap based on HEADER_BITS
}

impl PerfFileHeaderRest {
    pub fn byte_swap(&mut self) {
        self.attr_size = self.attr_size.swap_bytes();
        self.attrs.byte_swap();
        self.data.byte_swap();
        self.event_types.byte_swap();
        for i in 0..4 {
            self.flags[i] = self.flags[i].swap_bytes();
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
#[repr(C)]
pub struct PerfFileHeader {
    pub pipe: PerfFileHeaderPipe,
    pub rest: PerfFileHeaderRest,
}

impl PerfFileHeader {
    pub fn byte_swap(&mut self) {
        self.pipe.byte_swap();
        self.rest.byte_swap();
    }
}
