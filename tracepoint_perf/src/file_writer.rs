// Copyright (c) Microsoft Corporation.
// Licensed under the MIT license.

use core::mem;
use core::slice;
use core::str;

use std::collections;
use std::io;
use std::string;
use std::sync;
use std::vec;

use tracepoint_decode::*;

use crate::file_abi::*;
use crate::header_index::PerfHeaderIndex;
use crate::output_file::OutputFile;
use crate::PerfDataFileReader;

const U32_SIZE: usize = mem::size_of::<u32>();
const U64_SIZE: usize = mem::size_of::<u64>();

const NAME_MAX_SIZE: usize = 64 * 1024;
const SAMPLE_IDS_MAX_SIZE: usize = 0xFFFFFFFF;

/// Writes `perf.data` files.
///
/// Usage procedure:
/// - Construct a writer: `let mut writer = PerfDataFileWriter::new();`.
/// - Open the file: `writer.create_file(filename, PAGE_SIZE);``
///   - This writes headers and positions the file pointer for event data.
/// - Do the following (in any order):
///   - Call `write_event_data` to write event data to the file.
///   - Call `add_tracepoint_event_desc` to provide event information for events
///     with `tracefs` format information.
///   - Call `add_event_desc` to provide event information for events that don't
///     have `tracefs` format information.
///   - Call `set_header` to provide data for other headers in the file.
/// - Close the file: `writer.finalize_and_close();`
///   - This writes the file footers, finalizes the headers, then closes the file.
#[derive(Debug)]
pub struct PerfDataFileWriter {
    inner: DataFileWriter,
    file: Option<OutputFile>,
}

impl PerfDataFileWriter {
    /// Returns a new writer.
    pub fn new() -> Self {
        Self {
            inner: DataFileWriter::new(),
            file: None,
        }
    }

    /// Immediately closes the current output file (if any).
    /// Does not finalize headers - resulting file will not be usable.
    pub fn close_no_finalize(&mut self) {
        self.inner.close();
        self.file = None;
    }

    /// Writes footer, finalizes header, and closes the output file.
    pub fn finalize_and_close(&mut self) -> io::Result<()> {
        let file = match &mut self.file {
            None => return Err(io::ErrorKind::InvalidInput.into()),
            Some(file) => file,
        };

        self.inner.synthesize_tracing_data();
        self.inner.synthesize_event_desc();

        // Current implementation starts the data section immediately after the file header.
        // It ends wherever we are now.
        const DATA_START_POS: u64 = mem::size_of::<PerfFileHeader>() as u64;
        let data_end_pos = file.pos();

        let mut file_header = PerfFileHeader {
            pipe: PerfFileHeaderPipe {
                magic: PerfDataFileReader::PERFILE2_MAGIC_HOST_ENDIAN,
                size: mem::size_of::<PerfFileHeader>() as u64,
            },

            rest: PerfFileHeaderRest {
                attr_size: mem::size_of::<PerfEventAttr>() as u64
                    + mem::size_of::<PerfFileSection>() as u64,
                attrs: PerfFileSection::default(), // Will be updated by write_attrs().
                data: PerfFileSection {
                    offset: DATA_START_POS,
                    size: data_end_pos - DATA_START_POS,
                },
                event_types: PerfFileSection::default(),
                flags: [0; 4], // Will be updated by write_headers().
            },
        };

        // The sections for the perf headers must go immediately after the data section.
        // Current implementation puts the data for the perf headers right after that.
        file_header.rest.flags[0] = self.inner.write_headers(file)?;

        // The attr section contains a sequence of attr+idSection blocks.
        // Current implementation puts the id data right after that.
        file_header.rest.attrs = self.inner.write_attrs(file)?;

        // Rewrite the file header with the finalized section offsets and sizes.
        file.seek_absolute(0)?;
        file.write_struct(&file_header)?;
        file.flush()?;

        self.close_no_finalize();
        return Ok(());
    }

