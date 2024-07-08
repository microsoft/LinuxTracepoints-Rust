// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.
// Adapted from linux/uapi/linux/perf_event.h.

#![allow(non_upper_case_globals)]

use core::fmt;
use core::mem; // size_of

use crate::PerfByteReader;

/// perf_type_id: uint32 value for [`PerfEventAttr::attr_type`].
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PerfEventAttrType(pub u32);

impl PerfEventAttrType {
    /// PERF_TYPE_HARDWARE
    pub const Hardware: Self = Self(0);

    /// PERF_TYPE_SOFTWARE
    pub const Software: Self = Self(1);

    /// PERF_TYPE_TRACEPOINT
    pub const Tracepoint: Self = Self(2);

    /// PERF_TYPE_HW_CACHE
    pub const HwCache: Self = Self(3);

    /// PERF_TYPE_RAW
    pub const Raw: Self = Self(4);

    /// PERF_TYPE_BREAKPOINT
    pub const Breakpoint: Self = Self(5);

    /// Returns a string like "Hardware" or "Software" for the type.
    /// If type is unknown, returns None.
    pub const fn as_string(self) -> Option<&'static str> {
        const NAMES: [&str; 6] = [
            "Hardware",
            "Software",
            "Tracepoint",
            "HwCache",
            "Raw",
            "Breakpoint",
        ];
        let index = self.0 as usize;
        if index < NAMES.len() {
            return Some(NAMES[index]);
        } else {
            return None;
        }
    }
}

impl From<u32> for PerfEventAttrType {
    fn from(val: u32) -> Self {
        Self(val)
    }
}

impl From<PerfEventAttrType> for u32 {
    fn from(val: PerfEventAttrType) -> Self {
        val.0
    }
}

impl fmt::Display for PerfEventAttrType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(s) = self.as_string() {
            return f.pad(s);
        } else {
            return self.0.fmt(f);
        }
    }
}

/// Values for PerfEventAttr.Size.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PerfEventAttrSize(pub u32);

impl PerfEventAttrSize {
    /// Invalid value for size.
    pub const Zero: Self = Self(0);

    /// PERF_ATTR_SIZE_VER0 - first published struct
    pub const Ver0: Self = Self(64);

    /// PERF_ATTR_SIZE_VER1 - add: Config2
    pub const Ver1: Self = Self(72);

    /// PERF_ATTR_SIZE_VER2 - add: BranchSampleType
    pub const Ver2: Self = Self(80);

    /// PERF_ATTR_SIZE_VER3 - add: SampleRegsUser, SampleStackUser
    pub const Ver3: Self = Self(96);

    /// PERF_ATTR_SIZE_VER4 - add: SampleRegsIntr
    pub const Ver4: Self = Self(104);

    /// PERF_ATTR_SIZE_VER5 - add: AuxWatermark
    pub const Ver5: Self = Self(112);

    /// PERF_ATTR_SIZE_VER6 - add: AuxSampleSize
    pub const Ver6: Self = Self(120);

    /// PERF_ATTR_SIZE_VER7 - add: SigData
    pub const Ver7: Self = Self(128);
}

impl From<u32> for PerfEventAttrSize {
    fn from(val: u32) -> Self {
        Self(val)
    }
}

impl From<PerfEventAttrSize> for u32 {
    fn from(val: PerfEventAttrSize) -> Self {
        val.0
    }
}

/// perf_event_sample_format: bits that can be set in PerfEventAttr.SampleType.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PerfEventAttrSampleType(pub u64);

impl PerfEventAttrSampleType {
    /// No bits set.
    pub const None: Self = Self(0);

    /// PERF_SAMPLE_IP
    pub const IP: Self = Self(1 << 0);

    /// PERF_SAMPLE_TID
    pub const Tid: Self = Self(1 << 1);

    /// PERF_SAMPLE_TIME
    pub const Time: Self = Self(1 << 2);

    /// PERF_SAMPLE_ADDR
    pub const Addr: Self = Self(1 << 3);

    /// PERF_SAMPLE_READ
    pub const Read: Self = Self(1 << 4);

    /// PERF_SAMPLE_CALLCHAIN
    pub const Callchain: Self = Self(1 << 5);

    /// PERF_SAMPLE_ID
    pub const Id: Self = Self(1 << 6);

    /// PERF_SAMPLE_CPU
    pub const Cpu: Self = Self(1 << 7);

    /// PERF_SAMPLE_PERIOD
    pub const Period: Self = Self(1 << 8);

    /// PERF_SAMPLE_STREAM_ID
    pub const StreamId: Self = Self(1 << 9);

    /// PERF_SAMPLE_RAW
    pub const Raw: Self = Self(1 << 10);

    /// PERF_SAMPLE_BRANCH_STACK
    pub const BranchStack: Self = Self(1 << 11);

    /// PERF_SAMPLE_REGS_USER
    pub const RegsUser: Self = Self(1 << 12);

    /// PERF_SAMPLE_STACK_USER
    pub const StackUser: Self = Self(1 << 13);

    /// PERF_SAMPLE_WEIGHT
    pub const Weight: Self = Self(1 << 14);

    /// PERF_SAMPLE_DATA_SRC
    pub const DataSrc: Self = Self(1 << 15);

    /// PERF_SAMPLE_IDENTIFIER
    pub const Identifier: Self = Self(1 << 16);

    /// PERF_SAMPLE_TRANSACTION
    pub const Transaction: Self = Self(1 << 17);

    /// PERF_SAMPLE_REGS_INTR
    pub const RegsIntr: Self = Self(1 << 18);

    /// PERF_SAMPLE_PHYS_ADDR
    pub const PhysAddr: Self = Self(1 << 19);

    /// PERF_SAMPLE_AUX
    pub const Aux: Self = Self(1 << 20);

    /// PERF_SAMPLE_CGROUP
    pub const Cgroup: Self = Self(1 << 21);

    /// PERF_SAMPLE_DATA_PAGE_SIZE
    pub const DataPageSize: Self = Self(1 << 22);

