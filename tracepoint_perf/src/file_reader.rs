// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

use core::cmp;
use core::fmt;
use core::mem;
use core::ops;
use core::str;

use std::collections;
use std::fs;
use std::io;
use std::string;
use std::sync;
use std::vec;

use tracepoint_decode::*;

use crate::file_abi::*;
use crate::header_index::PerfHeaderIndex;
use crate::input_file::InputFile;

const PERF_FILE_SECTION_SIZE: usize = mem::size_of::<PerfFileSection>();
const PERF_EVENT_ATTR_SIZE: usize = mem::size_of::<PerfEventAttr>();
const PERF_EVENT_HEADER_SIZE: usize = mem::size_of::<PerfEventHeader>();

const U32_SIZE: usize = mem::size_of::<u32>();
const U64_SIZE: usize = mem::size_of::<u64>();
const U64_ALIGN_MASK: usize = !(U64_SIZE - 1);

const OFFSET_UNSET: i8 = -1;
const OFFSET_NOT_PRESENT: i8 = -2;
const NORMAL_EVENT_MAX_SIZE: usize = 0x10000;
const FREE_BUFFER_LARGER_THAN: usize = NORMAL_EVENT_MAX_SIZE;
const FREE_HEADER_LARGER_THAN: usize = 0x10000;

/// Reads a `perf.data` file, providing access to the events and associated data.
#[derive(Debug)]
pub struct PerfDataFileReader {
    inner: DataFileReader,
    current: EventBytesRef,
    file: Option<InputFile>,
}

impl PerfDataFileReader {
    /// A valid perf.data file starts with `PERFILE2_MAGIC_HOST_ENDIAN`
    /// or `PERFILE2_MAGIC_SWAP_ENDIAN`.
    pub const PERFILE2_MAGIC_HOST_ENDIAN: u64 = 0x32454C4946524550;

    /// A valid perf.data file starts with `PERFILE2_MAGIC_HOST_ENDIAN`
    /// or `PERFILE2_MAGIC_SWAP_ENDIAN`.
    pub const PERFILE2_MAGIC_SWAP_ENDIAN: u64 = 0x50455246494C4532;

    /// Opens the specified file, reads up to 8 bytes, closes it.
    /// If the file contains at least 8 bytes, returns the first 8 bytes as UInt64.
    /// Otherwise, returns 0.
    pub fn read_magic(filename: &str) -> io::Result<u64> {
        let mut file = fs::File::open(filename)?;
        let mut magic = [0u8; 8];
        match io::Read::read_exact(&mut file, &mut magic) {
            Ok(()) => return Ok(u64::from_ne_bytes(magic)),
            Err(e) => {
                if e.kind() == io::ErrorKind::UnexpectedEof {
                    return Ok(0);
                } else {
                    return Err(e);
                }
            }
        }
    }

    /// Opens the specified file, reads up to 8 bytes, closes it.
    /// Returns `true` if the file starts with
    /// [`Self::PERFILE2_MAGIC_HOST_ENDIAN`] or [`Self::PERFILE2_MAGIC_SWAP_ENDIAN`].
    pub fn file_starts_with_magic(filename: &str) -> io::Result<bool> {
        let magic = Self::read_magic(filename)?;
        return Ok(
            magic == Self::PERFILE2_MAGIC_HOST_ENDIAN || magic == Self::PERFILE2_MAGIC_SWAP_ENDIAN
        );
    }

    /// Returns a new reader that is not associated with any file.
    pub fn new() -> Self {
        Self {
            inner: DataFileReader::new(),
            current: EventBytesRef::default(),
            file: None,
        }
    }

    /// Resets the reader to its default-constructed state.
    pub fn close(&mut self) {
        self.inner.close();
        self.current = EventBytesRef::default();
        self.file = None;
    }

    /// Close the current file, if any, then attempts to open the specified file for
    /// reading.
    ///
    /// If not a pipe-mode file, loads headers/attributes.
    ///
    /// If a pipe-mode file, headers and attributes will be loaded as the header
    /// events are encountered by ReadEvent.
    ///
    /// On successful return, the file will be positioned before the first event.
    pub fn open_file(&mut self, path: &str, event_order: PerfDataFileEventOrder) -> io::Result<()> {
        self.close();

        let mut file = InputFile::new(path)?;
        self.inner.open(&mut file, event_order)?;
        self.file = Some(file);
        return Ok(());
    }

    /// Returns true if the the currently-opened file's event data is formatted in
    /// big-endian byte order. (Use [`Self::byte_reader`] to do byte-swapping as appropriate.)
    pub fn source_big_endian(&self) -> bool {
        self.inner.session_info.source_big_endian()
    }

    /// Returns a PerfByteReader configured for the byte order of the events
    /// in the currently-opened file, i.e. PerfByteReader(source_big_endian()).
    pub fn byte_reader(&self) -> PerfByteReader {
        self.inner.session_info.byte_reader()
    }

    /// Returns the position within the input file of the first event.
    pub fn data_begin_file_pos(&self) -> u64 {
        self.inner.data_begin_file_pos
    }

    /// If the input file was recorded in pipe mode, returns [`u64::MAX`].
    /// Otherwise, returns the position within the input file immediately after
    /// the last event.
    pub fn data_end_file_pos(&self) -> u64 {
        self.inner.data_end_file_pos
    }

    /// Gets session information with clock offsets.
    pub fn session_info(&self) -> &PerfSessionInfo {
        &self.inner.session_info
    }

    /// Combined data from `perf_file_header::attrs` and `PERF_RECORD_HEADER_ATTR`.
    pub fn event_desc_list(&self) -> &[PerfEventDesc] {
        &self.inner.attrs.event_desc_list
    }

    /// Combined data from `perf_file_header::attrs`, `PERF_RECORD_HEADER_ATTR`,
    /// and `HEADER_EVENT_DESC`, indexed by sample ID (from `attr.sample_id`).
    pub fn event_desc_by_id(&self, id: u64) -> Option<&PerfEventDesc> {
        self.inner
            .attrs
            .event_desc_id_to_index
            .get(&id)
            .map(|&index| &self.inner.attrs.event_desc_list[index])
    }

    /// Returns the `LongSize` parsed from a `PERF_HEADER_TRACING_DATA` header,
    /// or `0` if no `PERF_HEADER_TRACING_DATA` has been parsed.
    pub fn tracing_data_long_size(&self) -> u8 {
        self.inner.tracing_data_long_size
    }

    /// Returns the `PageSize` parsed from a `PERF_HEADER_TRACING_DATA` header,
    /// or `0` if no `PERF_HEADER_TRACING_DATA` has been parsed.
    pub fn tracing_data_page_size(&self) -> u32 {
        self.inner.tracing_data_page_size
    }

    /// Returns the `header_page` parsed from a `PERF_HEADER_TRACING_DATA` header,
    /// or empty if no `PERF_HEADER_TRACING_DATA` has been parsed.
    pub fn tracing_data_header_page(&self) -> &[u8] {
        &self.inner.headers[PerfHeaderIndex::TracingData.0 as usize][self.inner.header_page.clone()]
    }

    /// Returns the `header_event` parsed from a `PERF_HEADER_TRACING_DATA` header,
    /// or empty if no `PERF_HEADER_TRACING_DATA` has been parsed.
    pub fn tracing_data_header_event(&self) -> &[u8] {
        &self.inner.headers[PerfHeaderIndex::TracingData.0 as usize]
            [self.inner.header_event.clone()]
    }

    /// Returns the number of ftraces parsed from a `PERF_HEADER_TRACING_DATA` header,
    /// or 0 if no `PERF_HEADER_TRACING_DATA` has been parsed.
    pub fn tracing_data_ftrace_count(&self) -> usize {
        self.inner.ftraces.len()
    }

    /// Returns the `ftrace` at the given index parsed from a `PERF_HEADER_TRACING_DATA` header.
    /// Requires `index < tracing_data_ftrace_count()`.
    pub fn tracing_data_ftrace(&self, index: usize) -> &[u8] {
        &self.inner.headers[PerfHeaderIndex::TracingData.0 as usize]
            [self.inner.ftraces[index].clone()]
    }

    /// Returns the `kallsyms` parsed from a `PERF_HEADER_TRACING_DATA` header,
    /// or empty if no `PERF_HEADER_TRACING_DATA` has been parsed.
    pub fn tracing_data_kallsyms(&self) -> &[u8] {
        &self.inner.headers[PerfHeaderIndex::TracingData.0 as usize][self.inner.kallsyms.clone()]
    }