    /// Calls `close_no_finalize` to close any previous output file,
    /// then creates a new file and writes the file header.
    pub fn create_file(&mut self, path: &str) -> io::Result<()> {
        self.close_no_finalize();

        let mut file = OutputFile::new(path)?;
        file.write_struct(&PerfFileHeader::default())?;

        self.file = Some(file);
        return Ok(());
    }

    /// Returns the file offset at which the next call to `write_event_data` will write.
    /// Returns `None` if the file is closed.
    pub fn file_pos(&self) -> Option<u64> {
        self.file.as_ref().map(|f| f.pos())
    }

    /// Adds a block of event data to the output file (semantics similar to
    /// [`io::Write::write_all`]).
    ///
    /// Data should be a sequence of one or more `perf_event_header` blocks, i.e. a
    /// `perf_event_header`, then data, then another `perf_event_header`, etc.
    ///
    /// On success, returns `Ok`. On error, file state is unspecified (may have written
    /// some but not all of the data to the file).
    ///
    /// Notes:
    /// - The content of `data` is written directly to the event data section of
    ///   the output file without any validation.
    /// - Every `perf_event_header` block's size should be a multiple of 8.
    /// - `data.len()` should almost always be the sum of hdr.size for all headers written,
    ///   except for `PERF_RECORD_HEADER_TRACING_DATA` and `PERF_RECORD_AUXTRACE` which may
    ///   have additional data in the block beyond the size indicated in the header.
    /// - The trace file will be invalid if any events are written with an id
    ///   field that does not have a corresponding entry in the `event_desc` table. You
    ///   need to provide that information by calling `add_tracepoint_event_desc(...)` or
    ///   `add_event_desc(...)`.
    pub fn write_event_data(&mut self, data: &[u8]) -> io::Result<()> {
        let file = match &mut self.file {
            None => return Err(io::ErrorKind::InvalidInput.into()),
            Some(file) => file,
        };

        return file.write_all(data);
    }

    /// Advanced: Adds blocks of event data to the output file (semantics similar to
    /// [`io::Write::write_vectored`]).
    ///
    /// Similar to `write_event_data`, but accepts multiple blocks of data and returns
    /// the number of bytes written (which may be less than the number provided).
    pub fn write_event_data_vectored(&mut self, bufs: &[io::IoSlice]) -> io::Result<usize> {
        let file = match &mut self.file {
            None => return Err(io::ErrorKind::InvalidInput.into()),
            Some(file) => file,
        };

        return file.write_vectored(bufs);
    }

    /// Adds a `PERF_RECORD_FINISHED_INIT` record to the output file. This should be
    /// called after all "initial system state" data has been written to the file,
    ///  e.g. non-sample events like `PERF_RECORD_MMAP`, `PERF_RECORD_COMM`,
    /// `PERF_RECORD_ID_INDEX`, `PERF_RECORD_THREAD_MAP`, `PERF_RECORD_CPU_MAP`.
    pub fn write_finished_init(&mut self) -> io::Result<()> {
        const EVENT: PerfEventHeader = PerfEventHeader {
            header_type: PerfEventHeaderType::FinishedInit,
            misc: PerfEventHeaderMisc(0),
            size: mem::size_of::<PerfEventHeader>() as u16,
        };

        return match &mut self.file {
            None => Err(io::ErrorKind::InvalidInput.into()),
            Some(file) => file.write_struct(&EVENT),
        };
    }

    /// Adds a `PERF_RECORD_FINISHED_ROUND` record to the output file. This should be
    /// called each time you completely flush all buffers. This indicates that no
    /// events older than this point will be written to the file after this point.
    pub fn write_finished_round(&mut self) -> io::Result<()> {
        const EVENT: PerfEventHeader = PerfEventHeader {
            header_type: PerfEventHeaderType::FinishedRound,
            misc: PerfEventHeaderMisc(0),
            size: mem::size_of::<PerfEventHeader>() as u16,
        };

        return match &mut self.file {
            None => Err(io::ErrorKind::InvalidInput.into()),
            Some(file) => file.write_struct(&EVENT),
        };
    }

    /// Returns the data that has been set for the specified header, or empty
    /// if the header has not been set.
    pub fn get_header(&self, index: PerfHeaderIndex) -> &[u8] {
        match self.inner.headers.get(index.0 as usize) {
            Some(header) => header.as_slice(),
            None => &[],
        }
    }