    /// PERF_SAMPLE_CODE_PAGE_SIZE
    pub const CodePageSize: Self = Self(1 << 23);

    /// PERF_SAMPLE_WEIGHT_STRUCT
    pub const WeightStruct: Self = Self(1 << 24);

    /// PERF_SAMPLE_WEIGHT_TYPE = PERF_SAMPLE_WEIGHT | PERF_SAMPLE_WEIGHT_STRUCT
    pub const WeightType: Self = Self(Self::Weight.0 | Self::WeightStruct.0);

    /// Returns true if (self & mask) != 0.
    pub const fn has_flag(self, mask: Self) -> bool {
        0 != (self.0 & mask.0)
    }

    /// Returns `self | other`.
    pub const fn or(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl From<u64> for PerfEventAttrSampleType {
    fn from(val: u64) -> Self {
        Self(val)
    }
}

impl From<PerfEventAttrSampleType> for u64 {
    fn from(val: PerfEventAttrSampleType) -> Self {
        val.0
    }
}

/// perf_event_read_format: bits that can be set in PerfEventAttr.ReadFormat.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PerfEventAttrReadFormat(pub u64);

impl PerfEventAttrReadFormat {
    /// No bits set.
    pub const None: Self = Self(0);

    /// PERF_FORMAT_TOTAL_TIME_ENABLED
    pub const TotalTimeEnabled: Self = Self(1 << 0);

    /// PERF_FORMAT_TOTAL_TIME_RUNNING
    pub const TotalTimeRunning: Self = Self(1 << 1);

    /// PERF_FORMAT_ID
    pub const Id: Self = Self(1 << 2);

    /// PERF_FORMAT_GROUP
    pub const Group: Self = Self(1 << 3);

    /// PERF_FORMAT_LOST
    pub const Lost: Self = Self(1 << 4);

    /// Returns true if (self & mask) != 0.
    pub const fn has_flag(self, mask: Self) -> bool {
        0 != (self.0 & mask.0)
    }

    /// Returns `self | other`.
    pub const fn or(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl From<u64> for PerfEventAttrReadFormat {
    fn from(val: u64) -> Self {
        Self(val)
    }
}

impl From<PerfEventAttrReadFormat> for u64 {
    fn from(val: PerfEventAttrReadFormat) -> Self {
        val.0
    }
}

/// Bits for PerfEventAttr.Options.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PerfEventAttrOptions(pub u64);

impl PerfEventAttrOptions {
    /// No bits set.
    pub const None: Self = Self(0);

    /// disabled: off by default
    pub const Disabled: Self = Self(1 << 0);

    /// inherit: children inherit it
    pub const Inherit: Self = Self(1 << 1);

    /// pinned: must always be on PMU
    pub const Pinned: Self = Self(1 << 2);

    /// exclusive: only group on PMU
    pub const Exclusive: Self = Self(1 << 3);

    /// exclude_user: don't count user
    pub const ExcludeUser: Self = Self(1 << 4);

    /// exclude_kernel: don't count kernel
    pub const ExcludeKernel: Self = Self(1 << 5);

    /// exclude_hv: don't count hypervisor
    pub const ExcludeHypervisor: Self = Self(1 << 6);

    /// exclude_idle: don't count when idle
    pub const ExcludeIdle: Self = Self(1 << 7);

    /// mmap: include mmap data
    pub const Mmap: Self = Self(1 << 8);

    /// comm: include comm data
    pub const Comm: Self = Self(1 << 9);

    /// freq: use freq, not period
    pub const Freq: Self = Self(1 << 10);

    /// inherit_stat: per task counts
    pub const InheritStat: Self = Self(1 << 11);

    /// enable_on_exec: next exec enables
    pub const EnableOnExec: Self = Self(1 << 12);

    /// task: trace fork/exit
    pub const Task: Self = Self(1 << 13);

    /// watermark: Use WakeupWatermark instead of WakeupEvents
    pub const Watermark: Self = Self(1 << 14);

    /// precise_ip first bit:
    /// If unset, SAMPLE_IP can have arbitrary skid.
    /// If set, SAMPLE_IP must have constant skid.
    /// See also PERF_RECORD_MISC_EXACT_IP.
    pub const PreciseIPSkidConstant: Self = Self(1 << 15);

    /// precise_ip second bit:
    /// SAMPLE_IP requested to have 0 skid.
    /// If precise_ip_skid_constant is also set, SAMPLE_IP must have 0 skid.
    /// See also PERF_RECORD_MISC_EXACT_IP.
    pub const PreciseIPSkidZero: Self = Self(1 << 16);

    /// mmap_data: non-exec mmap data
    pub const MmapData: Self = Self(1 << 17);

    /// sample_id_all: SampleType all events
    pub const SampleIdAll: Self = Self(1 << 18);

    /// exclude_host: don't count in host
    pub const ExcludeHost: Self = Self(1 << 19);

    /// exclude_guest: don't count in guest
    pub const ExcludeGuest: Self = Self(1 << 20);

    /// exclude_callchain_kernel: exclude kernel callchains
    pub const ExcludeCallchainKernel: Self = Self(1 << 21);

    /// exclude_callchain_user: exclude user callchains
    pub const ExcludeCallchainUser: Self = Self(1 << 22);

    /// mmap2: include mmap with inode data
    pub const Mmap2: Self = Self(1 << 23);

    /// comm_exec: flag comm events that are due to an exec
    pub const CommExec: Self = Self(1 << 24);

    /// use_clockid: use @clockid for time fields
    pub const UseClockId: Self = Self(1 << 25);

    /// context_switch: context switch data
    pub const ContextSwitch: Self = Self(1 << 26);

    /// write_backward: Write ring buffer from end to beginning
    pub const WriteBackward: Self = Self(1 << 27);

    /// namespaces: include namespaces data
    pub const Namespaces: Self = Self(1 << 28);

    /// ksymbol: include ksymbol events
    pub const Ksymbol: Self = Self(1 << 29);

    /// bpf_event: include bpf events
    pub const BpfEvent: Self = Self(1 << 30);

    /// aux_output: generate AUX records instead of events
    pub const AuxOutput: Self = Self(1 << 31);

    /// cgroup: include cgroup events
    pub const Cgroup: Self = Self(1 << 32);

    /// text_poke: include text poke events
    pub const TextPoke: Self = Self(1 << 33);

    /// build_id: use build id in mmap2 events
    pub const BuildId: Self = Self(1 << 34);

    /// inherit_thread: children only inherit if cloned with CLONE_THREAD
    pub const InheritThread: Self = Self(1 << 35);

    /// remove_on_exec: event is removed from task on exec
    pub const RemoveOnExec: Self = Self(1 << 36);

    /// sigtrap: send synchronous SIGTRAP on event
    pub const Sigtrap: Self = Self(1 << 37);

    /// Returns true if (self & mask) != 0.
    pub const fn has_flag(self, mask: Self) -> bool {
        0 != (self.0 & mask.0)
    }

    /// Returns `self | other`.
    pub const fn or(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }
}

impl From<u64> for PerfEventAttrOptions {
    fn from(val: u64) -> Self {
        Self(val)
    }
}

impl From<PerfEventAttrOptions> for u64 {
    fn from(val: PerfEventAttrOptions) -> Self {
        val.0
    }
}

/// perf_event_attr: Event's collection parameters.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PerfEventAttr {
    /// type:
    /// Major type: hardware/software/tracepoint/etc.
    pub attr_type: PerfEventAttrType,

    /// size:
    /// Size of the attr structure, for fwd/bwd compat.
    pub size: PerfEventAttrSize,

    /// config:
    /// Type-specific configuration information.
    pub config: u64,

    /// sample_time: union of sample_period and sample_freq.
    pub sample_time: u64,

    /// sample_type
    pub sample_type: PerfEventAttrSampleType,

    /// read_format
    pub read_format: PerfEventAttrReadFormat,

    /// In C, this is a bit-field of various options:
    /// disabled, inherit, pinned, exclusive, exclude_user, exclude_kernel, exclude_hv,
    /// exclude_idle, mmap, comm, freq, inherit_stat, enable_on_exec, task, watermark,
    /// precise_ip (2 bits), mmap_data, sample_id_all, exclude_host, exclude_guest,
    /// exclude_callchain_kernel, exclude_callchain_user, mmap2, comm_exec, use_clockid,
    /// context_switch, write_backward, namespaces, ksymbol, bpf_event, aux_output,
    /// cgroup, text_poke, build_id, inherit_thread, remove_on_exec, sigtrap.
    pub options: PerfEventAttrOptions,

    /// wakeup_value: union of wakeup_events AND wakeup_watermark.
    ///
    /// wakeup_events:
    /// wakeup every n events.
    ///
    /// wakeup_watermark:
    /// bytes before wakeup.
    pub wakeup_value: u32,

    /// bp_type
    pub bp_type: u32,

    /// config1: union of bp_addr, kprobe_func (for perf_kprobe), uprobe_path
    /// (for perf_uprobe), config1 (extension of config).
    pub config1: u64,

    /// config2: union of bp_len, kprobe_addr (when kprobe_func == NULL),
    /// probe_offset (for perf_[k,u]probe), config2 (extension of config1).
    pub config2: u64,

    /// branch_sample_type:
    /// enum perf_branch_sample_type
    pub branch_sample_type: u64,

    /// sample_regs_user:
    /// Defines set of user regs to dump on samples.
    /// See asm/perf_regs.h for details.
    pub sample_regs_user: u64,

    /// sample_stack_user:
    /// Defines size of the user stack to dump on samples.
    pub sample_stack_user: u32,

    /// clockid
    pub clockid: u32,

    /// sample_regs_intr:
    /// Defines set of regs to dump for each sample state captured on:
    ///
    /// - precise = 0: PMU interrupt
    /// - precise > 0: sampled instruction
    ///
    /// See asm/perf_regs.h for details.
    pub sample_regs_intr: u64,

    /// aux_watermark:
    /// Wakeup watermark for AUX area
    pub aux_watermark: u32,

    /// sample_max_stack
    pub sample_max_stack: u16,

    /// reserved2
    pub reserved2: u16,

    /// aux_sample_size
    pub aux_sample_size: u32,

    /// reserved3
    pub reserved3: u32,

    /// sig_data:
    /// User provided data if sigtrap=1, passed back to user via
    /// siginfo_t::si_perf_data, e.g. to permit user to identify the event.
    /// Note, siginfo_t::si_perf_data is long-sized, and SigData will be
    /// truncated accordingly on 32 bit architectures.
    pub sig_data: u64,
}

impl PerfEventAttr {
    /// size_of::<PerfEventAttr>() == 128.
    pub const SIZE_OF: usize = mem::size_of::<Self>();

    /// Byte offset of PerfEventAttr.attr_type = 0.
    pub const ATTR_TYPE_OFFSET: usize = 0;

    /// Byte offset of PerfEventAttr.size = 4.
    pub const SIZE_OFFSET: usize = 4;

    /// Byte offset of PerfEventAttr.config = 8.
    pub const CONFIG_OFFSET: usize = 8;

    /// Reverse the endian order of all fields in this struct.
    pub fn byte_swap(&mut self) {
        self.attr_type.0 = self.attr_type.0.swap_bytes();
        self.size.0 = self.size.0.swap_bytes();
        self.config = self.config.swap_bytes();
        self.sample_time = self.sample_time.swap_bytes();
        self.sample_type.0 = self.sample_type.0.swap_bytes();
        self.read_format.0 = self.read_format.0.swap_bytes();

        // Bitfield: Reverse bits within each byte, don't reorder bytes.
        let options_u64 = self.options.0.reverse_bits(); // Reverse all bits.
        self.options.0 = options_u64.swap_bytes(); // Restore original byte order.

        self.wakeup_value = self.wakeup_value.swap_bytes();
        self.bp_type = self.bp_type.swap_bytes();
        self.config1 = self.config1.swap_bytes();
        self.config2 = self.config2.swap_bytes();
        self.branch_sample_type = self.branch_sample_type.swap_bytes();
        self.sample_regs_user = self.sample_regs_user.swap_bytes();
        self.sample_stack_user = self.sample_stack_user.swap_bytes();
        self.aux_watermark = self.aux_watermark.swap_bytes();
        self.sample_max_stack = self.sample_max_stack.swap_bytes();
        self.aux_sample_size = self.aux_sample_size.swap_bytes();
    }
}

/// perf_event_type: u32 value for PerfEventHeader.Type.
///
/// If perf_event_attr.sample_id_all is set then all event types will
/// have the SampleType selected fields related to where/when
/// (identity) an event took place (TID, TIME, ID, STREAM_ID, CPU,
/// IDENTIFIER) described in PERF_RECORD_SAMPLE below, it will be stashed
/// just after the perf_event_header and the fields already present for
/// the existing fields, i.e. at the end of the payload. That way a newer
/// perf.data file will be supported by older perf tools, with these new
/// optional fields being ignored.
/// ```C
/// struct sample_id {
///     { u32   pid, tid; } && PERF_SAMPLE_TID
///     { u64   time;     } && PERF_SAMPLE_TIME
///     { u64   id;       } && PERF_SAMPLE_ID
///     { u64   stream_id;} && PERF_SAMPLE_STREAM_ID
///     { u32   cpu, res; } && PERF_SAMPLE_CPU
///     { u64   id;       } && PERF_SAMPLE_IDENTIFIER
/// } && perf_event_attr::sample_id_all
/// ```
/// Note that PERF_SAMPLE_IDENTIFIER duplicates PERF_SAMPLE_ID.  The
/// advantage of PERF_SAMPLE_IDENTIFIER is that its position is fixed
/// relative to header.size.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PerfEventHeaderType(pub u32);

impl PerfEventHeaderType {
    /// Invalid event type.
    pub const None: Self = Self(0);

    /// PERF_RECORD_MMAP:
    ///
    /// The MMAP events record the PROT_EXEC mappings so that we can
    /// correlate userspace IPs to code. They have the following structure:
    /// ```C
    /// struct {
    ///    struct perf_event_header    header;
    ///
    ///    u32                pid, tid;
    ///    u64                addr;
    ///    u64                len;
    ///    u64                pgoff;
    ///    char               filename[];
    ///    struct sample_id   sample_id;
    /// };
    /// ```
    pub const Mmap: Self = Self(1);

    /// PERF_RECORD_LOST:
    /// ```C
    /// struct {
    ///    struct perf_event_header    header;
    ///    u64                id;
    ///    u64                lost;
    ///    struct sample_id   sample_id;
    /// };
    /// ```
    pub const Lost: Self = Self(2);

    /// PERF_RECORD_COMM:
    /// ```C
    /// struct {
    ///    struct perf_event_header    header;
    ///
    ///    u32                pid, tid;
    ///    char               comm[];
    ///    struct sample_id   sample_id;
    /// };
    /// ```
    pub const Comm: Self = Self(3);

    /// PERF_RECORD_EXIT:
    /// ```C
    /// struct {
    ///    struct perf_event_header    header;
    ///    u32                pid, ppid;
    ///    u32                tid, ptid;
    ///    u64                time;
    ///    struct sample_id   sample_id;
    /// };
    /// ```
    pub const Exit: Self = Self(4);

    /// PERF_RECORD_THROTTLE:
    /// ```C
    /// struct {
    ///    struct perf_event_header    header;
    ///    u64                time;
    ///    u64                id;
    ///    u64                stream_id;
    ///    struct sample_id   sample_id;
    /// };
    /// ```
    pub const Throttle: Self = Self(5);

    /// PERF_RECORD_UNTHROTTLE:
    /// ```C
    /// struct {
    ///    struct perf_event_header    header;
    ///    u64                time;
    ///    u64                id;
    ///    u64                stream_id;
    ///    struct sample_id   sample_id;
    /// };
    /// ```
    pub const Unthrottle: Self = Self(6);

    /// PERF_RECORD_FORK:
    /// ```C
    /// struct {
    ///    struct perf_event_header    header;
    ///    u32                pid, ppid;
    ///    u32                tid, ptid;
    ///    u64                time;
    ///    struct sample_id   sample_id;
    /// };
    /// ```
    pub const Fork: Self = Self(7);

    /// PERF_RECORD_READ:
    /// ```C
    /// struct {
    ///    struct perf_event_header    header;
    ///    u32                pid, tid;
    ///
    ///    struct read_format        values;
    ///     struct sample_id        sample_id;
    /// };
    /// ```
    pub const Read: Self = Self(8);

    /// PERF_RECORD_SAMPLE:
    /// ```C
    /// struct {
    ///    struct perf_event_header    header;
    ///
    ///    #
    ///    # Note that PERF_SAMPLE_IDENTIFIER duplicates PERF_SAMPLE_ID.
    ///    # The advantage of PERF_SAMPLE_IDENTIFIER is that its position
    ///    # is fixed relative to header.
    ///    #
    ///
    ///    { u64            id;      } && PERF_SAMPLE_IDENTIFIER
    ///    { u64            ip;      } && PERF_SAMPLE_IP
    ///    { u32            pid, tid; } && PERF_SAMPLE_TID
    ///    { u64            time;     } && PERF_SAMPLE_TIME
    ///    { u64            addr;     } && PERF_SAMPLE_ADDR
    ///    { u64            id;      } && PERF_SAMPLE_ID
    ///    { u64            stream_id;} && PERF_SAMPLE_STREAM_ID
    ///    { u32            cpu, res; } && PERF_SAMPLE_CPU
    ///    { u64            period;   } && PERF_SAMPLE_PERIOD
    ///
    ///    { struct read_format    values;      } && PERF_SAMPLE_READ
    ///
    ///    { u64            nr,
    ///      u64            ips[nr];  } && PERF_SAMPLE_CALLCHAIN
    ///
    ///    #
    ///    # The RAW record below is opaque data wrt the ABI
    ///    #
    ///    # That is, the ABI doesn't make any promises wrt to
    ///    # the stability of its content, it may vary depending
    ///    # on event, hardware, kernel version and phase of
    ///    # the moon.
    ///    #
    ///    # In other words, PERF_SAMPLE_RAW contents are not an ABI.
    ///    #
    ///
    ///    { u32            size;
    ///      char                  data[size];}&& PERF_SAMPLE_RAW
    ///
    ///    { u64                   nr;
    ///      { u64    hw_idx; } && PERF_SAMPLE_BRANCH_HW_INDEX
    ///        { u64 from, to, flags } lbr[nr];
    ///      } && PERF_SAMPLE_BRANCH_STACK
    ///
    ///     { u64            abi; # enum perf_sample_regs_abi
    ///       u64            regs[weight(mask)]; } && PERF_SAMPLE_REGS_USER
    ///
    ///     { u64            size;
    ///       char            data[size];
    ///       u64            dyn_size; } && PERF_SAMPLE_STACK_USER
    ///
    ///    { union perf_sample_weight
    ///     {
    ///        u64        full; && PERF_SAMPLE_WEIGHT
    ///    #if defined(__LITTLE_ENDIAN_BITFIELD)
    ///        struct {
    ///            u32    var1_dw;
    ///            u16    var2_w;
    ///            u16    var3_w;
    ///        } && PERF_SAMPLE_WEIGHT_STRUCT
    ///    #elif defined(__BIG_ENDIAN_BITFIELD)
    ///        struct {
    ///            u16    var3_w;
    ///            u16    var2_w;
    ///            u32    var1_dw;
    ///        } && PERF_SAMPLE_WEIGHT_STRUCT
    ///    #endif
    ///     }
    ///    }
    ///    { u64            data_src; } && PERF_SAMPLE_DATA_SRC
    ///    { u64            transaction; } && PERF_SAMPLE_TRANSACTION
    ///    { u64            abi; # enum perf_sample_regs_abi
    ///      u64            regs[weight(mask)]; } && PERF_SAMPLE_REGS_INTR
    ///    { u64            phys_addr;} && PERF_SAMPLE_PHYS_ADDR
    ///    { u64            size;
    ///      char            data[size]; } && PERF_SAMPLE_AUX
    ///    { u64            data_page_size;} && PERF_SAMPLE_DATA_PAGE_SIZE
    ///    { u64            code_page_size;} && PERF_SAMPLE_CODE_PAGE_SIZE
    /// };
    /// ```
    pub const Sample: Self = Self(9);

    /// PERF_RECORD_MMAP2:
    /// The MMAP2 records are an augmented version of MMAP, they add
    /// maj, min, ino numbers to be used to uniquely identify each mapping
    /// ```C
    /// struct {
    ///    struct perf_event_header    header;
    ///
    ///    u32                pid, tid;
    ///    u64                addr;
    ///    u64                len;
    ///    u64                pgoff;
    ///    union {
    ///        struct {
    ///            u32        maj;
    ///            u32        min;
    ///            u64        ino;
    ///            u64        ino_generation;
    ///        };
    ///        struct {
    ///            u8        build_id_size;
    ///            u8        __reserved_1;
    ///            u16        __reserved_2;
    ///            u8        build_id[20];
    ///        };
    ///    };
    ///    u32                prot, flags;
    ///    char                filename[];
    ///     struct sample_id        sample_id;
    /// };
    /// ```
    pub const Mmap2: Self = Self(10);

    /// PERF_RECORD_AUX:
    ///
    /// Records that new data landed in the AUX buffer part.
    /// ```C
    /// struct {
    ///     struct perf_event_header    header;
    ///
    ///     u64                aux_offset;
    ///     u64                aux_size;
    ///    u64                flags;
    ///     struct sample_id        sample_id;
    /// };
    /// ```
    pub const Aux: Self = Self(11);

    /// PERF_RECORD_ITRACE_START:
    ///
    /// Indicates that instruction trace has started
    /// ```C
    /// struct {
    ///    struct perf_event_header    header;
    ///    u32                pid;
    ///    u32                tid;
    ///    struct sample_id        sample_id;
    /// };
    /// ```
    pub const ItraceStart: Self = Self(12);

    /// PERF_RECORD_LOST_SAMPLES:
    ///
    /// Records the dropped/lost sample number.
    /// ```C
    /// struct {
    ///    struct perf_event_header    header;
    ///
    ///    u64                lost;
    ///    struct sample_id        sample_id;
    /// };
    /// ```
    pub const LostSamples: Self = Self(13);

    /// PERF_RECORD_SWITCH:
    ///
    /// Records a context switch in or out (flagged by
    /// PERF_RECORD_MISC_SWITCH_OUT). See also
    /// PERF_RECORD_SWITCH_CPU_WIDE.
    /// ```C
    /// struct {
    ///    struct perf_event_header    header;
    ///    struct sample_id        sample_id;
    /// };
    /// ```
    pub const Switch: Self = Self(14);

    /// PERF_RECORD_SWITCH_CPU_WIDE:
    ///
    /// CPU-wide version of PERF_RECORD_SWITCH with next_prev_pid and
    /// next_prev_tid that are the next (switching out) or previous
    /// (switching in) pid/tid.
    /// ```C
    /// struct {
    ///    struct perf_event_header    header;
    ///    u32                next_prev_pid;
    ///    u32                next_prev_tid;
    ///    struct sample_id        sample_id;
    /// };
    /// ```
    pub const SwitchCpuWide: Self = Self(15);

    /// PERF_RECORD_NAMESPACES:
    /// ```C
    /// struct {
    ///    struct perf_event_header    header;
    ///    u32                pid;
    ///    u32                tid;
    ///    u64                nr_namespaces;
    ///    { u64                dev, inode; } [nr_namespaces];
    ///    struct sample_id        sample_id;
    /// };
    /// ```
    pub const Namespaces: Self = Self(16);

    /// PERF_RECORD_KSYMBOL:
    ///
    /// Record ksymbol register/unregister events:
    /// ```C
    /// struct {
    ///    struct perf_event_header    header;
    ///    u64                addr;
    ///    u32                len;
    ///    u16                ksym_type;
    ///    u16                flags;
    ///    char                name[];
    ///    struct sample_id        sample_id;
    /// };
    /// ```
    pub const Ksymbol: Self = Self(17);

    /// PERF_RECORD_BPF_EVENT:
    ///
    /// Record bpf events:
    /// ```C
    ///  enum perf_bpf_event_type {
    ///    PERF_BPF_EVENT_UNKNOWN        = 0,
    ///    PERF_BPF_EVENT_PROG_LOAD    = 1,
    ///    PERF_BPF_EVENT_PROG_UNLOAD    = 2,
    ///  };
    ///
    /// struct {
    ///    struct perf_event_header    header;
    ///    u16                type;
    ///    u16                flags;
    ///    u32                id;
    ///    u8                tag[BPF_TAG_SIZE];
    ///    struct sample_id        sample_id;
    /// };
    /// ```
    pub const BpfEvent: Self = Self(18);

    /// PERF_RECORD_CGROUP:
    /// ```C
    /// struct {
    ///    struct perf_event_header    header;
    ///    u64                id;
    ///    char                path[];
    ///    struct sample_id        sample_id;
    /// };
    /// ```
    pub const Cgroup: Self = Self(19);

    /// PERF_RECORD_TEXT_POKE:
    ///
    /// Records changes to kernel text i.e. self-modified code. 'old_len' is
    /// the number of old bytes, 'new_len' is the number of new bytes. Either
    /// 'old_len' or 'new_len' may be zero to indicate, for example, the
    /// addition or removal of a trampoline. 'bytes' contains the old bytes
    /// followed immediately by the new bytes.
    /// ```C
    /// struct {
    ///    struct perf_event_header    header;
    ///    u64                addr;
    ///    u16                old_len;
    ///    u16                new_len;
    ///    u8                bytes[];
    ///    struct sample_id        sample_id;
    /// };
    /// ```
    pub const TextPoke: Self = Self(20);

    /// PERF_RECORD_AUX_OUTPUT_HW_ID:
    ///
    /// Data written to the AUX area by hardware due to aux_output, may need
    /// to be matched to the event by an architecture-specific hardware ID.
    /// This records the hardware ID, but requires sample_id to provide the
    /// event ID. e.g. Intel PT uses this record to disambiguate PEBS-via-PT
    /// records from multiple events.
    /// ```C
    /// struct {
    ///    struct perf_event_header    header;
    ///    u64                hw_id;
    ///    struct sample_id        sample_id;
    /// };
    /// ```
    pub const AuxOutputHwId: Self = Self(21);

    /// PERF_RECORD_HEADER_ATTR:
    /// ```C
    /// struct attr_event {
    ///     struct perf_event_header header;
    ///     struct perf_event_attr attr;
    ///     UInt64 id[];
    /// };
    /// ```
    pub const HeaderAttr: Self = Self(64);

    /// PERF_RECORD_USER_TYPE_START: non-ABI
    pub const UserTypeStart: Self = Self::HeaderAttr;

    /// PERF_RECORD_HEADER_EVENT_TYPE: deprecated
    /// ```C
    /// #define MAX_EVENT_NAME 64
    ///
    /// struct perf_trace_event_type {
    ///     UInt64    event_id;
    ///     char    name[MAX_EVENT_NAME];
    /// };
    ///
    /// struct event_type_event {
    ///     struct perf_event_header header;
    ///     struct perf_trace_event_type event_type;
    /// };
    /// ```
    pub const HeaderEventType: Self = Self(65);

    /// PERF_RECORD_HEADER_TRACING_DATA:
    /// ```C
    /// struct tracing_data_event {
    ///     struct perf_event_header header;
    ///     UInt32 size;
    /// };
    /// ```
    pub const HeaderTracingData: Self = Self(66);

    /// PERF_RECORD_HEADER_BUILD_ID:
    ///
    /// Define a ELF build ID for a referenced executable.
    pub const HeaderBuildId: Self = Self(67);

    /// PERF_RECORD_FINISHED_ROUND:
    ///
    /// No event reordering over this header. No payload.
    pub const FinishedRound: Self = Self(68);

    /// PERF_RECORD_ID_INDEX:
    ///
    /// Map event ids to CPUs and TIDs.
    /// ```C
    /// struct id_index_entry {
    ///     UInt64 id;
    ///     UInt64 idx;
    ///     UInt64 cpu;
    ///     UInt64 tid;
    /// };
    ///
    /// struct id_index_event {
    ///     struct perf_event_header header;
    ///     UInt64 nr;
    ///     struct id_index_entry entries[nr];
    /// };
    /// ```
    pub const IdIndex: Self = Self(69);

    /// PERF_RECORD_AUXTRACE_INFO:
    ///
    /// Auxtrace type specific information. Describe me
    /// ```C
    /// struct auxtrace_info_event {
    ///     struct perf_event_header header;
    ///     UInt32 type;
    ///     UInt32 reserved__; // For alignment
    ///     UInt64 priv[];
    /// };
    /// ```
    pub const AuxtraceInfo: Self = Self(70);

    /// PERF_RECORD_AUXTRACE:
    ///
    /// Defines auxtrace data. Followed by the actual data. The contents of
    /// the auxtrace data is dependent on the event and the CPU. For example
    /// for Intel Processor Trace it contains Processor Trace data generated
    /// by the CPU.
    /// ```C
    /// struct auxtrace_event {
    ///      struct perf_event_header header;
    ///      UInt64 size;
    ///      UInt64 offset;
    ///      UInt64 reference;
    ///      UInt32 idx;
    ///      UInt32 tid;
    ///      UInt32 cpu;
    ///      UInt32 reserved__; // For alignment
    /// };
    ///
    /// struct aux_event {
    ///      struct perf_event_header header;
    ///      UInt64    aux_offset;
    ///      UInt64    aux_size;
    ///      UInt64    flags;
    /// };
    /// ```
    pub const Auxtrace: Self = Self(71);

    /// PERF_RECORD_AUXTRACE_ERROR:
    ///
    /// Describes an error in hardware tracing
    /// ```C
    /// enum auxtrace_error_type {
    ///     PERF_AUXTRACE_ERROR_ITRACE  = 1,
    ///     PERF_AUXTRACE_ERROR_MAX
    /// };
    ///
    /// #define MAX_AUXTRACE_ERROR_MSG 64
    ///
    /// struct auxtrace_error_event {
    ///     struct perf_event_header header;
    ///     UInt32 type;
    ///     UInt32 code;
    ///     UInt32 cpu;
    ///     UInt32 pid;
    ///     UInt32 tid;
    ///     UInt32 reserved__; // For alignment
    ///     UInt64 ip;
    ///     char msg[MAX_AUXTRACE_ERROR_MSG];
    /// };
    /// ```
    pub const AuxtraceError: Self = Self(72);

    /// PERF_RECORD_THREAD_MAP
    pub const ThreadMap: Self = Self(73);

    /// PERF_RECORD_CPU_MAP
    pub const CpuMap: Self = Self(74);

    /// PERF_RECORD_STAT_CONFIG
    pub const StatConfig: Self = Self(75);

    /// PERF_RECORD_STAT
    pub const Stat: Self = Self(76);

    /// PERF_RECORD_STAT_ROUND
    pub const StatRound: Self = Self(77);

    /// PERF_RECORD_EVENT_UPDATE
    pub const EventUpdate: Self = Self(78);

    /// PERF_RECORD_TIME_CONV
    pub const TimeConv: Self = Self(79);

    /// PERF_RECORD_HEADER_FEATURE:
    ///
    /// Describes a header feature. These are records used in pipe-mode that
    /// contain information that otherwise would be in perf.data file's header.
    pub const HeaderFeature: Self = Self(80);

    /// PERF_RECORD_COMPRESSED:
    /// ```C
    /// struct compressed_event {
    ///     struct perf_event_header    header;
    ///     char                data[];
    /// };
    /// ```
    /// The header is followed by compressed data frame that can be decompressed
    /// into array of perf trace records. The size of the entire compressed event
    /// record including the header is limited by the max value of header.size.
    pub const Compressed: Self = Self(81);

    /// PERF_RECORD_FINISHED_INIT:
    ///
    /// Marks the end of records for the system, pre-existing threads in system wide
    /// sessions, etc. Those are the ones prefixed PERF_RECORD_USER_*.
    ///
    /// This is used, for instance, to 'perf inject' events after init and before
    /// regular events, those emitted by the kernel, to support combining guest and
    /// host records.
    pub const FinishedInit: Self = Self(82);

    /// Returns a string like "None", "Mmap", "Sample", etc., or None if the value is invalid.
    pub const fn as_string(self) -> Option<&'static str> {
        match self {
            Self::None => Some("None"),
            Self::Mmap => Some("Mmap"),
            Self::Lost => Some("Lost"),
            Self::Comm => Some("Comm"),
            Self::Exit => Some("Exit"),
            Self::Throttle => Some("Throttle"),
            Self::Unthrottle => Some("Unthrottle"),
            Self::Fork => Some("Fork"),
            Self::Read => Some("Read"),
            Self::Sample => Some("Sample"),
            Self::Mmap2 => Some("Mmap2"),
            Self::Aux => Some("Aux"),
            Self::ItraceStart => Some("ItraceStart"),
            Self::LostSamples => Some("LostSamples"),
            Self::Switch => Some("Switch"),
            Self::SwitchCpuWide => Some("SwitchCpuWide"),
            Self::Namespaces => Some("Namespaces"),
            Self::Ksymbol => Some("Ksymbol"),
            Self::BpfEvent => Some("BpfEvent"),
            Self::Cgroup => Some("Cgroup"),
            Self::TextPoke => Some("TextPoke"),
            Self::AuxOutputHwId => Some("AuxOutputHwId"),
            Self::HeaderAttr => Some("HeaderAttr"),
            Self::HeaderEventType => Some("HeaderEventType"),
            Self::HeaderTracingData => Some("HeaderTracingData"),
            Self::HeaderBuildId => Some("HeaderBuildId"),
            Self::FinishedRound => Some("FinishedRound"),
            Self::IdIndex => Some("IdIndex"),
            Self::AuxtraceInfo => Some("AuxtraceInfo"),
            Self::Auxtrace => Some("Auxtrace"),
            Self::AuxtraceError => Some("AuxtraceError"),
            Self::ThreadMap => Some("ThreadMap"),
            Self::CpuMap => Some("CpuMap"),
            Self::StatConfig => Some("StatConfig"),
            Self::Stat => Some("Stat"),
            Self::StatRound => Some("StatRound"),
            Self::EventUpdate => Some("EventUpdate"),
            Self::TimeConv => Some("TimeConv"),
            Self::HeaderFeature => Some("HeaderFeature"),
            Self::Compressed => Some("Compressed"),
            Self::FinishedInit => Some("FinishedInit"),
            _ => None,
        }
    }
}

impl From<u32> for PerfEventHeaderType {
    fn from(val: u32) -> Self {
        Self(val)
    }
}

impl From<PerfEventHeaderType> for u32 {
    fn from(val: PerfEventHeaderType) -> Self {
        val.0
    }
}

impl fmt::Display for PerfEventHeaderType {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if let Some(s) = self.as_string() {
            return f.pad(s);
        } else {
            return self.0.fmt(f);
        }
    }
}

/// Values for PerfEventHeaderMisc.CpuMode.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PerfEventHeaderMiscCpuMode(pub u8);

impl PerfEventHeaderMiscCpuMode {
    /// <summary>
    /// PERF_RECORD_MISC_CPUMODE_UNKNOWN
    /// </summary>
    pub const Unknown: Self = Self(0);