    /// Returns the `printk` parsed from a `PERF_HEADER_TRACING_DATA` header,
    /// or empty if no `PERF_HEADER_TRACING_DATA` has been parsed.
    pub fn tracing_data_printk(&self) -> &[u8] {
        &self.inner.headers[PerfHeaderIndex::TracingData.0 as usize][self.inner.printk.clone()]
    }

    /// Returns the `saved_cmdline` parsed from a `PERF_HEADER_TRACING_DATA` header,
    /// or empty if no `PERF_HEADER_TRACING_DATA` has been parsed.
    pub fn tracing_data_saved_cmd_line(&self) -> &[u8] {
        &self.inner.headers[PerfHeaderIndex::TracingData.0 as usize][self.inner.cmd_line.clone()]
    }

    /// Returns the raw data from the specified header. Data is in file-endian
    /// byte order (use [`Self::byte_reader`] to do byte-swapping as appropriate).
    /// Returns empty if the requested header was not loaded from the file.
    pub fn header(&self, index: PerfHeaderIndex) -> &[u8] {
        if (index.0 as usize) < self.inner.headers.len() {
            return &self.inner.headers[index.0 as usize];
        }
        return &[];
    }

    /// Assumes the specified header is a header followed by a nul-terminated string.
    /// Returns the content of the string, skipping the header, up to the first nul.
    pub fn header_string(&self, index: PerfHeaderIndex) -> &[u8] {
        if (index.0 as usize) < self.inner.headers.len() {
            let header = self.inner.headers[index.0 as usize].as_slice();
            if header.len() >= 4 {
                let nts = &header[4..];
                for i in 0..nts.len() {
                    if nts[i] == 0 {
                        return &nts[..i];
                    }
                }
                return nts;
            }
        }
        return b"";
    }

    /// Returns the header and data of the current event.
    /// If there is no current event, returns a default-constructed `PerfEventBytes`.
    pub fn current_event(&self) -> PerfEventBytes {
        if self.current.range.len() < PERF_EVENT_HEADER_SIZE {
            return PerfEventBytes::new(PerfEventHeader::default(), &[]);
        } else {
            let range = self.current.range.start as usize..self.current.range.end as usize;
            let bytes = &self.inner.buffers[self.current.buffer_index as usize][range];
            let mut header = unsafe {
                mem::transmute_copy::<[u8; PERF_EVENT_HEADER_SIZE], PerfEventHeader>(
                    bytes[..PERF_EVENT_HEADER_SIZE].try_into().unwrap(),
                )
            };
            if self.inner.session_info.byte_reader().byte_swap_needed() {
                header.byte_swap();
            }
            return PerfEventBytes::new(header, bytes);
        }
    }

    /// Reads the next event from the file. Returns `true` if an event was read,
    /// `false` if the end of the file was reached. The event can be accessed
    /// using [`Self::current_event`].
    pub fn move_next_event(&mut self) -> io::Result<bool> {
        let file = match &mut self.file {
            None => return Err(io::ErrorKind::InvalidInput.into()),
            Some(file) => file,
        };

        return match self.inner.event_order {
            PerfDataFileEventOrder::File => {
                self.inner.read_event_file_order(file, &mut self.current)
            }
            PerfDataFileEventOrder::Time => {
                self.inner.read_event_time_order(file, &mut self.current)
            }
        };
    }

    /// Tries to get event information from the event's prefix. The prefix is
    /// usually present only for sample events. If the event prefix is not
    /// present, this function may return an error or it may succeed but return
    /// incorrect information. In general, only use this on events where
    /// `event_bytes.header.header_type == PERF_RECORD_SAMPLE`.
    pub fn get_sample_event_info<'slf, 'dat>(
        &'slf self,
        event_bytes: &PerfEventBytes<'dat>,
    ) -> Result<PerfSampleEventInfo<'dat>, PerfDataFileError>
    where
        'slf: 'dat,
    {
        let byte_reader = self.inner.session_info.byte_reader();

        if self.inner.attrs.sample_id_offset < U64_SIZE as i8 {
            return Err(PerfDataFileError::NoData);
        } else if self.inner.attrs.sample_id_offset as usize + U64_SIZE > event_bytes.data.len() {
            return Err(PerfDataFileError::InvalidData);
        }

        let id =
            byte_reader.read_u64(&event_bytes.data[self.inner.attrs.sample_id_offset as usize..]);

        let event_desc = match self.inner.attrs.event_desc_id_to_index.get(&id) {
            Some(index) => &self.inner.attrs.event_desc_list[*index],
            None => {
                return Err(PerfDataFileError::IdNotFound);
            }
        };

        let info_sample_types = event_desc.attr().sample_type;

        debug_assert!(event_bytes.data.len() >= 2 * U64_SIZE); // Otherwise id lookup would have failed.
        let mut pos = PERF_EVENT_HEADER_SIZE; // Skip PerfEventHeader.
        let end_pos = event_bytes.data.len() & U64_ALIGN_MASK;

        let mut info = PerfSampleEventInfo {
            data: event_bytes.data,
            session_info: &self.inner.session_info,
            event_desc,
            id,
            ip: 0,
            pid: 0,
            tid: 0,
            time: 0,
            addr: 0,
            stream_id: 0,
            cpu: 0,
            cpu_reserved: 0,
            period: 0,
            read_range: 0..0,
            callchain_range: 0..0,
            raw_range: 0..0,
        };

        if info_sample_types.has_flag(PerfEventAttrSampleType::Identifier) {
            debug_assert!(pos != end_pos); // Otherwise id lookup would have failed.
            pos += U64_SIZE; // Was read in id lookup.
        }

        if info_sample_types.has_flag(PerfEventAttrSampleType::IP) {
            if pos == end_pos {
                return Err(PerfDataFileError::InvalidData);
            }

            info.ip = byte_reader.read_u64(&event_bytes.data[pos..]);
            pos += U64_SIZE;
        }

        if info_sample_types.has_flag(PerfEventAttrSampleType::Tid) {
            if pos == end_pos {
                return Err(PerfDataFileError::InvalidData);
            }

            info.pid = byte_reader.read_u32(&event_bytes.data[pos..]);
            pos += U32_SIZE;
            info.tid = byte_reader.read_u32(&event_bytes.data[pos..]);
            pos += U32_SIZE;
        }

        if info_sample_types.has_flag(PerfEventAttrSampleType::Time) {
            if pos == end_pos {
                return Err(PerfDataFileError::InvalidData);
            }

            info.time = byte_reader.read_u64(&event_bytes.data[pos..]);
            pos += U64_SIZE;
        }

        if info_sample_types.has_flag(PerfEventAttrSampleType::Addr) {
            if pos == end_pos {
                return Err(PerfDataFileError::InvalidData);
            }

            info.addr = byte_reader.read_u64(&event_bytes.data[pos..]);
            pos += U64_SIZE;
        }

        if info_sample_types.has_flag(PerfEventAttrSampleType::Id) {
            if pos == end_pos {
                return Err(PerfDataFileError::InvalidData);
            }

            pos += U64_SIZE; // Was read in id lookup.
        }

        if info_sample_types.has_flag(PerfEventAttrSampleType::StreamId) {
            if pos == end_pos {
                return Err(PerfDataFileError::InvalidData);
            }

            info.stream_id = byte_reader.read_u64(&event_bytes.data[pos..]);
            pos += U64_SIZE;
        }

        if info_sample_types.has_flag(PerfEventAttrSampleType::Cpu) {
            if pos == end_pos {
                return Err(PerfDataFileError::InvalidData);
            }

            info.cpu = byte_reader.read_u32(&event_bytes.data[pos..]);
            pos += U32_SIZE;
            info.cpu_reserved = byte_reader.read_u32(&event_bytes.data[pos..]);
            pos += U32_SIZE;
        }

        if info_sample_types.has_flag(PerfEventAttrSampleType::Period) {
            if pos == end_pos {
                return Err(PerfDataFileError::InvalidData);
            }

            info.period = byte_reader.read_u64(&event_bytes.data[pos..]);
            pos += U64_SIZE;
        }

        if info_sample_types.has_flag(PerfEventAttrSampleType::Read) {
            info.read_range.start = pos as u16;

            const SUPPORTED_READ_FORMATS: PerfEventAttrReadFormat = PerfEventAttrReadFormat(
                PerfEventAttrReadFormat::TotalTimeEnabled.0
                    | PerfEventAttrReadFormat::TotalTimeRunning.0
                    | PerfEventAttrReadFormat::Id.0
                    | PerfEventAttrReadFormat::Group.0
                    | PerfEventAttrReadFormat::Lost.0,
            );

            let attr_read_format = event_desc.attr().read_format;
            if attr_read_format.0 & !SUPPORTED_READ_FORMATS.0 != 0 {
                return Err(PerfDataFileError::NotSupported);
            } else if !attr_read_format.has_flag(PerfEventAttrReadFormat::Group) {
                let items_count = 1 // value
                    + attr_read_format.has_flag(PerfEventAttrReadFormat::TotalTimeEnabled) as usize
                    + attr_read_format.has_flag(PerfEventAttrReadFormat::TotalTimeRunning) as usize
                    + attr_read_format.has_flag(PerfEventAttrReadFormat::Id) as usize
                    + attr_read_format.has_flag(PerfEventAttrReadFormat::Lost) as usize;
                let size = items_count * U64_SIZE;
                if end_pos - pos < size {
                    return Err(PerfDataFileError::InvalidData);
                }

                pos += size;
            } else {
                if pos == end_pos {
                    return Err(PerfDataFileError::InvalidData);
                }

                let nr = byte_reader.read_u64(&event_bytes.data[pos..]);
                if nr >= 0x10000 / U64_SIZE as u64 {
                    return Err(PerfDataFileError::InvalidData);
                }

                let static_count = 1 // nr
                    + attr_read_format.has_flag(PerfEventAttrReadFormat::TotalTimeEnabled) as usize
                    + attr_read_format.has_flag(PerfEventAttrReadFormat::TotalTimeRunning) as usize;
                let dyn_count = 1 // value
                    + attr_read_format.has_flag(PerfEventAttrReadFormat::Id) as usize
                    + attr_read_format.has_flag(PerfEventAttrReadFormat::Lost) as usize;
                let size = U64_SIZE * (static_count + nr as usize * dyn_count);
                if end_pos - pos < size {
                    return Err(PerfDataFileError::InvalidData);
                }

                pos += size;
            }

            info.read_range.end = pos as u16;
        }

        if info_sample_types.has_flag(PerfEventAttrSampleType::Callchain) {
            if pos == end_pos {
                return Err(PerfDataFileError::InvalidData);
            }

            let nr = byte_reader.read_u64(&event_bytes.data[pos..]);
            if nr >= 0x10000 / U64_SIZE as u64 {
                return Err(PerfDataFileError::InvalidData);
            }

            let size = U64_SIZE * (1 + nr as usize);
            if end_pos - pos < size {
                return Err(PerfDataFileError::InvalidData);
            }

            info.callchain_range.start = pos as u16;
            pos += size;
            info.callchain_range.end = pos as u16;
        }

        if info_sample_types.has_flag(PerfEventAttrSampleType::Raw) {
            if pos == end_pos {
                return Err(PerfDataFileError::InvalidData);
            }

            let raw_size = byte_reader.read_u32(&event_bytes.data[pos..]);

            if raw_size > (end_pos - pos - U32_SIZE) as u32 {
                return Err(PerfDataFileError::InvalidData);
            }

            info.raw_range.start = (pos + U32_SIZE) as u16;
            info.raw_range.end = info.raw_range.start + (raw_size as u16);

            pos += (U32_SIZE + raw_size as usize + U64_SIZE - 1) & U64_ALIGN_MASK;
        }

        debug_assert!(pos <= end_pos);
        return Ok(info);
    }