    /// Directly sets or resets the data for the specified header.
    ///
    /// Returns false and does nothing if the specified header index is out of range
    /// (i.e. if it is greater than 31).
    ///
    /// Note that the `PerfDataFileWriter` class has special support for the
    /// following headers:
    ///
    /// - If no data has been set via `set_header(PERF_HEADER_TRACING_DATA, ...)` then
    ///   `finalize_and_close` will synthesize a `PERF_HEADER_TRACING_DATA` header using
    ///   data supplied via `add_tracepoint_event_desc(...)` and `set_tracing_data(...)`.
    /// - If no data has been set via `set_header(PERF_HEADER_EVENT_DESC, ...)` then
    ///   `finalize_and_close` will synthesize a PERF_HEADER_EVENT_DESC header using
    ///   data supplied via `add_tracepoint_event_desc(...)` and `add_event_desc(...)`.
    pub fn set_header(&mut self, index: PerfHeaderIndex, data: &[u8]) -> bool {
        let header = if let Some(header) = self.inner.get_header_vec(index) {
            header
        } else {
            return false;
        };

        header.clear();
        header.extend_from_slice(data);
        return true;
    }

    /// Sets or resets the data for the specified `perf_header_string` header.
    /// Use this for headers where the header value is a `perf_header_string`, e.g.
    /// `HOSTNAME`, `OSRELEASE`, `VERSION`, `ARCH`, `CPUDESC`, `CPUID`, `CMDLINE`.
    ///
    /// Returns false if and does nothing if the specified header index is out of range
    /// (i.e. if it is greater than 31) or if `data.len() >= 0x80000000`.
    ///
    /// The provided data should be an ASCII string (not validated).
    pub fn set_string_header(&mut self, index: PerfHeaderIndex, data: &[u8]) -> bool {
        let header = if let Some(header) = self.inner.get_header_vec(index) {
            header
        } else {
            return false;
        };

        let value_len = strlen(data);
        let header_len = (U32_SIZE + value_len + 1 + 7) & !7;
        header.clear();
        header.reserve(header_len);
        header.extend_from_slice(&((value_len + 1) as u32).to_ne_bytes());
        header.extend_from_slice(&data[..value_len]);
        header.resize(header_len, 0); // NUL-terminate and pad to 8-byte boundary.

        return true;
    }

    /// Sets the data for the `NRCPUS` header.
    pub fn set_nr_cpus_header(&mut self, available: u32, online: u32) {
        let header = self.inner.get_header_vec(PerfHeaderIndex::NrCpus).unwrap();
        header.clear();
        header.reserve(U32_SIZE * 2);
        header.extend_from_slice(&available.to_ne_bytes());
        header.extend_from_slice(&online.to_ne_bytes());
    }

    /// Sets the data for the `SAMPLE_TIME` header.
    pub fn set_sample_time_header(&mut self, first: u64, last: u64) {
        let header = self
            .inner
            .get_header_vec(PerfHeaderIndex::SampleTime)
            .unwrap();
        header.clear();
        header.reserve(U64_SIZE * 2);
        header.extend_from_slice(&first.to_ne_bytes());
        header.extend_from_slice(&last.to_ne_bytes());
    }

    /// Sets the data for the `CLOCKID` header.
    pub fn set_clockid_header(&mut self, clockid: u32) {
        let header = self.inner.get_header_vec(PerfHeaderIndex::ClockId).unwrap();
        header.clear();
        header.reserve(U64_SIZE * 1);
        header.extend_from_slice(&(clockid as u64).to_ne_bytes());
    }

    /// Sets the data for the `CLOCK_DATA` header.
    pub fn set_clock_data_header(
        &mut self,
        clockid: u32,
        wall_clock_ns: u64,
        clockid_time_ns: u64,
    ) {
        let header = self
            .inner
            .get_header_vec(PerfHeaderIndex::ClockData)
            .unwrap();
        header.clear();
        header.reserve(U32_SIZE * 2 + U64_SIZE * 2);
        header.extend_from_slice(&1u32.to_ne_bytes()); // version
        header.extend_from_slice(&clockid.to_ne_bytes());
        header.extend_from_slice(&wall_clock_ns.to_ne_bytes());
        header.extend_from_slice(&clockid_time_ns.to_ne_bytes());
    }