    /// <summary>
    /// PERF_RECORD_MISC_KERNEL
    /// </summary>
    pub const Kernel: Self = Self(1);

    /// <summary>
    /// PERF_RECORD_MISC_USER
    /// </summary>
    pub const User: Self = Self(2);

    /// <summary>
    /// PERF_RECORD_MISC_HYPERVISOR
    /// </summary>
    pub const Hypervisor: Self = Self(3);

    /// <summary>
    /// PERF_RECORD_MISC_GUEST_KERNEL
    /// </summary>
    pub const GuestKernel: Self = Self(4);

    /// <summary>
    /// PERF_RECORD_MISC_GUEST_USER
    /// </summary>
    pub const GuestUser: Self = Self(5);
}

impl From<u8> for PerfEventHeaderMiscCpuMode {
    fn from(val: u8) -> Self {
        Self(val)
    }
}

impl From<PerfEventHeaderMiscCpuMode> for u8 {
    fn from(val: PerfEventHeaderMiscCpuMode) -> Self {
        val.0
    }
}

/// <summary>
/// Value for PerfEventHeader.Misc.
/// </summary>
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PerfEventHeaderMisc(pub u16);

impl PerfEventHeaderMisc {
    /// PERF_RECORD_MISC_CPUMODE
    pub const fn cpu_mode(self) -> PerfEventHeaderMiscCpuMode {
        PerfEventHeaderMiscCpuMode((self.0 & 0x7) as u8)
    }