    /// Tries to get event information from the event's suffix. The suffix
    /// is usually present only for non-sample kernel-generated events.
    /// If the event suffix is not present, this function may return an error or
    /// it may succeed but return incorrect information. In general:
    ///
    /// - Only use this on events where `event_bytes.header.header_type != PERF_RECORD_SAMPLE`
    ///   and `event_bytes.header.header_type < PERF_RECORD_USER_TYPE_START`.
    ///
    /// - Only use this on events that come after the `PERF_RECORD_FINISHED_INIT`
    ///   event.
    ///
    /// `event_bytes` is the event to decode, usually returned from a call to `ReadEvent`.
    pub fn get_non_sample_event_info<'slf, 'dat>(
        &'slf self,
        event_bytes: &PerfEventBytes<'dat>,
    ) -> Result<PerfNonSampleEventInfo<'dat>, PerfDataFileError>
    where
        'slf: 'dat,
    {
        let byte_reader = self.inner.session_info.byte_reader();

        if event_bytes.header.ty >= PerfEventHeaderType::UserTypeStart {
            return Err(PerfDataFileError::IdNotFound);
        } else if self.inner.attrs.non_sample_id_offset < U64_SIZE as i8 {
            return Err(PerfDataFileError::NoData);
        } else if self.inner.attrs.non_sample_id_offset as usize > event_bytes.data.len() {
            return Err(PerfDataFileError::InvalidData);
        }

        let id = byte_reader.read_u64(
            &event_bytes.data
                [event_bytes.data.len() - self.inner.attrs.non_sample_id_offset as usize..],
        );

        let event_desc = match self.inner.attrs.event_desc_id_to_index.get(&id) {
            Some(index) => &self.inner.attrs.event_desc_list[*index],
            None => {
                return Err(PerfDataFileError::IdNotFound);
            }
        };

        let info_sample_types = event_desc.attr().sample_type;

        debug_assert!(event_bytes.data.len() >= U64_SIZE * 2); // Otherwise id lookup would have failed.
        let mut pos = event_bytes.data.len() & U64_ALIGN_MASK; // Read backwards.

        let mut info = PerfNonSampleEventInfo::<'dat> {
            data: event_bytes.data,
            session_info: &self.inner.session_info,
            event_desc,
            id,
            cpu_reserved: 0,
            cpu: 0,
            stream_id: 0,
            time: 0,
            pid: 0,
            tid: 0,
        };

        if info_sample_types.has_flag(PerfEventAttrSampleType::Identifier) {
            pos -= U64_SIZE; // Was read in id lookup.
            debug_assert!(pos != 0); // Otherwise id lookup would have failed.
        }

        if !info_sample_types.has_flag(PerfEventAttrSampleType::Cpu) {
            info.cpu_reserved = 0;
            info.cpu = 0;
        } else {
            pos -= U32_SIZE;
            info.cpu_reserved = byte_reader.read_u32(&event_bytes.data[pos..]);
            pos -= U32_SIZE;
            info.cpu = byte_reader.read_u32(&event_bytes.data[pos..]);

            if pos == 0 {
                return Err(PerfDataFileError::InvalidData);
            }
        }

        if !info_sample_types.has_flag(PerfEventAttrSampleType::StreamId) {
            info.stream_id = 0;
        } else {
            pos -= U64_SIZE;
            info.stream_id = byte_reader.read_u64(&event_bytes.data[pos..]);

            if pos == 0 {
                return Err(PerfDataFileError::InvalidData);
            }
        }

        if !info_sample_types.has_flag(PerfEventAttrSampleType::Id) {
            // Nothing to do.
        } else {
            pos -= U64_SIZE; // Was read in id lookup.

            if pos == 0 {
                return Err(PerfDataFileError::InvalidData);
            }
        }

        if !info_sample_types.has_flag(PerfEventAttrSampleType::Time) {
            info.time = 0;
        } else {
            pos -= U64_SIZE;
            info.time = byte_reader.read_u64(&event_bytes.data[pos..]);

            if pos == 0 {
                return Err(PerfDataFileError::InvalidData);
            }
        }

        if !info_sample_types.has_flag(PerfEventAttrSampleType::Tid) {
            info.pid = 0;
            info.tid = 0;
        } else {
            pos -= U64_SIZE;
            info.pid = byte_reader.read_u32(&event_bytes.data[pos..]);
            info.tid = byte_reader.read_u32(&event_bytes.data[pos + U32_SIZE..]);

            if pos == 0 {
                return Err(PerfDataFileError::InvalidData);
            }
        }

        debug_assert!(pos >= U64_SIZE);
        debug_assert!(pos < 0x10000 / U64_SIZE);
        return Ok(info);
    }
}

impl Default for PerfDataFileReader {
    fn default() -> Self {
        Self::new()
    }
}

/// Logic of the reader. Separate object for lifetime analysis reasons
/// (because we need to borrow `file` and `inner` mutably at the same time).
#[derive(Debug)]
struct DataFileReader {
    data_begin_file_pos: u64,
    data_end_file_pos: u64,

    attrs: ReaderAttrs,

    buffers: vec::Vec<vec::Vec<u8>>,
    headers: [vec::Vec<u8>; PerfHeaderIndex::LastFeature.0 as usize],

    event_queue: vec::Vec<QueueEntry>,
    event_queue_consumed: usize,

    session_info: PerfSessionInfo,

    current_buffer: u32,
    parsed_header_event_desc: bool,
    event_order: PerfDataFileEventOrder,
    event_queue_pending_result: Option<io::Result<bool>>,