    // Sets or resets the data for headers available in the specified `session_info`:
    // - `CLOCKID`: Set based on `session_info.clock_id()`; cleared if `clock_id() == 0xFFFFFFFF`.
    // - `CLOCK_DATA`: Set based on `session_info.clock_offset()`; cleared if `!session_info.clock_offset_known()`.
    pub fn set_session_info_headers(&mut self, session_info: &PerfSessionInfo) {
        let clock_id = session_info.clock_id();
        if clock_id == 0xFFFFFFFF {
            self.inner
                .get_header_vec(PerfHeaderIndex::ClockId)
                .unwrap()
                .clear();
        } else {
            self.set_clockid_header(clock_id);
        }

        if !session_info.clock_offset_known() {
            self.inner
                .get_header_vec(PerfHeaderIndex::ClockData)
                .unwrap()
                .clear();
        } else {
            let (wall_clock_ns, clock_offset_ns) = session_info.get_clock_data();
            self.set_clock_data_header(clock_id, wall_clock_ns, clock_offset_ns);
        }
    }

    /// Sets or resets the data for the `HOSTNAME`, `OSRELEASE`, and `ARCH` headers.
    /// The data should be ASCII strings (not validated).
    ///
    /// Returns false and does nothing if any of the strings are 2GB or longer.
    ///
    /// The values for these headers usually come from utsname:
    ///
    /// - `hostname` usually comes from `uts.nodename`.
    /// - `os_release` usually comes from `uts.release`.
    /// - `arch` usually comes from `uts.machine`.
    pub fn set_utsname_headers(&mut self, hostname: &[u8], os_release: &[u8], arch: &[u8]) -> bool {
        if hostname.len() >= 0x80000000
            || os_release.len() >= 0x80000000
            || arch.len() >= 0x80000000
        {
            return false;
        }

        self.set_string_header(PerfHeaderIndex::Hostname, hostname);
        self.set_string_header(PerfHeaderIndex::OSRelease, os_release);
        self.set_string_header(PerfHeaderIndex::Arch, arch);
        return true;
    }

    /// Configures information to be included in a synthesized
    /// `PERF_HEADER_TRACING_DATA` header. These settings are used by
    /// `finalize_and_close()` if no explicit header data was provided via
    /// `set_header(PERF_HEADER_TRACING_DATA, ...)`.
    ///
    /// Returns false and does nothing if any of the arguments are invalid:
    ///
    /// - `long_size == 0`
    /// - `page_size == 0`
    /// - `kallsyms.len() >= 0x80000000`
    /// - `printk.len() >= 0x80000000`
    /// - `ftraces.len() >= 0x80000000`
    ///
    /// The `long_size` and `page_size` parameters are required. The `finalize_and_close`
    /// function will not synthesize a `PERF_HEADER_TRACING_DATA` header if these values
    /// are unset.
    ///
    /// For all of the other parameters, a value of `None` indicates "keep the
    /// existing value".
    ///
    /// - `long_size`: Value should be sizeof(size_t) of the system where the trace data comes from.
    /// - `page_size`: Value should be sysconf(_SC_PAGESIZE) of the system where the trace data comes from.
    /// - `header_page`: If empty, will default to timestamp64+commit64+overwrite8+data4080.
    /// - `header_event`: If empty, will default to type_len:5, time_delta:27, array:32.
    pub fn set_tracing_data(
        &mut self,
        long_size: u8,
        page_size: u32,
        header_page: Option<&[u8]>,
        header_event: Option<&[u8]>,
        ftraces: Option<&[&[u8]]>,
        kallsyms: Option<&[u8]>,
        printk: Option<&[u8]>,
        saved_cmd_line: Option<&[u8]>,
    ) -> bool {
        if long_size == 0 || page_size == 0 {
            return false;
        } else if let Some(kallsyms) = kallsyms {
            if kallsyms.len() >= 0x80000000 {
                return false;
            }
        } else if let Some(printk) = printk {
            if printk.len() >= 0x80000000 {
                return false;
            }
        } else if let Some(ftraces) = ftraces {
            if ftraces.len() >= 0x80000000 {
                return false;
            }
        }

        self.inner.tracing_data_long_size = long_size;
        self.inner.tracing_data_page_size = page_size;

        if let Some(value) = header_page {
            self.inner.tracing_data_header_page.clear();
            self.inner.tracing_data_header_page.extend_from_slice(value);
        }

        if let Some(value) = header_event {
            self.inner.tracing_data_header_event.clear();
            self.inner
                .tracing_data_header_event
                .extend_from_slice(value);
        }

        if let Some(value) = ftraces {
            self.inner.tracing_data_ftraces.clear();
            self.inner.tracing_data_ftraces.reserve(value.len());
            for ftrace in value {
                self.inner.tracing_data_ftraces.push(ftrace.to_vec());
            }
        }

        if let Some(value) = kallsyms {
            self.inner.tracing_data_kallsyms.clear();
            self.inner.tracing_data_kallsyms.extend_from_slice(value);
        }

        if let Some(value) = printk {
            self.inner.tracing_data_printk.clear();
            self.inner.tracing_data_printk.extend_from_slice(value);
        }

        if let Some(value) = saved_cmd_line {
            self.inner.tracing_data_cmd_line.clear();
            self.inner.tracing_data_cmd_line.extend_from_slice(value);
        }

        return true;
    }