    /// PERF_RECORD_MISC_PROC_MAP_PARSE_TIMEOUT:
    /// Indicates that /proc/PID/maps parsing are truncated by time out.
    pub const fn proc_map_parse_timeout(self) -> bool {
        (self.0 & 0x1000) != 0
    }

    /// PERF_RECORD_MISC_MMAP_DATA (PERF_RECORD_MMAP* events only)
    pub const fn mmap_data(self) -> bool {
        (self.0 & 0x2000) != 0
    }

    /// PERF_RECORD_MISC_COMM_EXEC (PERF_RECORD_COMM events only)
    pub const fn comm_exec(self) -> bool {
        (self.0 & 0x2000) != 0
    }

    /// PERF_RECORD_MISC_FORK_EXEC (PERF_RECORD_FORK events only)
    pub const fn fork_exec(self) -> bool {
        (self.0 & 0x2000) != 0
    }

    /// PERF_RECORD_MISC_SWITCH_OUT (PERF_RECORD_SWITCH* events only)
    pub const fn switch_out(self) -> bool {
        (self.0 & 0x2000) != 0
    }

    /// PERF_RECORD_MISC_EXACT_IP (PERF_RECORD_SAMPLE precise events only)
    pub const fn exact_ip(self) -> bool {
        (self.0 & 0x4000) != 0
    }