    common_type_size: u8,
    common_type_offset: i8, // -1 = unset.

    // HEADER_TRACING_DATA
    parsed_tracing_data: bool,
    tracing_data_long_size: u8,
    tracing_data_page_size: u32,
    header_page: ops::Range<usize>,       // From headers[TracingData]
    header_event: ops::Range<usize>,      // From headers[TracingData]
    ftraces: vec::Vec<ops::Range<usize>>, // Indexes within headers[TracingData]
    kallsyms: ops::Range<usize>,          // Index within headers[TracingData]
    printk: ops::Range<usize>,            // Index within headers[TracingData]
    cmd_line: ops::Range<usize>,          // Index within headers[TracingData]
}

impl DataFileReader {
    fn new() -> Self {
        const EMPTY_VEC_U8: vec::Vec<u8> = vec::Vec::new();
        DataFileReader {
            data_begin_file_pos: 0,
            data_end_file_pos: 0,

            attrs: ReaderAttrs::new(),

            buffers: vec![vec::Vec::new()],
            headers: [EMPTY_VEC_U8; PerfHeaderIndex::LastFeature.0 as usize],

            event_queue: vec::Vec::new(),
            event_queue_consumed: 0,

            session_info: PerfSessionInfo::new(PerfByteReader::new(false)),

            current_buffer: 0,
            parsed_header_event_desc: false,
            event_order: PerfDataFileEventOrder::File,
            event_queue_pending_result: None,

            common_type_size: 0,
            common_type_offset: OFFSET_UNSET,

            // HEADER_TRACING_DATA
            parsed_tracing_data: false,
            tracing_data_long_size: 0,
            tracing_data_page_size: 0,
            header_page: 0..0,
            header_event: 0..0,
            ftraces: vec::Vec::new(),
            kallsyms: 0..0,
            printk: 0..0,
            cmd_line: 0..0,
        }
    }

    fn close(&mut self) {
        self.data_begin_file_pos = 0;
        self.data_end_file_pos = 0;

        self.attrs.clear();

        // Clear the first buffer but don't trim it.
        self.buffers[0].clear();

        // Clear the other buffers and trim excess capacity.
        for buffer in self.buffers.iter_mut().skip(1) {
            buffer.clear();
            if buffer.capacity() > FREE_BUFFER_LARGER_THAN {
                buffer.shrink_to_fit();
            }
        }

        // Clear the headers and trim excess capacity.
        for header in self.headers.iter_mut() {
            header.clear();
            if header.capacity() > FREE_HEADER_LARGER_THAN {
                header.shrink_to_fit();
            }
        }

        self.event_queue.clear();
        self.event_queue_consumed = 0;

        self.session_info = PerfSessionInfo::new(PerfByteReader::new(false));

        self.current_buffer = 0;
        self.parsed_header_event_desc = false;
        self.event_order = PerfDataFileEventOrder::File;
        self.event_queue_pending_result = None;

        self.common_type_size = 0;
        self.common_type_offset = OFFSET_UNSET;

        // HEADER_TRACING_DATA

        self.parsed_tracing_data = false;
        self.tracing_data_long_size = 0;
        self.tracing_data_page_size = 0;
        self.header_page = 0..0;
        self.header_event = 0..0;
        self.ftraces.clear();
        self.kallsyms = 0..0;
        self.printk = 0..0;
        self.cmd_line = 0..0;
    }

    fn open(
        &mut self,
        file: &mut InputFile,
        event_order: PerfDataFileEventOrder,
    ) -> io::Result<()> {
        const PERF_PIPE_HEADER_SIZE: usize = mem::size_of::<PerfFileHeaderPipe>();
        const PERF_FILE_HEADER_SIZE: usize = mem::size_of::<PerfFileHeader>();

        self.event_order = event_order;

        let mut header = PerfFileHeader::default();

        file.read_struct(&mut header.pipe)?;

        debug_assert_eq!(
            PerfDataFileReader::PERFILE2_MAGIC_HOST_ENDIAN,
            u64::swap_bytes(PerfDataFileReader::PERFILE2_MAGIC_SWAP_ENDIAN)
        );

        let header_size;
        let byte_reader;
        if header.pipe.magic == PerfDataFileReader::PERFILE2_MAGIC_HOST_ENDIAN {
            header_size = header.pipe.size;
            byte_reader = PerfByteReader::KEEP_ENDIAN;
        } else if header.pipe.magic == PerfDataFileReader::PERFILE2_MAGIC_SWAP_ENDIAN {
            header_size = u64::from_be(header.pipe.size);
            byte_reader = PerfByteReader::SWAP_ENDIAN;
        } else {
            // Bad magic.
            return Err(io::ErrorKind::InvalidData.into());
        }

        self.session_info = PerfSessionInfo::new(byte_reader);

        let buffer0 = &mut self.buffers[0];
        debug_assert!(buffer0.is_empty());
        buffer0.reserve(NORMAL_EVENT_MAX_SIZE);

        if header_size == PERF_PIPE_HEADER_SIZE as u64 {
            // Pipe mode, no attrs section, no seeking allowed.
            debug_assert_eq!(file.pos(), PERF_PIPE_HEADER_SIZE as u64);
            self.data_begin_file_pos = PERF_PIPE_HEADER_SIZE as u64;
            self.data_end_file_pos = u64::MAX;
            return Ok(());
        } else if header_size < PERF_FILE_HEADER_SIZE as u64 {
            // Bad header size.
            return Err(io::ErrorKind::InvalidData.into());
        }

        // Normal mode, file expected to be seekable.
        file.update_len()?;

        file.read_struct(&mut header.rest)?;

        if byte_reader.byte_swap_needed() {
            header.byte_swap();
        }

        if !Self::section_valid(file, &header.rest.attrs)
            || !Self::section_valid(file, &header.rest.data)
            || !Self::section_valid(file, &header.rest.event_types)
        {
            return Err(io::ErrorKind::InvalidData.into());
        }

        self.load_attrs(file, &header.rest.attrs, header.rest.attr_size)?;
        self.load_headers(file, &header.rest.data, header.rest.flags[0])?;
        file.seek_absolute(header.rest.data.offset)?;

        debug_assert_eq!(file.pos(), header.rest.data.offset);
        self.data_begin_file_pos = header.rest.data.offset;
        self.data_end_file_pos = header.rest.data.offset + header.rest.data.size;

        return Ok(());
    }

    fn read_event_file_order(
        &mut self,
        file: &mut InputFile,
        event_bytes: &mut EventBytesRef,
    ) -> io::Result<bool> {
        debug_assert_eq!(self.current_buffer, 0);
        self.buffers[0].clear();
        return self.read_one_event(file, event_bytes);
    }

    fn read_event_time_order(
        &mut self,
        file: &mut InputFile,
        event_bytes: &mut EventBytesRef,
    ) -> io::Result<bool> {
        let byte_reader = self.session_info.byte_reader();
        loop {
            if self.event_queue_consumed < self.event_queue.len() {
                let entry = &self.event_queue[self.event_queue_consumed];
                self.event_queue_consumed += 1;
                *event_bytes = entry.bytes_ref.clone();
                return Ok(true);
            }

            if let Some(result) = self.event_queue_pending_result.take() {
                *event_bytes = EventBytesRef::default();
                return result;
            }

            self.event_queue.clear();
            self.event_queue_consumed = 0;

            loop {
                let mut bytes_ref = EventBytesRef::default();
                let result = self.read_one_event(file, &mut bytes_ref);
                match result {
                    Ok(true) => (),
                    _ => {
                        self.event_queue_pending_result = Some(result);
                        break;
                    }
                }

                let mut entry = QueueEntry::default();

                let bytes = &self.buffers[bytes_ref.buffer_index as usize]
                    [bytes_ref.range.start as usize..bytes_ref.range.end as usize];
                let header_type = PerfEventHeaderType(byte_reader.read_u32(bytes));

                if header_type == PerfEventHeaderType::Sample {
                    if self.attrs.sample_time_offset < U64_SIZE as i8
                        || self.attrs.sample_time_offset as usize + U64_SIZE > bytes.len()
                    {
                        entry.time = 0;
                    } else {
                        entry.time =
                            byte_reader.read_u64(&bytes[self.attrs.sample_time_offset as usize..]);
                    }
                } else if header_type >= PerfEventHeaderType::UserTypeStart
                    || self.attrs.non_sample_time_offset < U64_SIZE as i8
                    || self.attrs.non_sample_time_offset as usize > bytes.len()
                {
                    entry.time = 0;
                } else {
                    entry.time = byte_reader.read_u64(
                        &bytes[bytes.len() - self.attrs.non_sample_time_offset as usize..],
                    );
                }

                entry.round_sequence = self.event_queue.len() as u32;
                entry.bytes_ref = bytes_ref;

                if header_type == PerfEventHeaderType::FinishedRound
                    || header_type == PerfEventHeaderType::FinishedInit
                {
                    // These events don't generally have a timestamp.
                    // Force them to show up at the end of the round.
                    entry.time = u64::MAX;
                    self.event_queue.push(entry);
                    break;
                }

                self.event_queue.push(entry);
            }

            // Sort using IComparable<QueueEntry>.
            self.event_queue.sort_unstable();
        }
    }