    /// Adds `perf_event_attr` and name information for the specified event ids.
    /// Use this for events that do NOT have `tracefs` format information, i.e.
    /// when `desc.format().is_empty()`.
    ///
    /// Does nothing and returns false if arguments are invalid:
    ///
    /// - If `desc.name().len() >= 0x10000`
    /// - If `desc.ids().len() >= 0xFFFFFFFF`
    /// - If more than 4 billion descriptors have been added.
    ///
    /// Note that each id used in the trace should map to exactly one attr provided
    /// by `add_tracepoint_event_desc` or add_event_desc``, but this is not validated by
    /// `PerfDataFileWriter`. For example, if the same id is provided in two different
    /// calls to `add_event_desc`, the resulting file may not decode properly.
    pub fn add_event_desc(&mut self, desc: &PerfEventDesc) -> bool {
        let name = desc.name();
        let ids = desc.ids();
        if name.len() >= NAME_MAX_SIZE
            || ids.len() >= SAMPLE_IDS_MAX_SIZE
            || self.inner.event_descs.len() >= 0xFFFFFFFF
        {
            return false;
        }

        self.inner.add_event_desc(name, ids, desc.attr());
        return true;
    }

    /// Returns true if there has been a successful call to
    /// `add_tracepoint_event_desc(desc)` where `desc.format().id() == common_type`.
    pub fn has_tracepoint_event_desc(&self, common_type: u32) -> bool {
        self.inner
            .event_format_by_common_type
            .contains_key(&common_type)
    }

    /// Adds `perf_event_attr`, `name`, and `format` for the specified event ids.
    /// Use this for events that DO have `tracefs` format information, i.e.
    /// when `!desc.format().is_empty()`.
    ///
    /// Does nothing and returns false if arguments are invalid:
    ///
    /// - If `desc.format().is_empty()`
    /// - If `desc.name().len() >= 0x10000`
    /// - If `desc.ids().len() >= 0xFFFFFFFF`
    /// - If more than 4 billion descriptors have been added.
    ///
    /// Does nothing and returns false if format has already been set for the `common_type`
    /// indicated by desc.format().id().
    ///
    /// Note that each id used in the trace should map to exactly one attr provided
    /// by `add_tracepoint_event_desc` or add_event_desc``, but this is not validated by
    /// `PerfDataFileWriter`. For example, if the same id is provided in two different
    /// calls to `add_event_desc`, the resulting file may not decode properly.
    pub fn add_tracepoint_event_desc(&mut self, desc: &PerfEventDesc) -> bool {
        let name = desc.name();
        let ids = desc.ids();
        if name.len() >= NAME_MAX_SIZE
            || ids.len() >= SAMPLE_IDS_MAX_SIZE
            || self.inner.event_descs.len() >= 0xFFFFFFFF
        {
            return false;
        }

        let format = match desc.format_arc() {
            Some(format) => format,
            None => return false,
        };

        match self.inner.event_format_by_common_type.entry(format.id()) {
            collections::btree_map::Entry::Occupied(_) => return false,
            collections::btree_map::Entry::Vacant(e) => {
                e.insert(format.clone());
                self.inner.add_event_desc(name, ids, desc.attr());
                return true;
            }
        };
    }
}

