// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

#![allow(non_upper_case_globals)]

use std::fmt;

/// From: perf.data-file-format.txt, perf/util/header.h.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PerfHeaderIndex(pub u8);

impl PerfHeaderIndex {
    /// PERF_HEADER_RESERVED
    /// always cleared
    pub const Reserved: Self = Self(0);

    /// PERF_HEADER_TRACING_DATA, PERF_HEADER_FIRST_FEATURE
    pub const TracingData: Self = Self(1);

    /// PERF_HEADER_BUILD_ID
    pub const BuildId: Self = Self(2);

    /// PERF_HEADER_HOSTNAME
    pub const Hostname: Self = Self(3);

    /// PERF_HEADER_OSRELEASE
    pub const OSRelease: Self = Self(4);

    /// PERF_HEADER_VERSION
    pub const Version: Self = Self(5);

    /// PERF_HEADER_ARCH
    pub const Arch: Self = Self(6);

    /// PERF_HEADER_NRCPUS
    pub const NrCpus: Self = Self(7);

    /// PERF_HEADER_CPUDESC
    pub const CpuDesc: Self = Self(8);

    /// PERF_HEADER_CPUID
    pub const CpuId: Self = Self(9);

    /// PERF_HEADER_TOTAL_MEM
    pub const TotalMem: Self = Self(10);

    /// PERF_HEADER_CMDLINE
    pub const Cmdline: Self = Self(11);

    /// PERF_HEADER_EVENT_DESC
    pub const EventDesc: Self = Self(12);

    /// PERF_HEADER_CPU_TOPOLOGY
    pub const CpuTopology: Self = Self(13);

    /// PERF_HEADER_NUMA_TOPOLOGY
    pub const NumaTopology: Self = Self(14);

    /// PERF_HEADER_BRANCH_STACK
    pub const BranchStack: Self = Self(15);

    /// PERF_HEADER_PMU_MAPPINGS
    pub const PmuMappings: Self = Self(16);

    /// PERF_HEADER_GROUP_DESC
    pub const GroupDesc: Self = Self(17);

    /// PERF_HEADER_AUXTRACE
    pub const AuxTrace: Self = Self(18);

    /// PERF_HEADER_STAT
    pub const Stat: Self = Self(19);

    /// PERF_HEADER_CACHE
    pub const Cache: Self = Self(20);

    /// PERF_HEADER_SAMPLE_TIME
    pub const SampleTime: Self = Self(21);

    /// PERF_HEADER_MEM_TOPOLOGY
    pub const MemTopology: Self = Self(22);

    /// PERF_HEADER_CLOCKID
    pub const ClockId: Self = Self(23);

    /// PERF_HEADER_DIR_FORMAT
    pub const DirFormat: Self = Self(24);

    /// PERF_HEADER_BPF_PROG_INFO
    pub const BpfProgInfo: Self = Self(25);

    /// PERF_HEADER_BPF_BTF
    pub const BpfBtf: Self = Self(26);

    /// PERF_HEADER_COMPRESSED
    pub const Compressed: Self = Self(27);

    /// PERF_HEADER_CPU_PMU_CAPS
    pub const CpuPmuCaps: Self = Self(28);

    /// PERF_HEADER_CLOCK_DATA
    pub const ClockData: Self = Self(29);

    /// PERF_HEADER_HYBRID_TOPOLOGY
    pub const HybridTopology: Self = Self(30);

    /// PERF_HEADER_PMU_CAPS
    pub const PmuCaps: Self = Self(31);

    /// PERF_HEADER_LAST_FEATURE
    pub const LastFeature: Self = Self(32);

    /// Returns a string like "TracingData" or "BuildId" for the type.
    /// If type is unknown, returns None.
    pub const fn as_string(self) -> Option<&'static str> {
        const NAMES: [&str; 33] = [
            "Reserved",
            "TracingData",
            "BuildId",
            "Hostname",
            "OSRelease",
            "Version",
            "Arch",
            "NrCpus",
            "CpuDesc",
            "CpuId",
            "TotalMem",
            "Cmdline",
            "EventDesc",
            "CpuTopology",
            "NumaTopology",
            "BranchStack",
            "PmuMappings",
            "GroupDesc",
            "AuxTrace",
            "Stat",
            "Cache",
            "SampleTime",
            "MemTopology",
            "ClockId",
            "DirFormat",
            "BpfProgInfo",
            "BpfBtf",
            "Compressed",
            "CpuPmuCaps",
            "ClockData",
            "HybridTopology",
            "PmuCaps",
            "LastFeature",
        ];
        let index = self.0 as usize;
        if index < NAMES.len() {
            return Some(NAMES[index]);
        } else {
            return None;
        }
    }
}

impl From<u8> for PerfHeaderIndex {
    fn from(val: u8) -> Self {
        Self(val)
    }
}

impl From<PerfHeaderIndex> for u8 {
    fn from(val: PerfHeaderIndex) -> Self {
        val.0
    }
}

impl fmt::Display for PerfHeaderIndex {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(s) = self.as_string() {
            return f.pad(s);
        } else {
            return self.0.fmt(f);
        }
    }
}