    fn read_one_event(
        &mut self,
        file: &mut InputFile,
        event_bytes: &mut EventBytesRef,
    ) -> io::Result<bool> {
        let cb = self.current_buffer as usize;

        debug_assert!(self.buffers[cb].capacity() >= NORMAL_EVENT_MAX_SIZE);
        debug_assert!(self.buffers[cb].len() <= NORMAL_EVENT_MAX_SIZE);

        let byte_reader = self.session_info.byte_reader();
        let event_start_file_pos = file.pos();

        if event_start_file_pos >= self.data_end_file_pos {
            *event_bytes = EventBytesRef::default();
            return Ok(false); // normal-mode has reached EOF.
        }

        if PERF_EVENT_HEADER_SIZE as u64 > self.data_end_file_pos - event_start_file_pos {
            *event_bytes = EventBytesRef::default();
            return Err(io::ErrorKind::InvalidData.into());
        }

        let mut event_header_file_endian = [0u8; PERF_EVENT_HEADER_SIZE];
        match file.read_exact(&mut event_header_file_endian) {
            Ok(()) => (),
            Err(e) => {
                if e.kind() == io::ErrorKind::UnexpectedEof && self.data_end_file_pos == u64::MAX {
                    *event_bytes = EventBytesRef::default();
                    return Ok(false); // pipe-mode has reached EOF.
                } else {
                    *event_bytes = EventBytesRef::default();
                    return Err(e);
                }
            }
        }
        let event_header = {
            let mut header: PerfEventHeader =
                unsafe { mem::transmute_copy(&event_header_file_endian) };
            if byte_reader.byte_swap_needed() {
                header.byte_swap();
            }
            header
        };

        // Event size must be at least the size of the header.
        if event_header.size < PERF_EVENT_HEADER_SIZE as u16 {
            *event_bytes = EventBytesRef::default();
            return Err(io::ErrorKind::InvalidData.into());
        }

        // Event size must not exceed the amount of data remaining in the file's event data section.
        let event_data_len = event_header.size - PERF_EVENT_HEADER_SIZE as u16;
        if event_data_len as u64 > self.data_end_file_pos - file.pos() {
            *event_bytes = EventBytesRef::default();
            return Err(io::ErrorKind::InvalidData.into());
        }

        if event_header.size as usize > NORMAL_EVENT_MAX_SIZE - self.buffers[cb].len() {
            // Time-order only: event won't fit into this buffer. Switch to next buffer.
            debug_assert_eq!(self.event_order, PerfDataFileEventOrder::Time);

            self.current_buffer += 1;
            if self.current_buffer < 1 {
                *event_bytes = EventBytesRef::default();
                return Err(io::ErrorKind::OutOfMemory.into());
            }

            if self.current_buffer as usize >= self.buffers.len() {
                debug_assert_eq!(self.current_buffer as usize, self.buffers.len());
                self.buffers
                    .push(vec::Vec::with_capacity(NORMAL_EVENT_MAX_SIZE));
            }

            debug_assert!(self.buffers[self.current_buffer as usize].is_empty());
            debug_assert!(
                self.buffers[self.current_buffer as usize].capacity() >= NORMAL_EVENT_MAX_SIZE
            );
        }
        let cb = self.current_buffer as usize;

        let header_pos = self.buffers[cb].len();
        self.buffers[cb].extend_from_slice(&event_header_file_endian);

        let event_data_pos = self.buffers[cb].len();
        file.read_append_vec(&mut self.buffers[cb], event_data_len as usize)?;

        // Successfully read the basic event data.
        // Check for any special cases based on the type.
        match event_header.ty {
            PerfEventHeaderType::HeaderAttr => {
                if event_data_len >= PerfEventAttrSize::Ver0.0 as u16 {
                    let attr_size = byte_reader
                        .read_u32(&self.buffers[cb][event_data_pos + PerfEventAttr::SIZE_OFFSET..]);
                    if attr_size >= PerfEventAttrSize::Ver0.0 && attr_size < event_data_len as u32 {
                        let attr_size_capped = cmp::min(attr_size, PERF_EVENT_ATTR_SIZE as u32);
                        self.attrs.add_attr(
                            byte_reader,
                            &self.buffers[cb]
                                [event_data_pos..event_data_pos + attr_size_capped as usize],
                            b"",
                            &self.buffers[cb][event_data_pos + attr_size as usize..],
                        );
                    }
                }
            }
            PerfEventHeaderType::HeaderTracingData => {
                if event_data_len < U32_SIZE as u16 {
                    *event_bytes = EventBytesRef::default();
                    return Err(io::ErrorKind::InvalidData.into());
                }

                match Self::read_post_event_data(
                    file,
                    self.data_end_file_pos,
                    byte_reader.read_u32(&self.buffers[cb][event_data_pos..]) as u64,
                    &mut self.buffers[cb],
                ) {
                    Ok(true) => (),
                    Ok(false) => {
                        *event_bytes = EventBytesRef::default();
                        return Err(io::ErrorKind::InvalidData.into());
                    }
                    Err(e) => {
                        *event_bytes = EventBytesRef::default();
                        return Err(e);
                    }
                }

                if !self.parsed_tracing_data {
                    Self::set_header(
                        &mut self.headers[PerfHeaderIndex::TracingData.0 as usize],
                        &self.buffers[cb][header_pos + event_header.size as usize..],
                    );
                    self.parse_tracing_data();
                }
            }
            PerfEventHeaderType::HeaderBuildId => {
                Self::set_header(
                    &mut self.headers[PerfHeaderIndex::BuildId.0 as usize],
                    &self.buffers[cb][event_data_pos..],
                );
            }
            PerfEventHeaderType::Auxtrace => {
                if event_data_len < U64_SIZE as u16 {
                    *event_bytes = EventBytesRef::default();
                    return Err(io::ErrorKind::InvalidData.into());
                }

                match Self::read_post_event_data(
                    file,
                    self.data_end_file_pos,
                    byte_reader.read_u64(&self.buffers[cb][event_data_pos..]),
                    &mut self.buffers[cb],
                ) {
                    Ok(true) => (),
                    Ok(false) => {
                        *event_bytes = EventBytesRef::default();
                        return Err(io::ErrorKind::InvalidData.into());
                    }
                    Err(e) => {
                        *event_bytes = EventBytesRef::default();
                        return Err(e);
                    }
                }
            }
            PerfEventHeaderType::HeaderFeature => {
                if event_data_len >= U64_SIZE as u16 {
                    let index64 = byte_reader.read_u64(&self.buffers[cb][event_data_pos..]);
                    if index64 < self.headers.len() as u64 {
                        Self::set_header(
                            &mut self.headers[index64 as usize],
                            &self.buffers[cb][event_data_pos + mem::size_of::<u64>()..],
                        );
                        match PerfHeaderIndex(index64 as u8) {
                            PerfHeaderIndex::ClockId => self.parse_header_clockid(),
                            PerfHeaderIndex::ClockData => self.parse_header_clock_data(),
                            _ => (),
                        }
                    }
                }
            }
            PerfEventHeaderType::FinishedInit => {
                if !self.headers[PerfHeaderIndex::EventDesc.0 as usize].is_empty() {
                    self.parse_header_event_desc();
                }
            }
            _ => (),
        }

        debug_assert!(file.pos() <= self.data_end_file_pos);
        debug_assert_eq!(event_data_pos, header_pos + PERF_EVENT_HEADER_SIZE);
        *event_bytes = EventBytesRef {
            buffer_index: cb as u32,
            range: header_pos as u32..self.buffers[cb].len() as u32,
        };
        return Ok(true);
    }