#[derive(Debug)]
struct DataFileWriter {
    event_descs: vec::Vec<EventDesc>,
    event_format_by_common_type: collections::BTreeMap<u32, sync::Arc<PerfEventFormat>>,
    headers: [vec::Vec<u8>; PerfHeaderIndex::LastFeature.0 as usize],
    tracing_data_header_page: vec::Vec<u8>,
    tracing_data_header_event: vec::Vec<u8>,
    tracing_data_ftraces: vec::Vec<vec::Vec<u8>>,
    tracing_data_kallsyms: vec::Vec<u8>,
    tracing_data_printk: vec::Vec<u8>,
    tracing_data_cmd_line: vec::Vec<u8>,
    tracing_data_page_size: u32,
    tracing_data_long_size: u8,
}

impl DataFileWriter {
    fn new() -> Self {
        Self {
            event_descs: vec::Vec::new(),
            event_format_by_common_type: collections::BTreeMap::new(),
            headers: Default::default(),
            tracing_data_header_page: vec::Vec::new(),
            tracing_data_header_event: vec::Vec::new(),
            tracing_data_ftraces: vec::Vec::new(),
            tracing_data_kallsyms: vec::Vec::new(),
            tracing_data_printk: vec::Vec::new(),
            tracing_data_cmd_line: vec::Vec::new(),
            tracing_data_page_size: 0,
            tracing_data_long_size: 0,
        }
    }

    fn close(&mut self) {
        self.event_descs.clear();
        self.event_format_by_common_type.clear();
        self.headers.iter_mut().for_each(|h| h.clear());
        self.tracing_data_header_page.clear();
        self.tracing_data_header_event.clear();
        self.tracing_data_ftraces.clear();
        self.tracing_data_kallsyms.clear();
        self.tracing_data_printk.clear();
        self.tracing_data_cmd_line.clear();
        self.tracing_data_page_size = 0;
        self.tracing_data_long_size = 0;
    }

    fn get_header_vec(&mut self, index: PerfHeaderIndex) -> Option<&mut vec::Vec<u8>> {
        self.headers.get_mut(index.0 as usize)
    }

    fn add_event_desc(&mut self, name: &str, ids: &[u64], attr: &PerfEventAttr) {
        let name_len = strlen(name.as_bytes());
        self.event_descs.push(EventDesc {
            name: (&name[..name_len]).to_string(),
            sample_ids: ids.to_vec(),
            attr: attr.clone(),
        });
    }