    /// PERF_RECORD_MISC_SWITCH_OUT_PREEMPT (PERF_RECORD_SWITCH* events only)
    pub const fn switch_out_preempt(self) -> bool {
        (self.0 & 0x4000) != 0
    }

    /// PERF_RECORD_MISC_MMAP_BUILD_ID (PERF_RECORD_MMAP2 events only)
    pub const fn mmap_build_id(self) -> bool {
        (self.0 & 0x4000) != 0
    }

    /// PERF_RECORD_MISC_EXT_RESERVED
    pub const fn ext_reserved(self) -> bool {
        (self.0 & 0x8000) != 0
    }
}

impl From<u16> for PerfEventHeaderMisc {
    fn from(val: u16) -> Self {
        Self(val)
    }
}

impl From<PerfEventHeaderMisc> for u16 {
    fn from(val: PerfEventHeaderMisc) -> Self {
        val.0
    }
}

/// perf_event_header: Information at the start of each event.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default)]
pub struct PerfEventHeader {
    /// perf_event_header::type: Type of event.
    pub header_type: PerfEventHeaderType,

    /// perf_event_header::misc:
    ///
    /// The misc field contains additional information about the sample.
    pub misc: PerfEventHeaderMisc,

    /// perf_event_header::size:
    ///
    /// This indicates the size of the record.
    pub size: u16,
}

impl PerfEventHeader {
    /// size_of::<PerfEventHeader>() == 8.
    pub const SIZE_OF: usize = mem::size_of::<Self>();

    /// Returns a header with all fields set to zero.
    pub const fn new() -> Self {
        Self {
            header_type: PerfEventHeaderType::None,
            misc: PerfEventHeaderMisc(0),
            size: 0,
        }
    }

    /// Reads a PerfEventHeader from a byte array.
    /// - If byte_reader.byte_swap_needed(), returns a byte-swapped copy of `bytes[0..8]`.
    /// - Otherwise, returns `bytes[0..8]`.
    pub const fn from_bytes(bytes: &[u8; 8], byte_reader: PerfByteReader) -> Self {
        let header: PerfEventHeader = unsafe { mem::transmute_copy(bytes) };
        if byte_reader.byte_swap_needed() {
            return header.byte_swap_copy();
        } else {
            return header;
        }
    }

    /// Reverse the endian order of all fields in this struct.
    pub fn byte_swap(&mut self) {
        *self = self.byte_swap_copy();
    }

    /// Return a copy of this struct with all fields byte-reversed.
    pub const fn byte_swap_copy(mut self) -> Self {
        self.header_type.0 = self.header_type.0.swap_bytes();
        self.misc.0 = self.misc.0.swap_bytes();
        self.size = self.size.swap_bytes();
        return self;
    }
}