    fn load_attrs(
        &mut self,
        file: &mut InputFile,
        attrs_section: &PerfFileSection,
        attr_and_id_section_size64: u64,
    ) -> io::Result<()> {
        debug_assert!(self.buffers[0].is_empty());

        if attrs_section.size >= 0x80000000
            || attr_and_id_section_size64
                < PerfEventAttrSize::Ver0.0 as u64 + PERF_FILE_SECTION_SIZE as u64
            || attr_and_id_section_size64 > 0x10000
        {
            return Err(io::ErrorKind::InvalidData.into());
        }

        let attr_and_id_section_size = attr_and_id_section_size64 as u32;
        let attr_size_in_file = attr_and_id_section_size - PERF_FILE_SECTION_SIZE as u32;

        let mut attr_buf = [0u8; PERF_EVENT_ATTR_SIZE];
        let attr_bytes =
            &mut attr_buf[0..cmp::min(attr_size_in_file as usize, PERF_EVENT_ATTR_SIZE)];
        let mut section = PerfFileSection { offset: 0, size: 0 };

        let new_event_desc_list_count = self.attrs.event_desc_list.len()
            + (attrs_section.size as u32 / attr_and_id_section_size) as usize;
        if new_event_desc_list_count > self.attrs.event_desc_list.capacity() {
            self.attrs
                .event_desc_list
                .reserve(new_event_desc_list_count - self.attrs.event_desc_list.len());
        }

        let attr_file_pos_end = attrs_section.offset + attrs_section.size;
        let mut attr_file_pos = attrs_section.offset;
        while attr_file_pos < attr_file_pos_end {
            file.seek_absolute(attr_file_pos)?;
            file.read_exact(attr_bytes)?;
            attr_file_pos += attr_size_in_file as u64;

            file.seek_absolute(attr_file_pos)?;
            file.read_struct(&mut section)?;
            attr_file_pos += PERF_FILE_SECTION_SIZE as u64;

            if !Self::section_valid(file, &section)
                || section.size & 7 != 0
                || section.size >= 0x80000000
            {
                return Err(io::ErrorKind::InvalidData.into());
            }

            let section_size = section.size as usize;

            file.seek_absolute(section.offset)?;
            file.read_assign_vec(&mut self.buffers[0], section_size)?;
            self.attrs.add_attr(
                self.session_info.byte_reader(),
                attr_bytes,
                b"",
                self.buffers[0].as_slice(),
            );
        }

        return Ok(());
    }

    fn load_headers(
        &mut self,
        file: &mut InputFile,
        data_section: &PerfFileSection,
        flags: u64,
    ) -> io::Result<()> {
        let mut section = PerfFileSection { offset: 0, size: 0 };

        let mut file_pos = data_section.offset + data_section.size;
        let mut mask = 1u64;
        for header_index in 0..self.headers.len() {
            if flags & mask != 0 {
                file.seek_absolute(file_pos)?;
                file.read_struct(&mut section)?;

                file_pos += PERF_FILE_SECTION_SIZE as u64;

                if !Self::section_valid(file, &section) || section.size >= 0x80000000 {
                    return Err(io::ErrorKind::InvalidData.into());
                }

                let section_size = section.size as usize;
                let header = &mut self.headers[header_index];

                file.seek_absolute(section.offset)?;
                file.read_assign_vec(header, section_size)?;

                match PerfHeaderIndex(header_index as u8) {
                    PerfHeaderIndex::ClockId => self.parse_header_clockid(),
                    PerfHeaderIndex::ClockData => self.parse_header_clock_data(),
                    PerfHeaderIndex::EventDesc => self.parse_header_event_desc(),
                    PerfHeaderIndex::TracingData => self.parse_tracing_data(),
                    _ => {}
                }
            }

            mask <<= 1;
        }

        return Ok(());
    }

    fn parse_tracing_data(&mut self) {
        const TRACING_SIGNATURE: &[u8] = b"\x17\x08\x44tracing";

        let data = self.headers[PerfHeaderIndex::TracingData.0 as usize].as_slice();

        if data.len() < TRACING_SIGNATURE.len()
            || &data[..TRACING_SIGNATURE.len()] != TRACING_SIGNATURE
        {
            return;
        }

        self.parsed_tracing_data = true;

        let mut pos = TRACING_SIGNATURE.len();

        // Version

        let version_sz = if let Some(sz) = Self::read_sz(data, pos) {
            sz
        } else {
            return; // Unexpected.
        };
        pos += version_sz.len() + 1;

        // SAFETY: parse<f64> treats input as byte (doesn't care about UTF-8 validity).
        let tracing_data_version = unsafe { str::from_utf8_unchecked(version_sz) }
            .parse::<f64>()
            .unwrap_or(0.0);

        // Big Endian, LongSize, PageSize

        if data.len() - pos < 1 + 1 + U32_SIZE {
            return; // Unexpected.
        }

        let data_byte_reader = PerfByteReader::new(data[pos] != 0);
        pos += 1;

        self.tracing_data_long_size = data[pos];
        pos += 1;

        self.tracing_data_page_size = data_byte_reader.read_u32(&data[pos..]);
        pos += U32_SIZE;

        // header_page

        let section_value =
            Self::read_named_section_64(data_byte_reader, data, pos, b"header_page\0");
        pos = section_value.end;
        if pos == 0 {
            return; // Unexpected.
        }

        self.header_page = section_value;

        // header_event (not really used anymore)

        let section_value =
            Self::read_named_section_64(data_byte_reader, data, pos, b"header_event\0");
        pos = section_value.end;
        if pos == 0 {
            return; // Unexpected.
        }

        self.header_event = section_value;

        // ftraces

        if data.len() - pos < U32_SIZE {
            return; // Unexpected.
        }

        let ftrace_count = data_byte_reader.read_u32(&data[pos..]);
        pos += U32_SIZE;
        if ftrace_count > (data.len() - pos) as u32 / U64_SIZE as u32 {
            return; // Unexpected.
        }

        self.ftraces
            .reserve(self.ftraces.len() + ftrace_count as usize);
        for _ in 0..ftrace_count {
            let section_value = Self::read_section(8, data_byte_reader, data, pos);
            pos = section_value.end;
            if pos == 0 {
                return; // Unexpected.
            }

            self.ftraces.push(section_value);
        }

        // systems (and events)

        if data.len() - pos < U32_SIZE {
            return; // Unexpected.
        }

        let mut system_name = String::new();
        let mut format_file_contents = String::new();

        let system_count = data_byte_reader.read_u32(&data[pos..]);
        pos += U32_SIZE;
        for _ in 0..system_count {
            let system_sz = if let Some(sz) = Self::read_sz(data, pos) {
                sz
            } else {
                return; // Unexpected.
            };
            pos += system_sz.len() + 1;

            assign_latin1(&mut system_name, system_sz);

            if data.len() - pos < U32_SIZE {
                return; // Unexpected.
            }

            let event_count = data_byte_reader.read_u32(&data[pos..]);
            pos += U32_SIZE;
            for _ in 0..event_count {
                let section_value = Self::read_section(8, data_byte_reader, data, pos);
                pos = section_value.end;
                if pos == 0 {
                    return; // Unexpected.
                }

                assign_latin1(&mut format_file_contents, &data[section_value]);

                let long_size_is_64 = self.tracing_data_long_size != 4;
                if let Some(event_format) =
                    PerfEventFormat::parse(long_size_is_64, &system_name, &format_file_contents)
                {
                    let mut common_type_offset = OFFSET_UNSET;
                    let mut common_type_size = 0;
                    for i in 0..event_format.common_field_count() {
                        let field = &event_format.fields()[i];
                        if field.name() == "common_type" {
                            if field.offset() <= i8::MAX as u16
                                && (field.size() == 1 || field.size() == 2 || field.size() == 4)
                                && field.array() == PerfFieldArray::None
                            {
                                common_type_offset = field.offset() as i8;
                                common_type_size = field.size() as u8;
                            }
                            break;
                        }
                    }

                    if common_type_offset == OFFSET_UNSET {
                        // Unexpected: did not find a usable "common_type" field.
                        continue;
                    } else if self.common_type_offset == OFFSET_UNSET {
                        // First event to be parsed. Use its "common_type" field.
                        self.common_type_offset = common_type_offset;
                        self.common_type_size = common_type_size;
                    } else if self.common_type_offset != common_type_offset
                        || self.common_type_size != common_type_size
                    {
                        // Unexpected: found a different "common_type" field.
                        continue;
                    }

                    self.attrs
                        .format_by_id
                        .insert(event_format.id(), sync::Arc::new(event_format));
                }
            }
        }

        // Update EventDesc with the new formats.
        for desc in &mut self.attrs.event_desc_list {
            if desc.format().is_none()
                && desc.attr().attr_type == PerfEventAttrType::Tracepoint
                && self
                    .attrs
                    .format_by_id
                    .contains_key(&(desc.attr().config as u32))
            {
                if let Some(format) = self.attrs.format_by_id.get(&(desc.attr().config as u32)) {
                    desc.set_format(format);
                }
            }
        }

        // kallsyms

        let section_value = Self::read_section(4, data_byte_reader, data, pos);
        pos = section_value.end;
        if pos == 0 {
            return; // Unexpected.
        }

        self.kallsyms = section_value;

        // printk

        let section_value = Self::read_section(4, data_byte_reader, data, pos);
        pos = section_value.end;
        if pos == 0 {
            return; // Unexpected.
        }

        self.printk = section_value;

        // saved_cmdline

        if tracing_data_version >= 0.6 {
            let section_value = Self::read_section(8, data_byte_reader, data, pos);
            pos = section_value.end;
            if pos == 0 {
                return; // Unexpected.
            }

            self.cmd_line = section_value;
        }
    }