    fn synthesize_tracing_data(&mut self) {
        const DEFAULT_HEADER_PAGE: &[u8] = b"\
\tfield: u64 timestamp;\toffset:0;\tsize:8;\tsigned:0;
\tfield: local_t commit;\toffset:8;\tsize:8;\tsigned:1;
\tfield: int overwrite;\toffset:8;\tsize:1;\tsigned:1;
\tfield: char data;\toffset:16;\tsize:4080;\tsigned:0;
";
        const DEFAULT_HEADER_EVENT: &[u8] = b"\
# compressed entry header
\ttype_len    :    5 bits
\ttime_delta  :   27 bits
\tarray       :   32 bits

\tpadding     : type == 29
\ttime_extend : type == 30
\ttime_stamp : type == 31
\tdata max type_len  == 28
";

        if self.tracing_data_long_size == 0 || self.tracing_data_page_size == 0 {
            return;
        }

        let header = &mut self.headers[PerfHeaderIndex::TracingData.0 as usize];
        debug_assert!(header.is_empty());
        header.clear();

        append_string_z(header, b"\x17\x08\x44tracing0.6");
        append_value::<u8>(header, &(cfg!(target_endian = "big") as u8));
        append_value::<u8>(header, &self.tracing_data_long_size);
        append_value::<u32>(header, &self.tracing_data_page_size);
        append_named_section64(
            header,
            b"header_page",
            DEFAULT_HEADER_PAGE,
            &self.tracing_data_header_page,
        );
        append_named_section64(
            header,
            b"header_event",
            DEFAULT_HEADER_EVENT,
            &self.tracing_data_header_event,
        );

        // ftraces
        append_value::<u32>(header, &(self.tracing_data_ftraces.len() as u32));
        for ftrace in &self.tracing_data_ftraces {
            append_section64(header, ftrace);
        }

        // systems (and events)

        // Group events by system.
        let mut formats_by_system: collections::BTreeMap<&str, Vec<&sync::Arc<PerfEventFormat>>> =
            collections::BTreeMap::new();
        for value in self.event_format_by_common_type.values() {
            formats_by_system
                .entry(value.system_name())
                .or_insert(Vec::new())
                .push(value);
        }

        // SystemCount
        append_value::<u32>(header, &(formats_by_system.len() as u32));

        // Systems
        let mut format_str = string::String::new();
        for (system_name, formats) in formats_by_system {
            // SystemName
            append_string_z(header, system_name.as_bytes());

            // EventCount
            append_value::<u32>(header, &(formats.len() as u32));

            // Events
            for format in formats {
                format_str.clear();
                format.write_to(&mut format_str).unwrap();
                append_section64(header, format_str.as_bytes());
            }
        }

        // Other stuff.
        append_section32(header, &self.tracing_data_kallsyms);
        append_section32(header, &self.tracing_data_printk);
        append_section64(header, &self.tracing_data_cmd_line);
    }

    fn synthesize_event_desc(&mut self) {
        let header = &mut self.headers[PerfHeaderIndex::EventDesc.0 as usize];
        debug_assert!(header.is_empty());
        header.clear();

        /*
        From perf.data-file-format.txt:
        struct {
            uint32_t nr; // number of events
            uint32_t attr_size;
            struct {
                struct perf_event_attr attr; // size is attr_size
                uint32_t nr_ids;
                struct perf_header_string event_string;
                uint64_t ids[nr_ids];
            } events[nr]; // Variable length records
        };
        */

        debug_assert!(self.event_descs.len() <= 0xFFFFFFFF);
        let nr = self.event_descs.len() as u32;

        append_value::<u32>(header, &nr); // nr
        append_value::<u32>(header, &(mem::size_of::<PerfEventAttr>() as u32)); // attr_size
        for desc in &self.event_descs {
            let name = desc.name.as_bytes();

            debug_assert!(desc.name.len() <= NAME_MAX_SIZE);
            let name_size = desc.name.len();
            let name_pad = 8 - (name_size & 7); // 1 to 8 bytes of '\0'.

            debug_assert!(desc.sample_ids.len() <= SAMPLE_IDS_MAX_SIZE);
            let nr_ids = desc.sample_ids.len() as u32;

            append_value::<PerfEventAttr>(header, &desc.attr); // attr
            append_value::<u32>(header, &nr_ids); // nr_ids
            append_value::<u32>(header, &((name_size + name_pad) as u32)); // event_string.len
            header.extend_from_slice(&name[..name_size as usize]); // event_string.string
            header.resize(header.len() + name_pad, 0); // NUL + pad to x8
            header.extend_from_slice(unsafe {
                slice::from_raw_parts(
                    desc.sample_ids.as_ptr() as *const u8,
                    desc.sample_ids.len() * mem::size_of::<u64>(),
                )
            });
        }
    }