    fn parse_header_clockid(&mut self) {
        let data = self.headers[PerfHeaderIndex::ClockId.0 as usize].as_slice();
        if data.len() >= U64_SIZE {
            self.session_info
                .set_clock_id(self.session_info.byte_reader().read_u64(data) as u32);
        }
    }

    fn parse_header_clock_data(&mut self) {
        const CLOCK_DATA_SIZE: usize = mem::size_of::<ClockData>();

        let data = self.headers[PerfHeaderIndex::ClockData.0 as usize].as_slice();
        if data.len() < CLOCK_DATA_SIZE {
            return;
        }

        // SAFETY: Extracting data from a byte array.
        let mut clock_data = unsafe {
            mem::transmute_copy::<[u8; CLOCK_DATA_SIZE], ClockData>(
                data[..CLOCK_DATA_SIZE].try_into().unwrap(),
            )
        };
        if self.session_info.byte_reader().byte_swap_needed() {
            clock_data.byte_swap();
        }
        if 1 <= clock_data.version {
            self.session_info.set_clock_data(
                clock_data.clockid,
                clock_data.wall_clock_ns,
                clock_data.clockid_time_ns,
            );
        }
    }

    fn parse_header_event_desc(&mut self) {
        let data = self.headers[PerfHeaderIndex::EventDesc.0 as usize].as_slice();

        if self.parsed_header_event_desc || data.len() < U32_SIZE + U32_SIZE {
            return;
        }

        self.parsed_header_event_desc = true;

        let byte_reader = self.session_info.byte_reader();
        let mut pos = 0;

        let event_count = byte_reader.read_u32(data);
        pos += U32_SIZE;

        let attr_size = byte_reader.read_u32(&data[pos..]);
        pos += U32_SIZE;
        if !(PerfEventAttrSize::Ver0.0..=0x10000).contains(&attr_size) {
            return; // Unexpected.
        }

        for _ in 0..event_count {
            if data.len() - pos < attr_size as usize + U32_SIZE + U32_SIZE {
                return; // Unexpected.
            }

            let attr_pos = pos;
            pos += attr_size as usize;

            let ids_count = byte_reader.read_u32(&data[pos..]);
            pos += U32_SIZE;

            let string_size = byte_reader.read_u32(&data[pos..]);
            pos += U32_SIZE;

            if attr_size != byte_reader.read_u32(&data[attr_pos + PerfEventAttr::SIZE_OFFSET..])
                || ids_count > 0x10000
                || string_size > 0x10000
                || data.len() - pos < string_size as usize + ids_count as usize * U64_SIZE
            {
                return; // Unexpected.
            }

            let string_pos = pos;
            pos += string_size as usize;

            let string_len = if let Some(nul_pos) = data
                [string_pos..string_pos + string_size as usize]
                .iter()
                .position(|&x| x == 0)
            {
                nul_pos
            } else {
                return; // Unexpected.
            };

            let ids_bytes = &data[pos..pos + ids_count as usize * U64_SIZE];
            pos += ids_bytes.len();

            let attr_size_capped = cmp::min(attr_size, PERF_EVENT_ATTR_SIZE as u32);
            self.attrs.add_attr(
                self.session_info.byte_reader(),
                &data[attr_pos..attr_pos + attr_size_capped as usize],
                &data[string_pos..string_pos + string_len],
                ids_bytes,
            );
        }
    }

    fn section_valid(file: &InputFile, section: &PerfFileSection) -> bool {
        section.offset < file.len() && section.size <= file.len() - section.offset
    }

    fn set_header(header: &mut vec::Vec<u8>, value: &[u8]) {
        header.clear();
        header.extend_from_slice(value);
    }

    /// Expects `data[pos..]` starts with: value (string) + nul.
    /// If nul is present, returns (value, position after the nul).
    /// Otherwise, returns ("", 0).
    fn read_sz(data: &[u8], pos: usize) -> Option<&[u8]> {
        for i in pos..data.len() {
            if data[i] == 0 {
                return Some(&data[pos..i]);
            }
        }
        return None;
    }

    /// Expects `data[pos..]` starts with: name (string) + nul + section_size (u64) + value (`[u8; section_size]`).
    /// expected_name must include the nul terminator.
    /// On success, returns value range; range end is new pos (non-zero).
    /// If name does not match, returns empty range; range end is pos.
    /// On failure, returns empty range; range end is zero.
    fn read_named_section_64(
        data_byte_reader: PerfByteReader,
        data: &[u8],
        pos: usize,
        expected_name: &[u8],
    ) -> ops::Range<usize> {
        debug_assert!(pos <= data.len());

        if data.len() - pos < expected_name.len()
            || *expected_name != data[pos..pos + expected_name.len()]
        {
            return pos..pos;
        }

        return Self::read_section(8, data_byte_reader, data, pos + expected_name.len());
    }

    /// Expects `data[pos..]` starts with: section_size (uN) + value (`[u8; section_size]`).
    /// size_of_section_size must be 4 (section_size is u32) or 8 (section_size is u64).
    /// On success, returns section range (range end is non-zero).
    /// On failure, returns empty range (range end is zero).
    fn read_section(
        size_of_section_size: usize,
        data_byte_reader: PerfByteReader,
        data: &[u8],
        pos: usize,
    ) -> ops::Range<usize> {
        debug_assert!(size_of_section_size == 4 || size_of_section_size == 8);
        debug_assert!(pos <= data.len());

        if data.len() - pos < size_of_section_size {
            return 0..0;
        }

        let section_size = if size_of_section_size == 8 {
            data_byte_reader.read_u64(&data[pos..])
        } else {
            data_byte_reader.read_u32(&data[pos..]) as u64
        };
        let pos = pos + size_of_section_size;

        if section_size > (data.len() - pos) as u64 {
            return 0..0;
        }

        return pos..pos + (section_size as usize);
    }

    fn read_post_event_data(
        file: &mut InputFile,
        data_end_file_pos: u64,
        data_size: u64,
        buffer: &mut vec::Vec<u8>,
    ) -> io::Result<bool> {
        if data_size >= 0x80000000
            || data_size % 8 != 0
            || data_size > data_end_file_pos - file.pos()
        {
            return Ok(false);
        }

        let data_size = data_size as usize;

        let new_end = buffer.len() + data_size;
        if new_end < data_size || new_end >= 0x80000000 {
            return Ok(false);
        }

        file.read_append_vec(buffer, data_size)?;
        return Ok(true);
    }
}

/// Separate struct to avoid lifetime conflicts in add_attr.
#[derive(Debug)]
struct ReaderAttrs {
    event_desc_list: vec::Vec<PerfEventDesc>,
    event_desc_id_to_index: collections::HashMap<u64, usize>,
    format_by_id: collections::HashMap<u32, sync::Arc<PerfEventFormat>>,
    sample_id_offset: i8,       // -1 = unset, -2 = no id.
    non_sample_id_offset: i8,   // -1 = unset, -2 = no id.
    sample_time_offset: i8,     // -1 = unset, -2 = no time.
    non_sample_time_offset: i8, // -1 = unset, -2 = no time.
}

impl ReaderAttrs {
    fn new() -> Self {
        ReaderAttrs {
            event_desc_list: vec::Vec::new(),
            event_desc_id_to_index: collections::HashMap::new(),
            format_by_id: collections::HashMap::new(),
            sample_id_offset: OFFSET_UNSET,
            non_sample_id_offset: OFFSET_UNSET,
            sample_time_offset: OFFSET_UNSET,
            non_sample_time_offset: OFFSET_UNSET,
        }
    }

    fn clear(&mut self) {
        self.event_desc_list.clear();
        self.event_desc_id_to_index.clear();
        self.format_by_id.clear();
        self.sample_id_offset = OFFSET_UNSET;
        self.non_sample_id_offset = OFFSET_UNSET;
        self.sample_time_offset = OFFSET_UNSET;
        self.non_sample_time_offset = OFFSET_UNSET;
    }