    fn write_headers(&mut self, file: &mut OutputFile) -> io::Result<u64> {
        let mut flags0: u64 = 0;

        // Update the flags and compute where the first perf header will go.
        let mut first_perf_header_offset = file.pos();
        for i in 0..PerfHeaderIndex::LastFeature.0 {
            let header = &self.headers[i as usize];
            if !header.is_empty() {
                flags0 |= 1 << i;
                first_perf_header_offset += mem::size_of::<PerfFileSection>() as u64;
            }
        }

        // Store perf_file_section for each perf header.
        let mut perf_header_offset = first_perf_header_offset;
        for i in 0..PerfHeaderIndex::LastFeature.0 {
            let header = &mut self.headers[i as usize];
            if !header.is_empty() {
                let header_size = header.len();

                let header_section = PerfFileSection {
                    offset: perf_header_offset,
                    size: header_size as u64,
                };
                perf_header_offset += header_size as u64;

                file.write_struct(&header_section)?;
            }
        }

        // Store data for each perf header that is present.
        for i in 0..PerfHeaderIndex::LastFeature.0 {
            let header = &mut self.headers[i as usize];
            if !header.is_empty() {
                file.write_all(header)?;
            }
        }

        assert_eq!(file.pos(), perf_header_offset);
        return Ok(flags0);
    }

    fn write_attrs(&self, file: &mut OutputFile) -> io::Result<PerfFileSection> {
        const ENTRY_SIZE: u64 =
            mem::size_of::<PerfEventAttr>() as u64 + mem::size_of::<PerfFileSection>() as u64;
        let attrs_section = PerfFileSection {
            offset: file.pos(),
            size: ENTRY_SIZE * self.event_descs.len() as u64,
        };

        let mut attr_ids_offset = attrs_section.offset + attrs_section.size;

        for desc in &self.event_descs {
            file.write_struct(&desc.attr)?;
            let ids_section = PerfFileSection {
                offset: attr_ids_offset,
                size: desc.sample_ids.len() as u64 * mem::size_of::<u64>() as u64,
            };
            attr_ids_offset += ids_section.size;

            file.write_struct(&ids_section)?;
        }

        for desc in &self.event_descs {
            file.write_all(unsafe {
                slice::from_raw_parts(
                    desc.sample_ids.as_ptr() as *const u8,
                    desc.sample_ids.len() * mem::size_of::<u64>(),
                )
            })?;
        }

        return Ok(attrs_section);
    }
}

#[derive(Debug)]
struct EventDesc {
    /// Max size is `NAME_MAX_SIZE`.
    name: string::String,

    /// Max size is `SAMPLE_IDS_MAX_SIZE`.
    sample_ids: vec::Vec<u64>,

    attr: PerfEventAttr,
}

/// Treats value as a slice of bytes and appends it to buf.
fn append_value<T>(buf: &mut vec::Vec<u8>, value: &T)
where
    T: Copy, // Proxy for "T is a plain-old-data struct"
{
    buf.extend_from_slice(unsafe {
        slice::from_raw_parts(value as *const T as *const u8, mem::size_of::<T>())
    });
}

/// Appends value to buf, then appends an additional `0` byte.
fn append_string_z(buf: &mut vec::Vec<u8>, value: &[u8]) {
    buf.reserve(value.len() + 1);
    buf.extend_from_slice(value);
    buf.push(0);
}

fn append_section32(buf: &mut Vec<u8>, value: &[u8]) {
    debug_assert!(value.len() <= u32::MAX as usize);
    buf.reserve(U32_SIZE + value.len());
    buf.extend_from_slice(&(value.len() as u32).to_ne_bytes());
    buf.extend_from_slice(value);
}

fn append_section64(buf: &mut Vec<u8>, value: &[u8]) {
    buf.reserve(U64_SIZE + value.len());
    buf.extend_from_slice(&(value.len() as u64).to_ne_bytes());
    buf.extend_from_slice(value);
}

fn append_named_section64(
    buf: &mut Vec<u8>,
    name: &[u8],
    default_value: &'static [u8],
    value: &[u8],
) {
    append_string_z(buf, name);
    append_section64(
        buf,
        if value.is_empty() {
            default_value
        } else {
            value
        },
    );
}

fn strlen(value: &[u8]) -> usize {
    return value.iter().position(|&c| c == 0).unwrap_or(value.len());
}