    fn add_attr(
        &mut self,
        byte_reader: PerfByteReader,
        attr_bytes: &[u8],
        name: &[u8],
        ids_bytes: &[u8],
    ) -> bool {
        debug_assert!(attr_bytes.len() <= PERF_EVENT_ATTR_SIZE);

        let mut attr = PerfEventAttr::default();

        // SAFETY: Extracting data from a byte array.
        let attr_as_array = unsafe {
            mem::transmute::<&mut PerfEventAttr, &mut [u8; PERF_EVENT_ATTR_SIZE]>(&mut attr)
        };
        attr_as_array[..attr_bytes.len()].copy_from_slice(attr_bytes);

        if byte_reader.byte_swap_needed() {
            attr.byte_swap();
        }

        attr.size = PerfEventAttrSize(attr_bytes.len() as u32);

        let sample_type = attr.sample_type;

        let sample_id_offset;
        let mut non_sample_id_offset;
        if sample_type.has_flag(PerfEventAttrSampleType::Identifier) {
            // ID is at a fixed offset.
            sample_id_offset = U64_SIZE as i8;
            non_sample_id_offset = U64_SIZE as i8;
        } else if !sample_type.has_flag(PerfEventAttrSampleType::Id) {
            // ID is not available.
            sample_id_offset = OFFSET_NOT_PRESENT;
            non_sample_id_offset = OFFSET_NOT_PRESENT;
        } else {
            // ID is at a sample_type-dependent offset.
            // sample_type.has_flag(PerfEventAttrSampleType::Identifier) is known to be 0.
            sample_id_offset = U64_SIZE as i8
                * (1 + sample_type.has_flag(PerfEventAttrSampleType::IP) as i8
                    + sample_type.has_flag(PerfEventAttrSampleType::Tid) as i8
                    + sample_type.has_flag(PerfEventAttrSampleType::Time) as i8
                    + sample_type.has_flag(PerfEventAttrSampleType::Addr) as i8);
            non_sample_id_offset = U64_SIZE as i8
                * (1 + sample_type.has_flag(PerfEventAttrSampleType::Cpu) as i8
                    + sample_type.has_flag(PerfEventAttrSampleType::StreamId) as i8);
        }

        let sample_time_offset;
        let mut non_sample_time_offset;
        if !sample_type.has_flag(PerfEventAttrSampleType::Time) {
            // Time is not available.
            sample_time_offset = OFFSET_NOT_PRESENT;
            non_sample_time_offset = OFFSET_NOT_PRESENT;
        } else {
            // Time is at a sample_type-dependent offset.
            sample_time_offset = U64_SIZE as i8
                * (1 + sample_type.has_flag(PerfEventAttrSampleType::Identifier) as i8
                    + sample_type.has_flag(PerfEventAttrSampleType::IP) as i8
                    + sample_type.has_flag(PerfEventAttrSampleType::Tid) as i8);
            non_sample_time_offset = U64_SIZE as i8
                * (1 + sample_type.has_flag(PerfEventAttrSampleType::Identifier) as i8
                    + sample_type.has_flag(PerfEventAttrSampleType::Cpu) as i8
                    + sample_type.has_flag(PerfEventAttrSampleType::StreamId) as i8
                    + sample_type.has_flag(PerfEventAttrSampleType::Id) as i8);
        }

        if !attr.options.has_flag(PerfEventAttrOptions::SampleIdAll) {
            // Fields not available for non-sample events.
            non_sample_id_offset = OFFSET_NOT_PRESENT;
            non_sample_time_offset = OFFSET_NOT_PRESENT;
        }

        if sample_id_offset != self.sample_id_offset {
            if self.sample_id_offset != OFFSET_UNSET {
                // Unexpected: Inconsistent sample_id_offset across the attrs in the trace.
                return false;
            }

            self.sample_id_offset = sample_id_offset;
        }

        if non_sample_id_offset != self.non_sample_id_offset {
            if self.non_sample_id_offset != OFFSET_UNSET {
                // Unexpected: Inconsistent non_sample_id_offset across the attrs in the trace.
                return false;
            }

            self.non_sample_id_offset = non_sample_id_offset;
        }

        if sample_time_offset != self.sample_time_offset {
            if self.sample_time_offset != OFFSET_UNSET {
                // Unexpected: Inconsistent sample_time_offset across the attrs in the trace.
                return false;
            }

            self.sample_time_offset = sample_time_offset;
        }

        if non_sample_time_offset != self.non_sample_time_offset {
            if self.non_sample_time_offset != OFFSET_UNSET {
                // Unexpected: Inconsistent non_sample_time_offset across the attrs in the trace.
                return false;
            }

            self.non_sample_time_offset = non_sample_time_offset;
        }

        let ids: Vec<u64> = ids_bytes
            .chunks_exact(U64_SIZE)
            .map(|chunk| byte_reader.read_u64(chunk))
            .collect();

        let format = if attr.attr_type != PerfEventAttrType::Tracepoint
            || !self.format_by_id.contains_key(&(attr.config as u32))
        {
            None
        } else {
            Some(&self.format_by_id[&(attr.config as u32)])
        };

        let event_desc_index = self.event_desc_list.len();
        self.event_desc_list.push(PerfEventDesc::new(
            attr,
            string_from_latin1(name),
            format,
            ids.into_boxed_slice(),
        ));

        for id in self.event_desc_list[event_desc_index].ids() {
            self.event_desc_id_to_index.insert(*id, event_desc_index);
        }

        return true;
    }
}

/// Error returned by GetSampleEventInfo or GetNonSampleEventInfo.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PerfDataFileError {
    /// GetSampleEventInfo or GetNonSampleEventInfo:
    /// Failed to get event info because the event contains invalid data.
    InvalidData,

    /// GetSampleEventInfo or GetNonSampleEventInfo:
    /// The event's ID was not found in the event attr table.
    IdNotFound,

    /// GetSampleEventInfo:
    /// Failed to get event info because the event contains headers that this
    /// decoder cannot parse.
    NotSupported,

    /// GetSampleEventInfo or GetNonSampleEventInfo:
    /// Cannot get sample information because the event's ID was not collected in
    /// the trace (the event's sample_type did not include PERF_SAMPLE_ID or
    /// PERF_SAMPLE_IDENTIFIER).
    NoData,
}

impl fmt::Display for PerfDataFileError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            PerfDataFileError::InvalidData => f.pad("InvalidData"),
            PerfDataFileError::IdNotFound => f.pad("IdNotFound"),
            PerfDataFileError::NotSupported => f.pad("NotSupported"),
            PerfDataFileError::NoData => f.pad("NoData"),
        }
    }
}

/// The order in which events are returned by ReadEvent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PerfDataFileEventOrder {
    /// Events are returned in the order they appear in the file.
    File,

    /// Events are sorted by timestamp, with ties broken by the order they appear
    /// in the file. Events with no timestamp are treated as having timestamp 0.
    ///
    /// More precisely: The file is split into "rounds" based on FinishedInit event,
    /// FinishedRound event, and EndOfFile. Within each round, events are
    /// stable-sorted by the event's timestamp.
    Time,
}

#[derive(Debug, Default)]
struct EventBytesRef {
    buffer_index: u32,
    range: ops::Range<u32>,
}

impl Clone for EventBytesRef {
    fn clone(&self) -> Self {
        EventBytesRef {
            buffer_index: self.buffer_index,
            range: self.range.clone(),
        }
    }
}

#[derive(Debug, Default)]
struct QueueEntry {
    time: u64,
    round_sequence: u32,
    bytes_ref: EventBytesRef,
}

impl PartialEq for QueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.time == other.time && self.round_sequence == other.round_sequence
    }
}

impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Eq for QueueEntry {}

impl Ord for QueueEntry {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        let time_cmp = self.time.cmp(&other.time);
        return if time_cmp != cmp::Ordering::Equal {
            time_cmp
        } else {
            self.round_sequence.cmp(&other.round_sequence)
        };
    }
}

fn string_from_latin1(bytes: &[u8]) -> string::String {
    let mut s = string::String::new();
    assign_latin1(&mut s, bytes);
    return s;
}

// TODO: is the encoding of strings in perf files specified anywhere?
// Used for: system_name, format_file_contents, event_desc name.
fn assign_latin1(s: &mut string::String, bytes: &[u8]) {
    s.clear();
    s.reserve(bytes.len());
    for &b in bytes {
        s.push(b as char);
    }
}
